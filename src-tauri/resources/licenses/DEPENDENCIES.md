# SkyDek DJ 0.9.1 — dependency and source inventory

Exact versions are in package-lock.json and src-tauri/Cargo.lock. Release packaging generates fresh locked Windows Cargo metadata and complete notices for the resolved main graph, including build dependencies, frontend production dependencies, and the separate helper. Missing license text blocks packaging. Inventory inclusion does not imply runtime inclusion. Matching MPL dependency sources accompany the notices; the full main dependency tree is not vendored.

| Component | Purpose | Upstream license |
|---|---|---|
| React / React DOM | UI | MIT |
| TypeScript / Vite | Build | Apache-2.0 / MIT |
| Lucide | Icons | ISC |
| Tauri / single-instance plugin | Windows host and tray | MIT OR Apache-2.0 |
| CPAL 0.15 | WASAPI audio output | Apache-2.0 |
| Symphonia 0.5 | WAV / MP3 decoding | MPL-2.0 |
| Crossbeam queue | Bounded callback queues | MIT OR Apache-2.0 |
| rfd | Native dialogs | MIT |
| Hound | WAV writing | Apache-2.0 |
| Separate skydek-mp3-encoder helper | 320 kbps MP3 recording in a replaceable process | MIT authored helper source; LGPL-3.0 encoder crates; native LAME retains upstream COPYING and per-file notices |
| serde / serde_json | Persistence and IPC | MIT OR Apache-2.0 |

Symphonia: https://github.com/pdeljanov/Symphonia
CPAL: https://github.com/RustAudio/cpal

No stem model or proprietary DJ engine is bundled. Approved SkyDek brand art is reused from the existing workspace. Reference screenshot is design evidence, not a runtime UI background. No music from the screenshot is bundled. Runtime verification uses generated test tones only.

## MP3 recording in 0.9.1

The proprietary SkyDek DJ executable does not link to mp3lame or LAME. It launches the separate skydek-mp3-encoder.exe using a bounded raw stereo f32LE stdin interface, command-line output/rate arguments, readiness confirmation and process exit status. The standalone utility is bundled in resources/encoder and runs without a console window. WAV remains an in-process recording format. Both receive post-mixer stereo master audio.

The helper's complete authored source is MIT licensed. Its locked dependencies retain their respective licenses, including mp3lame-encoder/mp3lame-sys LGPL-3.0 and native LAME upstream notices. The public distribution provides the helper's full source, Cargo lockfile, build instructions, complete vendored dependency tree (including native C sources/build scripts), LGPL/GPL texts, and notices. The extracted source archive is built offline before publication. The utility can be replaced with an interface-compatible modified build; no proprietary app source or application object files are needed for that separate process. Third-party modification and debugging rights are not restricted by the application's proprietary status.

The main application's MPL dependency source is supplied separately. Public packaging excludes personal sessions, music, credentials, build output directories and proprietary application source. Preserve license texts, notices and matching source archives when redistributing. Architecture guidance: https://www.gnu.org/licenses/gpl-faq.html.en#MereAggregation

MP3 is 320 kbps CBR stereo, supports input rates at least32 kHz, and resamples non-native rates to48 kHz. Finalization flushes delayed audio and writes duration/gapless metadata. A decoder ignoring gapless tags can include padding. The main app uses installed resources in release builds; development fallback is debug-only. Failed utility startup leaves recording inactive, and stopping finalizes the open recording and surfaces writer errors.