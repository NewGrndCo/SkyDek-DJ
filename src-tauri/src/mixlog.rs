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
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub seconds: f64,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MixLog {
    pub version: u8,
    pub audio_file: String,
    pub frames: Vec<Frame>,
    pub events: Vec<Event>,
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
            audio_file: path,
            frames: Vec::new(),
            events: vec![Event {
                seconds: 0.,
                label: "Recording started".into(),
            }],
        });
    }
    pub fn capture(
        &mut self,
        seconds: f64,
        view: &View,
        paths: &[Option<String>; 2],
        samples: &[Option<String>],
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
    fn captures_track_and_sample_changes() {
        let mut manager = Manager::default();
        manager.start("mix.wav".into());
        let mut view = View::default();
        manager.capture(0., &view, &[Some("song.wav".into()), None], &vec![None; 48]);
        view.decks[0].playing = true;
        view.sampler_voices[47] = 1;
        let mut samples = vec![None; 48];
        samples[47] = Some("airhorn.wav".into());
        manager.capture(0.2, &view, &[Some("song.wav".into()), None], &samples);
        assert_eq!(manager.active.as_ref().unwrap().frames.len(), 2);
        assert!(manager.recent.iter().any(|e| e.label.contains("airhorn")));
    }
}
