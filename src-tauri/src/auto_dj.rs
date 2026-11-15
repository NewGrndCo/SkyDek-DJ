use super::{audio, AppState, Cmd, Model, Track};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct State {
    pub enabled: bool,
    pub status: String,
    pub pending: Option<Pending>,
    pub samples_left: u8,
    pub next_sample_at: Option<Instant>,
    pub sample_deadline: Option<Instant>,
}

pub(super) struct Pending {
    deck: usize,
    path: String,
    track: Arc<Track>,
    sent: Instant,
}

#[derive(Clone, PartialEq)]
struct Source {
    deck: usize,
    path: String,
    ids: [usize; 2],
    kind: String,
}

struct Prepared {
    path: String,
    title: String,
    track: Arc<Track>,
}

pub(super) fn acknowledge(m: &mut Model) {
    let Some(pending) = &m.auto_dj.pending else {
        return;
    };
    let accepted = m.view.decks[pending.deck].track_id == Arc::as_ptr(&pending.track) as usize;
    if !accepted && pending.sent.elapsed() < Duration::from_secs(1) {
        return;
    }
    let pending = m.auto_dj.pending.take().unwrap();
    if accepted {
        m.cache.retain(|path, _| {
            m.loaded
                .iter()
                .chain(m.samples.iter())
                .flatten()
                .any(|p| p == path)
        });
        m.cache.insert(pending.path.clone(), pending.track);
        m.loaded[pending.deck] = Some(pending.path.clone());
        if m.session.preferences.auto_dj_source == "queue" {
            if let Some(at) = m
                .session
                .queue
                .iter()
                .position(|path| path == &pending.path)
            {
                m.session.queue.remove(at);
            }
        }
        m.sync_restore[pending.deck] = None;
        m.last = format!("Auto DJ → Deck {}", pending.deck + 1);
    }
}

fn source(m: &Model, previous: Option<&Source>) -> Option<Source> {
    if !m.auto_dj.enabled
        || m.auto_dj.pending.is_some()
        || m.pending_loads.iter().any(Option::is_some)
    {
        return None;
    }
    let [a, b] = m.view.decks;
    let deck = match (a.playing, b.playing) {
        (true, false) => 0,
        (false, true) => 1,
        (false, false) => {
            let previous = previous?;
            let old = m.view.decks[previous.deck];
            if old.track_id != previous.ids[previous.deck] || old.position < old.duration {
                return None;
            }
            previous.deck
        }
        (true, true) => return None,
    };
    let current = m.view.decks[deck];
    if current.looping || current.cue_held || (1..=3).contains(&current.effect) {
        return None;
    }
    let path = m.loaded[deck].as_ref()?;
    let kind = m.session.preferences.auto_dj_source.clone();
    let ids = m.view.decks.map(|d| d.track_id);
    for (i, loaded) in m.loaded.iter().enumerate() {
        let id = loaded
            .as_ref()
            .and_then(|p| m.cache.get(p))
            .map_or(0, |t| Arc::as_ptr(t) as usize);
        if id != ids[i] {
            return None;
        }
    }
    Some(Source {
        deck,
        path: path.clone(),
        ids,
        kind,
    })
}

fn folder_candidates<'a>(
    library: &'a [audio::TrackInfo],
    path: &str,
) -> Option<impl Iterator<Item = &'a audio::TrackInfo>> {
    let parent = Path::new(path).parent()?.to_str()?.to_owned();
    let index = library.iter().position(|t| t.path == path)?;
    Some(
        library[index + 1..]
            .iter()
            .chain(library[..index].iter())
            .filter(move |t| crate::navigation::in_folder(&t.path, &parent)),
    )
}

fn prepare(s: &AppState, source: &Source) -> Option<Prepared> {
    let candidates = {
        let m = s.model.lock().unwrap();
        let tracks: Vec<_> = if source.kind == "queue" {
            let current = m.session.queue.iter().position(|p| p == &source.path);
            m.session
                .queue
                .iter()
                .enumerate()
                .filter(|(i, _)| Some(*i) != current)
                .filter_map(|(_, path)| m.session.library.iter().find(|track| &track.path == path))
                .collect()
        } else {
            folder_candidates(&m.session.library, &source.path)?.collect()
        };
        tracks
            .into_iter()
            .map(|t| (t.path.clone(), t.title.clone()))
            .collect::<Vec<_>>()
    };
    for (path, title) in candidates {
        if self::source(&s.model.lock().unwrap(), Some(source)).as_ref() != Some(source) {
            break;
        }
        if let Ok((track, _)) = audio::decode(&path) {
            return Some(Prepared { path, title, track });
        }
    }
    None
}

