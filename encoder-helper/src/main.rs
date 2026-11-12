// SPDX-License-Identifier: MIT
use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushGap, InterleavedPcm, Quality, VbrMode};
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
const WRITE_FRAMES: usize = 4096;
struct RecordingWriter {
    encoder: Encoder,
    file: BufWriter<File>,
    output: Vec<u8>,
}
impl RecordingWriter {
    fn create(path: &Path, rate: u32) -> Result<Self, String> {
        if rate < 32_000 || rate > i32::MAX as u32 {
            return Err("Invalid MP3 sample rate (minimum 32000 Hz)".into());
        }
        let output_rate = match rate {
            32_000 | 44_100 | 48_000 => rate,
            _ => 48_000,
        };
        // Initialize before creating the file so invalid settings cannot
        // truncate an existing recording.
        let encoder = Builder::new()
            .ok_or("Could not allocate MP3 encoder")?
            .with_num_channels(2)
            .and_then(|b| b.with_sample_rate(rate))
            .and_then(|b| b.with_output_sample_rate(std::num::NonZeroU32::new(output_rate)))
            .and_then(|b| b.with_brate(Bitrate::Kbps320))
            .and_then(|b| b.with_vbr_mode(VbrMode::Off))
            .and_then(|b| b.with_quality(Quality::NearBest))
            .and_then(|b| b.with_to_write_vbr_tag(true))
            .and_then(Builder::build)
            .map_err(|e| format!("MP3 encoder initialization failed: {e}"))?;
        Ok(Self {
            encoder,
            file: BufWriter::new(File::create(path).map_err(|e| e.to_string())?),
            output: Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(WRITE_FRAMES)),
        })
    }
    fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        self.output.clear();
        self.encoder
            .encode_to_vec(InterleavedPcm(samples), &mut self.output)
            .map_err(|e| format!("MP3 encoding failed: {e}"))?;
        self.file.write_all(&self.output).map_err(|e| e.to_string())
    }
    fn finalize(self) -> Result<(), String> {
        let Self {
            mut encoder,
            mut file,
            mut output,
        } = self;
        output.clear();
        // A standalone file needs the full flush, including delayed audio.
        encoder
            .flush_to_vec::<FlushGap>(&mut output)
            .map_err(|e| format!("MP3 finalization failed: {e}"))?;
        file.write_all(&output).map_err(|e| e.to_string())?;
        output.clear();
        output.reserve(encoder.lame_tag_size());
        encoder
            .lame_tag_encode_to_vec(&mut output)
            .ok_or("MP3 duration tag could not be finalized")?;
        // LAME emits a placeholder frame at the start. Replace it with
        // the completed Info/LAME tag (duration, encoder delay, padding).
        file.seek(SeekFrom::Start(encoder.id3v2_tag_size() as u64))
            .map_err(|e| e.to_string())?;
        file.write_all(&output).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("Usage: skydek-mp3-encoder OUTPUT SAMPLE_RATE")?;
    let rate = args
        .next()
        .and_then(|s| s.to_str().and_then(|s| s.parse::<u32>().ok()))
        .ok_or("Invalid sample rate")?;
    if args.next().is_some() {
        return Err("Too many arguments".into());
    }
    let mut writer = RecordingWriter::create(Path::new(&path), rate)?;
    // Surface output initialization errors before the caller starts recording.
    writer.file.flush().map_err(|e| e.to_string())?;
    {
        let mut out = io::stdout().lock();
        out.write_all(b"READY\n")
            .and_then(|_| out.flush())
            .map_err(|e| e.to_string())?;
    }
    let mut input = io::stdin().lock();
    let mut bytes = [0u8; WRITE_FRAMES * 8];
    let mut pending = 0;
    loop {
        let n = match input.read(&mut bytes[pending..]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(|e| e.to_string())?,
        };
        if n == 0 {
            break;
        }
        let total = pending + n;
        let complete = total / 8 * 8;
        let samples: Vec<f32> = bytes[..complete]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        if samples.iter().any(|s| !s.is_finite()) {
            return Err("PCM contains non-finite samples".into());
        }
        if !samples.is_empty() {
            writer.write(&samples)?;
        }
        bytes.copy_within(complete..total, 0);
        pending = total - complete;
    }
    if pending != 0 {
        return Err("Incomplete stereo f32le frame".into());
    }
    writer.finalize()
}
fn main() {
    if let Err(error) = run() {
        // One bounded diagnostic, never echo arbitrary input or paths.
        let bounded: String = error.chars().take(1024).collect();
        eprintln!("MP3 encoder: {bounded}");
        std::process::exit(1);
    }
}
