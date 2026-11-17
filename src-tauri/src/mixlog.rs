use crate::audio::View;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub struct DeckFrame {
    pub path: Option<String>,
    pub position: f64,
    pub playing: bool,
    pub rate: f64,
    pub volume: f32,
    pub filter: f32,
    pub effect: u8,
    pub wet: f32,
    pub peak: f32,
    pub cue: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Frame {
    pub seconds: f64,
    pub decks: [DeckFrame; 2],
    pub crossfader: f32,
    pub master: f32,
    pub sampler_volume: f32,
    pub samples: Vec<String>,
    #[serde(default)]
    pub mic: Option<MicState>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MicState {
    pub volume: f32,
    pub mute: bool,
    pub ducking: bool,
}
impl Default for MicState {
    fn default() -> Self { Self { volume: 0.8, mute: false, ducking: false } }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub seconds: f64,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SampleCommand {
    pub seconds: f64,
    pub path: String,
    pub action: String,
    pub mode: crate::sampler::PlayMode,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct EffectCommand {
    pub seconds: f64,
    pub deck: usize,
    pub effect: u8,
    pub active: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MicCommand {
    pub seconds: f64,
    pub state: MicState,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MixLog {
    pub version: u8,
    #[serde(default)]
    pub capture_revision: u8,
    pub audio_file: String,
    pub frames: Vec<Frame>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub sample_commands: Vec<SampleCommand>,
    #[serde(default)]
    pub effect_commands: Vec<EffectCommand>,
    #[serde(default)]
    pub mic_file: Option<String>,
    #[serde(default)]
    pub mic_commands: Vec<MicCommand>,
}
#[derive(Default)]
pub struct Manager {
    pub active: Option<MixLog>,
    pub recent: Vec<Event>,
    pub last_path: String,
}
impl Manager {
    pub fn start(&mut self, path: String) {
        self.recent.clear();
        self.active = Some(MixLog {
            version: 1,
            capture_revision: 1,
            audio_file: path,
            frames: Vec::new(),
            events: vec![Event {
                seconds: 0.,
                label: "Recording started".into(),
            }],
            sample_commands: Vec::new(),
            effect_commands: Vec::new(),
            mic_file: None,
            mic_commands: Vec::new(),
        });
    }
    pub fn record_sample(&mut self, seconds: f64, path: String, action: &str, mode: crate::sampler::PlayMode) {
        if let Some(log) = self.active.as_mut() {
            if seconds.is_finite() && seconds >= 0. && log.sample_commands.len() < 500_000 {
                log.sample_commands.push(SampleCommand { seconds, path, action: action.into(), mode });
            }
        }
    }
    pub fn record_effect(&mut self, seconds: f64, deck: usize, effect: u8, active: bool) {
        if let Some(log) = self.active.as_mut() {
            if seconds.is_finite() && seconds >= 0. && log.effect_commands.len() < 500_000 {
                log.effect_commands.push(EffectCommand { seconds, deck, effect, active });
            }
        }
    }
    pub fn record_mic(&mut self, seconds: f64, state: MicState) {
        if let Some(log) = self.active.as_mut() {
            if seconds.is_finite() && seconds >= 0. && log.mic_commands.len() < 500_000 {
                log.mic_commands.push(MicCommand { seconds, state });
            }
        }
    }
    pub fn capture(
        &mut self,
        seconds: f64,
        view: &View,
        paths: &[Option<String>; 2],
        samples: &[Option<String>],
        mic: MicState,
    ) {
        let Some(log) = self.active.as_mut() else {
            return;
        };
        if !seconds.is_finite() || seconds < 0. || log.frames.len() >= 144_000 {
            return;
        }
        if log.frames.last().is_some_and(|f| seconds - f.seconds < 0.1) {
            return;
        }
        let frame = Frame {
            seconds,
            decks: std::array::from_fn(|i| {
                let d = view.decks[i];
                DeckFrame {
                    path: paths[i].clone(),
                    position: d.position,
                    playing: d.playing,
                    rate: d.rate,
                    volume: d.volume,
                    filter: d.filter,
                    effect: d.effect,
                    wet: d.wet,
                    peak: d.peak,
                    cue: d.cue,
                }
            }),
            crossfader: view.crossfader,
            master: view.master,
            sampler_volume: view.sampler_volume,
            samples: view
                .sampler_voices
                .iter()
                .enumerate()
                .filter_map(|(i, voices)| {
                    (*voices > 0)
                        .then(|| samples.get(i).and_then(Option::as_ref).cloned())
                        .flatten()
                })
                .collect(),
            mic: Some(mic),
        };
        let previous = log.frames.last();
        let mut labels = Vec::new();
        if let Some(old) = previous {
            for i in 0..2 {
                let a = &old.decks[i];
                let b = &frame.decks[i];
                if a.path != b.path {
                    if let Some(path) = &b.path {
                        labels.push(format!("DEK {} loaded {}", i + 1, file_name(path)));
                    }
                }
                if a.playing != b.playing {
                    labels.push(format!(
                        "DEK {} {}",
                        i + 1,
                        if b.playing { "play" } else { "pause" }
                    ));
                }
                if a.effect != b.effect {
                    labels.push(format!("DEK {} effect {}", i + 1, effect_name(b.effect)));
                }
                if (a.filter - b.filter).abs() >= 0.1 {
                    labels.push(format!("DEK {} filter {:.0}%", i + 1, b.filter * 100.));
                }
            }
            if (old.crossfader - frame.crossfader).abs() >= 0.1 {
                labels.push(format!("Crossfader {:.0}%", frame.crossfader * 100.));
            }
            for sample in &frame.samples {
                if !old.samples.contains(sample) {
                    labels.push(format!("Sample {}", file_name(sample)));
                }
            }
        } else {
            for (i, deck) in frame.decks.iter().enumerate() {
                if let Some(path) = &deck.path {
                    labels.push(format!("DEK {} loaded {}", i + 1, file_name(path)));
                }
            }
        }
        for label in labels {
            let event = Event { seconds, label };
            log.events.push(event.clone());
            self.recent.push(event);
        }
        if self.recent.len() > 40 {
            self.recent.drain(..self.recent.len() - 40);
        }
        log.frames.push(frame);
    }
    pub fn finish(&mut self) -> Result<String, String> {
        let mut log = self.active.take().ok_or("No mix log is active")?;
        let seconds = log.frames.last().map_or(0., |f| f.seconds);
        log.events.push(Event {
            seconds,
            label: "Recording stopped".into(),
        });
        let path = Path::new(&log.audio_file).with_extension("skydek.json");
        let temporary = path.with_extension("skydek.tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec(&log).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        self.last_path = path.to_string_lossy().into_owned();
        Ok(self.last_path.clone())
    }
}
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
fn effect_name(effect: u8) -> &'static str {
    match effect {
        1 => "backspin",
        2 => "brake",
        3 => "start",
        4 => "echo",
        5 => "reverb",
        _ => "off",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precise_commands_preserve_rapid_changes_and_legacy_logs_still_load() {
        let mut manager = Manager::default();
        manager.start("mix.wav".into());
        manager.record_sample(0.01, "horn.wav".into(), "press", crate::sampler::PlayMode::Restart);
        manager.record_sample(0.04, "horn.wav".into(), "press", crate::sampler::PlayMode::Restart);
        manager.record_effect(0.03, 0, 4, true);
        manager.record_effect(0.06, 0, 4, false);
        let log = manager.active.as_ref().unwrap();
        assert_eq!(log.sample_commands.len(), 2);
        assert_eq!(log.effect_commands.len(), 2);
        assert_eq!(log.sample_commands[1].seconds, 0.04);
        let mut legacy = serde_json::to_value(log).unwrap();
        for field in ["sample_commands", "effect_commands", "mic_commands", "mic_file", "capture_revision"] { legacy.as_object_mut().unwrap().remove(field); }
        let restored: MixLog = serde_json::from_value(legacy).unwrap();
        assert!(restored.sample_commands.is_empty());
        assert!(restored.mic_file.is_none());
        assert_eq!(restored.capture_revision, 0);
    }
    #[test]
    fn captures_track_and_sample_changes() {
        let mut manager = Manager::default();
        manager.start("mix.wav".into());
        let mut view = View::default();
        manager.capture(0., &view, &[Some("song.wav".into()), None], &vec![None; 48], MicState::default());
        view.decks[0].playing = true;
        view.sampler_voices[47] = 1;
        let mut samples = vec![None; 48];
        samples[47] = Some("airhorn.wav".into());
        manager.capture(0.2, &view, &[Some("song.wav".into()), None], &samples, MicState::default());
        assert_eq!(manager.active.as_ref().unwrap().frames.len(), 2);
        assert!(manager.recent.iter().any(|e| e.label.contains("airhorn")));
    }
}
