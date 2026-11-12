use crossbeam_queue::ArrayQueue;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};

const WRITE_FRAMES: usize = 4096;

enum RecordingWriter {
    Wav(hound::WavWriter<BufWriter<File>>),
    Mp3(Mp3Process),
}

struct Mp3Process {
    child: std::process::Child,
    input: Option<BufWriter<std::process::ChildStdin>>,
    stderr: Option<std::thread::JoinHandle<String>>,
}
impl Mp3Process {
    fn create(path: &Path, rate: u32) -> Result<Self, String> {
        let encoder_name = if cfg!(windows) { "skydek-mp3-encoder.exe" } else { "skydek-mp3-encoder" };
        let installed = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Executable has no parent")?
            .join(format!("resources/encoder/{encoder_name}"));
        let executable = if let Some(explicit) = std::env::var_os("SKYDEK_ENCODER_PATH") {
            let explicit = PathBuf::from(explicit);
            if !explicit.is_absolute() {
                return Err("SKYDEK_ENCODER_PATH must be absolute".into());
            }
            explicit
        } else if installed.is_file() {
            installed
        } else {
            #[cfg(debug_assertions)]
            {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("resources/encoder/{encoder_name}"))
            }
            #[cfg(not(debug_assertions))]
            {
                installed
            }
        };
        let mut command = std::process::Command::new(executable);
        command
            .arg(path)
            .arg(rate.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start MP3 encoder: {e}"))?;
        let mut errors = child.stderr.take().ok_or("Missing encoder stderr")?;
        // Drain concurrently even after the retained diagnostic reaches its limit.
        let stderr = std::thread::spawn(move || {
            let mut retained = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                match errors.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let keep = n.min(4096usize.saturating_sub(retained.len()));
                        retained.extend_from_slice(&buffer[..keep]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            String::from_utf8_lossy(&retained).trim().to_owned()
        });
        let mut stdout = child.stdout.take().ok_or("Missing encoder stdout")?;
        let input = child.stdin.take().map(BufWriter::new);
        let mut process = Self {
            child,
            input,
            stderr: Some(stderr),
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let handshake = std::thread::spawn(move || {
            let mut ready = [0u8; 6];
            let valid = stdout.read_exact(&mut ready).is_ok() && ready == *b"READY\n";
            let _ = sender.send(valid);
        });
        let ready = receiver.recv_timeout(std::time::Duration::from_secs(5));
        if !matches!(ready, Ok(true)) {
            let _ = process.child.kill();
            let diagnostic = process
                .finish()
                .err()
                .unwrap_or_else(|| "Missing READY handshake".into());
            let _ = handshake.join();
            return Err(format!(
                "MP3 encoder initialization failed (READY required within 5 seconds): {diagnostic}"
            ));
        }
        let _ = handshake.join();
        Ok(process)
    }
    fn finish(&mut self) -> Result<(), String> {
        let flushed = self.input.take().map(|mut input| input.flush()).transpose();
        let status = self.child.wait().map_err(|e| e.to_string());
        let diagnostic = self
            .stderr
            .take()
            .map(|thread| thread.join().unwrap_or_default())
            .unwrap_or_default();
        let status = status?;
        if !status.success() {
            return Err(format!("MP3 encoder exited with {status}: {diagnostic}"));
        }
        flushed.map_err(|e| format!("MP3 input flush failed: {e}"))?;
        Ok(())
    }
}
impl Drop for Mp3Process {
    fn drop(&mut self) {
        if self.stderr.is_some() {
            self.input.take();
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(thread) = self.stderr.take() {
                let _ = thread.join();
            }
        }
    }
}
impl RecordingWriter {
    fn create(path: &Path, rate: u32) -> Result<Self, String> {
        if rate == 0 || rate > i32::MAX as u32 {
            return Err("Invalid recording sample rate".into());
        }
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("mp3") => {
                if rate < 32_000 {
                    return Err("MP3 recording requires a sample rate of at least 32000 Hz".into());
                }
                Mp3Process::create(path, rate).map(Self::Mp3)
            }
            Some(ext) if !ext.eq_ignore_ascii_case("wav") => {
                Err("Unsupported recording extension; choose .wav or .mp3".into())
            }
            _ => hound::WavWriter::create(
                path,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .map(Self::Wav)
            .map_err(|e| e.to_string()),
        }
    }
    fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        match self {
            Self::Wav(writer) => {
                for &sample in samples {
                    writer.write_sample(sample).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            Self::Mp3(process) => {
                let mut bytes = Vec::with_capacity(samples.len() * 4);
                for sample in samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                process
                    .input
                    .as_mut()
                    .ok_or("MP3 encoder input closed")?
                    .write_all(&bytes)
                    .map_err(|e| format!("MP3 stream failed: {e}"))
            }
        }
    }
    fn finalize(self) -> Result<(), String> {
        match self {
            Self::Wav(writer) => writer.finalize().map_err(|e| e.to_string()),
            Self::Mp3(mut process) => process.finish(),
        }
    }
}

pub struct Recorder {
    pub active: AtomicBool,
    pub overflow: AtomicBool,
    pub frames: AtomicU64,
    pub queue: ArrayQueue<[f32; 2]>,
    pushing: AtomicUsize,
    writer: Mutex<Option<std::thread::JoinHandle<Result<(), String>>>>,
    pub path: Mutex<String>,
}
impl Recorder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            frames: AtomicU64::new(0),
            queue: ArrayQueue::new(48000 * 5),
            pushing: AtomicUsize::new(0),
            writer: Mutex::new(None),
            path: Mutex::new(String::new()),
        })
    }
    /// True until stop joins the writer, including after overflow or a write error.
    /// For lifecycle/UI guards only; this locks and must not run in the audio callback.
    pub fn is_open(&self) -> bool {
        self.writer.lock().unwrap().is_some()
    }

    pub fn push(&self, s: [f32; 2]) {
        if self.active.load(Ordering::Relaxed) {
            // Register an in-flight producer so stop cannot finalize between
            // the active check and the queue push. No locks or allocation here.
            self.pushing.fetch_add(1, Ordering::SeqCst);
            if !self.active.load(Ordering::SeqCst) {
                self.pushing.fetch_sub(1, Ordering::SeqCst);
                return;
            }
            if self.queue.push(s).is_err() {
                self.overflow.store(true, Ordering::Relaxed);
                self.active.store(false, Ordering::SeqCst);
            } else {
                self.frames.fetch_add(1, Ordering::Relaxed);
            }
            self.pushing.fetch_sub(1, Ordering::SeqCst);
        }
    }
    /// Start stereo recording: .mp3 selects 320 kbps MP3; .wav (or no extension)
    /// selects the original 32-bit float WAV output. Extensions ignore case.
    pub fn start(self: &Arc<Self>, path: PathBuf, rate: u32) -> Result<(), String> {
        let mut handle = self.writer.lock().unwrap();
        if handle.is_some() {
            return Err("Finish the current recording first".into());
        }
        let mut writer = RecordingWriter::create(&path, rate)?;
        while self.queue.pop().is_some() {}
        self.frames.store(0, Ordering::Relaxed);
        self.overflow.store(false, Ordering::Relaxed);
        *self.path.lock().unwrap() = path.to_string_lossy().into_owned();
        self.active.store(true, Ordering::Release);
        let r = self.clone();
        *handle = Some(std::thread::spawn(move || {
            let mut samples = Vec::with_capacity(WRITE_FRAMES * 2);
            loop {
                // Observe shutdown before draining, then wait for any producer
                // that already passed its active check. Accepted tail frames
                // must be written before finalizing either format.
                let finished =
                    !r.active.load(Ordering::SeqCst) && r.pushing.load(Ordering::SeqCst) == 0;
                samples.clear();
                while samples.len() < WRITE_FRAMES * 2 {
                    let Some(frame) = r.queue.pop() else { break };
                    samples.extend_from_slice(&frame);
                }
                if !samples.is_empty() {
                    if let Err(e) = writer.write(&samples) {
                        r.active.store(false, Ordering::SeqCst);
                        let _ = writer.finalize();
                        return Err(e);
                    }
                    continue;
                }
                if finished {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            writer.finalize()?;
            if r.overflow.load(Ordering::Relaxed) {
                Err("Recording stopped: disk writer fell behind; file was finalized".into())
            } else {
                Ok(())
            }
        }));
        Ok(())
    }
    pub fn stop(&self) -> Result<(), String> {
        // Hold the lifecycle lock through join so another start cannot reactivate
        // the recorder while the previous file is still being finalized.
        let mut handle = self.writer.lock().unwrap();
        self.active.store(false, Ordering::SeqCst);
        if let Some(h) = handle.take() {
            h.join()
                .map_err(|_| "Recording writer failed".to_string())??
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);
    impl TestFile {
        fn new(extension: &str) -> Self {
            Self(std::env::temp_dir().join(format!(
                "skydek-recording-{}-{}.{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed),
                extension,
            )))
        }
    }
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn tone(frame: usize, rate: u32) -> [f32; 2] {
        let phase = frame as f32 * std::f32::consts::TAU / rate as f32;
        [(phase * 440.).sin() * 0.4, (phase * 880.).sin() * 0.2]
    }

    fn decode_mp3(path: &Path, gapless: bool) -> (u32, Vec<f32>, Option<u64>) {
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let mut format = symphonia::default::get_probe()
            .format(
                &hint,
                MediaSourceStream::new(Box::new(File::open(path).unwrap()), Default::default()),
                &FormatOptions {
                    enable_gapless: gapless,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
            .unwrap()
            .format;
        let track = format.default_track().unwrap();
        let rate = track.codec_params.sample_rate.unwrap();
        let declared_frames = track.codec_params.n_frames;
        assert_eq!(track.codec_params.channels.unwrap().count(), 2);
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .unwrap();
        let mut samples = Vec::new();
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => panic!("MP3 packet error: {e}"),
            };
            let decoded = decoder.decode(&packet).unwrap();
            let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            buffer.copy_interleaved_ref(decoded);
            samples.extend_from_slice(buffer.samples());
        }
        (rate, samples, declared_frames)
    }

    #[test]
    fn mp3_round_trip_duration_rates_stereo_and_bitrate() {
        for input_rate in [32_000, 44_100, 48_000, 96_000] {
            for frames in [
                1,
                input_rate as usize / 10 + 17,
                input_rate as usize * 2 + 137,
            ] {
                let path = TestFile::new("MP3");
                let recorder = Recorder::new();
                recorder.start(path.0.clone(), input_rate).unwrap();
                for n in 0..frames {
                    recorder.push(tone(n, input_rate));
                }
                recorder.stop().unwrap();
                assert_eq!(recorder.frames.load(Ordering::Relaxed), frames as u64);
                assert!(!recorder.overflow.load(Ordering::Relaxed));
                let (rate, samples, declared_frames) = decode_mp3(&path.0, true);
                assert_eq!(rate, input_rate.min(48_000));
                let expected = frames as f64 * rate as f64 / input_rate as f64;
                let actual = samples.len() / 2;
                // Native rates trim exactly. LAME's resampling filter can add
                // a few samples (including for a one-frame recording).
                let tolerance = if input_rate == rate { 0. } else { 16. };
                assert!(
                    (actual as f64 - expected).abs() <= tolerance,
                    "{input_rate} Hz: {frames} input frames, {actual} decoded, expected {expected}"
                );
                assert_eq!(declared_frames, Some(actual as u64));
                if frames > input_rate as usize {
                    // The first MPEG header must specify the MPEG-1 320 kbps index.
                    let bytes = std::fs::read(&path.0).unwrap();
                    assert_eq!(bytes[0], 0xff);
                    assert_eq!(bytes[1] & 0xf8, 0xf8);
                    assert_eq!(bytes[2] >> 4, 14);
                    let duration = frames as f64 / input_rate as f64;
                    let kbps = bytes.len() as f64 * 8. / duration / 1000.;
                    assert!((320. ..340.).contains(&kbps), "bitrate: {kbps}");
                    let mut energy = [0.; 2];
                    for frame in samples.chunks_exact(2) {
                        for ch in 0..2 {
                            assert!(frame[ch].is_finite());
                            energy[ch] += frame[ch] * frame[ch];
                        }
                    }
                    let left_rms = (energy[0] / actual as f32).sqrt();
                    let right_rms = (energy[1] / actual as f32).sqrt();
                    assert!((0.24..0.32).contains(&left_rms), "left RMS: {left_rms}");
                    assert!((0.11..0.17).contains(&right_rms), "right RMS: {right_rms}");
                    // The app's current decoder uses default (non-gapless) options.
                    let (_, untrimmed, _) = decode_mp3(&path.0, false);
                    let extra = untrimmed.len() / 2 - actual;
                    assert!(extra <= 2304, "excessive MP3 delay/padding: {extra}");
                }
            }
        }
    }

    #[test]
    fn wav_regression_preserves_float_samples_rate_and_tail() {
        for rate in [44_100, 48_000, 96_000] {
            let path = TestFile::new("WAV");
            let recorder = Recorder::new();
            recorder.start(path.0.clone(), rate).unwrap();
            let frames = rate as usize + 137;
            for n in 0..frames {
                recorder.push(tone(n, rate));
            }
            recorder.stop().unwrap();
            let mut reader = hound::WavReader::open(&path.0).unwrap();
            assert_eq!(
                reader.spec(),
                hound::WavSpec {
                    channels: 2,
                    sample_rate: rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                }
            );
            assert_eq!(reader.duration(), frames as u32);
            let actual: Vec<f32> = reader.samples::<f32>().map(Result::unwrap).collect();
            let expected: Vec<f32> = (0..frames).flat_map(|n| tone(n, rate)).collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn empty_recordings_finalize_and_recorder_restarts_across_formats() {
        let recorder = Recorder::new();
        for ext in ["mp3", "wav", "mp3"] {
            let path = TestFile::new(ext);
            recorder.start(path.0.clone(), 48_000).unwrap();
            assert!(recorder.start(path.0.clone(), 48_000).is_err());
            recorder.stop().unwrap();
            recorder.stop().unwrap();
            assert_eq!(recorder.frames.load(Ordering::Relaxed), 0);
            assert!(!recorder.active.load(Ordering::Acquire));
            if ext == "wav" {
                assert_eq!(hound::WavReader::open(&path.0).unwrap().duration(), 0);
            } else {
                let (_, samples, _) = decode_mp3(&path.0, true);
                assert!(samples.is_empty());
            }
        }
    }

    #[test]
    fn stopped_producer_still_requires_writer_join() {
        let recorder = Recorder::new();
        let path = TestFile::new("mp3");
        assert!(!recorder.is_open());
        recorder.start(path.0.clone(), 48_000).unwrap();
        assert!(recorder.is_open());
        // Model the producer shutdown used by overflow/write errors. A completed
        // writer handle remains open until stop collects its result.
        recorder.active.store(false, Ordering::SeqCst);
        assert!(recorder.is_open());
        assert!(recorder.start(path.0.clone(), 48_000).is_err());
        recorder.stop().unwrap();
        assert!(!recorder.is_open());
    }

    #[test]
    fn invalid_format_or_rate_does_not_truncate_or_activate() {
        for (ext, rate) in [("flac", 48_000), ("mp3", 0), ("mp3", 24_000), ("wav", 0)] {
            let path = TestFile::new(ext);
            std::fs::write(&path.0, b"existing recording").unwrap();
            let recorder = Recorder::new();
            assert!(recorder.start(path.0.clone(), rate).is_err());
            assert!(!recorder.active.load(Ordering::Acquire));
            assert!(recorder.writer.lock().unwrap().is_none());
            assert_eq!(std::fs::read(&path.0).unwrap(), b"existing recording");
            recorder.stop().unwrap();
        }
    }

    #[test]
    fn file_creation_errors_leave_recorder_inactive_and_reusable() {
        let missing_directory = TestFile::new("missing");
        for ext in ["mp3", "wav"] {
            let recorder = Recorder::new();
            let error = recorder
                .start(missing_directory.0.join(format!("mix.{ext}")), 48_000)
                .unwrap_err();
            assert!(!error.is_empty());
            assert!(!recorder.active.load(Ordering::Acquire));
            assert!(recorder.writer.lock().unwrap().is_none());
            assert!(recorder.path.lock().unwrap().is_empty());
            let path = TestFile::new(ext);
            recorder.start(path.0.clone(), 48_000).unwrap();
            recorder.push([0.25, -0.5]);
            recorder.stop().unwrap();
        }
    }

    #[test]
    fn concurrent_stop_drains_every_accepted_wav_frame() {
        for _ in 0..20 {
            let path = TestFile::new("wav");
            let recorder = Recorder::new();
            recorder.start(path.0.clone(), 48_000).unwrap();
            let producer = recorder.clone();
            let thread = std::thread::spawn(move || {
                for _ in 0..100_000 {
                    producer.push([0.25, -0.5]);
                }
            });
            while recorder.frames.load(Ordering::Relaxed) < 100 {
                std::thread::yield_now();
            }
            recorder.stop().unwrap();
            thread.join().unwrap();
            assert_eq!(
                hound::WavReader::open(&path.0).unwrap().duration() as u64,
                recorder.frames.load(Ordering::Relaxed)
            );
            assert!(recorder.queue.is_empty());
        }
    }
}
