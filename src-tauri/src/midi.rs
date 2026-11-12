//! MIDI notes and absolute 7-bit CC. No hardware presets, HID or relative jog.
//! API reference: https://docs.rs/midir/0.11.1/midir/struct.MidiInput.html
use super::AppState;
use crossbeam_queue::ArrayQueue;
use midir::{MidiInput as Input, MidiInputConnection};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Weak,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Note,
    Cc,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Button,
    Absolute,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub id: String,
    pub kind: Kind,
    pub channel: u8,
    pub number: u8,
    pub mode: Mode,
    pub action: String,
    pub deck: usize,
    #[serde(default)]
    pub index: Option<usize>,
}
const BUTTONS: &[&str] = &[
    "none",
    "play",
    "set_cue",
    "load_deck",
    "browse_up",
    "browse_down",
    "queue_add",
    "panel",
    "tempo_match",
    "auto_cross",
    "cue_hold",
    "backspin",
    "brake",
    "start",
    "echo",
    "reverb",
    "sample",
    "next_sample",
    "stop_samples",
];
const ABSOLUTE: &[&str] = &["cross", "rate", "filter", "wet", "volume", "master"];
impl Mapping {
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err("Mapping ID must contain 1..128 bytes".into());
        }
        if self.channel > 15 || self.number > 127 || self.deck > 1 {
            return Err("Invalid MIDI channel, number or deck (deck must be 0 or 1)".into());
        }
        if self.kind == Kind::Note && self.mode != Mode::Button {
            return Err("Notes require button mode".into());
        }
        let allowed = if self.mode == Mode::Absolute {
            ABSOLUTE
        } else {
            BUTTONS
        };
        if !allowed.contains(&self.action.as_str()) {
            return Err("Unsupported MIDI action for this mode".into());
        }
        if self
            .index
            .is_some_and(|i| self.action != "sample" || i > 15)
        {
            return Err("Only sample accepts index (0..15)".into());
        }
        Ok(())
    }
    fn matches(&self, event: &Event) -> bool {
        self.kind == event.kind && self.channel == event.channel && self.number == event.number
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub kind: Kind,
    pub channel: u8,
    pub number: u8,
    pub value: u8,
    pub timestamp: u64,
}
fn parse(timestamp: u64, bytes: &[u8]) -> Option<Event> {
    if bytes.len() != 3 || bytes[1] > 127 || bytes[2] > 127 {
        return None;
    }
    let (kind, value) = match bytes[0] & 0xf0 {
        0x80 => (Kind::Note, 0),
        0x90 => (Kind::Note, bytes[2]),
        0xb0 => (Kind::Cc, bytes[2]),
        _ => return None,
    };
    Some(Event {
        kind,
        channel: bytes[0] & 15,
        number: bytes[1],
        value,
        timestamp,
    })
}
fn absolute(action: &str, value: u8) -> f64 {
    let v = f64::from(value) / 127.;
    match action {
        "cross" | "filter" => v * 2. - 1.,
        "rate" => v + 0.5,
        _ => v,
    }
}
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub connected: bool,
    pub device: Option<Device>,
    pub needs_guided_learn: bool,
    pub learning: bool,
    pub latest_input: Option<Event>,
    pub learned_event: Option<Event>,
    pub mappings: Vec<Mapping>,
    pub error: Option<String>,
    pub dropped_messages: u64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profiles {
    devices: BTreeMap<String, Vec<Mapping>>,
}
fn validate_mappings(mappings: &[Mapping]) -> Result<(), String> {
    if mappings.len() > 512 {
        return Err("At most 512 mappings per device".into());
    }
    for (i, mapping) in mappings.iter().enumerate() {
        mapping.validate()?;
        if mappings[..i].iter().any(|other| {
            other.id == mapping.id
                || (other.kind == mapping.kind
                    && other.channel == mapping.channel
                    && other.number == mapping.number)
        }) {
            return Err("Duplicate mapping ID or MIDI control".into());
        }
    }
    Ok(())
}
fn ports() -> Result<(Input, Vec<(Device, midir::MidiInputPort)>), String> {
    let input = Input::new("SkyDek MIDI input").map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for port in input.ports() {
        let name = input.port_name(&port).map_err(|e| e.to_string())?;
        let id = port.id();
        result.push((Device { id, name }, port));
    }
    Ok((input, result))
}
#[derive(Clone)]
struct Release {
    action: &'static str,
    deck: usize,
    index: Option<usize>,
}
impl Release {
    fn run(&self, state: &AppState) -> Result<(), String> {
        state.action(self.action, Some(self.deck), None, self.index)
    }
    fn cancel(mut self) -> Self {
        if self.action == "cue_hold_up" {
            self.action = "cue_hold_cancel";
        }
        self
    }
}
struct Held {
    release: Option<Release>,
}
struct Packet {
    generation: u64,
    event: Event,
}
type Job = Box<dyn FnOnce(&mut Runtime) + Send>;
pub struct Service {
    jobs: mpsc::SyncSender<Job>,
}
struct Runtime {
    app: Weak<AppState>,
    path: PathBuf,
    profiles: Profiles,
    profile_error: Option<String>,
    status: Status,
    connection: Option<MidiInputConnection<()>>,
    generation: u64,
    packets: Arc<ArrayQueue<Packet>>,
    dropped: Arc<AtomicU64>,
    seen_dropped: u64,
    held: HashMap<String, Held>,
    pending_release: Vec<Release>,
    suppressed: HashSet<(u8, u8, u8)>,
    auto_connect: bool,
    preferred_device: Option<String>,
}
impl Service {
    pub fn start(app: &Arc<AppState>, path: PathBuf) -> Self {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(32);
        let app = Arc::downgrade(app);
        std::thread::Builder::new().name("skydek-midi-dispatch".into()).spawn(move || {
            let loaded = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<Profiles>(&bytes).map_err(|e| e.to_string()).and_then(|p| {
                    for mappings in p.devices.values() { validate_mappings(mappings)?; } Ok(p)
                }),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Profiles::default()),
                Err(e) => Err(e.to_string()),
            };
            let (profiles, profile_error) = match loaded { Ok(p) => (p, None), Err(e) => (Profiles::default(), Some(format!("MIDI profiles could not be read; file preserved: {e}"))) };
            let mut runtime = Runtime {
                app, path, profiles, status: Status { error: profile_error.clone(), ..Status::default() }, profile_error,
                connection: None, generation: 0, packets: Arc::new(ArrayQueue::new(512)),
                dropped: Arc::new(AtomicU64::new(0)), seen_dropped: 0, held: HashMap::new(), pending_release: Vec::new(), suppressed: HashSet::new(), auto_connect: true, preferred_device: None,
            };
            runtime.check_presence();
            let mut last_presence = Instant::now();
            loop {
                match receiver.recv_timeout(Duration::from_millis(5)) {
                    Ok(job) => job(&mut runtime),
                    Err(mpsc::RecvTimeoutError::Disconnected) => { runtime.disconnect(); break; },
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                }
                runtime.retry_releases();
                if last_presence.elapsed() >= Duration::from_secs(1) {
                    runtime.check_presence();
                    last_presence = Instant::now();
                }
                let dropped = runtime.dropped.load(Ordering::Relaxed);
                if dropped != runtime.seen_dropped {
                    runtime.auto_connect = false;
                    runtime.disconnect();
                    runtime.status.error = Some("MIDI queue overflow: released held controls and disconnected. Reconnect manually.".into());
                    runtime.seen_dropped = dropped;
                }
                runtime.status.dropped_messages = dropped;
                for _ in 0..128 {
                    let Some(packet) = runtime.packets.pop() else { break; };
                    if packet.generation == runtime.generation && runtime.status.connected { runtime.receive(packet.event); }
                }
                if runtime.app.strong_count() == 0 { break; }
            }
        }).expect("MIDI dispatch thread could not start");
        Self { jobs }
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let jobs = self.jobs.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (tx, rx) = mpsc::sync_channel(1);
            jobs.send(Box::new(move |runtime| {
                let _ = tx.send(f(runtime));
            }))
            .map_err(|_| "MIDI worker stopped".to_string())?;
            rx.recv().map_err(|_| "MIDI worker stopped".to_string())?
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
impl Runtime {
    fn retry_releases(&mut self) {
        if let Some(app) = self.app.upgrade() {
            self.pending_release.retain(|r| r.run(&app).is_err());
        }
    }
    fn release_all(&mut self) {
        for mapping in &self.status.mappings {
            if self.held.contains_key(&mapping.id) {
                self.suppressed.insert((
                    if mapping.kind == Kind::Note { 0 } else { 1 },
                    mapping.channel,
                    mapping.number,
                ));
            }
        }
        for (_, held) in self.held.drain() {
            if let Some(release) = held.release {
                self.pending_release.push(release.cancel());
            }
        }
        self.retry_releases();
    }
    fn learn(&mut self, armed: bool) -> Result<Status, String> {
        if !self.status.connected {
            return Err("Connect a MIDI device first".into());
        }
        self.release_all();
        while self.packets.pop().is_some() {}
        self.status.learning = armed;
        self.status.latest_input = None;
        if armed {
            self.status.learned_event = None;
        }
        Ok(self.status.clone())
    }
    fn check_presence(&mut self) {
        let Some(device) = self.status.device.clone() else {
            if !self.auto_connect || self.profile_error.is_some() {
                return;
            }
            if let Ok((_, devices)) = ports() {
                let candidates: Vec<_> = devices
                    .into_iter()
                    .map(|(d, _)| d)
                    .filter(|d| {
                        self.profiles
                            .devices
                            .get(&d.id)
                            .is_some_and(|m| !m.is_empty())
                            && self.preferred_device.as_ref().is_none_or(|id| *id == d.id)
                    })
                    .collect();
                if candidates.len() == 1 {
                    if let Err(error) = self.connect(candidates[0].id.clone()) {
                        self.status.error = Some(error);
                    }
                }
            }
            return;
        };
        let error = match ports() {
            Ok((_, ports)) if ports.iter().any(|(d, _)| d == &device) => return,
            Ok(_) => "MIDI device unplugged. Waiting for the same saved device; manual reconnect is also available.".to_string(),
            Err(e) => format!("MIDI device presence check failed: {e}. Reconnect manually."),
        };
        self.disconnect();
        self.status.error = Some(error);
    }
    fn disconnect(&mut self) {
        self.connection.take();
        self.generation = self.generation.wrapping_add(1);
        while self.packets.pop().is_some() {}
        self.release_all();
        self.status.connected = false;
        self.status.device = None;
        self.status.learning = false;
        self.status.latest_input = None;
        self.status.learned_event = None;
        self.status.mappings.clear();
        self.status.needs_guided_learn = false;
        self.suppressed.clear();
    }
    fn connect(&mut self, id: String) -> Result<Status, String> {
        let (input, devices) = ports()?;
        let (device, port) = devices
            .into_iter()
            .find(|(device, _)| device.id == id)
            .ok_or("MIDI device unavailable; refresh the device list")?;
        self.disconnect();
        let packets = self.packets.clone();
        let dropped = self.dropped.clone();
        let generation = self.generation;
        let connection = input
            .connect(
                &port,
                "SkyDek MIDI",
                move |timestamp, bytes, _| {
                    // Bounded, allocation-free callback; no AppState locks, disk I/O or audio decode.
                    if let Some(event) = parse(timestamp, bytes) {
                        if packets.push(Packet { generation, event }).is_err() {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        self.connection = Some(connection);
        self.status.mappings = self
            .profiles
            .devices
            .get(&device.id)
            .cloned()
            .unwrap_or_default();
        self.status.needs_guided_learn = self.status.mappings.is_empty();
        self.status.device = Some(device);
        self.preferred_device = self.status.device.as_ref().map(|d| d.id.clone());
        self.auto_connect = true;
        self.status.connected = true;
        self.status.error = self.profile_error.clone();
        Ok(self.status.clone())
    }
    fn persist(&mut self, mappings: Vec<Mapping>) -> Result<(), String> {
        validate_mappings(&mappings)?;
        if let Some(error) = &self.profile_error {
            return Err(error.clone());
        }
        let device = self
            .status
            .device
            .as_ref()
            .ok_or("Connect a MIDI device first")?;
        let mut profiles = self.profiles.clone();
        profiles.devices.insert(device.id.clone(), mappings.clone());
        let bytes = serde_json::to_vec_pretty(&profiles).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("tmp");
        std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|e| e.to_string())?;
        self.release_all();
        self.profiles = profiles;
        self.status.error = self.profile_error.clone();
        self.status.mappings = mappings;
        self.status.needs_guided_learn = self.status.mappings.is_empty();
        Ok(())
    }
    fn receive(&mut self, event: Event) {
        self.status.latest_input = Some(event.clone());
        let key = (
            if event.kind == Kind::Note { 0 } else { 1 },
            event.channel,
            event.number,
        );
        if self.status.learning {
            if event.value > 0 {
                self.suppressed.insert(key);
                if self.status.learned_event.is_none() {
                    self.status.learned_event = Some(event);
                }
            } else {
                self.suppressed.remove(&key);
            }
            return;
        }
        let Some(mapping) = self
            .status
            .mappings
            .iter()
            .find(|m| m.matches(&event))
            .cloned()
        else {
            return;
        };
        let Some(app) = self.app.upgrade() else {
            return;
        };
        if mapping.mode == Mode::Button && self.suppressed.contains(&key) {
            if event.value == 0 {
                self.suppressed.remove(&key);
            }
            return;
        }
        let result = if mapping.mode == Mode::Absolute {
            app.mapped(
                &mapping.action,
                mapping.deck,
                Some(absolute(&mapping.action, event.value)),
            )
        } else if event.value == 0 {
            if let Some(held) = self.held.remove(&mapping.id) {
                if let Some(release) = held.release {
                    self.pending_release.push(release);
                    self.retry_releases();
                }
            }
            Ok(())
        } else if self.held.contains_key(&mapping.id) {
            Ok(())
        } else {
            self.held.insert(mapping.id.clone(), Held { release: None });
            if !self.pending_release.is_empty() {
                self.status.error =
                    Some("Waiting for held MIDI controls to release; retry after release".into());
                return;
            }
            let (result, release) = press(&app, &mapping);
            if let Some(held) = self.held.get_mut(&mapping.id) {
                held.release = release;
            }
            result
        };
        if let Err(error) = result {
            self.status.error = Some(error);
        } else {
            self.status.error = self.profile_error.clone();
        }
    }
}
fn press(app: &AppState, m: &Mapping) -> (Result<(), String>, Option<Release>) {
    let effect = match m.action.as_str() {
        "backspin" => Some(1),
        "brake" => Some(2),
        "start" => Some(3),
        "echo" => Some(4),
        "reverb" => Some(5),
        _ => None,
    };
    if let Some(index) = effect {
        let result = app.action("fx_down", Some(m.deck), None, Some(index));
        let release = result.is_ok().then_some(Release {
            action: "fx_up",
            deck: m.deck,
            index: Some(index),
        });
        return (result, release);
    }
    if m.action == "cue_hold" {
        let result = app.action("cue_hold_down", Some(m.deck), None, None);
        let release = result.is_ok().then_some(Release {
            action: "cue_hold_up",
            deck: m.deck,
            index: None,
        });
        return (result, release);
    }
    if m.action == "sample" {
        return match app.sample_press(m.index) {
            Ok((index, mode)) => (
                Ok(()),
                (mode == super::sampler::PlayMode::Hold).then_some(Release {
                    action: "sample_stop",
                    deck: m.deck,
                    index: Some(index),
                }),
            ),
            Err(error) => (Err(error), None),
        };
    }
    (app.mapped(&m.action, m.deck, None), None)
}

#[tauri::command]
pub async fn midi_devices() -> Result<Vec<Device>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        ports().map(|(_, ports)| ports.into_iter().map(|(device, _)| device).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn midi_connect(
    s: tauri::State<'_, Service>,
    device_id: String,
) -> Result<Status, String> {
    s.call(move |r| r.connect(device_id)).await
}
#[tauri::command]
pub async fn midi_disconnect(s: tauri::State<'_, Service>) -> Result<Status, String> {
    s.call(|r| {
        r.auto_connect = false;
        r.disconnect();
        Ok(r.status.clone())
    })
    .await
}
#[tauri::command]
pub async fn midi_status(s: tauri::State<'_, Service>) -> Result<Status, String> {
    s.call(|r| Ok(r.status.clone())).await
}
#[tauri::command]
pub async fn midi_mappings(s: tauri::State<'_, Service>) -> Result<Vec<Mapping>, String> {
    s.call(|r| Ok(r.status.mappings.clone())).await
}
#[tauri::command]
pub async fn midi_save_mapping(
    s: tauri::State<'_, Service>,
    mapping: Mapping,
) -> Result<Status, String> {
    mapping.validate()?;
    s.call(move |r| {
        let mut mappings = r.status.mappings.clone();
        if let Some(existing) = mappings.iter_mut().find(|m| m.id == mapping.id) {
            *existing = mapping;
        } else {
            mappings.push(mapping);
        }
        r.persist(mappings)?;
        Ok(r.status.clone())
    })
    .await
}
#[tauri::command]
pub async fn midi_delete_mapping(
    s: tauri::State<'_, Service>,
    id: String,
) -> Result<Status, String> {
    s.call(move |r| {
        let mut mappings = r.status.mappings.clone();
        let i = mappings
            .iter()
            .position(|m| m.id == id)
            .ok_or("Mapping not found")?;
        mappings.remove(i);
        r.persist(mappings)?;
        Ok(r.status.clone())
    })
    .await
}
#[tauri::command]
pub async fn midi_learn(s: tauri::State<'_, Service>, armed: bool) -> Result<Status, String> {
    s.call(move |r| r.learn(armed)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Arc<AppState> {
        use super::super::*;
        Arc::new(AppState {
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
                samples: vec![None; crate::sampler::SAMPLER_SLOTS],
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
            path: PathBuf::new(),
            device: String::new(),
            rate: 48000,
            recorder: recording::Recorder::new(),
            save_guard: Mutex::new(()),
            sample_guard: Mutex::new(()),
        })
    }
    fn runtime(app: &Arc<AppState>) -> Runtime {
        Runtime {
            app: Arc::downgrade(app),
            path: PathBuf::new(),
            profiles: Profiles::default(),
            profile_error: None,
            status: Status {
                connected: true,
                device: Some(Device {
                    id: "test-device".into(),
                    name: "Test".into(),
                }),
                ..Status::default()
            },
            connection: None,
            generation: 0,
            packets: Arc::new(ArrayQueue::new(8)),
            dropped: Arc::new(AtomicU64::new(0)),
            seen_dropped: 0,
            held: HashMap::new(),
            pending_release: Vec::new(),
            suppressed: HashSet::new(),
            auto_connect: true,
            preferred_device: None,
        }
    }
    #[test]
    fn edges_release_original_deck_and_disconnect_cancels() {
        use super::super::audio::Cmd;
        let app = app();
        let mut r = runtime(&app);
        let mut m = mapping();
        m.action = "cue_hold".into();
        m.deck = 1;
        r.status.mappings.push(m);
        r.receive(parse(0, &[0x90, 60, 127]).unwrap());
        r.receive(parse(1, &[0x90, 60, 127]).unwrap());
        assert!(matches!(app.commands.pop(), Some(Cmd::CueHold(1, 1))));
        assert!(app.commands.pop().is_none());
        app.model.lock().unwrap().target = Some(0);
        r.receive(parse(2, &[0x90, 60, 0]).unwrap());
        assert!(matches!(app.commands.pop(), Some(Cmd::CueHold(1, 0))));
        r.receive(parse(3, &[0x90, 60, 127]).unwrap());
        app.commands.pop();
        r.disconnect();
        assert!(matches!(app.commands.pop(), Some(Cmd::CueHold(1, 2))));
        assert!(r.held.is_empty());
    }
    #[test]
    fn learn_latches_press_suppresses_actions_and_requires_button_release() {
        let app = app();
        let mut r = runtime(&app);
        let mut m = mapping();
        m.action = "set_cue".into();
        r.status.mappings.push(m);
        r.learn(true).unwrap();
        r.receive(parse(1, &[0x90, 60, 127]).unwrap());
        r.receive(parse(2, &[0x80, 60, 64]).unwrap());
        r.receive(parse(3, &[0x90, 61, 127]).unwrap());
        assert_eq!(r.status.learned_event.as_ref().unwrap().number, 60);
        assert_eq!(r.status.learned_event.as_ref().unwrap().value, 127);
        assert!(app.commands.pop().is_none());
        r.receive(parse(4, &[0x90, 60, 127]).unwrap());
        r.learn(false).unwrap();
        r.receive(parse(5, &[0x90, 60, 127]).unwrap());
        assert!(app.commands.pop().is_none());
        r.receive(parse(6, &[0x90, 60, 0]).unwrap());
        r.receive(parse(7, &[0x90, 60, 127]).unwrap());
        assert!(matches!(
            app.commands.pop(),
            Some(super::super::audio::Cmd::SetCue(0))
        ));
    }
    #[test]
    fn saved_profiles_roundtrip_and_mapping_changes_cancel_hold() {
        let app = app();
        let mut r = runtime(&app);
        let dir = std::env::temp_dir().join(format!(
            "skydek-midi-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        r.path = dir.join("midi.json");
        let mut m = mapping();
        m.action = "cue_hold".into();
        r.persist(vec![m]).unwrap();
        r.receive(parse(0, &[0x90, 60, 127]).unwrap());
        app.commands.pop();
        r.persist(vec![]).unwrap();
        assert!(matches!(
            app.commands.pop(),
            Some(super::super::audio::Cmd::CueHold(0, 2))
        ));
        let profiles: Profiles = serde_json::from_slice(&std::fs::read(&r.path).unwrap()).unwrap();
        assert!(profiles.devices["test-device"].is_empty());
        assert!(!profiles.devices.contains_key("different-device"));
        std::fs::remove_file(&r.path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
    fn mapping() -> Mapping {
        Mapping {
            id: "play-a".into(),
            kind: Kind::Note,
            channel: 0,
            number: 60,
            mode: Mode::Button,
            action: "play".into(),
            deck: 0,
            index: None,
        }
    }
    #[test]
    fn note_release_channel_and_invalid_messages() {
        let event = parse(10, &[0x9f, 60, 100]).unwrap();
        assert_eq!(event.channel, 15);
        assert_eq!(event.value, 100);
        assert_eq!(event.timestamp, 10);
        assert_eq!(parse(0, &[0x90, 60, 0]).unwrap().value, 0);
        assert_eq!(parse(0, &[0x80, 60, 127]).unwrap().value, 0);
        for bytes in [
            &[0x90, 60][..],
            &[0x90, 128, 1],
            &[0x90, 60, 128],
            &[0xe0, 0, 64],
            &[0xf8],
            &[0x10, 0, 0],
        ] {
            assert!(parse(0, bytes).is_none());
        }
        assert_eq!(parse(0, &[0xb3, 1, 127]).unwrap().kind, Kind::Cc);
    }
    #[test]
    fn absolute_ranges() {
        assert_eq!(absolute("cross", 0), -1.);
        assert_eq!(absolute("cross", 127), 1.);
        assert_eq!(absolute("rate", 0), 0.5);
        assert_eq!(absolute("rate", 127), 1.5);
        assert_eq!(absolute("filter", 0), -1.);
        assert_eq!(absolute("filter", 127), 1.);
        for action in ["wet", "volume", "master"] {
            assert_eq!(absolute(action, 0), 0.);
            assert_eq!(absolute(action, 127), 1.);
        }
    }
    #[test]
    fn validate_deck_mode_duplicates_and_indices() {
        let mut m = mapping();
        m.validate().unwrap();
        m.deck = 2;
        assert!(m.validate().is_err());
        m.deck = 1;
        m.channel = 16;
        assert!(m.validate().is_err());
        m.channel = 0;
        m.number = 128;
        assert!(m.validate().is_err());
        m.number = 60;
        m.mode = Mode::Absolute;
        assert!(m.validate().is_err());
        m.kind = Kind::Cc;
        m.action = "rate".into();
        m.validate().unwrap();
        m.action = "jog".into();
        assert!(m.validate().is_err());
        m.mode = Mode::Button;
        m.action = "sample".into();
        m.index = Some(15);
        m.validate().unwrap();
        m.index = Some(16);
        assert!(m.validate().is_err());
        let a = mapping();
        let mut b = a.clone();
        b.id = "other".into();
        assert!(validate_mappings(&[a.clone(), b]).is_err());
        let bytes = serde_json::to_vec(&a).unwrap();
        let restored: Mapping = serde_json::from_slice(&bytes).unwrap();
        restored.validate().unwrap();
    }
}
