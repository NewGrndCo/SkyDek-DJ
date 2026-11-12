use crate::{
    audio::{self, Cmd, Mixer},
    recording::Recorder,
};
use crossbeam_queue::ArrayQueue;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub fn run(dir: PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut log = Vec::new();
    let mut tracks = Vec::new();
    for (i, freq) in [220., 330.].iter().enumerate() {
        let path = dir.join(format!("fixture-deck-{}.wav", i + 1));
        let mut w = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|e| e.to_string())?;
        for n in 0..48000 * 3 {
            let x = ((n as f32 * freq * 2. * std::f32::consts::PI / 48000.).sin() * 5000.) as i16;
            w.write_sample(x).map_err(|e| e.to_string())?;
            w.write_sample(x).map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
        let (t, info) = audio::decode(path.to_str().unwrap())?;
        if info.waveform.is_empty() || info.duration < 2.9 {
            return Err("Decode verification failed".into());
        }
        tracks.push(t);
        log.push(format!(
            "PASS WAV {} decoded; duration={}; peaks={}",
            i + 1,
            info.duration,
            info.waveform.len()
        ));
    }
    let mut mix = Mixer::new(48000);
    let mp3 = dir.join("fixture-deck-1.mp3");
    if mp3.exists() {
        let (_, info) = audio::decode(mp3.to_str().unwrap())?;
        if info.duration < 2.8 {
            return Err("MP3 decode failed".into());
        }
        log.push(format!("PASS MP3 fixture decoded: {} sec", info.duration));
    }
    for (i, track) in tracks.iter().enumerate() {
        mix.apply(Cmd::Load(i, track.clone(), [None; 8], false));
        mix.apply(Cmd::Toggle(i));
    }
    let recorder = Recorder::new();
    recorder.start(dir.join("verified-master-mix.wav"), 48000)?;
    let mut max = 0f32;
    for _ in 0..48000 {
        let s = mix.frame();
        max = max.max(s[0].abs());
        recorder.push(s);
    }
    recorder.stop()?;
    let (recorded, info) = audio::decode(dir.join("verified-master-mix.wav").to_str().unwrap())?;
    if recorded.samples.len() != 48000 || max <= 0.01 {
        return Err("Master recording verification failed".into());
    }
    log.push(format!(
        "PASS two-deck master rendered and WAV recording re-decoded: {} sec, peak {}",
        info.duration, max
    ));
    let mp3_path = dir.join("verified-master-mix.mp3");
    recorder.start(mp3_path.clone(), 48000)?;
    for _ in 0..48000 {
        recorder.push(mix.frame());
    }
    recorder.stop()?;
    let (mp3, info) = audio::decode(mp3_path.to_str().unwrap())?;
    if !(0.95..1.15).contains(&info.duration)
        || !mp3.samples.iter().flatten().any(|x| x.abs() > 0.01)
        || mp3.samples.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Bundled MP3 recording verification failed".into());
    }
    log.push(format!(
        "PASS bundled MP3 utility encoded and finalized stereo master; re-decoded {} sec",
        info.duration
    ));
    let commands = Arc::new(ArrayQueue::new(256));
    let views = Arc::new(ArrayQueue::new(16));
    let error = Arc::new(Mutex::new(String::new()));
    let (device, rate) = audio::start(
        commands.clone(),
        views.clone(),
        error.clone(),
        Recorder::new(),
    )?;
    commands.push(Cmd::Master(0.)).ok();
    for (i, track) in tracks.iter().enumerate() {
        commands
            .push(Cmd::Load(i, track.clone(), [None; 8], false))
            .ok();
        commands.push(Cmd::Toggle(i)).ok();
    }
    let until = Instant::now() + Duration::from_secs(3);
    let mut last = None;
    while Instant::now() < until {
        while let Some(v) = views.pop() {
            last = Some(v)
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let view = last.ok_or_else(|| format!("No WASAPI callback: {}", error.lock().unwrap()))?;
    if view.decks.iter().any(|d| d.position < 1.) {
        return Err("Live deck positions did not advance".into());
    }
    log.push(format!("PASS real output callback: {device}, {rate} Hz, clock={}, deck positions={:?}. Muted test; audible listening not verified.",view.clock,view.decks.map(|d|d.position)));
    let input = crate::controller::read_xinput();
    log.push(format!(
        "Controller observed connected={}; physical button demonstration pending",
        input.connected
    ));
    let midi = midir::MidiInput::new("SkyDek verification").map_err(|e| e.to_string())?;
    let ports = midi.ports();
    log.push(format!("PASS Windows MIDI input enumeration: {} port(s). Physical MIDI control verification pending.", ports.len()));
    std::fs::write(dir.join("runtime-verification.txt"), log.join("\n"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
