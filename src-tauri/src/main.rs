#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod analysis;
mod audio;
mod auto_dj;
mod controller;
mod input;
mod mapping;
mod midi;
mod navigation;
mod preferences;
mod recording;
mod sampler;
mod verification;
use audio::{Cmd, Track, TrackInfo, View};
use crossbeam_queue::ArrayQueue;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;

#[derive(Serialize, Deserialize, Default)]
struct Session {
    #[serde(default)]
    preferences: preferences::Preferences,
    library: Vec<TrackInfo>,
    queue: Vec<String>,
    #[serde(default)]
    mappings: mapping::Mappings,
    #[serde(default)]
    sound_folder: Option<String>,
    #[serde(default)]
    sounds: Vec<SoundFile>,
    #[serde(default = "default_pad_paths")]
    pad_paths: Vec<Option<String>>,
    #[serde(default = "default_sensitivity")]
    sensitivity: f64,
    #[serde(default)]
    music_roots: Vec<String>,
    #[serde(default)]
    library_folder: String,
}
fn default_sensitivity() -> f64 {
    1.
}
fn default_pad_paths() -> Vec<Option<String>> {
    vec![None; sampler::SAMPLER_SLOTS]
}

fn read_session(path: &std::path::Path) -> Result<Session, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut session: Session = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            session.pad_paths.resize(sampler::SAMPLER_SLOTS, None);
            session.preferences.sampler_modes.resize(sampler::SAMPLER_SLOTS, sampler::PlayMode::Restart);
            Ok(session)
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Session::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn open_session(dir: &std::path::Path) -> Result<(PathBuf, Session, String), String> {
    let original = dir.join("session.json");
    match read_session(&original) {
        Ok(session) => Ok((original, session, String::new())),
        Err(error) => {
            let recovery = dir.join("session-recovery.json");
            let backup = dir.join("session.backup.json");
            let session = if recovery.exists() {
                read_session(&recovery)
            } else {
                read_session(&backup)
            }
                .map_err(|e| format!("Both session files are unreadable. Files preserved. {error}; {e}"))?;
            Ok((recovery, session, format!(
                "Session could not be opened. The original was preserved. Changes are saved to session-recovery.json. {error}"
            )))
        }
    }
}

