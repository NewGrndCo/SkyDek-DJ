use crossbeam_queue::ArrayQueue;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct Track {
    pub samples: Vec<[f32; 2]>,
    pub rate: u32,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct TrackInfo {
    #[serde(default)]
    pub analysis_version: u8,
    #[serde(default)]
    pub analysis_error: Option<String>,
    #[serde(default)]
    pub bpm_confidence: f32,
    pub path: String,
    pub title: String,
    pub duration: f64,
    pub waveform: Vec<f32>,
    pub bpm: Option<f64>,
    pub cues: [Option<f64>; 8],
    #[serde(default)]
    pub main_cue: f64,
}
pub fn decode(path: &str) -> Result<(Arc<Track>, TrackInfo), String> {
    decode_with_limit(path, 30 * 60, 768 * 1024 * 1024)
}

pub fn decode_sample(path: &str) -> Result<(Arc<Track>, TrackInfo), String> {
    decode_with_limit(path, 30 * 60, SAMPLE_BYTES)
}
pub const SAMPLE_BYTES: usize = 32 * 1024 * 1024;

fn decode_with_limit(
    path: &str,
    seconds: usize,
    bytes: usize,
) -> Result<(Arc<Track>, TrackInfo), String> {
    let mut samples = Vec::new();
    let max_frames = bytes / std::mem::size_of::<[f32; 2]>();
    let (_, rate) = visit_audio(path, |frame, rate| {
        if samples.len() >= rate as usize * seconds {
            return Err("Tracks over 30 minutes cannot be loaded in this build".into());
        }
        if samples.len() >= max_frames {
            return Err(format!("Decoded audio exceeds the {} MiB limit for this slot.", bytes / 1024 / 1024));
        }
        if samples.len() == samples.capacity() {
            let capacity = (samples.len() * 2).clamp(4096, max_frames);
            samples.try_reserve_exact(capacity - samples.len())
                .map_err(|_| "Insufficient memory to decode this audio".to_string())?;
        }
        samples.push(frame);
        Ok(())
    })?;
    let waveform = samples
        .chunks((samples.len() / 1400).max(1))
        .map(|c| {
            c.iter()
                .fold(0f32, |a, s| a.max(s[0].abs()).max(s[1].abs()))
        })
        .collect();
    let info = TrackInfo {
        path: path.into(),
        title: std::path::Path::new(path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        duration: samples.len() as f64 / rate as f64,
        waveform,
        ..Default::default()
    };
    Ok((Arc::new(Track { samples, rate }), info))
}

/// Decode into a streaming visitor. Analysis does not retain full-song PCM.
pub fn visit_audio(
    path: &str,
    mut visit: impl FnMut([f32; 2], u32) -> Result<(), String>,
) -> Result<(u64, u32), String> {
    use std::io::{Read, Seek, SeekFrom};
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 64];
    let n = file.read(&mut header).map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&header[..n])
        .unwrap_or("")
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if text.starts_with("<!doctype html") || text.starts_with("<html") {
        return Err("This file contains a web page, not audio. Download the actual audio file and import it again.".into());
    }
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
    {
        hint.with_extension(ext);
    }
    let probe = symphonia::default::get_probe()
        .format(
            &hint,
            MediaSourceStream::new(Box::new(file), Default::default()),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut format = probe.format;
    let t = format.default_track().ok_or("No audio track")?;
    let id = t.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&t.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut rate = 0;
    let mut frames = 0u64;
    let mut consecutive_errors = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            // Demux errors have no packet-local recovery guarantee. In particular,
            // retrying here could spin forever without advancing the stream.
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != id {
            continue;
        }
        // Symphonia documents DecodeError/IoError as packet-local recoverable errors.
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => {
                consecutive_errors = 0;
                decoded
            }
            Err(Error::DecodeError(_) | Error::IoError(_)) => {
                consecutive_errors += 1;
                if consecutive_errors > 128 {
                    return Err("Too many consecutive unreadable audio packets".into());
                }
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        let spec = *decoded.spec();
        if rate != 0 && rate != spec.rate {
            return Err("Sample rate changes inside track are unsupported".into());
        }
        rate = spec.rate;
        let channels = spec.channels.count();
        if channels == 0 {
            return Err("No audio channels".into());
        }
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(channels) {
            visit([frame[0], *frame.get(1).unwrap_or(&frame[0])], rate)?;
            frames += 1;
        }
    }
    if rate == 0 || frames == 0 {
        return Err("Audio file contains no samples".into());
    }
    Ok((frames, rate))
}

#[derive(Clone, Copy, Serialize, Default, Debug)]
pub struct DeckView {
    pub end_events: u64,
    pub previewing: bool,
    pub looping: bool,
    #[serde(skip)]
    pub track_id: usize,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub volume: f32,
    pub filter: f32,
    pub rate: f64,
    pub cue: f64,
    pub cue_held: bool,
    pub cue_elapsed: f64,
    pub effect: u8,
    pub wet: f32,
    pub hot: [Option<f64>; 8],
    pub peak: f32,
}
#[derive(Clone, Serialize, Debug)]
pub struct View {
    pub sampler_voices: Vec<u8>,
    pub decks: [DeckView; 2],
    pub crossfader: f32,
    pub master: f32,
    pub sampler_volume: f32,
    pub peak: f32,
    pub clock: u64,
}
impl Default for View {
    fn default() -> Self {
        Self {
            sampler_voices: vec![0; crate::sampler::SAMPLER_SLOTS],
            decks: [DeckView {
                volume: 0.8,
                rate: 1.,
                wet: 0.35,
                ..Default::default()
            }; 2],
            crossfader: 0.,
            master: 0.8,
            sampler_volume: 0.8,
            peak: 0.,
            clock: 0,
        }
    }
}
pub enum Cmd {
    LoadWithCue {
        deck: usize,
        track: Arc<Track>,
        hot: [Option<f64>; 8],
        cue: f64,
        force: bool,
        result: Arc<std::sync::atomic::AtomicU8>,
    },
    SamplePress(usize, crate::sampler::PlayMode),
    CueSeconds(f64),
    AutoCross(f64),
    AutoMix {
        from: usize,
        expected: [usize; 2],
        track: Arc<Track>,
        hot: [Option<f64>; 8],
        cue: f64,
        seconds: f64,
    },
    FxGate(usize, u8, bool),
    StopSample(usize),
    Load(usize, Arc<Track>, [Option<f64>; 8], bool),
    Toggle(usize),
    Cue(usize, bool),
    CueHold(usize, u8),
    SetCue(usize),
    RestoreCue(usize, f64),
    Seek(usize, f64),
    ScrubEnd(usize, f64),
    Volume(usize, f32),
    Filter(usize, f32),
    Rate(usize, f64),
    Cross(f32),
    CrossDelta(f32),
    ClearSample(usize),
    Master(f32),
    SamplerVolume(f32),
    Hot(usize, usize, bool),
    Fx(usize, u8),
    Wet(usize, f32),
    Loop(usize, f64),
    Sampler(usize, Arc<Track>),
    SwapSamples(usize, usize),
    StopSamples,
    Linear(bool),
}

// Parallel damped combs followed by all-pass diffusion: a dense room tail,
// rather than distinct repeated echoes. All storage is allocated off the callback.
struct Reverb {
    combs: [[Vec<f32>; 2]; 8],
    comb_at: [[usize; 2]; 8],
    damped: [[f32; 2]; 8],
    diffusers: [[Vec<f32>; 2]; 4],
    diffuser_at: [[usize; 2]; 4],
    feedback: [f32; 8],
    damping: f32,
    tail_frames: usize,
    remaining: usize,
}
impl Reverb {
    fn new(rate: u32) -> Self {
        let times = [
            0.0253, 0.0269, 0.0289, 0.0307, 0.0329, 0.0347, 0.0371, 0.0397,
        ];
        let buffers = |time: f64| {
            std::array::from_fn(|c| {
                vec![0.; ((time + c as f64 * 0.00073) * rate as f64).round().max(1.) as usize]
            })
        };
        Self {
            combs: times.map(buffers),
            comb_at: [[0; 2]; 8],
            damped: [[0.; 2]; 8],
            diffusers: [0.0051, 0.0037, 0.0023, 0.0017].map(buffers),
            diffuser_at: [[0; 2]; 4],
            // Approximately 1.2 seconds to -60 dB, independent of device rate.
            feedback: times.map(|t| 10f32.powf(-3. * t as f32 / 1.2)),
            damping: 1. - (-2. * std::f32::consts::PI * 4500. / rate.max(1) as f32).exp(),
            tail_frames: rate as usize * 4,
            remaining: 0,
        }
    }
    fn clear(&mut self) {
        for pair in self.combs.iter_mut().chain(self.diffusers.iter_mut()) {
            for buffer in pair {
                buffer.fill(0.);
            }
        }
        self.comb_at = [[0; 2]; 8];
        self.diffuser_at = [[0; 2]; 4];
        self.damped = [[0.; 2]; 8];
        self.remaining = 0;
    }
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if input != [0.; 2] {
            self.remaining = self.tail_frames;
        } else if self.remaining == 0 {
            return [0.; 2];
        } else {
            self.remaining -= 1;
            if self.remaining == 0 {
                // Far below audibility after four seconds. Clear once to avoid
                // denormals and skip all DSP work on idle decks thereafter.
                self.clear();
                return [0.; 2];
            }
        }
        let mut out = [0.; 2];
        for i in 0..8 {
            for c in 0..2 {
                let at = self.comb_at[i][c];
                let delayed = self.combs[i][c][at];
                self.damped[i][c] += self.damping * (delayed - self.damped[i][c]);
                self.combs[i][c][at] = input[c] * 0.125 + self.damped[i][c] * self.feedback[i];
                out[c] += delayed;
                self.comb_at[i][c] = (at + 1) % self.combs[i][c].len();
            }
        }
        for i in 0..4 {
            for (c, sample) in out.iter_mut().enumerate() {
                let at = self.diffuser_at[i][c];
                let delayed = self.diffusers[i][c][at];
                let stored = *sample + delayed * 0.5;
                self.diffusers[i][c][at] = stored;
                *sample = delayed - stored * 0.5;
                self.diffuser_at[i][c] = (at + 1) % self.diffusers[i][c].len();
            }
        }
        out
    }
}

