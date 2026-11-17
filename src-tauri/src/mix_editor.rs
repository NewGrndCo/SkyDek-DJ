use crate::{
    audio::{self, Cmd, Mixer, Track},
    mixlog::MixLog,
    recording::RecordingWriter,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

const FORMAT: &str = "skydek.mix";
const VERSION: u8 = 1;
const MAX_EDITS: usize = 500_000;
const MAX_RENDER_SECONDS: f64 = 6.0 * 60.0 * 60.0;
const RENDER_RATE: u32 = 48_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct Edit {
    pub id: String,
    pub seconds: f64,
    pub lane: String,
    pub value: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub seconds: f64,
    pub kind: String,
    pub source: String,
    pub gain: f32,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub mode: Option<crate::sampler::PlayMode>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Source {
    pub kind: String,
    pub audio_file: String,
    pub log: MixLog,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub format: String,
    pub version: u8,
    pub name: String,
    pub created_at: String,
    pub modified_at: String,
    pub source: Source,
    #[serde(default)]
    pub edits: Vec<Edit>,
    #[serde(default)]
    pub triggers: Option<Vec<Trigger>>,
    #[serde(default)]
    pub relinks: HashMap<String, String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
pub struct OpenedSession {
    pub path: String,
    pub text: String,
}

#[derive(Default)]
pub struct PendingOpen(pub std::sync::Mutex<Option<String>>);

#[derive(Default)]
pub struct RenderStatus {
    running: AtomicBool,
    cancel: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
}
#[derive(Serialize)]
pub struct RenderProgress {
    pub running: bool,
    pub done: u64,
    pub total: u64,
}

#[tauri::command]
pub fn mix_editor_render_status(status: tauri::State<'_, Arc<RenderStatus>>) -> RenderProgress {
    RenderProgress {
        running: status.running.load(Ordering::Relaxed),
        done: status.done.load(Ordering::Relaxed),
        total: status.total.load(Ordering::Relaxed),
    }
}
#[tauri::command]
pub fn mix_editor_cancel_render(status: tauri::State<'_, Arc<RenderStatus>>) {
    status.cancel.store(true, Ordering::Relaxed);
}

pub fn dek_argument(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|value| {
            Path::new(value)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("dek"))
        })
        .cloned()
}

#[tauri::command]
pub fn mix_editor_take_open(
    pending: tauri::State<'_, PendingOpen>,
) -> Result<Option<OpenedSession>, String> {
    let Some(path) = pending
        .0
        .lock()
        .map_err(|_| "Pending session lock failed")?
        .take()
    else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    parse(&text)?;
    Ok(Some(OpenedSession {
        path: path.to_string_lossy().into_owned(),
        text,
    }))
}

#[derive(Serialize)]
pub struct Resolution {
    pub original: String,
    pub resolved: Option<String>,
    pub kind: String,
}

fn lane_bounds(lane: &str) -> Option<(f64, f64)> {
    match lane {
        "crossfader" | "filter_0" | "filter_1" => Some((-1., 1.)),
        "master" | "sampler_volume" | "volume_0" | "volume_1" | "wet_0" | "wet_1" => Some((0., 1.)),
        "rate_0" | "rate_1" => Some((0.5, 2.)),
        "effect_0" | "effect_1" => Some((0., 5.)),
        _ => None,
    }
}

fn validate(session: &Session) -> Result<(), String> {
    if session.format != FORMAT || session.version != VERSION || session.source.kind != "legacy-log"
    {
        return Err("Unsupported SkyDek .dek session version".into());
    }
    if session.name.trim().is_empty() || session.name.len() > 512 {
        return Err("Session name is invalid".into());
    }
    if session.source.log.version != 1
        || session.source.log.capture_revision > 1
        || session.source.log.frames.is_empty()
        || session.source.log.frames.len() > 144_000
        || session.source.log.sample_commands.len() > 500_000
        || session.source.log.effect_commands.len() > 500_000
        || session.source.log.mic_commands.len() > 500_000
    {
        return Err("Session contains an invalid legacy performance log".into());
    }
    if session.edits.len() > MAX_EDITS {
        return Err("Session contains too many edits".into());
    }
    let duration = session.source.log.frames.last().map_or(0., |f| f.seconds);
    if !duration.is_finite() || !(0. ..=MAX_RENDER_SECONDS).contains(&duration) {
        return Err("Session duration is invalid or unsupported".into());
    }
    let mut previous = -1.;
    for frame in &session.source.log.frames {
        if !frame.seconds.is_finite() || frame.seconds < previous || frame.seconds < 0. {
            return Err("Performance frames are invalid or out of order".into());
        }
        previous = frame.seconds;
        for deck in &frame.decks {
            if !deck.position.is_finite()
                || deck.position < 0.
                || !deck.rate.is_finite()
                || !(0.5..=2.).contains(&deck.rate)
                || !deck.volume.is_finite()
                || !deck.filter.is_finite()
                || !deck.wet.is_finite()
                || !(0.0..=1.0).contains(&deck.volume)
                || !(-1.0..=1.0).contains(&deck.filter)
                || !(0.0..=1.0).contains(&deck.wet)
            {
                return Err("A recorded deck state is invalid".into());
            }
        }
        if !(-1.0..=1.0).contains(&frame.crossfader)
            || !(0.0..=1.0).contains(&frame.master)
            || !(0.0..=1.0).contains(&frame.sampler_volume)
        {
            return Err("A recorded mixer state is invalid".into());
        }
        if frame.mic.as_ref().is_some_and(|mic| !mic.volume.is_finite() || !(0. ..=1.).contains(&mic.volume)) {
            return Err("A recorded microphone state is invalid".into());
        }
    }
    if session.source.log.mic_commands.iter().any(|command| !command.seconds.is_finite() || command.seconds < 0. || command.seconds > duration + 0.2 || !command.state.volume.is_finite() || !(0. ..=1.).contains(&command.state.volume)) {
        return Err("A recorded microphone command is invalid".into());
    }
    let mut identifiers = HashSet::new();
    for edit in &session.edits {
        let (min, max) = lane_bounds(&edit.lane)
            .ok_or_else(|| format!("Unsupported automation lane: {}", edit.lane))?;
        if edit.id.is_empty()
            || edit.id.len() > 256
            || !identifiers.insert(edit.id.clone())
            || !edit.seconds.is_finite()
            || !(0.0..=duration).contains(&edit.seconds)
            || !edit.value.is_finite()
            || !(min..=max).contains(&edit.value)
        {
            return Err(format!("Invalid automation point in {}", edit.lane));
        }
    }
    for trigger in trigger_events(session) {
        if trigger.id.is_empty()
            || trigger.id.len() > 256
            || !identifiers.insert(trigger.id.clone())
            || trigger.kind != "sample"
            || trigger.source.is_empty()
            || !trigger.seconds.is_finite()
            || !(0.0..=duration).contains(&trigger.seconds)
            || !trigger.gain.is_finite()
            || !(0.0..=2.0).contains(&trigger.gain)
            || trigger.action.as_deref().is_some_and(|action| !matches!(action, "press" | "release"))
        {
            return Err("Session contains an invalid sample trigger".into());
        }
    }
    for (from, to) in &session.relinks {
        if from.len() > 32_768
            || to.len() > 32_768
            || to.trim().is_empty()
            || !Path::new(to).is_absolute()
        {
            return Err("A media relink is invalid".into());
        }
    }
    if let Some(value) = session.extra.get("source_refs") {
        let references = value
            .as_array()
            .ok_or("Session media references are invalid")?;
        if references.len() > 100_000 {
            return Err("Session contains too many media references".into());
        }
        for reference in references {
            let original = reference
                .get("original_path")
                .and_then(|value| value.as_str())
                .ok_or("A media reference has no original path")?;
            let role = reference
                .get("role")
                .and_then(|value| value.as_str())
                .ok_or("A media reference has no role")?;
            if original.is_empty() || !matches!(role, "deck" | "sample" | "recording" | "mic") {
                return Err("A media reference is invalid".into());
            }
            if let Some(relative) = reference
                .get("relative_path")
                .and_then(|value| value.as_str())
            {
                if !safe_relative(Path::new(relative)) {
                    return Err("A media reference contains an unsafe relative path".into());
                }
            }
            if let Some(seconds) = reference.get("duration").and_then(|value| value.as_f64()) {
                if !seconds.is_finite() || !(0.0..=MAX_RENDER_SECONDS).contains(&seconds) {
                    return Err("A media reference duration is invalid".into());
                }
            }
        }
    }
    if let Some(value) = session.extra.get("markers") {
        let markers = value.as_array().ok_or("Session markers are invalid")?;
        if markers.len() > MAX_EDITS {
            return Err("Session contains too many markers".into());
        }
        for marker in markers {
            let seconds = marker
                .get("seconds")
                .and_then(|value| value.as_f64())
                .ok_or("A marker timestamp is invalid")?;
            let label = marker
                .get("label")
                .and_then(|value| value.as_str())
                .ok_or("A marker label is invalid")?;
            let kind = marker
                .get("kind")
                .and_then(|value| value.as_str())
                .ok_or("A marker kind is invalid")?;
            if !(0.0..=duration).contains(&seconds)
                || label.len() > 4096
                || !matches!(kind, "recorded" | "editor")
            {
                return Err("A session marker is invalid".into());
            }
        }
    }
    Ok(())
}

fn parse(text: &str) -> Result<Session, String> {
    if text.len() > 128 * 1024 * 1024 {
        return Err("Session file is too large".into());
    }
    let session: Session =
        serde_json::from_str(text).map_err(|e| format!("Invalid .dek JSON: {e}"))?;
    validate(&session)?;
    Ok(session)
}

fn safe_relative(path: &Path) -> bool {
    path.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn resolve_path(
    original: &str,
    session_path: Option<&Path>,
    relinks: &HashMap<String, String>,
) -> Option<PathBuf> {
    if let Some(relinked) = relinks.get(original) {
        let path = PathBuf::from(relinked);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = PathBuf::from(original);
    if path.is_absolute() && path.is_file() {
        return Some(path);
    }
    if path.is_relative() && safe_relative(&path) {
        if let Some(parent) = session_path.and_then(Path::parent) {
            let candidate = parent.join(&path);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let (Some(parent), Some(name)) = (session_path.and_then(Path::parent), path.file_name()) {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn relative_reference<'a>(session: &'a Session, original: &str) -> Option<&'a str> {
    session
        .extra
        .get("source_refs")?
        .as_array()?
        .iter()
        .find(|reference| {
            reference
                .get("original_path")
                .and_then(|value| value.as_str())
                == Some(original)
        })?
        .get("relative_path")?
        .as_str()
}

fn resolve_media(
    session: &Session,
    original: &str,
    session_path: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = resolve_path(original, None, &session.relinks) {
        return Some(path);
    }
    if let (Some(parent), Some(relative)) = (
        session_path.and_then(Path::parent),
        relative_reference(session, original),
    ) {
        let relative = Path::new(relative);
        if safe_relative(relative) {
            let candidate = parent.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    resolve_path(original, session_path, &HashMap::new())
}

fn annotate_relative_sources(session: &mut Session, session_path: &Path) {
    let Some(parent) = session_path.parent() else {
        return;
    };
    let originals: Vec<String> = session
        .extra
        .get("source_refs")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|reference| {
            reference
                .get("original_path")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .collect();
    let updates: HashMap<String, String> = originals
        .into_iter()
        .filter_map(|original| {
            let resolved = resolve_media(session, &original, Some(session_path))?;
            let relative = resolved.strip_prefix(parent).ok()?;
            safe_relative(relative)
                .then(|| (original, relative.to_string_lossy().replace('\\', "/")))
        })
        .collect();
    let Some(references) = session
        .extra
        .get_mut("source_refs")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    for reference in references {
        let Some(original) = reference
            .get("original_path")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if let Some(relative) = updates.get(original) {
            reference["relative_path"] = serde_json::Value::String(relative.clone());
        }
    }
}

fn media_paths(session: &Session) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for frame in &session.source.log.frames {
        for deck in &frame.decks {
            if let Some(path) = &deck.path {
                paths.insert(path.clone());
            }
        }
    }
    paths.extend(trigger_events(session).into_iter().map(|t| t.source));
    if let Some(mic_file) = session.source.log.mic_file.as_ref().filter(|path| !path.is_empty()) {
        paths.insert(mic_file.clone());
    }
    paths
}

fn trigger_events(session: &Session) -> Vec<Trigger> {
    if let Some(triggers) = &session.triggers {
        let mut out = triggers.clone();
        out.sort_by(|a, b| {
            a.seconds
                .total_cmp(&b.seconds)
                .then_with(|| a.id.cmp(&b.id))
        });
        return out;
    }
    if !session.source.log.sample_commands.is_empty() {
        let duration = session.source.log.frames.last().map_or(0., |frame| frame.seconds);
        let mut out: Vec<_> = session.source.log.sample_commands.iter().enumerate().map(|(index, command)| Trigger {
            id: format!("sample-command-{index}"), seconds: command.seconds.min(duration),
            kind: "sample".into(), source: command.path.clone(), gain: 0.65,
            action: Some(command.action.clone()), mode: Some(command.mode),
        }).collect();
        out.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then_with(|| a.id.cmp(&b.id)));
        return out;
    }
    let mut out = Vec::new();
    let mut active = BTreeSet::new();
    for frame in &session.source.log.frames {
        let next: BTreeSet<String> = frame.samples.iter().cloned().collect();
        for source in next.difference(&active) {
            out.push(Trigger {
                id: format!("sample-{}-{}", out.len(), frame.seconds),
                seconds: frame.seconds,
                kind: "sample".into(),
                source: source.clone(),
                gain: 0.65,
                action: None,
                mode: None,
            });
        }
        active = next;
    }
    out
}

#[tauri::command]
pub async fn mix_editor_open() -> Result<Option<OpenedSession>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open SkyDek Mix Editor session")
            .add_filter("SkyDek Mix Session or Recovery Backup", &["dek", "backup"])
            .pick_file()
        else {
            return Ok(None);
        };
        let path = if path.extension().and_then(|ext| ext.to_str()) == Some("backup") {
            restore_backup(&path)?
        } else { path };
        let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read session: {e}"))?;
        parse(&text)?;
        Ok(Some(OpenedSession {
            path: path.to_string_lossy().into_owned(),
            text,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn restore_backup(backup: &Path) -> Result<PathBuf, String> {
    let original = backup.with_extension("");
    if original.exists() { return Err("The original session still exists. Choose its .dek file instead of the recovery backup.".into()); }
    let text = std::fs::read_to_string(backup).map_err(|e| format!("Could not read recovery backup: {e}"))?;
    parse(&text)?;
    std::fs::copy(backup, &original).map_err(|e| format!("Could not restore recovery backup: {e}"))?;
    Ok(original)
}

#[tauri::command]
pub async fn mix_editor_import_legacy() -> Result<Option<OpenedSession>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import an older SkyDek mix log")
            .add_filter("SkyDek Mix Log", &["json", "md"])
            .pick_file()
        else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("Could not read the selected mix log: {e}"))?;
        if text.len() > 128 * 1024 * 1024 {
            return Err("The selected mix log is too large".into());
        }
        Ok(Some(OpenedSession {
            path: path.to_string_lossy().into_owned(),
            text,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("The selected path has no parent folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp).map_err(|e| format!("Could not create temporary session: {e}"))?;
        file.write_all(bytes).map_err(|e| format!("Could not write temporary session: {e}"))?;
        file.sync_all().map_err(|e| format!("Could not sync temporary session: {e}"))?;
    }
    let backup = path.with_extension("dek.backup");
    if path.exists() {
        std::fs::copy(path, &backup)
            .map_err(|e| format!("Could not preserve previous session: {e}"))?;
        std::fs::OpenOptions::new().write(true).open(&backup).and_then(|file| file.sync_all())
            .map_err(|e| format!("Could not sync recovery backup: {e}"))?;
        std::fs::remove_file(path)
            .map_err(|e| format!("Could not replace previous session: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&temp, path) {
        if backup.exists() { let _ = std::fs::copy(&backup, path); }
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Could not finalize session: {e}"));
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

fn atomic_replace_render(temp: &Path, output: &Path) -> Result<(), String> {
    let backup = output.with_extension(format!(
        "{}.backup",
        output
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("audio")
    ));
    let _ = std::fs::remove_file(&backup);
    if output.exists() {
        std::fs::rename(output, &backup)
            .map_err(|error| format!("Could not preserve the previous export: {error}"))?;
    }
    if let Err(error) = std::fs::rename(temp, output) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, output);
        }
        let _ = std::fs::remove_file(temp);
        return Err(format!("Could not finalize export: {error}"));
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

#[tauri::command]
pub async fn mix_editor_save(text: String, path: Option<String>) -> Result<Option<String>, String> {
    let mut session = parse(&text)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = match path {
            Some(path) => PathBuf::from(path),
            None => match rfd::FileDialog::new()
                .add_filter("SkyDek Mix Session", &["dek"])
                .set_file_name(format!("{}.dek", safe_name(&session.name)))
                .save_file()
            {
                Some(path) => path,
                None => return Ok(None),
            },
        };
        let path = path.with_extension("dek");
        annotate_relative_sources(&mut session, &path);
        let bytes = serde_json::to_vec_pretty(&session).map_err(|e| e.to_string())?;
        atomic_write(&path, &bytes)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn safe_name(name: &str) -> String {
    let value: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let value = value.trim().trim_end_matches('.');
    if value.is_empty() {
        "SkyDek Mix".into()
    } else {
        value.chars().take(120).collect()
    }
}

#[tauri::command]
pub async fn mix_editor_sources(
    text: String,
    session_path: Option<String>,
) -> Result<Vec<Resolution>, String> {
    let session = parse(&text)?;
    let session_path = session_path.map(PathBuf::from);
    let sample_sources: HashSet<String> = trigger_events(&session).into_iter().map(|event| event.source).collect();
    let mut resolutions: Vec<Resolution> = media_paths(&session)
        .into_iter()
        .map(|original| {
            let resolved = resolve_media(&session, &original, session_path.as_deref())
                .map(|p| p.to_string_lossy().into_owned());
            let kind = if session.source.log.mic_file.as_deref() == Some(original.as_str()) {
                "mic"
            } else if sample_sources.contains(&original) {
                "sample"
            } else {
                "deck"
            };
            Resolution {
                original,
                resolved,
                kind: kind.into(),
            }
        })
        .collect();
    let recording = session.source.audio_file.trim();
    if !recording.is_empty() && !resolutions.iter().any(|item| item.original == recording) {
        resolutions.push(Resolution {
            original: recording.to_string(),
            resolved: resolve_media(&session, recording, session_path.as_deref())
                .map(|path| path.to_string_lossy().into_owned()),
            kind: "recording".into(),
        });
    }
    Ok(resolutions)
}

#[tauri::command]
pub async fn mix_editor_relink(original: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(rfd::FileDialog::new()
            .set_title(format!(
                "Relink {}",
                Path::new(&original)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .add_filter("Audio", &["mp3", "wav"])
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn apply_state(
    mixer: &mut Mixer,
    session: &Session,
    frame_index: usize,
    active_edits: &HashMap<&str, f64>,
    tracks: &HashMap<String, Arc<Track>>,
    loaded: &mut [Option<String>; 2],
    effects: &mut [u8; 2],
) {
    let frame = &session.source.log.frames[frame_index];
    let mut cross = frame.crossfader as f64;
    let mut master = frame.master as f64;
    let mut sampler_volume = frame.sampler_volume as f64;
    let mut volume = [frame.decks[0].volume as f64, frame.decks[1].volume as f64];
    let mut rate = [frame.decks[0].rate, frame.decks[1].rate];
    let mut filter = [frame.decks[0].filter as f64, frame.decks[1].filter as f64];
    let mut wet = [frame.decks[0].wet as f64, frame.decks[1].wet as f64];
    let mut desired_effect = [frame.decks[0].effect, frame.decks[1].effect];
    if let Some(&value) = active_edits.get("crossfader") { cross = value; }
    if let Some(&value) = active_edits.get("master") { master = value; }
    if let Some(&value) = active_edits.get("sampler_volume") { sampler_volume = value; }
    for i in 0..2 {
        for (name, target) in [("volume", &mut volume[i]), ("rate", &mut rate[i]), ("filter", &mut filter[i]), ("wet", &mut wet[i])] {
            if let Some(&value) = active_edits.get(format!("{name}_{i}").as_str()) { *target = value; }
        }
        if let Some(&value) = active_edits.get(format!("effect_{i}").as_str()) { desired_effect[i] = value.round() as u8; }
    }
    mixer.apply(Cmd::Cross(cross as f32));
    mixer.apply(Cmd::Master(master as f32));
    mixer.apply(Cmd::SamplerVolume(sampler_volume as f32));
    if session.source.log.mic_file.is_some() {
        if let Some(mic) = &frame.mic {
            mixer.apply(Cmd::MicVolume(mic.volume));
            mixer.apply(Cmd::MicMute(mic.mute));
            mixer.apply(Cmd::MicDucking(mic.ducking));
        }
    }
    for i in 0..2 {
        let deck = &frame.decks[i];
        let path_changed = loaded[i] != deck.path;
        if path_changed {
            if let Some(path) = &deck.path {
                if let Some(track) = tracks.get(path) {
                    mixer.apply(Cmd::Load(i, track.clone(), [None; 8], true));
                }
            } else {
                mixer.apply(Cmd::Unload(i));
            }
            loaded[i] = deck.path.clone();
            effects[i] = 0;
        }
        let observed = mixer.view().decks[i];
        if path_changed || (observed.position - deck.position).abs() > 0.035 {
            mixer.apply(Cmd::Seek(i, deck.position));
            effects[i] = 0;
        }
        mixer.apply(Cmd::Volume(i, volume[i] as f32));
        mixer.apply(Cmd::Rate(i, rate[i]));
        mixer.apply(Cmd::Filter(i, filter[i] as f32));
        mixer.apply(Cmd::Wet(i, wet[i] as f32));
        if mixer.view().decks[i].playing != deck.playing {
            mixer.apply(Cmd::Toggle(i));
            effects[i] = 0;
        }
        if effects[i] != desired_effect[i] {
            if effects[i] != 0 {
                mixer.apply(Cmd::FxGate(i, effects[i], false));
            }
            if desired_effect[i] != 0 {
                mixer.apply(Cmd::FxGate(i, desired_effect[i], true));
            }
            effects[i] = desired_effect[i];
        }
    }
}

/// Streams the synchronized raw microphone stem; only two source frames stay in memory.
struct MicStemReader {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    rate: u32,
    index: u64,
    current: [f32; 2],
    next: [f32; 2],
}
impl MicStemReader {
    fn read_frame(reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>) -> Result<[f32; 2], String> {
        let mut samples = reader.samples::<f32>();
        let left = samples.next().transpose().map_err(|error| format!("Microphone stem read failed: {error}"))?.unwrap_or(0.);
        let right = samples.next().transpose().map_err(|error| format!("Microphone stem read failed: {error}"))?.unwrap_or(0.);
        Ok([left, right])
    }
    fn open(path: &Path, at: f64, end: f64) -> Result<Self, String> {
        let mut reader = hound::WavReader::open(path)
            .map_err(|error| format!("Could not open microphone stem {}: {error}", path.display()))?;
        let spec = reader.spec();
        if spec.channels != 2 || spec.bits_per_sample != 32 || spec.sample_format != hound::SampleFormat::Float {
            return Err("Microphone stem must be stereo 32-bit float WAV".into());
        }
        if (reader.duration() as f64 + 1.) / (spec.sample_rate as f64) < end {
            return Err("Microphone stem is shorter than the performance log. Relink the complete stem.".into());
        }
        let index = (at * spec.sample_rate as f64).floor() as u64;
        reader.seek(index.min(u32::MAX as u64) as u32)
            .map_err(|error| format!("Could not seek microphone stem: {error}"))?;
        let current = Self::read_frame(&mut reader)?;
        let next = Self::read_frame(&mut reader)?;
        Ok(Self { reader, rate: spec.sample_rate, index, current, next })
    }
    fn at(&mut self, seconds: f64) -> Result<[f32; 2], String> {
        let position = seconds * self.rate as f64;
        let floor = position.floor() as u64;
        while self.index < floor {
            self.current = self.next;
            self.next = Self::read_frame(&mut self.reader)?;
            self.index += 1;
        }
        let mix = (position - self.index as f64) as f32;
        Ok([self.current[0] + (self.next[0] - self.current[0]) * mix,
         self.current[1] + (self.next[1] - self.current[1]) * mix])
    }
}

fn render(
    session: Session,
    session_path: Option<PathBuf>,
    output: PathBuf,
    progress: Option<&RenderStatus>,
    region: Option<(f64, f64)>,
) -> Result<(), String> {
    validate(&session)?;
    let triggers = trigger_events(&session);
    let unique_samples: BTreeSet<_> = triggers.iter().map(|event| &event.source).collect();
    if unique_samples.len() > crate::sampler::SAMPLER_SLOTS {
        return Err(format!("This mix uses {} distinct sample files; this renderer supports at most {}. Remove or consolidate samples before export.", unique_samples.len(), crate::sampler::SAMPLER_SLOTS));
    }
    let mut resolved = HashMap::new();
    for original in media_paths(&session) {
        let path = resolve_media(&session, &original, session_path.as_deref())
            .ok_or_else(|| format!("Missing source: {original}"))?;
        resolved.insert(original, path);
    }
    let duration = session.source.log.frames.last().unwrap().seconds;
    let parent = output
        .parent()
        .ok_or("The export path has no parent folder")?;
    let ext = output
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("wav")
        .to_ascii_lowercase();
    if ext != "wav" && ext != "mp3" {
        return Err("Choose a WAV or MP3 export path".into());
    }
    let temp = parent.join(format!(
        ".{}.part.{ext}",
        output.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let mut mixer = Mixer::new(RENDER_RATE);
    mixer.apply(Cmd::Normalize(true));
    let mut sample_slots = HashMap::new();
    let mut sample_bytes = 0usize;
    for original in unique_samples {
        let slot = sample_slots.len();
        let path = resolved.get(original).ok_or("Sample source is missing")?;
        let (track, _) = audio::decode_sample(&path.to_string_lossy())
            .map_err(|e| format!("Could not decode sample {}: {e}", path.display()))?;
        sample_bytes = sample_bytes.saturating_add(track.samples.len() * std::mem::size_of::<[f32; 2]>());
        if sample_bytes > 256 * 1024 * 1024 {
            return Err("Samples in this mix exceed the 256 MiB render memory budget. Remove or shorten some samples.".into());
        }
        mixer.apply(Cmd::Sampler(slot, track));
        sample_slots.insert(original.clone(), slot);
    }
    let mut tracks = HashMap::new();
    let _partial_cleanup = RemoveOnDrop(temp.clone());
    let mut writer = RecordingWriter::create(&temp, RENDER_RATE)?;
    let mut frame_index = 0usize;
    let mut ordered_edits: Vec<&Edit> = session.edits.iter().collect();
    ordered_edits.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then_with(|| a.id.cmp(&b.id)));
    let mut active_edits: HashMap<&str, f64> = HashMap::new();
    let mut edit_index = 0usize;
    let mut mic_commands: Vec<_> = session.source.log.mic_commands.iter().collect();
    mic_commands.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut mic_index = 0usize;
    let mut active_mic = None;
    let mut loaded = [None, None];
    let mut effects = [0, 0];
    let mut applied_frame = usize::MAX;
    let (start, end) = region.unwrap_or((0., duration));
    if !start.is_finite() || !end.is_finite() || start < 0. || end > duration || end < start { return Err("Invalid preview range".into()); }
    let output_start = (start * RENDER_RATE as f64).round() as u64;
    let warm_start = if region.is_some() { (start - 5.).max(0.) } else { 0. };
    let warm_frame = (warm_start * RENDER_RATE as f64).round() as u64;
    let end_frame = (end * RENDER_RATE as f64).ceil() as u64;
    let mic_queue = session.source.log.mic_file.as_ref().map(|_| Arc::new(crossbeam_queue::ArrayQueue::new(16)));
    let mut mic_reader = if let Some(path) = &session.source.log.mic_file {
        let resolved = resolved.get(path).ok_or("Microphone stem is missing")?;
        Some(MicStemReader::open(resolved, warm_frame as f64 / RENDER_RATE as f64, end)?)
    } else { None };
    if let Some(queue) = &mic_queue { mixer.set_mic_queue(queue.clone()); }
    let mut trigger_index = triggers.partition_point(|event| event.seconds < warm_start);
    let mut buffer = Vec::with_capacity(8192);
    let frames = end_frame.saturating_sub(warm_frame);
    if let Some(progress) = progress {
        progress.total.store(frames, Ordering::Relaxed);
        progress.done.store(0, Ordering::Relaxed);
    }
    for n in 0..frames {
        if n % 4096 == 0 {
            if let Some(progress) = progress {
                progress.done.store(n, Ordering::Relaxed);
                if progress.cancel.load(Ordering::Relaxed) {
                    drop(writer);
                    let _ = std::fs::remove_file(&temp);
                    return Err("Render cancelled".into());
                }
            }
        }
        let absolute_frame = warm_frame + n;
        let time = absolute_frame as f64 / RENDER_RATE as f64;
        while frame_index + 1 < session.source.log.frames.len()
            && session.source.log.frames[frame_index + 1].seconds <= time
        {
            frame_index += 1;
        }
        let before_edits = edit_index;
        while edit_index < ordered_edits.len() && ordered_edits[edit_index].seconds <= time {
            let edit = ordered_edits[edit_index];
            active_edits.insert(edit.lane.as_str(), edit.value);
            edit_index += 1;
        }
        let before_mic = mic_index;
        while mic_index < mic_commands.len() && mic_commands[mic_index].seconds <= time {
            active_mic = Some(&mic_commands[mic_index].state);
            mic_index += 1;
        }
        if applied_frame != frame_index || before_edits != edit_index || before_mic != mic_index {
            let wanted: HashSet<_> = session.source.log.frames[frame_index].decks.iter()
                .filter_map(|deck| deck.path.as_ref()).collect();
            // Release the previous deck buffer before decoding a replacement.
            for (i, loaded_path) in loaded.iter_mut().enumerate() {
                if loaded_path.as_ref() != session.source.log.frames[frame_index].decks[i].path.as_ref() {
                    mixer.apply(Cmd::Unload(i));
                    *loaded_path = None;
                }
            }
            tracks.retain(|path, _| wanted.contains(path));
            for path in &wanted {
                if !tracks.contains_key(*path) {
                    let source = resolved.get(*path).ok_or("Deck source is missing")?;
                    let (track, _) = audio::decode(&source.to_string_lossy())
                        .map_err(|e| format!("Could not decode {}: {e}", source.display()))?;
                    tracks.insert((*path).clone(), track);
                }
            }
            apply_state(
                &mut mixer,
                &session,
                frame_index,
                &active_edits,
                &tracks,
                &mut loaded,
                &mut effects,
            );
            if let Some(mic) = active_mic {
                mixer.apply(Cmd::MicVolume(mic.volume));
                mixer.apply(Cmd::MicMute(mic.mute));
                mixer.apply(Cmd::MicDucking(mic.ducking));
            }
            if n == 0 && warm_frame > 0 {
                let observed_frame = &session.source.log.frames[frame_index];
                let elapsed = (time - observed_frame.seconds).max(0.);
                for i in 0..2 {
                    if observed_frame.decks[i].playing && observed_frame.decks[i].path.is_some() {
                        mixer.apply(Cmd::Seek(i, observed_frame.decks[i].position + elapsed * observed_frame.decks[i].rate));
                    }
                }
            }
            applied_frame = frame_index;
        }
        while trigger_index < triggers.len()
            && triggers[trigger_index].seconds <= time + 0.5 / RENDER_RATE as f64
        {
            if let Some(slot) = sample_slots.get(&triggers[trigger_index].source) {
                let event = &triggers[trigger_index];
                if event.action.as_deref() == Some("release") {
                    mixer.apply(Cmd::StopSample(*slot));
                } else {
                    mixer.apply(Cmd::SamplerGain(*slot, event.gain));
                    mixer.apply(Cmd::SamplePress(*slot, event.mode.unwrap_or(crate::sampler::PlayMode::Restart)));
                }
            }
            trigger_index += 1;
        }
        if let (Some(reader), Some(queue)) = (&mut mic_reader, &mic_queue) {
            let _ = queue.push(reader.at(time)?);
        }
        let sample = mixer.frame();
        if absolute_frame >= output_start { buffer.extend_from_slice(&sample); }
        if buffer.len() >= 8192 {
            writer.write(&buffer)?;
            buffer.clear();
        }
    }
    if !buffer.is_empty() {
        writer.write(&buffer)?;
    }
    if let Some(progress) = progress {
        progress.done.store(frames, Ordering::Relaxed);
    }
    if let Err(e) = writer.finalize() {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    atomic_replace_render(&temp, &output)
}

#[tauri::command]
pub async fn mix_editor_render(
    status: tauri::State<'_, Arc<RenderStatus>>,
    text: String,
    session_path: Option<String>,
    format: String,
) -> Result<Option<String>, String> {
    let session = parse(&text)?;
    let format = if format.eq_ignore_ascii_case("mp3") {
        "mp3"
    } else {
        "wav"
    };
    let Some(output) = rfd::AsyncFileDialog::new()
        .add_filter(format.to_uppercase(), &[format])
        .set_file_name(format!("{}.{}", safe_name(&session.name), format))
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let output = output.path().with_extension(format);
    let result = output.clone();
    if status.running.swap(true, Ordering::SeqCst) {
        return Err("Another preview or export is already running".into());
    }
    status.cancel.store(false, Ordering::Relaxed);
    status.done.store(0, Ordering::Relaxed);
    status.total.store(0, Ordering::Relaxed);
    let progress = status.inner().clone();
    let completed = tauri::async_runtime::spawn_blocking(move || {
        render(
            session,
            session_path.map(PathBuf::from),
            output,
            Some(&progress),
            None,
        )
    })
    .await
    .map_err(|e| e.to_string());
    status.running.store(false, Ordering::SeqCst);
    completed??;
    Ok(Some(result.to_string_lossy().into_owned()))
}

#[derive(Serialize)]
pub struct PreviewResult { pub path: String, pub start: f64, pub end: f64 }

#[tauri::command]
pub async fn mix_editor_preview(
    status: tauri::State<'_, Arc<RenderStatus>>,
    text: String,
    session_path: Option<String>,
    position: f64,
) -> Result<PreviewResult, String> {
    let session = parse(&text)?;
    if !position.is_finite() { return Err("Invalid preview position".into()); }
    let duration = session.source.log.frames.last().unwrap().seconds;
    let start = ((position.min(duration) - 2.).max(0.) * RENDER_RATE as f64).round() / RENDER_RATE as f64;
    let end = (start + 30.).min(duration);
    static PREVIEW_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let output = std::env::temp_dir().join(format!(
        "skydek-mix-editor-preview-{}-{}.wav",
        std::process::id(), PREVIEW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = output.clone();
    if status.running.swap(true, Ordering::SeqCst) {
        return Err("Another preview or export is already running".into());
    }
    status.cancel.store(false, Ordering::Relaxed);
    status.done.store(0, Ordering::Relaxed);
    status.total.store(0, Ordering::Relaxed);
    let progress = status.inner().clone();
    let completed = tauri::async_runtime::spawn_blocking(move || {
        render(
            session,
            session_path.map(PathBuf::from),
            output,
            Some(&progress),
            Some((start, end)),
        )
    })
    .await
    .map_err(|e| e.to_string());
    status.running.store(false, Ordering::SeqCst);
    completed??;
    Ok(PreviewResult { path: result.to_string_lossy().into_owned(), start, end })
}

#[tauri::command]
pub fn mix_editor_discard_preview(path: String) -> Result<(), String> {
    let target = PathBuf::from(path);
    if target.parent() != Some(std::env::temp_dir().as_path()) ||
        !target.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("skydek-mix-editor-preview-") && name.ends_with(".wav")) {
        return Err("Invalid preview file".into());
    }
    std::thread::spawn(move || {
        for _ in 0..20 {
            if !target.exists() || std::fs::remove_file(&target).is_ok() { break; }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session {
            format: FORMAT.into(),
            version: 1,
            name: "Test".into(),
            created_at: "now".into(),
            modified_at: "now".into(),
            source: Source {
                kind: "legacy-log".into(),
                audio_file: "mix.wav".into(),
                log: MixLog {
                    version: 1,
                    capture_revision: 0,
                    audio_file: "mix.wav".into(),
                    frames: vec![crate::mixlog::Frame {
                        seconds: 0.,
                        decks: std::array::from_fn(|_| crate::mixlog::DeckFrame {
                            path: None,
                            position: 0.,
                            playing: false,
                            rate: 1.,
                            volume: 0.8,
                            filter: 0.,
                            effect: 0,
                            wet: 0.35,
                            peak: 0.,
                            cue: 0.,
                        }),
                        crossfader: 0.,
                        master: 0.8,
                        sampler_volume: 0.8,
                        samples: vec![],
                        mic: None,
                    }],
                    events: vec![],
                    sample_commands: vec![],
                    effect_commands: vec![],
                    mic_file: None,
                    mic_commands: vec![],
                },
                extra: HashMap::new(),
            },
            edits: vec![],
            triggers: Some(vec![]),
            relinks: HashMap::new(),
            extra: HashMap::new(),
        }
    }
    #[test]
    fn validates_session_and_rejects_bad_edit() {
        let mut s = session();
        assert!(validate(&s).is_ok());
        s.edits.push(Edit {
            id: "x".into(),
            seconds: 0.,
            lane: "crossfader".into(),
            value: 2.,
        });
        assert!(validate(&s).is_err());
    }
    #[test]
    fn validates_supported_effect_selection_automation() {
        let mut s = session();
        s.edits.push(Edit {
            id: "fx".into(),
            seconds: 0.,
            lane: "effect_1".into(),
            value: 4.,
        });
        assert!(validate(&s).is_ok());
        s.edits[0].value = 6.;
        assert!(validate(&s).is_err());
    }
    #[test]
    fn rejects_duplicate_ids_and_unsafe_relative_reference_metadata() {
        let mut s = session();
        s.edits = vec![
            Edit {
                id: "same".into(),
                seconds: 0.,
                lane: "master".into(),
                value: 0.5,
            },
            Edit {
                id: "same".into(),
                seconds: 0.,
                lane: "master".into(),
                value: 0.6,
            },
        ];
        assert!(validate(&s).unwrap_err().contains("Invalid automation"));
        let mut s = session();
        s.extra.insert(
            "source_refs".into(),
            serde_json::json!([{
                "id":"media-1", "original_path":"song.wav", "relative_path":"../secret.wav",
                "display_name":"song.wav", "media_type":"wav", "role":"deck",
                "duration":null, "fingerprint":null
            }]),
        );
        assert!(validate(&s).unwrap_err().contains("unsafe relative path"));
    }
    #[test]
    fn relative_paths_cannot_escape() {
        assert!(safe_relative(Path::new("audio/song.wav")));
        assert!(!safe_relative(Path::new("../secret.wav")));
    }
    #[test]
    fn resolves_explicit_relative_media_reference_next_to_session() {
        let root =
            std::env::temp_dir().join(format!("skydek-editor-relative-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("media")).unwrap();
        let media = root.join("media/song.wav");
        std::fs::write(&media, b"fixture").unwrap();
        let mut s = session();
        s.extra.insert(
            "source_refs".into(),
            serde_json::json!([{
                "id":"media-1", "original_path":"C:\\moved\\song.wav",
                "relative_path":"media/song.wav", "display_name":"song.wav",
                "media_type":"wav", "role":"deck", "duration":null, "fingerprint":null
            }]),
        );
        let resolved =
            resolve_media(&s, "C:\\moved\\song.wav", Some(&root.join("mix.dek"))).unwrap();
        assert_eq!(resolved, media);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn render_replacement_is_recoverable_and_removes_backup() {
        let root =
            std::env::temp_dir().join(format!("skydek-editor-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join("mix.wav");
        let temp = root.join(".mix.part.wav");
        std::fs::write(&output, b"previous").unwrap();
        std::fs::write(&temp, b"replacement").unwrap();
        atomic_replace_render(&temp, &output).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"replacement");
        assert!(!root.join("mix.wav.backup").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn canonical_round_trip() {
        let s = session();
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&text).unwrap().format, FORMAT);
    }
    #[test]
    fn explicit_empty_triggers_remove_last_sample_and_unused_media() {
        let mut s = session();
        s.source.log.frames[0].samples = vec!["C:\\missing\\old-sample.wav".into()];
        s.triggers = None;
        assert_eq!(trigger_events(&s).len(), 1);
        s.triggers = Some(vec![]);
        assert!(trigger_events(&s).is_empty());
        assert!(media_paths(&s).is_empty());
        assert!(validate(&s).is_ok());
    }
    #[test]
    fn new_log_uses_precise_sample_commands_between_snapshots() {
        let mut s = session();
        s.triggers = None;
        s.source.log.frames[0].seconds = 1.;
        s.source.log.sample_commands = vec![
            crate::mixlog::SampleCommand { seconds: 0.2, path: "horn.wav".into(), action: "press".into(), mode: crate::sampler::PlayMode::Restart },
            crate::mixlog::SampleCommand { seconds: 0.24, path: "horn.wav".into(), action: "press".into(), mode: crate::sampler::PlayMode::Restart },
        ];
        let triggers = trigger_events(&s);
        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[1].seconds, 0.24);
        assert!(validate(&s).is_ok());
    }
    #[test]
    fn sample_limit_is_explicit_before_render_writes_output() {
        let mut s = session();
        s.triggers = Some((0..49).map(|index| Trigger {
            id: format!("sample-{index}"), seconds: 0., kind: "sample".into(),
            source: format!("C:\\samples\\{index}.wav"), gain: 0.65,
            action: None, mode: None,
        }).collect());
        let output = std::env::temp_dir().join(format!("skydek-sample-limit-{}.wav", std::process::id()));
        let error = render(s, None, output.clone(), None, None).unwrap_err();
        assert!(error.contains("at most 48"));
        assert!(!output.exists());
    }
    #[test]
    fn atomic_session_save_recovers_valid_previous_revision() {
        let root = std::env::temp_dir().join(format!("skydek-editor-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("mix.dek");
        let previous = serde_json::to_vec(&session()).unwrap();
        std::fs::write(&path, &previous).unwrap();
        let mut next = session(); next.name = "Next".into();
        atomic_write(&path, &serde_json::to_vec(&next).unwrap()).unwrap();
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()).unwrap().name, "Next");
        assert!(!path.with_extension("dek.backup").exists());
        let backup = path.with_extension("dek.backup");
        std::fs::copy(&path, &backup).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(restore_backup(&backup).unwrap(), path);
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()).unwrap().name, "Next");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn renders_audible_wav_from_original_deck_media() {
        let root =
            std::env::temp_dir().join(format!("skydek-editor-render-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.wav");
        let mut wav = hound::WavWriter::create(
            &source,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for n in 0..48_000 {
            let x = (n as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.2;
            wav.write_sample(x).unwrap();
            wav.write_sample(x).unwrap();
        }
        wav.finalize().unwrap();
        let deck = |path: Option<String>, position: f64, playing: bool| crate::mixlog::DeckFrame {
            path,
            position,
            playing,
            rate: 1.,
            volume: 0.8,
            filter: 0.,
            effect: 0,
            wet: 0.35,
            peak: 0.,
            cue: 0.,
        };
        let mut s = session();
        s.source.log.frames = vec![
            crate::mixlog::Frame {
                seconds: 0.,
                decks: [
                    deck(Some(source.to_string_lossy().into_owned()), 0., true),
                    deck(None, 0., false),
                ],
                crossfader: -1.,
                master: 0.8,
                sampler_volume: 0.8,
                samples: vec![],
                    mic: None,
            },
            crate::mixlog::Frame {
                seconds: 0.25,
                decks: [
                    deck(Some(source.to_string_lossy().into_owned()), 0.25, true),
                    deck(None, 0., false),
                ],
                crossfader: -1.,
                master: 0.8,
                sampler_volume: 0.8,
                samples: vec![],
                    mic: None,
            },
        ];
        let output = root.join("render.wav");
        render(s, None, output.clone(), None, None).unwrap();
        let mut reader = hound::WavReader::open(output).unwrap();
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert!(reader
            .samples::<f32>()
            .take(20_000)
            .any(|x| x.unwrap().abs() > 0.01));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn deck_unload_creates_silence_and_region_matches_full_render() {
        let root = std::env::temp_dir().join(format!("skydek-editor-unload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        let mut writer = hound::WavWriter::create(&source, hound::WavSpec {
            channels: 2, sample_rate: RENDER_RATE, bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        }).unwrap();
        for _ in 0..RENDER_RATE { writer.write_sample(0.5f32).unwrap(); writer.write_sample(0.5f32).unwrap(); }
        writer.finalize().unwrap();
        let mut s = session();
        let mut first = s.source.log.frames[0].clone();
        first.decks[0].path = Some(source.to_string_lossy().into_owned());
        first.decks[0].playing = true;
        first.crossfader = -1.;
        let mut second = first.clone(); second.seconds = 0.25;
        second.decks[0].position = 0.25;
        let mut third = second.clone(); third.seconds = 0.5;
        third.decks[0].path = None; third.decks[0].playing = false;
        let mut fourth = third.clone(); fourth.seconds = 0.75;
        s.source.log.frames = vec![first, second, third, fourth];
        let full = root.join("full.wav");
        let region = root.join("region.wav");
        render(s.clone(), None, full.clone(), None, None).unwrap();
        render(s.clone(), None, region.clone(), None, Some((0.25, 0.75))).unwrap();
        let full_pcm: Vec<f32> = hound::WavReader::open(full).unwrap().samples::<f32>().map(Result::unwrap).collect();
        let region_pcm: Vec<f32> = hound::WavReader::open(&region).unwrap().samples::<f32>().map(Result::unwrap).collect();
        let offset = (0.25 * RENDER_RATE as f64) as usize * 2;
        assert_eq!(region_pcm.len(), (0.5 * RENDER_RATE as f64) as usize * 2);
        assert!(full_pcm[0..(0.25 * RENDER_RATE as f64) as usize * 2].iter().any(|x| *x > 0.1));
        assert!(full_pcm[(0.55 * RENDER_RATE as f64) as usize * 2..].iter().all(|x| x.abs() < 0.001));
        for (a, b) in region_pcm.iter().zip(&full_pcm[offset..]) { assert!((a - b).abs() < 0.0001); }
        // A cancelled rebuild must leave the last good export intact and clean its partial file.
        let previous = std::fs::read(&region).unwrap();
        let progress = RenderStatus::default();
        progress.cancel.store(true, Ordering::Relaxed);
        let cancelled = render(s, None, region.clone(), Some(&progress), Some((0.25, 0.75)));
        assert!(cancelled.unwrap_err().contains("cancelled"));
        assert_eq!(std::fs::read(&region).unwrap(), previous);
        assert!(!root.join(".region.part.wav").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn microphone_stem_resamples_and_contributes_to_render() {
        let root = std::env::temp_dir().join(format!("skydek-editor-mic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let stem = root.join("mic.wav");
        let mut writer = hound::WavWriter::create(&stem, hound::WavSpec {
            channels: 2, sample_rate: 44_100, bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        }).unwrap();
        for _ in 0..44_100 { writer.write_sample(0.2f32).unwrap(); writer.write_sample(-0.2f32).unwrap(); }
        writer.finalize().unwrap();
        let mut reader = MicStemReader::open(&stem, 0.1, 0.25).unwrap();
        assert!((reader.at(0.1).unwrap()[0] - 0.2).abs() < 0.001);
        assert!((reader.at(0.2).unwrap()[1] + 0.2).abs() < 0.001);
        let mut s = session();
        s.source.log.mic_file = Some(stem.to_string_lossy().into_owned());
        s.source.log.frames[0].mic = Some(crate::mixlog::MicState { volume: 1., mute: false, ducking: false });
        s.source.log.frames[0].master = 1.;
        let mut last = s.source.log.frames[0].clone(); last.seconds = 0.25;
        s.source.log.frames.push(last);
        let output = root.join("mix.wav");
        render(s, None, output.clone(), None, None).unwrap();
        let samples: Vec<f32> = hound::WavReader::open(output).unwrap().samples::<f32>().map(Result::unwrap).take(1000).collect();
        assert!(samples.iter().step_by(2).all(|x| (*x - 0.2).abs() < 0.01));
        assert!(samples.iter().skip(1).step_by(2).all(|x| (*x + 0.2).abs() < 0.01));
        std::fs::remove_dir_all(root).unwrap();
    }
}