#[cfg(test)]
#[test]
fn unreadable_session_uses_backup_and_preserves_original() {
    let dir = std::env::temp_dir().join(format!("skydek-recovery-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("session.json");
    let backup = dir.join("session.backup.json");
    std::fs::write(&original, b"interrupted save").unwrap();
    let session = Session { queue: vec!["saved-track.wav".into()], ..Session::default() };
    std::fs::write(&backup, serde_json::to_vec(&session).unwrap()).unwrap();
    let (recovery, loaded, warning) = open_session(&dir).unwrap();
    assert_eq!(recovery, dir.join("session-recovery.json"));
    assert_eq!(loaded.queue, session.queue);
    assert!(warning.contains("preserved"));
    assert_eq!(std::fs::read(&original).unwrap(), b"interrupted save");
    std::fs::remove_file(original).unwrap();
    std::fs::remove_file(backup).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
#[derive(Clone, Serialize, Deserialize)]
struct SoundFile {
    path: String,
    name: String,
}
struct Model {
    auto_dj: auto_dj::State,
    session: Session,
    cache: HashMap<String, Arc<Track>>,
    loaded: [Option<String>; 2],
    pending_loads: [Option<PendingLoad>; 2],
    library_revision: u32,
    selected: usize,
    target: Option<usize>,
    shift: bool,
    sampler: usize,
    samples: Vec<Option<String>>,
    sound_revision: u64,
    last: String,
    connected: bool,
    battery: Option<u8>,
    view: View,
    tab: String,
    library_query: String,
}
struct PendingLoad {
    path: String,
    track: Arc<Track>,
    result: Arc<std::sync::atomic::AtomicU8>,
}
struct AppState {
    analysis: Mutex<analysis::Status>,
    cancel_analysis: std::sync::atomic::AtomicBool,
    model: Mutex<Model>,
    commands: Arc<ArrayQueue<Cmd>>,
    views: Arc<ArrayQueue<View>>,
    error: Arc<Mutex<String>>,
    path: PathBuf,
    device: String,
    rate: u32,
    recorder: Arc<recording::Recorder>,
    save_guard: Mutex<()>,
    sample_guard: Mutex<()>,
}
#[derive(Serialize)]
struct Snapshot {
    auto_dj: bool,
    auto_dj_status: String,
    analysis: analysis::Status,
    preferences: preferences::Preferences,
    sensitivity: f64,
    library: Option<Vec<TrackInfo>>,
    library_revision: u32,
    queue: Vec<String>,
    loaded: [Option<String>; 2],
    selected: usize,
    target: usize,
    shift: bool,
    sampler: usize,
    samples: Vec<Option<String>>,
    last: String,
    connected: bool,
    battery: Option<u8>,
    audio: View,
    error: String,
    device: String,
    sample_rate: u32,
    tab: String,
    recording: bool,
    recorded_seconds: f64,
    recording_path: String,
    mappings: mapping::Mappings,
    sound_folder: Option<String>,
    sound_revision: u64,
    music_roots: Vec<String>,
    library_folder: String,
}

fn library_indices(m: &Model) -> Vec<usize> {
    m.session
        .library
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            navigation::in_folder(&t.path, &m.session.library_folder)
                && t.title
                    .to_lowercase()
                    .contains(&m.library_query.to_lowercase())
        })
        .map(|(i, _)| i)
        .collect()
}
#[tauri::command]
fn library_view(
    s: tauri::State<Arc<AppState>>,
    folder: String,
    query: String,
) -> Result<(), String> {
    let mut m = s.model.lock().unwrap();
    m.session.library_folder = folder;
    m.library_query = query;
    let ids = library_indices(&m);
    if !ids.contains(&m.selected) {
        m.selected = ids.first().copied().unwrap_or(0)
    }
    drop(m);
    s.save()
}
#[tauri::command]
async fn add_sample(s: tauri::State<'_, Arc<AppState>>, path: String) -> Result<usize, String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || s.assign_slot(None, path))
        .await
        .map_err(|e| e.to_string())?
}
impl AppState {
    fn sample_press(&self, index: Option<usize>) -> Result<(usize, sampler::PlayMode), String> {
        let mut m = self.model.lock().unwrap();
        let i = index.unwrap_or(m.sampler).min(sampler::SAMPLER_SLOTS - 1);
        if m.samples[i].is_none() {
            return Err(format!("Assign a sound to Pad {} first", i + 1));
        }
        let mode = m.session.preferences.sampler_modes[i];
        self.send(Cmd::SamplePress(i, mode))?;
        m.sampler = i;
        m.tab = "sampler".into();
        m.last = format!("Sampler pad {} · {:?}", i + 1, mode);
        Ok((i, mode))
    }
    fn swap(&self, from: usize, to: usize) -> Result<(), String> {
        if from >= 16 || to >= 16 {
            return Err("Invalid sampler tile".into());
        }
        let _guard = self.sample_guard.lock().unwrap();
        let mut m = self.model.lock().unwrap();
        if from == to {
            return Ok(());
        }
        if m.samples[from].is_none() {
            return Err("Source tile is empty".into());
        }
        self.send(Cmd::SwapSamples(from, to))?;
        m.samples.swap(from, to);
        m.session.pad_paths.swap(from, to);
        m.session.preferences.sampler_modes.swap(from, to);
        if m.sampler == from {
            m.sampler = to
        } else if m.sampler == to {
            m.sampler = from
        }
        drop(m);
        self.save()
    }
}
#[tauri::command]
fn swap_samples(s: tauri::State<Arc<AppState>>, from: usize, to: usize) -> Result<(), String> {
    s.swap(from, to)
}

fn target(m: &Model) -> usize {
    m.target.unwrap_or_else(
        || match (m.view.decks[0].playing, m.view.decks[1].playing) {
            (false, true) => 1,
            (false, false) if m.loaded[0].is_none() && m.loaded[1].is_some() => 1,
            (true, true) if m.view.crossfader > 0.15 => 1,
            _ => 0,
        },
    )
}
fn preparation(m: &Model) -> Result<usize, String> {
    if m.loaded.iter().all(Option::is_none) {
        return Ok(0);
    }
    if !m.view.decks[1].playing {
        Ok(1)
    } else if !m.view.decks[0].playing {
        Ok(0)
    } else {
        Err("Both decks are playing. Choose a deck and confirm replacement.".into())
    }
}
impl AppState {
    fn send(&self, c: Cmd) -> Result<(), String> {
        self.commands
            .push(c)
            .map_err(|_| "Engine command queue is full; retry the action".into())
    }
    fn save(&self) -> Result<(), String> {
        let _guard = self.save_guard.lock().unwrap();
        let m = self.model.lock().unwrap();
        let bytes = serde_json::to_vec_pretty(&m.session).map_err(|e| e.to_string())?;
        drop(m);
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        if self.path.file_name().and_then(|name| name.to_str()) == Some("session.json") {
            match std::fs::read(&self.path) {
                Ok(previous) => {
                    serde_json::from_slice::<Session>(&previous)
                        .map_err(|error| format!("Existing session changed or became unreadable. It was preserved. {error}"))?;
                    let backup = self.path.with_extension("backup.json");
                    let staging = self.path.with_extension("backup.tmp");
                    std::fs::write(&staging, previous).map_err(|e| e.to_string())?;
                    std::fs::rename(staging, backup).map_err(|e| e.to_string())?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        std::fs::rename(tmp, &self.path).map_err(|e| e.to_string())
    }
    fn load(&self, path: String, deck: usize, force: bool) -> Result<(), String> {
        if deck > 1 {
            return Err("Invalid deck".into());
        }
        let mut m = self.model.lock().unwrap();
        if m.view.decks[deck].playing && !force {
            return Err("Playing deck protected. Confirm replacement.".into());
        }
        let track = m.cache.get(&path).cloned();
        drop(m);
        let track = if let Some(t) = track {
            t
        } else {
            let (t, info) = audio::decode(&path)?;
            m = self.model.lock().unwrap();
            let active: Vec<String> = m
                .loaded
                .iter()
                .chain(m.samples.iter())
                .flatten()
                .cloned()
                .chain(m.pending_loads.iter().flatten().map(|load| load.path.clone()))
                .collect();
            m.cache.retain(|p, _| active.contains(p));
            m.cache.insert(path.clone(), t.clone());
            if let Some(existing) = m.session.library.iter_mut().find(|entry| entry.path == path) {
                existing.waveform = info.waveform;
                existing.duration = info.duration;
                m.library_revision = m.library_revision.wrapping_add(1);
            } else {
                m.session.library.push(info);
                m.library_revision = m.library_revision.wrapping_add(1);
            }
            drop(m);
            t
        };
        m = self.model.lock().unwrap();
        if m.auto_dj.pending.is_some() {
            return Err("Auto DJ is starting a transition. Retry loading the track.".into());
        }
        let hot = m
            .session
            .library
            .iter()
            .find(|t| t.path == path)
            .map(|t| t.cues)
            .unwrap_or([None; 8]);
        if m.pending_loads[deck].is_some() {
            return Err("This deck already has a pending load.".into());
        }
        let cue = m
            .session
            .library
            .iter()
            .find(|t| t.path == path)
            .map(|t| t.main_cue)
            .unwrap_or(0.);
        let result = Arc::new(std::sync::atomic::AtomicU8::new(0));
        self.send(Cmd::LoadWithCue {
            deck,
            track: track.clone(),
            hot,
            cue,
            force,
            result: result.clone(),
        })?;
        m.pending_loads[deck] = Some(PendingLoad { path, track, result });
        Ok(())
    }
    fn action(
        &self,
        action: &str,
        deck: Option<usize>,
        value: Option<f64>,
        index: Option<usize>,
    ) -> Result<(), String> {
        let mut m = self.model.lock().unwrap();
        let d = deck.unwrap_or_else(|| target(&m));
        if d > 1 {
            return Err("Invalid deck".into());
        }
        let v = value.unwrap_or(0.);
        if !v.is_finite() {
            return Err("Invalid numeric value".into());
        }
        match action {
            "auto_dj" => {
                m.auto_dj.enabled = v != 0.;
                m.auto_dj.status.clear();
            }
            "auto_dj_seconds" => m.session.preferences.auto_dj_seconds = v.clamp(1., 60.),
            "cue_seconds" => {
                m.session.preferences.cue_seconds = v.clamp(0.1, 10.);
                self.send(Cmd::CueSeconds(m.session.preferences.cue_seconds))?;
            }
            "crossfade_seconds" => m.session.preferences.crossfade_seconds = v.clamp(0.1, 10.),
            "auto_cross" => self.send(Cmd::AutoCross(m.session.preferences.crossfade_seconds))?,
            "fx_down" | "fx_up" => self.send(Cmd::FxGate(
                d,
                index.unwrap_or(0).min(5) as u8,
                action == "fx_down",
            ))?,
            "sample_stop" => self.send(Cmd::StopSample(index.unwrap_or(m.sampler).min(sampler::SAMPLER_SLOTS - 1)))?,
            "sensitivity" => m.session.sensitivity = v.clamp(0.1, 4.),
            "play" => {
                if m.loaded[d].is_none() {
                    return Err(format!("Deck {} has no track loaded", d + 1));
                }
                self.send(Cmd::Toggle(d))?
            }
            "cue_down" => self.send(Cmd::Cue(d, true))?,
            "cue_up" => self.send(Cmd::Cue(d, false))?,
            "cue_hold_down" => self.send(Cmd::CueHold(d, 1))?,
            "cue_hold_up" => self.send(Cmd::CueHold(d, 0))?,
            "cue_hold_cancel" => self.send(Cmd::CueHold(d, 2))?,
            "set_cue" => self.send(Cmd::SetCue(d))?,
            "seek" => self.send(Cmd::Seek(d, v))?,
            "scrub_end" => self.send(Cmd::ScrubEnd(d, v))?,
            "volume" => self.send(Cmd::Volume(d, v as f32))?,
            "filter" => self.send(Cmd::Filter(d, v as f32))?,
            "rate" => self.send(Cmd::Rate(d, v))?,
            "cross" => self.send(Cmd::Cross(v as f32))?,
            "master" => self.send(Cmd::Master(v as f32))?,
            "sampler_volume" => {
                m.session.preferences.sampler_volume = v.clamp(0., 1.);
                self.send(Cmd::SamplerVolume(m.session.preferences.sampler_volume as f32))?;
            }
            "curve" => self.send(Cmd::Linear(v > 0.))?,
            "target" => m.target = Some(d),
            "auto_target" => m.target = None,
            "shift" => m.shift = !m.shift,
            "select" => {
                m.tab = "library".into();
                m.selected = index
                    .unwrap_or(0)
                    .min(m.session.library.len().saturating_sub(1))
            }
            "browse" => {
                if m.tab == "folders" {
                    let folders = navigation::folders(
                        &m.session
                            .library
                            .iter()
                            .map(|t| t.path.clone())
                            .collect::<Vec<_>>(),
                        &m.session.music_roots,
                    );
                    let at = folders
                        .iter()
                        .position(|p| {
                            navigation::normalize(p)
                                == navigation::normalize(&m.session.library_folder)
                        })
                        .unwrap_or(0);
                    if let Some(i) =
                        navigation::browse(&(0..folders.len()).collect::<Vec<_>>(), at, v as isize)
                    {
                        m.session.library_folder = folders[i].clone();
                        m.selected = library_indices(&m).first().copied().unwrap_or(0);
                    }
                } else if m.tab == "sampler" {
                    m.sampler = navigation::pad_step(m.sampler, 0, v.signum() as isize);
                } else {
                    let indices = library_indices(&m);
                    if let Some(i) = navigation::browse(&indices, m.selected, v as isize) {
                        m.selected = i;
                    }
                }
            }
            "sample_select" => {
                m.sampler = index.unwrap_or(m.sampler).min(sampler::SAMPLER_SLOTS - 1);
                m.tab = "sampler".into();
            }
            "focus_library" => m.tab = "library".into(),
            "focus_folders" => m.tab = "folders".into(),
            "focus_sampler" => m.tab = "sampler".into(),
            "panel_left" | "panel_right" => {
                if m.tab == "sampler" && !m.shift {
                    if action == "panel_left" && m.sampler.is_multiple_of(4) {
                        m.tab = "library".into();
                    } else {
                        m.sampler = navigation::pad_step(
                            m.sampler,
                            if action == "panel_left" { -1 } else { 1 },
                            0,
                        );
                    }
                    m.last = "Sampler grid navigation".into();
                    return Ok(());
                }
                let panels = ["folders", "library", "sampler"];
                let pos = panels.iter().position(|p| *p == m.tab).unwrap_or(1);
                let next = if action == "panel_left" {
                    pos.saturating_sub(1)
                } else {
                    (pos + 1).min(2)
                };
                m.tab = panels[next].into();
            }
            "panel" => {
                m.tab = if m.tab == "library" {
                    "sampler".into()
                } else {
                    "library".into()
                }
            }
            "queue_add" => {
                if let Some(t) = m.session.library.get(m.selected) {
                    let p = t.path.clone();
                    m.session.queue.push(p)
                }
            }
            "queue_remove" => {
                if let Some(i) = index {
                    if i < m.session.queue.len() {
                        m.session.queue.remove(i);
                    }
                }
            }
            "load_selected" => {
                let p = m
                    .session
                    .library
                    .get(m.selected)
                    .ok_or("Select a track first")?
                    .path
                    .clone();
                let prep = preparation(&m)?;
                drop(m);
                return self.load(p, prep, false);
            }
            "load_next" => {
                let p = m.session.queue.first().ok_or("Queue is empty")?.clone();
                let prep = preparation(&m)?;
                drop(m);
                self.load(p, prep, false)?;
                m = self.model.lock().unwrap();
                m.session.queue.remove(0);
            }
            "hot" | "clear_hot" => self.send(Cmd::Hot(
                d,
                index.unwrap_or(0).min(7),
                action == "clear_hot",
            ))?,
            "backspin" => self.send(Cmd::Fx(d, 1))?,
            "brake" => self.send(Cmd::Fx(d, 2))?,
            "start" => self.send(Cmd::Fx(d, 3))?,
            "echo" => self.send(Cmd::Fx(d, 4))?,
            "reverb" => self.send(Cmd::Fx(d, 5))?,
            "wet" => self.send(Cmd::Wet(d, v as f32))?,
            "loop" => {
                let bpm = m.loaded[d]
                    .as_ref()
                    .and_then(|p| m.session.library.iter().find(|t| &t.path == p))
                    .and_then(|t| t.bpm)
                    .ok_or("Set BPM before enabling a beat-length loop")?;
                self.send(Cmd::Loop(d, 60. / bpm * v.clamp(0., 64.)))?;
            }
            "load_deck" => {
                let p = m
                    .session
                    .library
                    .get(m.selected)
                    .ok_or("Select a track first")?
                    .path
                    .clone();
                drop(m);
                return self.load(p, d, false);
            }
            "sample" => {
                drop(m);
                return self.sample_press(index).map(|_| ());
            }
            "next_sample" => m.sampler = (m.sampler + 1) % 16,
            "stop_samples" => self.send(Cmd::StopSamples)?,
            "bpm" => {
                if let Some(path) = m.loaded[d].clone() {
                    if let Some(t) = m.session.library.iter_mut().find(|t| t.path == path) {
                        t.bpm = if (30. ..=300.).contains(&v) {
                            Some(v)
                        } else {
                            None
                        };
                        m.library_revision = m.library_revision.wrapping_add(1);
                    }
                }
            }
            "tempo_match" => {
                let a = m.loaded[d]
                    .as_ref()
                    .and_then(|p| m.session.library.iter().find(|t| &t.path == p))
                    .and_then(|t| t.bpm);
                let b = m.loaded[1 - d]
                    .as_ref()
                    .and_then(|p| m.session.library.iter().find(|t| &t.path == p))
                    .and_then(|t| t.bpm);
                if let (Some(a), Some(b)) = (a, b) {
                    let ratio = b * m.view.decks[1 - d].rate / a;
                    if !ratio.is_finite() || !(0.5..=1.5).contains(&ratio) {
                        return Err("Tempo sync exceeds the ±50% tempo range. Check the analyzed BPM values.".into());
                    }
                    self.send(Cmd::Rate(d, ratio))?
                } else {
                    return Err("Analyze both loaded songs before using Sync. Sync matches tempo; beat-phase alignment is not available.".into());
                }
            }
            _ => return Err(format!("{action} is not available in this build")),
        }
        m.last = format!("{} → Deck {}", action, d + 1);
        Ok(())
    }
}
#[tauri::command]
fn snapshot(s: tauri::State<Arc<AppState>>, library_revision: Option<u32>) -> Snapshot {
    let m = s.model.lock().unwrap();
    Snapshot {
        auto_dj: m.auto_dj.enabled,
        auto_dj_status: m.auto_dj.status.clone(),
        analysis: s.analysis.lock().unwrap().clone(),
        preferences: m.session.preferences.clone(),
        sensitivity: m.session.sensitivity,
        library: (library_revision != Some(m.library_revision)).then(|| m.session.library.clone()),
        library_revision: m.library_revision,
        queue: m.session.queue.clone(),
        loaded: m.loaded.clone(),
        selected: m.selected,
        target: target(&m),
        shift: m.shift,
        sampler: m.sampler,
        samples: m.samples.clone(),
        last: m.last.clone(),
        connected: m.connected,
        battery: m.battery,
        audio: m.view.clone(),
        error: s.error.lock().unwrap().clone(),
        device: s.device.clone(),
        sample_rate: s.rate,
        tab: m.tab.clone(),
        recording: s.recorder.is_open(),
        recorded_seconds: s.recorder.frames.load(std::sync::atomic::Ordering::Relaxed) as f64
            / s.rate as f64,
        recording_path: s.recorder.path.lock().unwrap().clone(),
        mappings: m.session.mappings.clone(),
        sound_folder: m.session.sound_folder.clone(),
        sound_revision: m.sound_revision,
        music_roots: m.session.music_roots.clone(),
        library_folder: m.session.library_folder.clone(),
    }
}
#[tauri::command]
fn command(
    s: tauri::State<Arc<AppState>>,
    action: String,
    deck: Option<usize>,
    value: Option<f64>,
    index: Option<usize>,
) -> Result<(), String> {
    s.action(&action, deck, value, index)?;
    if [
        "bpm",
        "queue_add",
        "queue_remove",
        "sensitivity",
        "cue_seconds",
        "crossfade_seconds",
        "auto_dj_seconds",
    ]
    .contains(&action.as_str())
    {
        s.save()?
    }
    Ok(())
}
#[tauri::command]
async fn load_track(
    s: tauri::State<'_, Arc<AppState>>,
    path: String,
    deck: usize,
    force: bool,
) -> Result<(), String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || s.load(path, deck, force))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn import_audio(s: tauri::State<'_, Arc<AppState>>, folder: bool) -> Result<usize, String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut imported_root = None;
        let paths = if folder {
            let Some(root) = rfd::FileDialog::new().pick_folder() else {
                return Ok(0);
            };
            imported_root = Some(root.to_string_lossy().into_owned());
            fn walk(p: &std::path::Path, out: &mut Vec<PathBuf>) {
                if let Ok(entries) = std::fs::read_dir(p) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() && !p.is_symlink() {
                            walk(&p, out)
                        } else if p
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| ["mp3", "wav"].contains(&s.to_lowercase().as_str()))
                            .unwrap_or(false)
                        {
                            out.push(p)
                        }
                    }
                }
            }
            let mut list = Vec::new();
            walk(&root, &mut list);
            list
        } else {
            rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3"])
                .pick_files()
                .unwrap_or_default()
        };
        if let Some(root) = imported_root {
            let mut m = s.model.lock().unwrap();
            if !m.session.music_roots.contains(&root) {
                m.session.music_roots.push(root.clone());
            }
            m.session.library_folder = root;
        }
        let mut count = 0;
        for p in paths {
            let path = p.to_string_lossy().into_owned();
            let mut m = s.model.lock().unwrap();
            if m.session.library.iter().any(|t| t.path == path) {
                continue;
            }
            m.session.library.push(TrackInfo {
                title: p.file_stem().unwrap_or_default().to_string_lossy().into(),
                path,
                ..Default::default()
            });
            count += 1;
            m.library_revision = m.library_revision.wrapping_add(1);
        }
        s.save()?;
        start_analysis(s.clone(), false)?;
        Ok(count)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn assign_sample(
    s: tauri::State<'_, Arc<AppState>>,
    index: usize,
    path: Option<String>,
) -> Result<(), String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if index >= 16 {
            return Err("Invalid sample pad".into());
        }
        let p = if let Some(path) = path {
            PathBuf::from(path)
        } else {
            let Some(p) = rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3"])
                .pick_file()
            else {
                return Ok(());
            };
            p
        };
        let path = p.to_string_lossy().into_owned();
        s.assign(index, path)
    })
    .await
    .map_err(|e| e.to_string())?
}

