# SkyDek MP3 encoder

This standalone executable receives raw interleaved stereo IEEE 754 f32
little-endian PCM on stdin and writes 320 kbps CBR MP3. All helper source and
its documentation are MIT licensed (see LICENSE). This grant applies only to
this helper directory; it does not license the proprietary parent application.
Dependencies retain their own licenses, including LAME's LGPL terms. The
parent distribution supplies dependency sources and licenses separately.

From this directory:

```powershell
cargo build --release --locked
cargo test --locked
```

CLI: `skydek-mp3-encoder OUTPUT SAMPLE_RATE`

The sample rate must be 32000 through 2147483647 Hz. Native 32000, 44100,
and 48000 Hz remain unchanged; other accepted rates are resampled by LAME to
48000 Hz, subject to LAME initialization support. Encoder initialization occurs
before output creation. On success, stdout emits exactly `READY\n` after the
output has been opened. The caller must read READY before sending PCM. The app enforces a five-second
handshake timeout and kills/reaps the child on failure. On EOF,
the helper flushes delayed audio and rewrites the Info/LAME duration, delay,
and padding tag, flushes the file, then exits zero. Empty streams are supported.
Incomplete stereo frames and non-finite samples fail. Failures exit nonzero
and emit one diagnostic capped at 1024 characters on stderr. Callers should
drain stderr concurrently and close stdin before waiting for exit.

The parent build script builds this crate with `--release --locked` and copies
the executable into `src-tauri/resources/encoder/`. Bundle that directory as
Tauri resources so the installed executable sits at
`resources/encoder/skydek-mp3-encoder.exe` relative to the application executable.
The app uses an absolute installed path, then its compile-time resources path
only in debug builds for development. Release builds never fall back to the
workspace. An explicit absolute `SKYDEK_ENCODER_PATH` overrides both;
no PATH executable search is used.

Before running parent tests, build the helper and set the override (from the
parent project directory):

```powershell
cargo build --manifest-path encoder-helper/Cargo.toml --locked
$env:SKYDEK_ENCODER_PATH = (Resolve-Path encoder-helper/target/debug/skydek-mp3-encoder.exe).Path
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

For source distribution, include Cargo.toml, Cargo.lock, LICENSE, README.md,
and all files under src/ (and tests/, if present). Dependency vendoring and
license/source packaging are performed separately by the parent packaging job.
Do not include build target directories or proprietary parent sources.