pub(super) fn start(s: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut previous = None;
        let mut next = None;
        loop {
            std::thread::sleep(Duration::from_millis(40));
            {
                let mut m = s.model.lock().unwrap();
                if m.auto_dj
                    .sample_deadline
                    .is_some_and(|end| Instant::now() >= end)
                {
                    m.auto_dj.samples_left = 0;
                    m.auto_dj.next_sample_at = None;
                    m.auto_dj.sample_deadline = None;
                }
                if m.auto_dj.enabled
                    && m.auto_dj.samples_left > 0
                    && m.auto_dj
                        .next_sample_at
                        .is_some_and(|at| Instant::now() >= at)
                {
                    let available: Vec<usize> = m
                        .samples
                        .iter()
                        .enumerate()
                        .filter_map(|(index, sample)| sample.as_ref().map(|_| index))
                        .collect();
                    if !available.is_empty() {
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .subsec_nanos() as usize;
                        let slot =
                            available[(seed + m.auto_dj.samples_left as usize) % available.len()];
                        let _ = s.send(Cmd::SamplePress(slot, crate::sampler::PlayMode::Restart));
                    }
                    m.auto_dj.samples_left -= 1;
                    m.auto_dj.next_sample_at = if m.auto_dj.samples_left > 0 {
                        Some(
                            Instant::now()
                                + Duration::from_secs_f64(
                                    m.session.preferences.auto_dj_sample_spacing,
                                ),
                        )
                    } else {
                        None
                    };
                }
            }
            let current = source(&s.model.lock().unwrap(), previous.as_ref());
            let Some(current) = current else {
                let mut m = s.model.lock().unwrap();
                if m.auto_dj.enabled && m.auto_dj.pending.is_none() {
                    m.auto_dj.status = if m.view.decks.iter().all(|d| d.playing) {
                        "Both decks playing".into()
                    } else {
                        "Waiting for one playing deck".into()
                    };
                }
                previous = None;
                next = None;
                continue;
            };
            if previous.as_ref() != Some(&current) {
                next = prepare(&s, &current);
                previous = Some(current.clone());
            }
            let mut m = s.model.lock().unwrap();
            if source(&m, Some(&current)).as_ref() != Some(&current) {
                continue;
            }
            let Some(prepared) = &next else {
                m.auto_dj.status = "No other playable tracks in this folder".into();
                continue;
            };
            m.auto_dj.status = format!("Next: {}", prepared.title);
            let deck = m.view.decks[current.deck];
            let remaining = (deck.duration - deck.position) / deck.rate.max(0.01);
            let incoming_seconds = prepared.track.samples.len() as f64 / prepared.track.rate as f64;
            let lead = m
                .session
                .preferences
                .auto_dj_seconds
                .max(m.session.preferences.auto_dj_crossfade_seconds)
                .min(deck.duration / deck.rate.max(0.01) / 2.)
                .min(incoming_seconds / 2.);
            if remaining > lead {
                continue;
            }
            let seconds = m
                .session
                .preferences
                .auto_dj_crossfade_seconds
                .min(remaining.max(0.05));
            let (hot, cue) = m
                .session
                .library
                .iter()
                .find(|t| t.path == prepared.path)
                .map_or(([None; 8], 0.), |t| (t.cues, t.main_cue));
            if s.send(Cmd::AutoMix {
                from: current.deck,
                expected: current.ids,
                track: prepared.track.clone(),
                hot,
                cue,
                seconds,
            })
            .is_ok()
            {
                m.auto_dj.samples_left = if m.session.preferences.auto_dj_random_samples {
                    m.session.preferences.auto_dj_sample_count.min(
                        (seconds / m.session.preferences.auto_dj_sample_spacing)
                            .ceil()
                            .max(1.) as u8,
                    )
                } else {
                    0
                };
                m.auto_dj.next_sample_at = (m.auto_dj.samples_left > 0).then(Instant::now);
                m.auto_dj.sample_deadline = (m.auto_dj.samples_left > 0)
                    .then(|| Instant::now() + Duration::from_secs_f64(seconds));
                m.auto_dj.pending = Some(Pending {
                    deck: 1 - current.deck,
                    path: prepared.path.clone(),
                    track: prepared.track.clone(),
                    sent: Instant::now(),
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_order_wraps_and_ignores_other_folders() {
        let library = [
            "C:/Music/Set/first.wav",
            "C:/Music/Set/current.wav",
            "C:/Music/Other/track.wav",
            "C:/Music/Set/Subfolder/track.wav",
            "C:/Music/Set/last.wav",
        ]
        .map(|path| audio::TrackInfo {
            path: path.into(),
            ..Default::default()
        });
        let paths: Vec<_> = folder_candidates(&library, &library[1].path)
            .unwrap()
            .map(|track| track.path.as_str())
            .collect();
        assert_eq!(paths, [&library[4].path, &library[0].path]);
    }
}