impl AppState {
    fn assign(&self, index: usize, path: String) -> Result<(), String> {
        self.assign_slot(Some(index), path).map(|_| ())
    }
    fn assign_slot(&self, index: Option<usize>, path: String) -> Result<usize, String> {
        self.assign_slot_with_focus(index, path, true)
    }
    fn assign_slot_with_focus(
        &self,
        index: Option<usize>,
        path: String,
        focus: bool,
    ) -> Result<usize, String> {
        let _guard = self.sample_guard.lock().unwrap();
        {
            let mut m = self.model.lock().unwrap();
            m.samples.resize(sampler::SAMPLER_SLOTS, None);
            m.session.pad_paths.resize(sampler::SAMPLER_SLOTS, None);
            m.session.preferences.sampler_modes.resize(sampler::SAMPLER_SLOTS, sampler::PlayMode::Restart);
        }
        let index = index
            .or_else(|| {
                self.model
                    .lock()
                    .unwrap()
                    .samples
                    .iter()
                    .position(Option::is_none)
            })
            .ok_or("All 48 pads are full. Drag onto a tile to replace it or clear a tile first.")?;
        if index >= sampler::SAMPLER_SLOTS {
            return Err("Invalid sampler pad".into());
        }
        if !focus {
            let m = self.model.lock().unwrap();
            if m.session.pad_paths[index].as_deref() != Some(path.as_str())
                || m.samples[index].is_some()
            {
                return Ok(index);
            }
        }
        let cached = self.model.lock().unwrap().cache.get(&path).cloned();
        let t = if let Some(t) = cached {
            if t.samples.len() > audio::SAMPLE_BYTES / std::mem::size_of::<[f32; 2]>() {
                return Err("Decoded audio exceeds the 32 MiB limit for a sampler pad.".into());
            }
            t
        } else {
            let (t, _) = audio::decode_sample(&path)?;
            let mut m = self.model.lock().unwrap();
            let active: Vec<String> = m
                .loaded
                .iter()
                .chain(m.samples.iter())
                .flatten()
                .cloned()
                .collect();
            m.cache.retain(|p, _| active.contains(p));
            m.cache.insert(path.clone(), t.clone());
            t
        };
        self.send(Cmd::Sampler(index, t))?;
        {
            let mut m = self.model.lock().unwrap();
            m.samples.resize(sampler::SAMPLER_SLOTS, None);
            m.session.pad_paths.resize(sampler::SAMPLER_SLOTS, None);
            m.session.preferences.sampler_modes.resize(sampler::SAMPLER_SLOTS, sampler::PlayMode::Restart);
            m.samples[index] = Some(path.clone());
            if focus {
                m.sampler = index;
                m.tab = "sampler".into();
                m.last = format!("Assigned sampler pad {}", index + 1);
            }
            m.session.pad_paths[index] = Some(path);
        }
        self.save()?;
        Ok(index)
    }
    fn mapped(&self, action: &str, deck: usize, value: Option<f64>) -> Result<(), String> {
        match action {
            "none" => Ok(()),
            "browse_up" => self.action("browse", None, Some(-1.), None),
            "browse_down" => self.action("browse", None, Some(1.), None),
            "target_next" => self.action("target", Some(1 - deck), None, None),
            "load_1" => self.action("load_deck", Some(0), None, None),
            "load_2" => self.action("load_deck", Some(1), None, None),
            a => self.action(a, Some(deck), value, None),
        }
    }
}
#[tauri::command]
fn save_mappings(
    s: tauri::State<Arc<AppState>>,
    mappings: mapping::Mappings,
) -> Result<(), String> {
    let name = s
        .model
        .lock()
        .unwrap()
        .session
        .preferences
        .active_profile
        .clone();
    s.profile("save".into(), name, Some(mappings))
}
#[tauri::command]
fn default_mappings() -> mapping::Mappings {
    mapping::Mappings::default()
}

