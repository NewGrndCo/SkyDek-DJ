use crate::audio;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

pub const VERSION: u8 = 3;
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub running: bool,
    pub completed: usize,
    pub total: usize,
    pub failed: usize,
    pub current: String,
    pub message: String,
}
#[derive(Debug)]
pub struct ResultInfo {
    pub duration: f64,
    pub bpm: Option<f64>,
    pub confidence: f32,
}

pub fn analyze(path: &str, cancel: &AtomicBool) -> Result<ResultInfo, String> {
    let mut energy = Vec::new();
    let mut sum = 0.;
    let mut count = 0;
    let (frames, rate) = audio::visit_audio(path, |frame, rate| {
        if cancel.load(Ordering::Relaxed) {
            return Err("Analysis cancelled".into());
        }
        if energy.len() < 18000 {
            sum += (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
            count += 1;
            if count >= (rate / 100).max(1) {
                energy.push((sum / count as f32).sqrt());
                sum = 0.;
                count = 0;
            }
        }
        Ok(())
    })?;
    let (bpm, confidence) = estimate(&energy, rate as f64 / (rate / 100).max(1) as f64);
    Ok(ResultInfo {
        duration: frames as f64 / rate as f64,
        bpm,
        confidence,
    })
}

/// Onset-envelope autocorrelation. Tempo only; this does not infer downbeats.
fn estimate(energy: &[f32], envelope_rate: f64) -> (Option<f64>, f32) {
    if energy.len() < 800 {
        return (None, 0.);
    }
    let mut previous = energy[0];
    let mut onset: Vec<f32> = energy
        .iter()
        .map(|&e| {
            let v = (e - previous).max(0.);
            previous = e;
            v
        })
        .collect();
    // Center the envelope: otherwise positive-only random transients create a
    // correlation floor and can masquerade as a confident tempo.
    let mean = onset.iter().sum::<f32>() / onset.len() as f32;
    for value in &mut onset {
        *value -= mean;
    }
    let power: f64 = onset.iter().map(|&x| (x * x) as f64).sum();
    if power < 1e-7 {
        return (None, 0.);
    }
    let correlation = |lag: f64| {
        let offset = lag.floor() as usize;
        let fraction = lag.fract();
        let mut numerator = 0.;
        let mut a = 0.;
        let mut b = 0.;
        for i in offset + 1..onset.len() {
            let x = onset[i] as f64;
            let y = onset[i - offset] as f64 * (1. - fraction)
                + onset[i - offset - 1] as f64 * fraction;
            numerator += x * y;
            a += x * x;
            b += y * y;
        }
        numerator / (a * b).sqrt().max(1e-12)
    };
    let mut best = (0., 0., 0.);
    for tenth in 600..=2000 {
        let bpm = tenth as f64 / 10.;
        let raw = correlation(60. * envelope_rate / bpm);
        let score = raw * (0.96 + 0.04 * (-(bpm - 115.).powi(2) / 3200.).exp());
        if score > best.0 {
            best = (score, bpm, raw);
        }
    }
    if best.2 < 0.16 {
        (None, best.2 as f32)
    } else {
        (Some(best.1), best.2.min(1.) as f32)
    }
}

/// Signed peaks and RMS at 256-frame resolution, requested only for loaded decks.
pub fn waveform(track: &audio::Track) -> Vec<[f32; 3]> {
    track
        .samples
        .chunks(256)
        .map(|chunk| {
            let mut low = 0f32;
            let mut high = 0f32;
            let mut power = 0f32;
            for frame in chunk {
                low = low.min(frame[0]).min(frame[1]);
                high = high.max(frame[0]).max(frame[1]);
                power += (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
            }
            [low, high, (power / chunk.len() as f32).sqrt()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_scan_reads_duration_and_supports_cancel() {
        let path = std::env::temp_dir().join(format!("skydek-analysis-{}.wav", std::process::id()));
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 12000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for n in 0..120000 {
            writer
                .write_sample(if n % 6000 < 120 { 12000i16 } else { 0i16 })
                .unwrap();
        }
        writer.finalize().unwrap();
        let info = analyze(path.to_str().unwrap(), &AtomicBool::new(false)).unwrap();
        assert!((info.duration - 10.).abs() < 0.01);
        assert!((info.bpm.unwrap() - 120.).abs() < 1.);
        assert!(analyze(path.to_str().unwrap(), &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn detects_regular_beats_and_rejects_silence() {
        for bpm in [77., 120., 150.] {
            let mut energy = vec![0.; 2000];
            for beat in 0..(20. * bpm / 60.) as usize {
                let at = (beat as f64 * 6000. / bpm).round() as usize;
                if at < energy.len() {
                    energy[at] = 1.;
                }
            }
            let (found, confidence) = estimate(&energy, 100.);
            assert!((found.unwrap() - bpm).abs() < 1., "{bpm}: {found:?}");
            assert!(confidence > 0.16);
        }
        assert_eq!(estimate(&vec![0.; 2000], 100.).0, None);
        assert_eq!(estimate(&vec![1.; 2000], 100.).0, None);
        let mut seed = 123456789u32;
        let noise: Vec<f32> = (0..6000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 8) as f32 / (1u32 << 24) as f32
            })
            .collect();
        assert_eq!(
            estimate(&noise, 100.).0,
            None,
            "Uncorrelated noise must not receive a fabricated BPM"
        );
    }
    #[test]
    fn waveform_keeps_signed_peaks_and_rms() {
        let track = audio::Track {
            samples: vec![[-0.75, 0.5]; 512],
            rate: 48000,
        };
        let data = waveform(&track);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0][0], -0.75);
        assert_eq!(data[0][1], 0.5);
        assert!(data[0][2] > 0.6);
    }
}
