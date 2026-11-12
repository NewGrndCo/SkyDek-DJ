// SPDX-License-Identifier: MIT
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Output(std::path::PathBuf);
impl Output {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "encoder-test-{}-{}.mp3",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn command(&self, rate: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_skydek-mp3-encoder"));
        command
            .arg(&self.0)
            .arg(rate)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[test]
fn readiness_precedes_pcm_and_split_reads_finalize_tag() {
    let output = Output::new();
    let mut child = output.command("48000").spawn().unwrap();
    let mut ready = [0; 6];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(&ready, b"READY\n");
    assert!(output.0.is_file());
    let mut input = child.stdin.take().unwrap();
    let frame = [0.25f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
    // Byte boundaries need not coincide with sample or stereo-frame boundaries.
    for _ in 0..100 {
        for byte in &frame {
            input.write_all(&[*byte]).unwrap();
        }
    }
    drop(input);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    let bytes = std::fs::read(&output.0).unwrap();
    assert!(bytes.windows(4).any(|w| w == b"Info"));
    assert!(bytes.windows(4).any(|w| w == b"LAME"));
    assert_eq!(bytes[2] >> 4, 14);
}
#[test]
fn invalid_initialization_preserves_existing_output() {
    let output = Output::new();
    std::fs::write(&output.0, b"keep existing").unwrap();
    let result = output.command("24000").output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read(&output.0).unwrap(), b"keep existing");
}
#[test]
fn missing_output_directory_never_announces_ready() {
    let output = Output(Output::new().0.join("missing.mp3"));
    let result = output.command("48000").output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(!result.stderr.is_empty());
    assert!(result.stderr.len() < 4096);
}
#[test]
fn malformed_pcm_exits_with_bounded_diagnostic() {
    for bytes in [
        vec![0],
        [f32::NAN.to_le_bytes(), 0.0f32.to_le_bytes()].concat(),
    ] {
        let output = Output::new();
        let mut child = output.command("48000").spawn().unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(!result.status.success());
        assert_eq!(result.stdout, b"READY\n");
        assert!(!result.stderr.is_empty());
        assert!(result.stderr.len() < 4096);
    }
}