#[tauri::command]
fn mapping_profile(
    s: tauri::State<Arc<AppState>>,
    operation: String,
    name: String,
    mappings: Option<mapping::Mappings>,
) -> Result<(), String> {
    s.profile(operation, name, mappings)
}
impl AppState {
    fn profile(
        &self,
        operation: String,
        name: String,
        mappings: Option<mapping::Mappings>,
    ) -> Result<(), String> {
        let name = name.trim().to_string();
        if name.is_empty() || name.len() > 80 {
            return Err("Profile name must contain 1–80 characters".into());
        }
        let old = {
            let mut m = self.model.lock().unwrap();
            let old = (m.session.preferences.clone(), m.session.mappings.clone());
            match operation.as_str() {
                "save" => {
                    let mappings = mappings.ok_or("No mappings provided")?;
                    mappings.validate()?;
                    if !m.session.preferences.profiles.contains_key(&name)
                        && m.session.preferences.profiles.len() >= 100
                    {
                        return Err("Maximum 100 profiles".into());
                    }
                    m.session
                        .preferences
                        .profiles
                        .insert(name.clone(), mappings.clone());
                    m.session.preferences.active_profile = name;
                    m.session.mappings = mappings;
                }
                "select" => {
                    m.session.mappings = m
                        .session
                        .preferences
                        .profiles
                        .get(&name)
                        .ok_or("Profile not found")?
                        .clone();
                    m.session.preferences.active_profile = name;
                }
                "delete" => {
                    if m.session.preferences.profiles.len() <= 1 {
                        return Err("Keep at least one mapping profile".into());
                    }
                    if m.session.preferences.profiles.remove(&name).is_none() {
                        return Err("Profile not found".into());
                    }
                    if m.session.preferences.active_profile == name {
                        let (key, value) = m.session.preferences.profiles.iter().next().unwrap();
                        let (key, value) = (key.clone(), value.clone());
                        m.session.mappings = value;
                        m.session.preferences.active_profile = key;
                    }
                }
                _ => return Err("Unknown profile operation".into()),
            }
            old
        };
        if let Err(error) = self.save() {
            let mut m = self.model.lock().unwrap();
            (m.session.preferences, m.session.mappings) = old;
            return Err(error);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
enum HeldAction {
    Cue(usize),
    Effect(usize, u8),
    Sample(usize),
}
impl HeldAction {
    fn release(&self, s: &AppState, cancel: bool) -> Result<(), String> {
        s.send(match *self {
            Self::Cue(deck) => Cmd::CueHold(deck, if cancel { 2 } else { 0 }),
            Self::Effect(deck, effect) => Cmd::FxGate(deck, effect, false),
            Self::Sample(index) => Cmd::StopSample(index),
        })
    }
}

fn effect_id(action: &str) -> Option<u8> {
    ["backspin", "brake", "start", "echo", "reverb"]
        .iter()
        .position(|f| *f == action)
        .map(|i| i as u8 + 1)
}

fn scan_sounds(root: &std::path::Path) -> Result<Vec<SoundFile>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(p)
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .map(|e| ["wav", "mp3"].contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
            {
                out.push(SoundFile {
                    path: p.to_string_lossy().into(),
                    name: p.file_name().unwrap_or_default().to_string_lossy().into(),
                });
                if out.len() > 10000 {
                    return Err(
                        "Sound folder exceeds 10,000 audio files. Choose a smaller folder.".into(),
                    );
                }
            }
        }
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    Ok(out)
}
#[tauri::command]
fn sound_files(s: tauri::State<Arc<AppState>>) -> Vec<SoundFile> {
    s.model.lock().unwrap().session.sounds.clone()
}

#[tauri::command]
fn analyze_library(
    s: tauri::State<Arc<AppState>>,
    force: bool,
    cancel: bool,
) -> Result<(), String> {
    if cancel {
        s.cancel_analysis
            .store(true, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }
    start_analysis(s.inner().clone(), force)
}

fn start_analysis(s: Arc<AppState>, force: bool) -> Result<(), String> {
    {
        let mut status = s.analysis.lock().unwrap();
        if status.running {
            return Ok(());
        }
        *status = analysis::Status {
            running: true,
            message: "Scanning library".into(),
            ..Default::default()
        };
        s.cancel_analysis
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if force {
        let mut m = s.model.lock().unwrap();
        for track in &mut m.session.library {
            track.analysis_version = 0;
        }
        m.library_revision = m.library_revision.wrapping_add(1);
    }
    std::thread::spawn(move || {
        let mut done = 0;
        let mut failed = 0;
        loop {
            if s.cancel_analysis.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let next = {
                let m = s.model.lock().unwrap();
                m.session
                    .library
                    .iter()
                    .find(|t| t.analysis_version < analysis::VERSION)
                    .map(|t| {
                        (
                            t.path.clone(),
                            t.title.clone(),
                            m.session
                                .library
                                .iter()
                                .filter(|t| t.analysis_version < analysis::VERSION)
                                .count(),
                        )
                    })
            };
            let Some((path, title, remaining)) = next else {
                break;
            };
            {
                let mut status = s.analysis.lock().unwrap();
                status.current = title;
                status.total = done + remaining;
            }
            let result = analysis::analyze(&path, &s.cancel_analysis);
            if s.cancel_analysis.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            {
                let mut m = s.model.lock().unwrap();
                if let Some(t) = m.session.library.iter_mut().find(|t| t.path == path) {
                    t.analysis_version = analysis::VERSION;
                    match result {
                        Ok(info) => {
                            t.duration = info.duration;
                            if t.bpm.is_none() || t.bpm_confidence > 0. {
                                t.bpm = info.bpm;
                                t.bpm_confidence = info.confidence;
                            }
                            t.analysis_error = None;
                        }
                        Err(error) => {
                            failed += 1;
                            t.analysis_error = Some(error);
                        }
                    }
                    m.library_revision = m.library_revision.wrapping_add(1);
                }
            }
            done += 1;
            {
                let mut status = s.analysis.lock().unwrap();
                status.completed = done;
                status.failed = failed;
            }
            if done % 10 == 0 {
                if let Err(error) = s.save() {
                    s.analysis.lock().unwrap().message =
                        format!("Unable to save analysis: {error}");
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let saved = s.save();
        let mut status = s.analysis.lock().unwrap();
        status.running = false;
        status.current.clear();
        if let Err(error) = saved {
            status.message = format!("Unable to save analysis: {error}");
        } else if s.cancel_analysis.load(std::sync::atomic::Ordering::Relaxed) {
            status.message = "Scan paused; completed results saved".into();
        } else if !status.message.starts_with("Unable") {
            status.message = format!("Scan complete · {done} tracks · {failed} unreadable");
        }
        let retry = !status.message.starts_with("Unable")
            && !s.cancel_analysis.load(std::sync::atomic::Ordering::Relaxed);
        drop(status);
        if retry
            && s.model
                .lock()
                .unwrap()
                .session
                .library
                .iter()
                .any(|t| t.analysis_version < analysis::VERSION)
        {
            let _ = start_analysis(s.clone(), false);
        }
    });
    Ok(())
}

#[tauri::command]
async fn waveform_data(
    s: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<Vec<[f32; 3]>, String> {
    let track = s
        .model
        .lock()
        .unwrap()
        .cache
        .get(&path)
        .cloned()
        .ok_or("Load the track first")?;
    tauri::async_runtime::spawn_blocking(move || analysis::waveform(&track))
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn import_sound_folder(
    s: tauri::State<'_, Arc<AppState>>,
    refresh: bool,
) -> Result<usize, String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = if refresh {
            s.model
                .lock()
                .unwrap()
                .session
                .sound_folder
                .clone()
                .map(PathBuf::from)
                .ok_or("Choose a sound-effects folder first")?
        } else {
            let Some(p) = rfd::FileDialog::new()
                .set_title("Choose your sound-effects folder")
                .pick_folder()
            else {
                return Ok(0);
            };
            p
        };
        let sounds = scan_sounds(&root)?;
        let n = sounds.len();
        {
            let mut m = s.model.lock().unwrap();
            m.session.sound_folder = Some(root.to_string_lossy().into());
            m.session.sounds = sounds;
            m.sound_revision += 1;
        }
        s.save()?;
        Ok(n)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn controller_input() -> controller::ControllerSnapshot {
    controller::read_xinput()
}
#[tauri::command]
fn audio_devices() -> Result<Vec<String>, String> {
    audio::output_devices()
}

#[tauri::command]
fn select_audio_device(
    s: tauri::State<Arc<AppState>>,
    device: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if s.recorder.is_open() {
        return Err("Stop recording before changing audio output".into());
    }
    if let Some(name) = &device {
        if !audio::output_devices()?.contains(name) {
            return Err(
                "This audio output is no longer connected. Refresh the device list.".into(),
            );
        }
    }
    let old = {
        let mut m = s.model.lock().unwrap();
        std::mem::replace(&mut m.session.preferences.audio_device, device)
    };
    if let Err(error) = s.save() {
        s.model.lock().unwrap().session.preferences.audio_device = old;
        return Err(error);
    }
    app.restart();
}

#[tauri::command]
fn toggle_fullscreen(window: tauri::WebviewWindow) -> Result<bool, String> {
    let fullscreen = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window
        .set_fullscreen(fullscreen)
        .map_err(|e| e.to_string())?;
    Ok(fullscreen)
}

#[tauri::command]
fn clear_sample(s: tauri::State<Arc<AppState>>, index: usize) -> Result<(), String> {
    if index >= 16 {
        return Err("Invalid sampler pad".into());
    }
    let _guard = s.sample_guard.lock().unwrap();
    s.send(Cmd::ClearSample(index))?;
    {
        let mut m = s.model.lock().unwrap();
        m.samples[index] = None;
        m.session.pad_paths[index] = None;
    }
    s.save()
}

#[tauri::command]
fn sampler_mode(
    s: tauri::State<Arc<AppState>>,
    index: usize,
    mode: sampler::PlayMode,
    all: bool,
) -> Result<(), String> {
    s.set_sampler_mode(index, mode, all)
}
impl AppState {
    fn set_sampler_mode(
        &self,
        index: usize,
        mode: sampler::PlayMode,
        all: bool,
    ) -> Result<(), String> {
        if index >= 16 {
            return Err("Invalid sampler pad".into());
        }
        let _guard = self.sample_guard.lock().unwrap();
        let old = {
            let mut m = self.model.lock().unwrap();
            let old = m.session.preferences.sampler_modes.clone();
            if all {
                self.send(Cmd::StopSamples)?;
                m.session.preferences.sampler_modes = vec![mode; sampler::SAMPLER_SLOTS];
            } else {
                self.send(Cmd::StopSample(index))?;
                m.session.preferences.sampler_modes[index] = mode;
            }
            old
        };
        if let Err(error) = self.save() {
            self.model.lock().unwrap().session.preferences.sampler_modes = old;
            return Err(error);
        }
        Ok(())
    }
}

fn worker(s: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut prev = controller::read_xinput();
        let mut armed = false;
        let mut right_armed = false;
        let mut holds: HashMap<String, HeldAction> = HashMap::new();
        let mut last_mapping: Option<(bool, mapping::Mappings)> = None;
        let mut last_axes: [Option<f32>; 4] = [None; 4];
        let mut trigger_down = [false; 2];
        loop {
            while let Some(v) = s.views.pop() {
                let mut m = s.model.lock().unwrap();
                m.view = v.clone();
                auto_dj::acknowledge(&mut m);
                for d in 0..2 {
                    if let Some(pending) = &m.pending_loads[d] {
                        match pending.result.load(std::sync::atomic::Ordering::Acquire) {
                            1 if Arc::as_ptr(&pending.track) as usize == v.decks[d].track_id => {
                                let pending = m.pending_loads[d].take().unwrap();
                                m.loaded[d] = Some(pending.path.clone());
                                m.cache.insert(pending.path, pending.track);
                                m.last = format!("Load → Deck {}", d + 1);
                            }
                            2 => {
                                m.pending_loads[d] = None;
                                m.last = "Playing deck protected. Retry or confirm replacement.".into();
                            }
                            _ => {}
                        }
                    }
                    if let Some(p) = m.loaded[d].clone() {
                        // Ignore snapshots queued before this deck's new track was accepted.
                        if m.cache.get(&p).map(|t| Arc::as_ptr(t) as usize)
                            != Some(v.decks[d].track_id)
                        {
                            continue;
                        }
                        if let Some(t) = m.session.library.iter_mut().find(|t| t.path == p) {
                            let cue_changed = t.cues != v.decks[d].hot || t.main_cue != v.decks[d].cue;
                            t.cues = v.decks[d].hot;
                            t.main_cue = v.decks[d].cue;
                            if t.waveform.is_empty() {
                                if let Some(track) = m.cache.get(&p) {
                                    let wave = track
                                        .samples
                                        .chunks((track.samples.len() / 1400).max(1))
                                        .map(|c| {
                                            c.iter().fold(0f32, |a, v| {
                                                a.max(v[0].abs()).max(v[1].abs())
                                            })
                                        })
                                        .collect::<Vec<_>>();
                                    let duration = track.samples.len() as f64 / track.rate as f64;
                                    if let Some(t) =
                                        m.session.library.iter_mut().find(|t| t.path == p)
                                    {
                                        t.waveform = wave;
                                        t.duration = duration;
                                        m.library_revision = m.library_revision.wrapping_add(1);
                                    }
                                }
                            }
                            if cue_changed {
                                m.library_revision = m.library_revision.wrapping_add(1);
                            }
                        }
                    }
                }
            }
            let input = controller::read_xinput();
            let mapping_changed = {
                let m = s.model.lock().unwrap();
                let current = (m.shift, m.session.mappings.clone());
                let changed = last_mapping.as_ref() != Some(&current);
                last_mapping = Some(current);
                changed
            };
            if mapping_changed {
                last_axes = [None; 4];
            }
            {
                let mut m = s.model.lock().unwrap();
                m.connected = input.connected;
                m.battery = input.battery_percent;
            }
            if input.connected && prev.connected {
                for control in mapping::BUTTONS {
                    let pressed = match *control {
                        "lt" => input.left_trigger >= if trigger_down[0] { 0.06 } else { 0.14 },
                        "rt" => input.right_trigger >= if trigger_down[1] { 0.06 } else { 0.14 },
                        c => input.buttons.get(c).copied().unwrap_or(false),
                    };
                    let was = match *control {
                        "lt" => trigger_down[0],
                        "rt" => trigger_down[1],
                        c => prev.buttons.get(c).copied().unwrap_or(false),
                    };
                    if *control == "lt" {
                        trigger_down[0] = pressed;
                    }
                    if *control == "rt" {
                        trigger_down[1] = pressed;
                    }
                    if pressed && !was {
                        let (action, deck) = {
                            let m = s.model.lock().unwrap();
                            (m.session.mappings.get(m.shift, control), target(&m))
                        };
                        let action = if action == "panel" {
                            if *control == "dpad-left" {
                                "panel_left".into()
                            } else {
                                "panel_right".into()
                            }
                        } else {
                            action
                        };
                        let result = if action == "cue_hold" {
                            holds.insert(control.to_string(), HeldAction::Cue(deck));
                            s.action("cue_hold_down", Some(deck), None, None)
                        } else if let Some(effect) = effect_id(&action) {
                            holds.insert(control.to_string(), HeldAction::Effect(deck, effect));
                            s.send(Cmd::FxGate(deck, effect, true))
                        } else if action == "sample" {
                            s.sample_press(None).map(|(index, mode)| {
                                if mode == sampler::PlayMode::Hold {
                                    holds.insert(control.to_string(), HeldAction::Sample(index));
                                }
                            })
                        } else {
                            s.mapped(&action, deck, None)
                        };
                        if let Err(e) = result {
                            s.model.lock().unwrap().last = e;
                        }
                    } else if !pressed && was {
                        if let Some(held) = holds.remove(*control) {
                            if !holds.values().any(|other| *other == held) {
                                let _ = held.release(&s, false);
                            }
                        }
                    }
                }
                armed = input::armed(
                    armed,
                    input.left_stick_x,
                    input.left_stick_y,
                    input.buttons.get("l3").copied().unwrap_or(false),
                );
                right_armed = input::armed(
                    right_armed,
                    input.right_stick_x,
                    input.right_stick_y,
                    input.buttons.get("r3").copied().unwrap_or(false),
                );
                for (axis_index, (control, value, _before, is_armed)) in [
                    ("left-x", input.left_stick_x, prev.left_stick_x, armed),
                    ("left-y", input.left_stick_y, prev.left_stick_y, armed),
                    (
                        "right-x",
                        input.right_stick_x,
                        prev.right_stick_x,
                        right_armed,
                    ),
                    (
                        "right-y",
                        input.right_stick_y,
                        prev.right_stick_y,
                        right_armed,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    if is_armed {
                        let (action, deck) = {
                            let m = s.model.lock().unwrap();
                            (m.session.mappings.get(m.shift, control), target(&m))
                        };
                        let axis = input::axis(value);
                        if action == "cross" {
                            if axis != 0. {
                                let m = s.model.lock().unwrap();
                                let v = axis as f64 * m.session.sensitivity * 0.008;
                                drop(m);
                                let _ = s.send(Cmd::CrossDelta(v as f32));
                            }
                            continue;
                        }
                        if !input::changed(last_axes[axis_index], axis) {
                            continue;
                        }
                        let value = if ["volume", "master", "wet"].contains(&action.as_str()) {
                            (axis as f64 + 1.) * 0.5
                        } else {
                            axis as f64
                        };
                        if s.mapped(&action, deck, Some(value)).is_ok() {
                            last_axes[axis_index] = Some(axis);
                        }
                    }
                }
            } else {
                for (_, held) in holds.drain() {
                    let _ = held.release(&s, true);
                }
                armed = false;
                right_armed = false;
                last_axes = [None; 4];
                trigger_down = [input.left_trigger > 0.06, input.right_trigger > 0.06];
            }
            prev = input.clone();
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    });
}
#[tauri::command]
async fn record(
    s: tauri::State<'_, Arc<AppState>>,
    start: bool,
    format: Option<String>,
) -> Result<(), String> {
    let s = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if start {
            let format = format.as_deref().unwrap_or("wav");
            if !["wav", "mp3"].contains(&format) {
                return Err("Choose WAV or MP3 recording format".into());
            }
            let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("SkyDek Mix.{format}"))
                .add_filter(if format == "mp3" { "MP3" } else { "WAV" }, &[format])
                .save_file()
            else {
                return Ok(());
            };
            s.recorder.start(path.with_extension(format), s.rate)
        } else {
            s.recorder.stop()
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--verify") {
        let dir = PathBuf::from(
            args.get(i + 1)
                .expect("Verification output directory required"),
        );
        if let Err(e) = verification::run(dir.clone()) {
            let _ = std::fs::write(dir.join("verification-error.txt"), e);
            std::process::exit(1)
        }
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let (path, mut session, session_warning) = open_session(&dir)
                .map_err(std::io::Error::other)?;
            if session.mappings.validate().is_err() {
                session.mappings = mapping::Mappings::default();
            }
            if session.preferences.validate().is_err() {
                session.preferences.cue_seconds = 3.;
                session.preferences.crossfade_seconds = 1.;
            }
            if session.preferences.input_version == 0 {
                for map in [&mut session.mappings.normal, &mut session.mappings.shift] {
                    if map.get("l3").map(String::as_str) == Some("none") {
                        map.insert("l3".into(), "auto_cross".into());
                    }
                }
                session.preferences.input_version = 1;
            }
            session
                .preferences
                .profiles
                .retain(|_, m| m.validate().is_ok());
            session.preferences.profiles.insert(
                session.preferences.active_profile.clone(),
                session.mappings.clone(),
            );
            if !session.sensitivity.is_finite() || session.sensitivity <= 0. {
                session.sensitivity = 1.;
            }
            let samples = session.pad_paths.clone();
            let commands = Arc::new(ArrayQueue::new(256));
            let _ = commands.push(Cmd::CueSeconds(session.preferences.cue_seconds));
            let views = Arc::new(ArrayQueue::new(16));
            let error = Arc::new(Mutex::new(session_warning));
            let recorder = recording::Recorder::new();
            let (device, rate) = audio::start_on_device(
                commands.clone(),
                views.clone(),
                error.clone(),
                recorder.clone(),
                session.preferences.audio_device.as_deref(),
            )
            .or_else(|cause| {
                if session.preferences.audio_device.is_none() {
                    return Err(cause);
                }
                let fallback = audio::start_on_device(
                    commands.clone(), views.clone(), error.clone(), recorder.clone(), None,
                )?;
                let mut warning = error.lock().unwrap();
                if !warning.is_empty() {
                    warning.push(' ');
                }
                warning.push_str(&format!("{cause} Using the default output."));
                Ok(fallback)
            })
            .unwrap_or_else(|e| {
                let mut warning = error.lock().unwrap();
                if !warning.is_empty() {
                    warning.push(' ');
                }
                warning.push_str(&e);
                (String::new(), 48000)
            });
            let state = Arc::new(AppState {
                analysis: Mutex::new(analysis::Status::default()),
                cancel_analysis: std::sync::atomic::AtomicBool::new(false),
                model: Mutex::new(Model {
                    auto_dj: auto_dj::State::default(),
                    session,
                    cache: HashMap::new(),
                    loaded: [None, None],
                    pending_loads: [None, None],
                    library_revision: 0,
                    selected: 0,
                    target: None,
                    shift: false,
                    sampler: 0,
                    samples: vec![None; sampler::SAMPLER_SLOTS],
                    sound_revision: 0,
                    last: "Ready. Import WAV or MP3 audio.".into(),
                    connected: false,
                    battery: None,
                    view: View::default(),
                    tab: "library".into(),
                    library_query: String::new(),
                }),
                commands,
                views,
                error,
                path,
                device,
                rate,
                recorder,
                save_guard: Mutex::new(()),
                sample_guard: Mutex::new(()),
            });
            worker(state.clone());
            auto_dj::start(state.clone());
            start_analysis(state.clone(), false)?;
            let restore = state.clone();
            std::thread::spawn(move || {
                for (i, path) in samples.into_iter().enumerate() {
                    if let Some(path) = path {
                        if let Err(e) = restore.assign_slot_with_focus(Some(i), path, false) {
                            restore.model.lock().unwrap().last =
                                format!("Pad {} restore failed: {e}", i + 1);
                        }
                    }
                }
            });
            app.manage(midi::Service::start(&state, dir.join("midi-profiles.json")));
            app.manage(state.clone());
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Show SkyDek DJ", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit SkyDek DJ", true, None::<&str>)?;
            let mut tray = TrayIconBuilder::new();
            #[cfg(windows)]
            {
                tray = tray.icon(tauri::image::Image::from_app_icon_resource(32)?);
            }
            tray.tooltip("SkyDek DJ")
                .menu(&Menu::with_items(app, &[&show, &exit])?)
                .on_menu_event(|a, e| match e.id.as_ref() {
                    "show" => {
                        if let Some(w) = a.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "exit" => {
                        let s = a.state::<Arc<AppState>>();
                        if s.recorder.is_open() {
                            rfd::MessageDialog::new()
                                .set_title("Recording is active")
                                .set_description(
                                    "Stop and finalize the recording in SkyDek DJ before exiting.",
                                )
                                .show();
                            return;
                        }
                        let _ = s.recorder.stop();
                        if let Err(error) = s.save() {
                            rfd::MessageDialog::new()
                                .set_title("Could not save session")
                                .set_description(error)
                                .show();
                            return;
                        }
                        a.exit(0)
                    }
                    _ => {}
                })
                .build(app)?;
            if let Some(w) = app.get_webview_window("main") {
                let copy = w.clone();
                w.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        if let Err(error) = state.save() {
                            *state.error.lock().unwrap() = format!("Could not save session: {error}");
                            return;
                        }
                        let _ = copy.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            midi::midi_devices,
            midi::midi_learn,
            midi::midi_connect,
            midi::midi_disconnect,
            midi::midi_status,
            midi::midi_mappings,
            midi::midi_save_mapping,
            midi::midi_delete_mapping,
            snapshot,
            command,
            import_audio,
            load_track,
            assign_sample,
            add_sample,
            swap_samples,
            clear_sample,
            sampler_mode,
            controller_input,
            audio_devices,
            select_audio_device,
            toggle_fullscreen,
            library_view,
            save_mappings,
            default_mappings,
            mapping_profile,
            import_sound_folder,
            sound_files,
            analyze_library,
            waveform_data,
            record
        ])
        .run(tauri::generate_context!())
        .expect("SkyDek DJ failed to start");
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    #[test]
    fn legacy_session_migrates_without_losing_library() {
        let s: Session = serde_json::from_str(r#"{"library":[],"queue":["kept.mp3"]}"#).unwrap();
        assert_eq!(s.queue, vec!["kept.mp3"]);
        assert_eq!(s.mappings.get(false, "b"), "cue_hold");
        assert!(s.pad_paths.iter().all(Option::is_none));
    }
    #[test]
    fn sound_folder_scan_and_sampler_persistence() {
        let root = std::env::temp_dir().join(format!(
            "skydek-sounds-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        let sound = root.join("nested/beep.wav");
        let mut wav = hound::WavWriter::create(
            &sound,
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..480 {
            wav.write_sample(1000i16).unwrap()
        }
        wav.finalize().unwrap();
        std::fs::write(root.join("ignored.txt"), "not audio").unwrap();
        let files = scan_sounds(&root).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "beep.wav");
        let state = AppState {
            analysis: Mutex::new(analysis::Status::default()),
            cancel_analysis: std::sync::atomic::AtomicBool::new(false),
            model: Mutex::new(Model {
                auto_dj: auto_dj::State::default(),
                session: Session::default(),
                cache: HashMap::new(),
                loaded: [None, None],
                pending_loads: [None, None],
                library_revision: 0,
                selected: 0,
                target: None,
                shift: false,
                sampler: 0,
                samples: vec![None; sampler::SAMPLER_SLOTS],
                sound_revision: 0,
                last: String::new(),
                connected: false,
                battery: None,
                view: View::default(),
                tab: "library".into(),
                library_query: String::new(),
            }),
            commands: Arc::new(ArrayQueue::new(256)),
            views: Arc::new(ArrayQueue::new(16)),
            error: Arc::new(Mutex::new(String::new())),
            path: root.join("session.json"),
            device: String::new(),
            rate: 48000,
            recorder: recording::Recorder::new(),
            save_guard: Mutex::new(()),
            sample_guard: Mutex::new(()),
        };
        state.assign(3, sound.to_string_lossy().into()).unwrap();
        let mut custom = mapping::Mappings::default();
        custom.normal.insert("a".into(), "sample".into());
        state
            .profile(
                "save".into(),
                "Original".into(),
                Some(mapping::Mappings::default()),
            )
            .unwrap();
        state
            .profile("save".into(), "Samples".into(), Some(custom))
            .unwrap();
        assert_eq!(
            state.model.lock().unwrap().session.mappings.get(false, "a"),
            "sample"
        );
        let saved: Session = serde_json::from_slice(&std::fs::read(&state.path).unwrap()).unwrap();
        assert_eq!(saved.preferences.active_profile, "Samples");
        assert_eq!(saved.preferences.profiles.len(), 2);
        state
            .profile("select".into(), "Original".into(), None)
            .unwrap();
        assert_eq!(
            state.model.lock().unwrap().session.mappings.get(false, "a"),
            "load_selected"
        );
        state
            .profile("delete".into(), "Samples".into(), None)
            .unwrap();
        assert!(state
            .profile("delete".into(), "Original".into(), None)
            .is_err());
        state.action("focus_library", None, None, None).unwrap();
        state.action("panel_left", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().tab, "folders");
        state.action("panel_right", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().tab, "library");
        state.action("panel_right", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().tab, "sampler");
        assert!(matches!(state.commands.pop(), Some(Cmd::Sampler(3, _))));
        let persisted: Session =
            serde_json::from_slice(&std::fs::read(&state.path).unwrap()).unwrap();
        assert_eq!(persisted.pad_paths[3].as_deref(), sound.to_str());
        assert!(state.assign(sampler::SAMPLER_SLOTS, sound.to_string_lossy().into()).is_err());
        state.action("sample", None, None, Some(3)).unwrap();
        assert!(matches!(
            state.commands.pop(),
            Some(Cmd::SamplePress(3, crate::sampler::PlayMode::Restart))
        ));
        assert_eq!(
            state
                .assign_slot(None, sound.to_string_lossy().into())
                .unwrap(),
            0
        );
        assert_eq!(
            state
                .assign_slot(None, sound.to_string_lossy().into())
                .unwrap(),
            1
        );
        while state.commands.pop().is_some() {}
        state.action("focus_library", None, None, None).unwrap();
        state.action("focus_sampler", None, None, None).unwrap();
        state.action("sample_select", None, None, Some(1)).unwrap();
        state.action("browse", None, Some(1.), None).unwrap();
        assert_eq!(state.model.lock().unwrap().sampler, 5);
        state.action("panel_right", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().sampler, 6);
        state.action("panel_left", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().sampler, 5);
        state.action("sample_select", None, None, Some(4)).unwrap();
        state.action("panel_left", None, None, None).unwrap();
        assert_eq!(state.model.lock().unwrap().tab, "library");
        state.action("sample_select", None, None, Some(3)).unwrap();
        state.swap(3, 2).unwrap();
        assert!(matches!(state.commands.pop(), Some(Cmd::SwapSamples(3, 2))));
        {
            let m = state.model.lock().unwrap();
            assert!(m.samples[3].is_none());
            assert!(m.samples[2].is_some());
            assert_eq!(m.sampler, 2);
        }
        state.action("sample", None, None, None).unwrap();
        assert!(matches!(
            state.commands.pop(),
            Some(Cmd::SamplePress(2, crate::sampler::PlayMode::Restart))
        ));
        let persisted: Session =
            serde_json::from_slice(&std::fs::read(&state.path).unwrap()).unwrap();
        assert!(persisted.pad_paths[3].is_none());
        assert!(persisted.pad_paths[2].is_some());
        for _ in 0..45 {
            state
                .assign_slot(None, sound.to_string_lossy().into())
                .unwrap();
        }
        assert!(state
            .assign_slot(None, sound.to_string_lossy().into())
            .is_err());
        while state.commands.pop().is_some() {}
        state
            .set_sampler_mode(2, sampler::PlayMode::Toggle, false)
            .unwrap();
        assert!(matches!(state.commands.pop(), Some(Cmd::StopSample(2))));
        assert_eq!(
            state.sample_press(Some(2)).unwrap(),
            (2, sampler::PlayMode::Toggle)
        );
        assert!(matches!(
            state.commands.pop(),
            Some(Cmd::SamplePress(2, sampler::PlayMode::Toggle))
        ));
        let saved: Session = serde_json::from_slice(&std::fs::read(&state.path).unwrap()).unwrap();
        assert_eq!(
            saved.preferences.sampler_modes[2],
            sampler::PlayMode::Toggle
        );
        state.swap(2, 3).unwrap();
        assert_eq!(
            state
                .model
                .lock()
                .unwrap()
                .session
                .preferences
                .sampler_modes[3],
            sampler::PlayMode::Toggle
        );
        state
            .set_sampler_mode(0, sampler::PlayMode::Overlap, true)
            .unwrap();
        assert_eq!(
            state
                .model
                .lock()
                .unwrap()
                .session
                .preferences
                .sampler_modes,
            [sampler::PlayMode::Overlap; sampler::SAMPLER_SLOTS]
        );
        assert!(state
            .set_sampler_mode(16, sampler::PlayMode::Hold, false)
            .is_err());
        {
            let mut m = state.model.lock().unwrap();
            m.samples[3] = None;
            m.sampler = 1;
            m.tab = "folders".into();
        }
        state
            .assign_slot_with_focus(Some(3), sound.to_string_lossy().into(), false)
            .unwrap();
        {
            let m = state.model.lock().unwrap();
            assert_eq!(m.sampler, 1);
            assert_eq!(m.tab, "folders");
            assert!(m.samples[3].is_some());
        }
        // Exercise the same command used by mouse Sync and mapped Shift+X.
        while state.commands.pop().is_some() {}
        {
            let mut m = state.model.lock().unwrap();
            m.loaded = [Some("tempo-a.wav".into()), Some("tempo-b.wav".into())];
            m.session.library.extend([
                TrackInfo {
                    path: "tempo-a.wav".into(),
                    bpm: Some(120.),
                    ..Default::default()
                },
                TrackInfo {
                    path: "tempo-b.wav".into(),
                    bpm: Some(130.),
                    ..Default::default()
                },
            ]);
            m.view.decks[1].rate = 1.1;
        }
        state.action("tempo_match", Some(0), None, None).unwrap();
        assert!(matches!(state.commands.pop(),Some(Cmd::Rate(0, r)) if (r-143./120.).abs()<1e-9));
        {
            let mut m = state.model.lock().unwrap();
            let t = m
                .session
                .library
                .iter_mut()
                .find(|t| t.path == "tempo-a.wav")
                .unwrap();
            t.bpm = Some(40.);
        }
        assert!(state.action("tempo_match", Some(0), None, None).is_err());
        // Optional local acceptance fixture: exercises the real sampler assignment
        // path without bundling or changing the user's audio file.
        if let Ok(sample_path) = std::env::var("SKYDEK_TEST_SAMPLE") {
            state.assign(15, sample_path.clone()).unwrap();
            let m = state.model.lock().unwrap();
            assert_eq!(m.samples[15].as_deref(), Some(sample_path.as_str()));
            assert!(!m.cache[&sample_path].samples.is_empty());
        }
        {
            let mut m = state.model.lock().unwrap();
            m.session
                .library
                .iter_mut()
                .find(|t| t.path == "tempo-a.wav")
                .unwrap()
                .bpm = None;
        }
        assert!(state.action("tempo_match", Some(0), None, None).is_err());
        std::fs::remove_file(root.join("session.json")).unwrap();
        std::fs::remove_file(root.join("session.backup.json")).unwrap();
        std::fs::remove_file(root.join("ignored.txt")).unwrap();
        std::fs::remove_file(&sound).unwrap();
        std::fs::remove_dir(root.join("nested")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
