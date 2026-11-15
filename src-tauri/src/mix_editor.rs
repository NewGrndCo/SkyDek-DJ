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
    pub triggers: Vec<Trigger>,
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
        || session.source.log.frames.is_empty()
        || session.source.log.frames.len() > 144_000
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
            if original.is_empty() || !matches!(role, "deck" | "sample" | "recording") {
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
        paths.extend(frame.samples.iter().cloned());
    }
    paths.extend(trigger_events(session).into_iter().map(|t| t.source));
    paths
}

fn trigger_events(session: &Session) -> Vec<Trigger> {
    if !session.triggers.is_empty() {
        let mut out = session.triggers.clone();
        out.sort_by(|a, b| {
            a.seconds
                .total_cmp(&b.seconds)
                .then_with(|| a.id.cmp(&b.id))
        });
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
            .add_filter("SkyDek Mix Session", &["dek"])
            .pick_file()
        else {
            return Ok(None);
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("Could not read session: {e}"))?;
        Ok(Some(OpenedSession {
            path: path.to_string_lossy().into_owned(),
            text,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
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
    std::fs::write(&temp, bytes).map_err(|e| format!("Could not write temporary session: {e}"))?;
    let backup = path.with_extension("dek.backup");
    if path.exists() {
        std::fs::copy(path, &backup)
            .map_err(|e| format!("Could not preserve previous session: {e}"))?;
        std::fs::remove_file(path)
            .map_err(|e| format!("Could not replace previous session: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&temp, path) {
        if backup.exists() {
            let _ = std::fs::copy(&backup, path);
        }
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
    let mut resolutions: Vec<Resolution> = media_paths(&session)
        .into_iter()
        .map(|original| {
            let resolved = resolve_media(&session, &original, session_path.as_deref())
                .map(|p| p.to_string_lossy().into_owned());
            let kind = if session
                .source
                .log
                .frames
                .iter()
                .any(|f| f.samples.contains(&original))
                || trigger_events(&session)
                    .iter()
                    .any(|event| event.source == original)
            {
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
    time: f64,
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
    let mut edits: Vec<&Edit> = session.edits.iter().filter(|e| e.seconds <= time).collect();
    edits.sort_by(|a, b| {
        a.seconds
            .total_cmp(&b.seconds)
            .then_with(|| a.id.cmp(&b.id))
    });
    for e in edits {
        match e.lane.as_str() {
            "crossfader" => cross = e.value,
            "master" => master = e.value,
            "sampler_volume" => sampler_volume = e.value,
            "volume_0" => volume[0] = e.value,
            "volume_1" => volume[1] = e.value,
            "rate_0" => rate[0] = e.value,
            "rate_1" => rate[1] = e.value,
            "filter_0" => filter[0] = e.value,
            "filter_1" => filter[1] = e.value,
            "wet_0" => wet[0] = e.value,
            "wet_1" => wet[1] = e.value,
            "effect_0" => desired_effect[0] = e.value.round() as u8,
            "effect_1" => desired_effect[1] = e.value.round() as u8,
            _ => {}
        }
    }
    mixer.apply(Cmd::Cross(cross as f32));
    mixer.apply(Cmd::Master(master as f32));
    mixer.apply(Cmd::SamplerVolume(sampler_volume as f32));
    for i in 0..2 {
        let deck = &frame.decks[i];
        let path_changed = loaded[i] != deck.path;
        if path_changed {
            if let Some(path) = &deck.path {
                if let Some(track) = tracks.get(path) {
                    mixer.apply(Cmd::Load(i, track.clone(), [None; 8], true));
                }
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

fn render(
    session: Session,
    session_path: Option<PathBuf>,
    output: PathBuf,
    progress: Option<&RenderStatus>,
) -> Result<(), String> {
    validate(&session)?;
    let mut tracks = HashMap::new();
    for original in media_paths(&session) {
        let resolved = resolve_media(&session, &original, session_path.as_deref())
            .ok_or_else(|| format!("Missing source: {original}"))?;
        let (track, _) = audio::decode(&resolved.to_string_lossy())
            .map_err(|e| format!("Could not decode {}: {e}", resolved.display()))?;
        tracks.insert(original, track);
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
    let mut writer = RecordingWriter::create(&temp, RENDER_RATE)?;
    let mut mixer = Mixer::new(RENDER_RATE);
    mixer.apply(Cmd::Normalize(true));
    let mut sample_slots = HashMap::new();
    let triggers = trigger_events(&session);
    for original in triggers
        .iter()
        .map(|event| &event.source)
        .collect::<BTreeSet<_>>()
    {
        if sample_slots.len() >= crate::sampler::SAMPLER_SLOTS {
            break;
        }
        let slot = sample_slots.len();
        if let Some(track) = tracks.get(original) {
            mixer.apply(Cmd::Sampler(slot, track.clone()));
            sample_slots.insert(original.clone(), slot);
        }
    }
    let mut frame_index = 0usize;
    let mut loaded = [None, None];
    let mut effects = [0, 0];
    let mut applied_frame = usize::MAX;
    let mut applied_edits = usize::MAX;
    let mut trigger_index = 0usize;
    let mut buffer = Vec::with_capacity(8192);
    let frames = (duration * RENDER_RATE as f64).ceil() as u64;
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
        let time = n as f64 / RENDER_RATE as f64;
        while frame_index + 1 < session.source.log.frames.len()
            && session.source.log.frames[frame_index + 1].seconds <= time
        {
            frame_index += 1;
        }
        let edit_count = session.edits.iter().filter(|e| e.seconds <= time).count();
        if applied_frame != frame_index || applied_edits != edit_count {
            apply_state(
                &mut mixer,
                &session,
                frame_index,
                time,
                &tracks,
                &mut loaded,
                &mut effects,
            );
            applied_frame = frame_index;
            applied_edits = edit_count;
        }
        while trigger_index < triggers.len()
            && triggers[trigger_index].seconds <= time + 0.5 / RENDER_RATE as f64
        {
            if let Some(slot) = sample_slots.get(&triggers[trigger_index].source) {
                mixer.apply(Cmd::SamplerGain(*slot, triggers[trigger_index].gain));
                mixer.apply(Cmd::SamplePress(*slot, crate::sampler::PlayMode::Restart));
            }
            trigger_index += 1;
        }
        let sample = mixer.frame();
        buffer.extend_from_slice(&sample);
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
        )
    })
    .await
    .map_err(|e| e.to_string());
    status.running.store(false, Ordering::SeqCst);
    completed??;
    Ok(Some(result.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn mix_editor_preview(
    status: tauri::State<'_, Arc<RenderStatus>>,
    text: String,
    session_path: Option<String>,
) -> Result<String, String> {
    let session = parse(&text)?;
    let output = std::env::temp_dir().join(format!(
        "skydek-mix-editor-preview-{}.wav",
        std::process::id()
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
        )
    })
    .await
    .map_err(|e| e.to_string());
    status.running.store(false, Ordering::SeqCst);
    completed??;
    Ok(result.to_string_lossy().into_owned())
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
                    }],
                    events: vec![],
                },
                extra: HashMap::new(),
            },
            edits: vec![],
            triggers: vec![],
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
            },
        ];
        let output = root.join("render.wav");
        render(s, None, output.clone(), None).unwrap();
        let mut reader = hound::WavReader::open(output).unwrap();
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert!(reader
            .samples::<f32>()
            .take(20_000)
            .any(|x| x.unwrap().abs() > 0.01));
        std::fs::remove_dir_all(root).unwrap();
    }
}