const BACKSPIN_SECONDS: f64 = 0.8;
struct Deck {
    end_events: u64,
    track: Option<Arc<Track>>,
    position: f64,
    playing: bool,
    volume: f32,
    filter: f32,
    filter_alpha: f32,
    low: [f32; 2],
    rate: f64,
    cue: f64,
    pending_cue: Option<f64>,
    preview: bool,
    cue_held: bool,
    cue_elapsed: f64,
    hot: [Option<f64>; 8],
    fx: u8,
    fx_time: f64,
    delay: Vec<[f32; 2]>,
    delay_at: usize,
    peak: f32,
    wet: f32,
    loop_range: Option<(f64, f64)>,
    reverb: Reverb,
}
impl Deck {
    fn new(rate: u32) -> Self {
        Self {
            end_events: 0,
            track: None,
            position: 0.,
            playing: false,
            volume: 0.8,
            filter: 0.,
            filter_alpha: 0.,
            low: [0.; 2],
            rate: 1.,
            cue: 0.,
            pending_cue: None,
            preview: false,
            cue_held: false,
            cue_elapsed: 0.,
            hot: [None; 8],
            fx: 0,
            fx_time: 0.,
            delay: vec![[0.; 2]; rate as usize * 2],
            delay_at: 0,
            peak: 0.,
            wet: 0.35,
            loop_range: None,
            reverb: Reverb::new(rate),
        }
    }
}
pub struct Mixer {
    sample_layers: [[Option<f64>; 7]; crate::sampler::SAMPLER_SLOTS],
    cue_seconds: f64,
    cross_ramp: Option<(f32, f32)>,
    auto_mix_outgoing: Option<usize>,
    decks: [Deck; 2],
    samplers: [Option<Arc<Track>>; crate::sampler::SAMPLER_SLOTS],
    sample_pos: [Option<f64>; crate::sampler::SAMPLER_SLOTS],
    cross: f32,
    master: f32,
    sampler_volume: f32,
    linear: bool,
    rate: u32,
    clock: u64,
    peak: f32,
}
pub fn cross_gains(x: f32, linear: bool) -> [f32; 2] {
    let p = (x.clamp(-1., 1.) + 1.) * 0.5;
    if linear {
        [1. - p, p]
    } else {
        [
            (p * std::f32::consts::FRAC_PI_2).cos(),
            (p * std::f32::consts::FRAC_PI_2).sin(),
        ]
    }
}
impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            sample_layers: [[None; 7]; crate::sampler::SAMPLER_SLOTS],
            cue_seconds: 3.,
            cross_ramp: None,
            auto_mix_outgoing: None,
            decks: [Deck::new(rate), Deck::new(rate)],
            samplers: std::array::from_fn(|_| None),
            sample_pos: [None; crate::sampler::SAMPLER_SLOTS],
            cross: 0.,
            master: 0.8,
            sampler_volume: 0.8,
            linear: false,
            rate,
            clock: 0,
            peak: 0.,
        }
    }
    pub fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::SamplePress(i, mode) => {
                use crate::sampler::PlayMode;
                if i < crate::sampler::SAMPLER_SLOTS && self.samplers[i].is_some() {
                    match mode {
                        PlayMode::Restart | PlayMode::Hold => {
                            self.sample_pos[i] = Some(0.);
                            self.sample_layers[i] = [None; 7];
                        }
                        PlayMode::Toggle => {
                            let playing = self.sample_pos[i].is_some()
                                || self.sample_layers[i].iter().any(Option::is_some);
                            self.sample_pos[i] = if playing { None } else { Some(0.) };
                            self.sample_layers[i] = [None; 7];
                        }
                        PlayMode::Overlap => {
                            if self.sample_pos[i].is_none() {
                                self.sample_pos[i] = Some(0.);
                            } else if let Some(slot) =
                                self.sample_layers[i].iter_mut().find(|p| p.is_none())
                            {
                                *slot = Some(0.);
                            } else {
                                // Fixed eight-voice budget per pad: replace the oldest hit.
                                let oldest = self.sample_layers[i]
                                    .iter()
                                    .enumerate()
                                    .max_by(|(_, a), (_, b)| {
                                        a.unwrap_or(0.).total_cmp(&b.unwrap_or(0.))
                                    })
                                    .map(|(n, _)| n)
                                    .unwrap_or(0);
                                if self.sample_pos[i].unwrap_or(0.)
                                    >= self.sample_layers[i][oldest].unwrap_or(0.)
                                {
                                    self.sample_pos[i] = Some(0.);
                                } else {
                                    self.sample_layers[i][oldest] = Some(0.);
                                }
                            }
                        }
                    }
                }
            }
            Cmd::CueSeconds(seconds) => {
                if seconds.is_finite() {
                    self.cue_seconds = seconds.clamp(0.1, 10.);
                }
            }
            Cmd::AutoCross(seconds) => {
                self.auto_mix_outgoing = None;
                if seconds.is_finite() {
                    self.cross_ramp = Some((
                        if self.cross <= 0. { 1. } else { -1. },
                        2. / seconds.clamp(0.1, 10.) as f32,
                    ));
                }
            }
            Cmd::AutoMix {
                from,
                expected,
                track,
                hot,
                cue,
                seconds,
            } => {
                let to = 1 - from;
                let ids = self
                    .decks
                    .each_ref()
                    .map(|d| d.track.as_ref().map_or(0, |t| Arc::as_ptr(t) as usize));
                let source = &self.decks[from];
                let ended = source
                    .track
                    .as_ref()
                    .is_some_and(|t| source.position >= t.samples.len() as f64 / t.rate as f64);
                if ids != expected
                    || (!source.playing && !ended)
                    || self.decks[to].playing
                    || self.decks[from].preview
                    || self.decks[from].cue_held
                    || self.decks[from].loop_range.is_some()
                {
                    return;
                }
                self.apply(Cmd::Load(to, track, hot, false));
                self.apply(Cmd::RestoreCue(to, cue));
                self.decks[to].playing = true;
                let target = if to == 0 { -1. } else { 1. };
                self.cross_ramp = Some((target, (target - self.cross).abs() / seconds as f32));
                self.auto_mix_outgoing = Some(from);
            }
            Cmd::StopSample(i) => {
                if i < crate::sampler::SAMPLER_SLOTS {
                    self.sample_pos[i] = None;
                    self.sample_layers[i] = [None; 7];
                }
            }
            Cmd::FxGate(i, effect, down) => {
                if let Some(d) = self.decks.get_mut(i) {
                    if down {
                        d.fx = effect;
                        d.fx_time = 0.;
                        if effect == 3 {
                            d.playing = true;
                        }
                    } else if d.fx == effect {
                        d.fx = 0;
                    }
                }
            }
            Cmd::SwapSamples(a, b) => {
                if a < crate::sampler::SAMPLER_SLOTS && b < crate::sampler::SAMPLER_SLOTS {
                    self.samplers.swap(a, b);
                    self.sample_pos.swap(a, b);
                    self.sample_layers.swap(a, b);
                }
            }
            Cmd::RestoreCue(i, p) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.cue = p.max(0.);
                }
            }
            Cmd::SetCue(i) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.cue = d.position;
                }
            }
            Cmd::CueHold(i, phase) => {
                if let Some(d) = self.decks.get_mut(i) {
                    if d.track.is_some() {
                        if phase == 1 {
                            if let Some(position) = d.pending_cue.take() {
                                if let Some(slot) = d.hot[..3].iter_mut().find(|slot| slot.is_none()) {
                                    *slot = Some(position);
                                    d.cue = position;
                                    return;
                                }
                            }
                            d.position = d.cue;
                            d.playing = true;
                            d.preview = false;
                            d.cue_held = true;
                            d.cue_elapsed = 0.;
                            d.fx = 0;
                        } else if d.cue_held {
                            d.cue_held = false;
                            if phase == 2 || d.cue_elapsed + 1e-6 < self.cue_seconds {
                                d.playing = false;
                                d.position = d.cue;
                            }
                            d.cue_elapsed = 0.;
                        }
                    }
                }
            }
            Cmd::Wet(i, v) => {
                if let Some(d) = self.decks.get_mut(i).filter(|_| v.is_finite()) {
                    d.wet = v.clamp(0., 1.)
                }
            }
            Cmd::Loop(i, seconds) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.loop_range = if seconds <= 0. || d.loop_range.is_some() {
                        None
                    } else {
                        Some((d.position, d.position + seconds))
                    };
                }
            }
            Cmd::Cross(x) => {
                self.auto_mix_outgoing = None;
                self.cross_ramp = None;
                self.cross = x.clamp(-1., 1.);
            }
            Cmd::CrossDelta(x) => {
                self.auto_mix_outgoing = None;
                self.cross_ramp = None;
                self.cross = (self.cross + x).clamp(-1., 1.);
            }
            Cmd::ClearSample(i) => {
                if i < crate::sampler::SAMPLER_SLOTS {
                    self.samplers[i] = None;
                    self.sample_pos[i] = None;
                    self.sample_layers[i] = [None; 7];
                }
            }
            Cmd::Master(x) => self.master = x.clamp(0., 1.),
            Cmd::SamplerVolume(x) => self.sampler_volume = x.clamp(0., 1.),
            Cmd::Linear(x) => self.linear = x,
            Cmd::Sampler(i, t) => {
                if i < crate::sampler::SAMPLER_SLOTS {
                    self.samplers[i] = Some(t);
                    self.sample_layers[i] = [None; 7];
                    self.sample_pos[i] = None
                }
            }
            Cmd::StopSamples => {
                self.sample_pos = [None; crate::sampler::SAMPLER_SLOTS];
                self.sample_layers = [[None; 7]; crate::sampler::SAMPLER_SLOTS];
            }
            Cmd::LoadWithCue { deck, track, hot, cue, force, result } => {
                let accepted = deck < self.decks.len() && (!self.decks[deck].playing || force);
                if accepted {
                    self.apply(Cmd::Load(deck, track, hot, force));
                    self.apply(Cmd::RestoreCue(deck, cue));
                }
                result.store(if accepted { 1 } else { 2 }, std::sync::atomic::Ordering::Release);
            }
            Cmd::Load(i, t, hot, force) => {
                if i < 2 && (!self.decks[i].playing || force) {
                    let d = &mut self.decks[i];
                    d.track = Some(t);
                    d.position = 0.;
                    d.rate = 1.;
                    d.playing = false;
                    d.hot = hot;
                    d.cue = 0.;
                    d.pending_cue = None;
                    d.preview = false;
                    d.cue_held = false;
                    d.fx = 0;
                    d.loop_range = None;
                    d.low = [0.; 2];
                    d.delay.fill([0.; 2]);
                    d.reverb.clear();
                }
            }
            Cmd::Toggle(i) => {
                if self.auto_mix_outgoing.take().is_some() {
                    self.cross_ramp = None;
                }
                if let Some(d) = self.decks.get_mut(i) {
                    if d.track.is_some() {
                        d.playing = !d.playing;
                        d.preview = false;
                        d.cue_held = false;
                        d.fx = 0;
                    }
                }
            }
            Cmd::Cue(i, down) => {
                if let Some(d) = self.decks.get_mut(i) {
                    if down {
                        if d.playing {
                            d.position = d.cue;
                            d.playing = false
                        } else {
                            d.cue = d.position;
                            d.preview = true;
                            d.playing = true
                        }
                    } else if d.preview {
                        d.playing = false;
                        d.position = d.cue;
                        d.preview = false
                    }
                }
            }
            Cmd::ScrubEnd(i, v) => {
                self.apply(Cmd::Seek(i, v));
                if let Some(d) = self.decks.get_mut(i).filter(|d| d.track.is_some()) {
                    d.pending_cue = Some(d.position);
                }
            }
            Cmd::Seek(i, v) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.pending_cue = None;
                    d.fx = 0;
                    d.loop_range = None;
                    d.low = [0.; 2];
                    d.position = v.max(0.).min(
                        d.track
                            .as_ref()
                            .map(|t| t.samples.len() as f64 / t.rate as f64)
                            .unwrap_or(0.),
                    );
                }
            }
            Cmd::Volume(i, v) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.volume = v.clamp(0., 1.);
                }
            }
            Cmd::Filter(i, v) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.filter = v.clamp(-1., 1.);
                    let cutoff = if d.filter < 0. {
                        20000f32 * 0.004f32.powf(-d.filter)
                    } else {
                        30f32 * 400f32.powf(d.filter)
                    };
                    d.filter_alpha =
                        1. - (-2. * std::f32::consts::PI * cutoff / self.rate as f32).exp();
                }
            }
            Cmd::Rate(i, v) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.rate = v.clamp(0.5, 2.);
                }
            }
            Cmd::Hot(i, n, clear) => {
                if let Some(d) = self.decks.get_mut(i) {
                    if n < 8 {
                        if clear {
                            d.hot[n] = None
                        } else if let Some(p) = d.hot[n] {
                            d.position = p;
                            d.playing = true
                        } else {
                            d.hot[n] = Some(d.position)
                        }
                    }
                }
            }
            Cmd::Fx(i, f) => {
                if let Some(d) = self.decks.get_mut(i) {
                    d.fx = if d.fx == f { 0 } else { f };
                    d.fx_time = 0.;
                    if f == 3 {
                        d.playing = true
                    }
                }
            }
        }
    }
    pub fn frame(&mut self) -> [f32; 2] {
        if let Some((target, speed)) = self.cross_ramp {
            let delta = target - self.cross;
            let step = speed / self.rate as f32;
            if delta.abs() <= step {
                self.cross = target;
                self.cross_ramp = None;
                if let Some(outgoing) = self.auto_mix_outgoing.take() {
                    self.decks[outgoing].playing = false;
                }
            } else {
                self.cross += step.copysign(delta);
            }
        }
        let mut out = [0.; 2];
        let gains = cross_gains(self.cross, self.linear);
        let dt = 1. / self.rate as f64;
        for (i, d) in self.decks.iter_mut().enumerate() {
            if d.cue_held {
                d.cue_elapsed += dt;
            }
            let mut s = [0.; 2];
            if let Some(t) = &d.track {
                if d.playing {
                    let p = d.position * t.rate as f64;
                    let j = p.floor() as usize;
                    if j < t.samples.len() {
                        let a = t.samples[j];
                        let b = t.samples[(j + 1).min(t.samples.len() - 1)];
                        let f = (p - p.floor()) as f32;
                        for c in 0..2 {
                            s[c] = a[c] + (b[c] - a[c]) * f;
                        }
                        let speed = match d.fx {
                            // Wet controls the initial reverse throw (1x to 6x).
                            // Quadratic drag quickly drops the pitch into a short stop.
                            1 => {
                                -(1. + 5. * d.wet as f64)
                                    * (1. - d.fx_time / BACKSPIN_SECONDS).max(0.).powi(2)
                            }
                            2 => (1. - d.fx_time / 1.5).max(0.),
                            3 => (d.fx_time / 1.5).min(1.),
                            _ => 1.,
                        };
                        if d.fx == 1 {
                            let fade = ((BACKSPIN_SECONDS - d.fx_time) / 0.02).clamp(0., 1.) as f32;
                            for sample in &mut s {
                                *sample *= fade;
                            }
                        }
                        d.position += dt * d.rate * speed;
                        d.fx_time += dt;
                        if (d.fx == 1 && d.fx_time >= BACKSPIN_SECONDS)
                            || (d.fx == 2 && d.fx_time >= 1.5)
                        {
                            d.playing = false;
                            d.fx = 0
                        }
                        if d.fx == 3 && d.fx_time >= 1.5 {
                            d.fx = 0
                        }
                        if d.position < 0. || d.position >= t.samples.len() as f64 / t.rate as f64 {
                            if d.position >= t.samples.len() as f64 / t.rate as f64
                                && !d.preview
                                && !d.cue_held
                                && d.loop_range.is_none()
                                && speed > 0.
                                && d.fx != 2
                            {
                                d.end_events = d.end_events.wrapping_add(1);
                            }
                            d.position =
                                d.position.clamp(0., t.samples.len() as f64 / t.rate as f64);
                            d.playing = false
                        }
                    } else {
                        d.playing = false
                    }
                }
            }
            let alpha = d.filter_alpha;
            for (c, sample) in s.iter_mut().enumerate() {
                d.low[c] += alpha * (*sample - d.low[c]);
                if d.filter.abs() > 0.02 {
                    *sample = if d.filter < 0. {
                        d.low[c]
                    } else {
                        *sample - d.low[c]
                    }
                }
            }
            if let Some((start, end)) = d.loop_range {
                if d.position >= end {
                    d.position = start + (d.position - end) % (end - start)
                }
            }
            // Drain silently while bypassed so re-engaging cannot revive a frozen tail.
            let reverb = d.reverb.process(if d.fx == 5 { s } else { [0.; 2] });
            if d.fx == 5 {
                for c in 0..2 {
                    s[c] = s[c] * (1. - d.wet) + reverb[c] * d.wet;
                }
            }
            if d.fx == 4 {
                let delay = self.rate as usize / 3;
                let read = (d.delay_at + d.delay.len() - delay) % d.delay.len();
                for (c, sample) in s.iter_mut().enumerate() {
                    let wet = d.delay[read][c];
                    d.delay[d.delay_at][c] = *sample + wet * 0.48;
                    *sample = *sample * (1. - d.wet) + wet * d.wet;
                }
                d.delay_at = (d.delay_at + 1) % d.delay.len();
            }
            d.peak = (d.peak * 0.9995).max(s[0].abs().max(s[1].abs()) * d.volume);
            for c in 0..2 {
                out[c] += s[c] * d.volume * gains[i];
            }
        }
        for i in 0..16 {
            if let Some(t) = &self.samplers[i] {
                for position in
                    std::iter::once(&mut self.sample_pos[i]).chain(self.sample_layers[i].iter_mut())
                {
                    if let Some(p) = *position {
                        let j = p as usize;
                        if j < t.samples.len() {
                            let next = (j + 1).min(t.samples.len() - 1);
                            let fraction = (p - j as f64) as f32;
                            for (c, sample) in out.iter_mut().enumerate() {
                                *sample += (t.samples[j][c]
                                    + (t.samples[next][c] - t.samples[j][c]) * fraction)
                                    * 0.65 * self.sampler_volume;
                            }
                            let advanced = p + t.rate as f64 / self.rate as f64;
                            *position = if advanced < t.samples.len() as f64 {
                                Some(advanced)
                            } else {
                                None
                            };
                        } else {
                            *position = None;
                        }
                    }
                }
            }
        }
        for x in &mut out {
            *x *= self.master;
        }
        self.peak = (self.peak * 0.9995).max(out[0].abs().max(out[1].abs()));
        self.clock += 1;
        for x in &mut out {
            *x = x.clamp(-1., 1.);
        }
        out
    }
    pub fn view(&self) -> View {
        View {
            sampler_voices: (0..crate::sampler::SAMPLER_SLOTS).map(|i| {
                u8::from(self.sample_pos[i].is_some())
                    + self.sample_layers[i].iter().filter(|p| p.is_some()).count() as u8
            }).collect(),
            decks: std::array::from_fn(|i| {
                let d = &self.decks[i];
                DeckView {
                    end_events: d.end_events,
                    previewing: d.preview || d.cue_held,
                    looping: d.loop_range.is_some(),
                    track_id: d
                        .track
                        .as_ref()
                        .map(|t| Arc::as_ptr(t) as usize)
                        .unwrap_or(0),
                    position: d.position,
                    duration: d
                        .track
                        .as_ref()
                        .map(|t| t.samples.len() as f64 / t.rate as f64)
                        .unwrap_or(0.),
                    playing: d.playing,
                    volume: d.volume,
                    filter: d.filter,
                    rate: d.rate,
                    cue: d.cue,
                    cue_held: d.cue_held,
                    cue_elapsed: d.cue_elapsed,
                    effect: d.fx,
                    wet: d.wet,
                    hot: d.hot,
                    peak: d.peak,
                }
            }),
            crossfader: self.cross,
            master: self.master,
            sampler_volume: self.sampler_volume,
            peak: self.peak,
            clock: self.clock,
        }
    }
}
pub fn start(
    commands: Arc<ArrayQueue<Cmd>>,
    views: Arc<ArrayQueue<View>>,
    errors: Arc<std::sync::Mutex<String>>,
    recorder: Arc<crate::recording::Recorder>,
) -> Result<(String, u32), String> {
    start_on_device(commands, views, errors, recorder, None)
}
pub fn output_devices() -> Result<Vec<String>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    cpal::default_host()
        .output_devices()
        .map_err(|e| e.to_string())?
        .map(|d| d.name().map_err(|e| e.to_string()))
        .collect()
}
pub fn start_on_device(
    commands: Arc<ArrayQueue<Cmd>>,
    views: Arc<ArrayQueue<View>>,
    errors: Arc<std::sync::Mutex<String>>,
    recorder: Arc<crate::recording::Recorder>,
    selected: Option<&str>,
) -> Result<(String, u32), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = if let Some(name) = selected {
        host.output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| {
                format!("Audio device '{name}' is unavailable. Choose another output in Settings.")
            })?
    } else {
        host.default_output_device().ok_or("No output device")?
    };
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let name = device.name().unwrap_or_default();
    let rate = config.sample_rate().0;
    let device_name = name.clone();
    std::thread::spawn(move || {
        let mixer = Arc::new(std::sync::Mutex::new(Mixer::new(rate)));
        let mut first = Some(device);
        loop {
            let output = first.take().or_else(|| {
                cpal::default_host().output_devices().ok()?.find(|device| {
                    device.name().ok().as_deref() == Some(device_name.as_str())
                })
            });
            let Some(output) = output else {
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            };
            let (failed, failure) = std::sync::mpsc::sync_channel(1);
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => output_stream::<f32>(&output, &config, &mixer, &commands, &views, &recorder, failed),
                cpal::SampleFormat::I16 => output_stream::<i16>(&output, &config, &mixer, &commands, &views, &recorder, failed),
                cpal::SampleFormat::U16 => output_stream::<u16>(&output, &config, &mixer, &commands, &views, &recorder, failed),
                cpal::SampleFormat::I32 => output_stream::<i32>(&output, &config, &mixer, &commands, &views, &recorder, failed),
                cpal::SampleFormat::F64 => output_stream::<f64>(&output, &config, &mixer, &commands, &views, &recorder, failed),
                _ => {
                    *errors.lock().unwrap() = "Unsupported output format. Choose another audio device.".into();
                    return;
                }
            };
            let error = match stream {
                Ok(stream) => match stream.play() {
                    Ok(()) => {
                        {
                            let mut status = errors.lock().unwrap();
                            if status.starts_with("Audio output interrupted:") {
                                status.clear();
                            }
                        }
                        let error = failure.recv().unwrap_or_else(|_| "Stream closed".into());
                        drop(stream);
                        error
                    }
                    Err(error) => error.to_string(),
                },
                Err(error) => error.to_string(),
            };
            *errors.lock().unwrap() = format!("Audio output interrupted: {error}. Reconnecting to {device_name}.");
            if recorder.is_open() {
                let _ = recorder.stop().map_err(|error| {
                    errors.lock().unwrap().push_str(&format!(" Recording finalization failed: {error}"));
                });
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    Ok((name, rate))
}
fn output_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mixer: &Arc<std::sync::Mutex<Mixer>>,
    commands: &Arc<ArrayQueue<Cmd>>,
    views: &Arc<ArrayQueue<View>>,
    recorder: &Arc<crate::recording::Recorder>,
    failed: std::sync::mpsc::SyncSender<String>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    use cpal::traits::DeviceTrait;
    let channels = config.channels() as usize;
    let (mixer, commands, views, recorder) =
        (mixer.clone(), commands.clone(), views.clone(), recorder.clone());
    device.build_output_stream(
        &config.config(),
        move |data: &mut [T], _| {
            let Ok(mut mixer) = mixer.try_lock() else {
                data.fill(T::EQUILIBRIUM);
                return;
            };
            for _ in 0..64 {
                let Some(command) = commands.pop() else { break; };
                mixer.apply(command);
            }
            for frame in data.chunks_mut(channels) {
                let sample = mixer.frame();
                recorder.push(sample);
                for (channel, output) in frame.iter_mut().enumerate() {
                    let value = match channels {
                        1 => (sample[0] + sample[1]) * 0.5,
                        _ => sample.get(channel).copied().unwrap_or(0.),
                    };
                    *output = T::from_sample(value);
                }
            }
            let _ = views.push(mixer.view());
        },
        move |error| { let _ = failed.try_send(error.to_string()); },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waveform_drags_fill_three_cues_then_preserve_them() {
        let mut mixer = Mixer::new(48000);
        mixer.apply(Cmd::Load(0, tone(), [None; 8], false));
        for time in [0.1, 0.2, 0.3, 0.4] {
            mixer.apply(Cmd::ScrubEnd(0, time));
            mixer.apply(Cmd::CueHold(0, 1));
            mixer.apply(Cmd::CueHold(0, 0));
        }
        let deck = mixer.view().decks[0];
        assert_eq!(deck.hot[..3], [Some(0.1), Some(0.2), Some(0.3)]);
        assert_eq!(deck.position, 0.3);
    }
    #[test]
    fn rejected_load_keeps_current_track_and_cue() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let mut mixer = Mixer::new(48000);
        let original = tone();
        mixer.apply(Cmd::Load(0, original.clone(), [None; 8], false));
        mixer.apply(Cmd::RestoreCue(0, 0.1));
        mixer.apply(Cmd::Toggle(0));
        let result = Arc::new(AtomicU8::new(0));
        mixer.apply(Cmd::LoadWithCue {
            deck: 0,
            track: tone(),
            hot: [None; 8],
            cue: 0.2,
            force: false,
            result: result.clone(),
        });
        assert_eq!(result.load(Ordering::Acquire), 2);
        assert_eq!(mixer.view().decks[0].track_id, Arc::as_ptr(&original) as usize);
        assert_eq!(mixer.view().decks[0].cue, 0.1);
    }
    #[test]
    fn web_page_disguised_as_mp3_has_actionable_error() {
        let path = std::env::temp_dir().join(format!("skydek-html-{}.mp3", std::process::id()));
        std::fs::write(&path, b"<!DOCTYPE html><html>Not an audio download</html>").unwrap();
        let result = decode(path.to_str().unwrap());
        std::fs::remove_file(path).unwrap();
        assert!(matches!(result,Err(e) if e.contains("web page, not audio")));
    }
    #[test]
    fn malformed_mp3_reservoir_recovers_for_streaming_and_sampler_decode() {
        use symphonia::core::{
            codecs::DecoderOptions, errors::Error, io::MediaSourceStream, probe::Hint,
        };
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../docs/verification/0.8.0/fixture-deck-1.mp3");
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let mut format = symphonia::default::get_probe()
            .format(
                &hint,
                MediaSourceStream::new(
                    Box::new(std::fs::File::open(fixture).unwrap()),
                    Default::default(),
                ),
                &Default::default(),
                &Default::default(),
            )
            .unwrap()
            .format;
        let params = format.default_track().unwrap().codec_params.clone();
        let mut packets = Vec::new();
        loop {
            match format.next_packet() {
                Ok(packet) => packets.push(packet),
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => panic!("fixture demux failed: {e}"),
            }
        }
        // Synthesize corrupt MPEG1 stereo side information in one packet. A
        // missing 511-byte reservoir skips granule 0, whose exaggerated lengths
        // then place granule 1 beyond the available main data. Plain truncation
        // alone is tolerated by this fixture and does not reproduce the error.
        let mut damaged = packets[0].clone();
        assert_eq!(damaged.data[1] & 0x0f, 0x0b, "MPEG1 layer III without CRC");
        assert_ne!(damaged.data[3] >> 6, 3, "stereo fixture required");
        damaged.data[4..36].fill(0);
        let mut set_bits = |offset: usize, count: usize| {
            for bit in offset..offset + count {
                damaged.data[4 + bit / 8] |= 1 << (7 - bit % 8);
            }
        };
        set_bits(0, 9); // main_data_begin = 511
        set_bits(20, 12); // granule 0 left part2_3_length = 4095
        set_bits(79, 12); // granule 0 right part2_3_length = 4095
        let mut decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .unwrap();
        assert!(matches!(
            decoder.decode(&damaged),
            Err(Error::DecodeError("mpa: invalid main_data offset"))
        ));
        struct TempAudio(std::path::PathBuf);
        impl Drop for TempAudio {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let temp = TempAudio(
            std::env::temp_dir().join(format!("skydek-reservoir-{}.mp3", std::process::id())),
        );
        let bytes: Vec<u8> = std::iter::once(&damaged)
            .chain(packets.iter())
            .flat_map(|p| p.data.iter().copied())
            .collect();
        std::fs::write(&temp.0, bytes).unwrap();
        let path = temp.0.to_str().unwrap();
        let mut visited = Vec::new();
        let (count, rate) = visit_audio(path, |frame, _| {
            visited.push(frame);
            Ok(())
        })
        .unwrap();
        assert!(
            count > rate as u64,
            "recover at least a second of usable audio"
        );
        let (track, _) = decode(path).unwrap();
        assert_eq!(track.samples, visited);
        assert_eq!(track.rate, rate);
        assert!(visited.iter().flatten().all(|s| s.is_finite()));
        let mut mixer = Mixer::new(48000);
        mixer.apply(Cmd::Sampler(0, track));
        mixer.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        let mut energy = 0.;
        for _ in 0..48000 {
            for sample in mixer.frame() {
                assert!(sample.is_finite() && sample.abs() <= 1.);
                energy += sample * sample;
            }
        }
        assert!(energy > 0.01);
    }
    /// Optional read-only check against a real malformed MP3 (no private fixture
    /// paths or copyrighted audio in the repository).
    #[test]
    #[ignore = "set SKYDEK_AUDIO_TEST_FILE to a local audio fixture"]
    fn decoder_fixture_matches_streaming_and_sampler_pcm() {
        let path = std::env::var("SKYDEK_AUDIO_TEST_FILE").expect("audio fixture path");
        let hash_frame = |hash: u64, frame: [f32; 2]| {
            frame.iter().fold(hash, |hash, sample| {
                assert!(sample.is_finite());
                (hash ^ sample.to_bits() as u64).wrapping_mul(1099511628211)
            })
        };
        let mut streamed_hash = 14695981039346656037;
        let (frames, rate) = visit_audio(&path, |frame, _| {
            streamed_hash = hash_frame(streamed_hash, frame);
            Ok(())
        })
        .expect("streaming decode must recover usable PCM");
        let (track, info) = decode(&path).expect("deck/sampler decode must recover usable PCM");
        assert_eq!(frames, track.samples.len() as u64);
        assert_eq!(rate, track.rate);
        assert_eq!(
            streamed_hash,
            track
                .samples
                .iter()
                .copied()
                .fold(14695981039346656037, hash_frame)
        );
        assert_eq!(info.duration, frames as f64 / rate as f64);
        let mut mixer = Mixer::new(48000);
        mixer.apply(Cmd::Sampler(0, track));
        mixer.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        for _ in 0..4800 {
            assert!(mixer.frame().iter().all(|v| v.is_finite() && v.abs() <= 1.));
        }
    }
    #[test]
    fn restart_retriggers_at_zero_then_finishes_without_release_command() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        for _ in 0..1000 {
            m.frame();
        }
        assert_eq!(m.sample_pos[0], Some(1000.));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        assert_eq!(m.sample_pos[0], Some(0.));
        for _ in 0..48000 {
            m.frame();
        }
        assert_eq!(m.view().sampler_voices[0], 0);
    }
    #[test]
    fn toggle_starts_stops_and_starts_again() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        for expected in [1, 0, 1] {
            m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Toggle));
            assert_eq!(m.view().sampler_voices[0], expected);
        }
    }
    #[test]
    fn overlap_is_polyphonic_bounded_and_stop_clears_every_voice() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        for hit in 1..=20 {
            m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Overlap));
            assert_eq!(m.view().sampler_voices[0], hit.min(8));
            for _ in 0..100 {
                m.frame();
            }
        }
        m.apply(Cmd::StopSample(0));
        assert_eq!(m.view().sampler_voices[0], 0);
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Overlap));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Overlap));
        m.apply(Cmd::StopSamples);
        assert_eq!(m.view().sampler_voices, vec![0; crate::sampler::SAMPLER_SLOTS]);
    }
    #[test]
    fn restart_and_replacement_clear_previous_overlaps() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        for _ in 0..4 {
            m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Overlap));
        }
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        assert_eq!(m.view().sampler_voices[0], 1);
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Overlap));
        m.apply(Cmd::Sampler(0, tone()));
        assert_eq!(m.view().sampler_voices[0], 0);
    }
    #[test]
    fn gated_effect_release_is_deck_and_effect_specific() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::FxGate(0, 4, true));
        assert_eq!(m.view().decks[0].effect, 4);
        m.apply(Cmd::FxGate(1, 5, true));
        m.apply(Cmd::FxGate(0, 4, false));
        assert_eq!(m.view().decks[0].effect, 0);
        assert_eq!(m.view().decks[1].effect, 5);
        m.apply(Cmd::FxGate(0, 2, true));
        m.apply(Cmd::FxGate(0, 4, false));
        assert_eq!(m.view().decks[0].effect, 2);
    }
    #[test]
    fn sample_release_stops_only_the_original_voice() {
        let mut m = Mixer::new(48000);
        for i in 0..2 {
            m.apply(Cmd::Sampler(i, tone()));
            m.apply(Cmd::SamplePress(i, crate::sampler::PlayMode::Restart));
        }
        m.apply(Cmd::StopSample(0));
        assert!(m.sample_pos[0].is_none());
        assert!(m.sample_pos[1].is_some());
    }
    #[test]
    fn configurable_short_cue_hold_commits_and_disconnect_cancels() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        m.apply(Cmd::CueSeconds(0.25));
        m.apply(Cmd::CueHold(0, 1));
        for _ in 0..12000 {
            m.frame();
        }
        m.apply(Cmd::CueHold(0, 0));
        assert!(m.decks[0].playing);
        m.apply(Cmd::CueHold(0, 1));
        for _ in 0..12000 {
            m.frame();
        }
        m.apply(Cmd::CueHold(0, 2));
        assert!(!m.decks[0].playing);
    }
    #[test]
    fn auto_crossfade_reaches_opposite_side_and_manual_input_cancels() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Cross(-1.));
        m.apply(Cmd::AutoCross(0.25));
        for _ in 0..12100 {
            m.frame();
        }
        assert_eq!(m.cross, 1.);
        assert!(m.cross_ramp.is_none());
        m.apply(Cmd::AutoCross(1.));
        m.frame();
        m.apply(Cmd::Cross(0.3));
        for _ in 0..4800 {
            m.frame();
        }
        assert_eq!(m.cross, 0.3);
    }
    #[test]
    fn relative_crossfader_accumulates_without_snapshot_feedback() {
        let mut m = Mixer::new(48000);
        for _ in 0..100 {
            m.apply(Cmd::CrossDelta(0.008));
        }
        assert!((m.cross - 0.8).abs() < 0.0001);
        m.apply(Cmd::CrossDelta(1.));
        assert_eq!(m.cross, 1.);
        m.apply(Cmd::CrossDelta(-0.25));
        assert_eq!(m.cross, 0.75);
    }
    #[test]
    fn new_track_resets_speed_and_seek_cancels_brake_and_loop() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Rate(0, 0.5));
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        assert_eq!(m.decks[0].rate, 1.);
        m.apply(Cmd::Loop(0, 0.2));
        m.apply(Cmd::Fx(0, 2));
        m.apply(Cmd::Seek(0, 0.7));
        m.apply(Cmd::SetCue(0));
        assert_eq!(m.decks[0].fx, 0);
        assert!(m.decks[0].loop_range.is_none());
        assert_eq!(m.decks[0].cue, 0.7);
    }
    #[test]
    fn clearing_pad_stops_voice_and_allows_reassignment() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        m.apply(Cmd::ClearSample(0));
        assert!(m.samplers[0].is_none());
        assert!(m.sample_pos[0].is_none());
        m.apply(Cmd::Sampler(0, tone()));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        assert_eq!(m.sample_pos[0], Some(0.));
    }
    fn tone() -> Arc<Track> {
        Arc::new(Track {
            samples: vec![[0.5; 2]; 48000],
            rate: 48000,
        })
    }
    fn effect_mixer(rate: u32, samples: Vec<[f32; 2]>) -> Mixer {
        let mut m = Mixer::new(rate);
        m.apply(Cmd::Load(
            0,
            Arc::new(Track { samples, rate }),
            [None; 8],
            false,
        ));
        m.apply(Cmd::Cross(-1.));
        m.apply(Cmd::Volume(0, 1.));
        m.apply(Cmd::Master(1.));
        m.apply(Cmd::Toggle(0));
        m
    }
    #[test]
    fn reverb_impulse_is_dense_deterministic_and_decays_at_device_rates() {
        for rate in [8000, 44100, 48000, 96000] {
            let mut a = Reverb::new(rate);
            let mut b = Reverb::new(rate);
            let mut energy = [0f64; 3];
            let mut occupied = 0;
            let mut stereo_difference = 0.;
            for n in 0..rate as usize * 3 {
                let input = if n == 0 { [1.; 2] } else { [0.; 2] };
                let frame = a.process(input);
                assert_eq!(frame, b.process(input));
                assert!(frame.iter().all(|v| v.is_finite() && v.abs() < 1.));
                energy[n / rate as usize] += frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
                if n > rate as usize / 10 && n < rate as usize / 2 {
                    occupied += usize::from(frame[0].abs() > 1e-7);
                    stereo_difference += (frame[0] - frame[1]).abs();
                }
            }
            assert!(energy[0] > 0.01);
            assert!(
                energy[1] < energy[0] * 0.01,
                "tail should decay: {energy:?}"
            );
            assert!(energy[2] < energy[1] * 0.01);
            assert!(
                occupied > rate as usize / 5,
                "tail must diffuse beyond sparse repeats"
            );
            assert!(
                stereo_difference > 0.01,
                "mono input should have stereo space"
            );
        }
    }
    #[test]
    fn reverb_keeps_channels_separate_and_clear_removes_tail() {
        let mut reverb = Reverb::new(48000);
        let mut left_energy = 0.;
        for n in 0..24000 {
            let frame = reverb.process(if n == 0 { [1., 0.] } else { [0.; 2] });
            left_energy += frame[0] * frame[0];
            assert_eq!(frame[1], 0., "left input must not leak into right");
        }
        assert!(left_energy > 0.01);
        reverb.clear();
        for _ in 0..4800 {
            assert_eq!(reverb.process([0.; 2]), [0.; 2]);
        }
    }
    #[test]
    fn reverb_wet_endpoints_mix_and_tail_survives_track_end() {
        let rate = 48000;
        let mut input = vec![[0.; 2]; 16];
        input[0] = [0.5, -0.25];
        let mut dry = effect_mixer(rate, input.clone());
        let mut wet = effect_mixer(rate, input.clone());
        let mut mixed = effect_mixer(rate, input);
        for (m, amount) in [(&mut dry, 0.), (&mut wet, 1.), (&mut mixed, 0.35)] {
            m.apply(Cmd::Wet(0, amount));
            m.apply(Cmd::Fx(0, 5));
        }
        let mut tail = 0.;
        for n in 0..24000 {
            let d = dry.frame();
            let w = wet.frame();
            let mid = mixed.frame();
            for c in 0..2 {
                assert!((mid[c] - (d[c] * 0.65 + w[c] * 0.35)).abs() < 1e-6);
            }
            if n == 0 {
                assert_eq!(d, [0.5, -0.25]);
                assert_eq!(w, [0.; 2]);
            } else {
                assert_eq!(d, [0.; 2]);
                tail += w[0].abs();
            }
        }
        assert!(!wet.decks[0].playing);
        assert!(tail > 0.1);
        // Load must clear both delay and diffusion history, even while FX is off.
        wet.apply(Cmd::Load(0, tone(), [None; 8], false));
        wet.apply(Cmd::Fx(0, 5));
        for _ in 0..4800 {
            assert_eq!(wet.frame(), [0.; 2]);
        }
    }
    #[test]
    fn reverb_gate_cuts_output_and_bypass_drains_history() {
        let mut m = effect_mixer(48000, vec![[0.5; 2]]);
        m.apply(Cmd::Wet(0, 1.));
        m.apply(Cmd::FxGate(0, 5, true));
        for _ in 0..4800 {
            m.frame();
        }
        m.apply(Cmd::FxGate(0, 5, false));
        for _ in 0..144000 {
            assert_eq!(m.frame(), [0.; 2]);
        }
        m.apply(Cmd::FxGate(0, 5, true));
        for _ in 0..4800 {
            assert!(m.frame().iter().all(|v| v.abs() < 1e-7));
        }
    }
    #[test]
    fn wet_rejects_nonfinite_values_and_clamps_valid_values() {
        let mut m = Mixer::new(48000);
        assert_eq!(View::default().decks[0].wet, 0.35);
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            m.apply(Cmd::Wet(0, value));
            assert_eq!(m.decks[0].wet, 0.35);
        }
        m.apply(Cmd::Wet(0, -1.));
        assert_eq!(m.decks[0].wet, 0.);
        m.apply(Cmd::Wet(0, 2.));
        assert_eq!(m.decks[0].wet, 1.);
        let view = serde_json::to_value(m.view()).unwrap();
        assert_eq!(view["decks"][0]["wet"], 1.);
        assert_eq!(view["decks"][1]["wet"], serde_json::json!(0.35f32));
    }
    #[test]
    fn backspin_is_fast_wet_scaled_finite_and_stops_with_a_fade() {
        for rate in [44100, 48000, 96000] {
            for wet in [0., 0.35, 1.] {
                let mut m = effect_mixer(rate, vec![[0.5, -0.25]; rate as usize * 6]);
                m.apply(Cmd::Seek(0, 4.));
                m.apply(Cmd::Wet(0, wet));
                m.apply(Cmd::FxGate(0, 1, true));
                let start = m.decks[0].position;
                let first = m.frame();
                assert_eq!(first, [0.5, -0.25]);
                let initial_speed = (start - m.decks[0].position) * rate as f64;
                assert!((initial_speed - (1. + 5. * wet as f64)).abs() < 1e-8);
                let mut last = first;
                for _ in 1..(rate as f64 * BACKSPIN_SECONDS).ceil() as usize + 2 {
                    let was_playing = m.decks[0].playing;
                    let frame = m.frame();
                    assert!(frame.iter().all(|v| v.is_finite() && v.abs() <= 1.));
                    assert_eq!(frame[1], -frame[0] * 0.5);
                    if was_playing {
                        last = frame;
                    }
                }
                assert!(!m.decks[0].playing);
                assert_eq!(m.decks[0].fx, 0);
                assert!(
                    last[0].abs() < 0.001,
                    "stop must fade instead of cutting DC"
                );
                let distance = start - m.decks[0].position;
                assert!((distance - initial_speed * BACKSPIN_SECONDS / 3.).abs() < 0.001);
                assert_eq!(m.frame(), [0.; 2]);
                assert_eq!(m.decks[0].rate, 1.);
            }
        }
    }
    #[test]
    fn backspin_release_resumes_forward_and_start_boundary_stays_safe() {
        let mut m = effect_mixer(48000, vec![[0.2; 2]; 48000 * 4]);
        m.apply(Cmd::Seek(0, 2.));
        m.apply(Cmd::FxGate(0, 1, true));
        for _ in 0..4800 {
            m.frame();
        }
        let reversed = m.decks[0].position;
        assert!(reversed < 1.8);
        m.apply(Cmd::FxGate(0, 1, false));
        m.frame();
        assert!(m.decks[0].playing && m.decks[0].position > reversed);
        m.apply(Cmd::Seek(0, 0.00001));
        m.apply(Cmd::FxGate(0, 1, true));
        for _ in 0..10 {
            assert!(m.frame().iter().all(|v| v.is_finite()));
        }
        assert!(!m.decks[0].playing);
        assert_eq!(m.decks[0].position, 0.);
    }
    #[test]
    fn decks_mix_and_clock() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        m.apply(Cmd::Load(1, tone(), [None; 8], false));
        m.apply(Cmd::Toggle(0));
        m.apply(Cmd::Toggle(1));
        assert!(m.frame()[0] > 0.4);
        for _ in 0..4799 {
            m.frame();
        }
        assert!((m.view().decks[0].position - 0.1).abs() < 0.0001);
    }
    #[test]
    fn load_guard_and_cue() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        m.apply(Cmd::Toggle(0));
        m.frame();
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        assert!(m.view().decks[0].playing);
        m.apply(Cmd::Cue(0, true));
        assert!(!m.view().decks[0].playing);
        m.apply(Cmd::Cue(0, true));
        assert!(m.view().decks[0].playing);
        m.apply(Cmd::Cue(0, false));
        assert!(!m.view().decks[0].playing);
    }
    #[test]
    fn crossfade_endpoints() {
        assert_eq!(cross_gains(-1., true), [1., 0.]);
        assert_eq!(cross_gains(1., true), [0., 1.]);
        assert!(cross_gains(1., false)[0].abs() < 0.00001);
    }
    #[test]
    fn sampler_plays_without_decks() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        assert!(m.frame()[0] > 0.2);
        m.apply(Cmd::StopSamples);
        assert_eq!(m.frame(), [0., 0.]);
    }
    #[test]
    fn moving_sampler_tile_preserves_voice() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Sampler(0, tone()));
        m.apply(Cmd::SamplePress(0, crate::sampler::PlayMode::Restart));
        m.frame();
        let pos = m.sample_pos[0];
        m.apply(Cmd::SwapSamples(0, 4));
        assert!(m.samplers[0].is_none());
        assert_eq!(m.sample_pos[4], pos);
        assert!(m.frame()[0] > 0.2);
    }
    #[test]
    fn hotcue_recall_and_clear() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        m.apply(Cmd::Seek(0, 0.25));
        m.apply(Cmd::Hot(0, 2, false));
        m.apply(Cmd::Seek(0, 0.5));
        m.apply(Cmd::Hot(0, 2, false));
        assert_eq!(m.view().decks[0].position, 0.25);
        assert!(m.view().decks[0].playing);
        m.apply(Cmd::Hot(0, 2, true));
        assert_eq!(m.view().decks[0].hot[2], None);
    }
    #[test]
    fn brake_stops_and_backspin_reverses() {
        let mut m = Mixer::new(48000);
        let long = Arc::new(Track {
            samples: vec![[0.1; 2]; 48000 * 4],
            rate: 48000,
        });
        m.apply(Cmd::Load(0, long, [None; 8], false));
        m.apply(Cmd::Seek(0, 2.));
        m.apply(Cmd::Toggle(0));
        m.apply(Cmd::Fx(0, 1));
        for _ in 0..4800 {
            m.frame();
        }
        assert!(m.view().decks[0].position < 2.);
        m.apply(Cmd::Fx(0, 2));
        for _ in 0..73000 {
            m.frame();
        }
        assert!(!m.view().decks[0].playing);
    }
    #[test]
    fn loop_keeps_playhead_in_range() {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(0, tone(), [None; 8], false));
        m.apply(Cmd::Loop(0, 0.2));
        m.apply(Cmd::Toggle(0));
        for _ in 0..24000 {
            m.frame();
        }
        assert!(m.view().decks[0].position < 0.2);
    }
    #[test]
    fn natural_end_event_excludes_pauses_seeks_and_cue_previews() {
        let mut mixer = Mixer::new(48000);
        let track = tone();
        let frames = track.samples.len();
        mixer.apply(Cmd::Load(0, track.clone(), [None; 8], false));
        mixer.apply(Cmd::Toggle(0));
        mixer.frame();
        mixer.apply(Cmd::Toggle(0));
        assert_eq!(mixer.view().decks[0].end_events, 0);
        mixer.apply(Cmd::Seek(0, 9999.));
        mixer.apply(Cmd::Toggle(0));
        mixer.frame();
        assert_eq!(mixer.view().decks[0].end_events, 0);
        mixer.apply(Cmd::Load(0, track.clone(), [None; 8], true));
        mixer.apply(Cmd::CueHold(0, 1));
        for _ in 0..frames + 4 {
            mixer.frame();
        }
        assert_eq!(mixer.view().decks[0].end_events, 0);
        mixer.apply(Cmd::Load(0, track, [None; 8], true));
        mixer.apply(Cmd::Toggle(0));
        for _ in 0..frames + 4 {
            mixer.frame();
        }
        assert_eq!(mixer.view().decks[0].end_events, 1);
        for _ in 0..100 {
            mixer.frame();
        }
        assert_eq!(mixer.view().decks[0].end_events, 1);
    }
    #[test]
    fn auto_dj_starts_incoming_and_releases_outgoing_deck() {
        let mut mixer = cue_mixer();
        let source = mixer.view().decks[0].track_id;
        let incoming = tone();
        let transition = || Cmd::AutoMix {
            from: 0,
            expected: [source, 0],
            track: incoming.clone(),
            hot: [None; 8],
            cue: 0.,
            seconds: 0.1,
        };
        mixer.apply(transition());
        assert_eq!(mixer.view().decks[1].track_id, 0);
        mixer.apply(Cmd::Toggle(0));
        mixer.apply(Cmd::Cross(-1.));
        mixer.apply(transition());
        assert!(mixer.view().decks.iter().all(|d| d.playing));
        for _ in 0..4900 {
            mixer.frame();
        }
        let view = mixer.view();
        assert!(!view.decks[0].playing);
        assert!(view.decks[1].playing);
        assert_eq!(view.crossfader, 1.);
    }
    fn cue_mixer() -> Mixer {
        let mut m = Mixer::new(48000);
        m.apply(Cmd::Load(
            0,
            Arc::new(Track {
                samples: vec![[0.2; 2]; 48000 * 8],
                rate: 48000,
            }),
            [None; 8],
            false,
        ));
        m.apply(Cmd::Seek(0, 0.5));
        m.apply(Cmd::SetCue(0));
        m
    }
    #[test]
    fn cue_tap_returns_to_fixed_mark() {
        let mut m = cue_mixer();
        m.apply(Cmd::Seek(0, 2.));
        m.apply(Cmd::CueHold(0, 1));
        assert_eq!(m.view().decks[0].position, 0.5);
        for _ in 0..4800 {
            m.frame();
        }
        m.apply(Cmd::CueHold(0, 0));
        assert_eq!(m.view().decks[0].position, 0.5);
        assert!(!m.view().decks[0].playing);
        m.apply(Cmd::CueHold(0, 1));
        m.apply(Cmd::CueHold(0, 0));
        assert_eq!(m.view().decks[0].cue, 0.5);
    }
    #[test]
    fn cue_three_seconds_commits_on_release() {
        let mut m = cue_mixer();
        m.apply(Cmd::CueHold(0, 1));
        for _ in 0..144000 {
            m.frame();
        }
        m.apply(Cmd::CueHold(0, 0));
        assert!(m.view().decks[0].playing);
        assert!(!m.view().decks[0].cue_held);
        assert!(m.view().decks[0].position > 3.49);
    }
    #[test]
    fn cue_short_hold_and_disconnect_never_commit() {
        for (frames, phase) in [(143000, 0), (150000, 2)] {
            let mut m = cue_mixer();
            m.apply(Cmd::CueHold(0, 1));
            for _ in 0..frames {
                m.frame();
            }
            m.apply(Cmd::CueHold(0, phase));
            assert!(!m.view().decks[0].playing);
            assert_eq!(m.view().decks[0].position, 0.5);
        }
    }
    #[test]
    fn cue_release_stays_on_original_deck() {
        let mut m = cue_mixer();
        m.apply(Cmd::CueHold(0, 1));
        m.apply(Cmd::Load(
            1,
            Arc::new(Track {
                samples: vec![[0.1; 2]; 48000],
                rate: 48000,
            }),
            [None; 8],
            false,
        ));
        m.apply(Cmd::Toggle(1));
        m.apply(Cmd::CueHold(0, 0));
        assert!(!m.view().decks[0].playing);
        assert!(m.view().decks[1].playing);
    }
}
