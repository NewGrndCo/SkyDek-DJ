# SkyDek DJ

Standalone Windows DJ application by New Ground Enterprise. This is the private development workspace. Public installer/documentation distributions are assembled separately under release/. VirtualDJ and loopMIDI are not required.

Current local build: **0.9.1 preview**. See [0.9.1 update notes](docs/UPDATE-0.9.1.md). The full build contract remains in docs/BUILD-CONTRACT.txt; this preview is not a completed implementation of every contracted feature.

## Development and verification

Use Node.js with built-in TypeScript stripping support, Rust's Windows MSVC toolchain, Visual Studio C++ Build Tools, Windows SDK, and WebView2.

```text
npm ci
npm run desktop
npm run build
npm run test:ui
npm run build:encoder
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run package
```

The Windows NSIS setup is built under src-tauri/target/release/bundle/nsis/. Application source and personal session files must not be copied into the public distribution.

## Music and library analysis

Import WAV/MP3 tracks or a folder. Folder navigation shows direct files in the selected folder; All tracks removes the folder filter. Search narrows that selection.

New and existing unprocessed tracks are analyzed in a background worker before playback. Duration is decoded from the file and BPM is estimated. The toolbar shows progress and can pause/resume the scan. General settings can rescan all tracks. Results and file errors persist in the existing session. Manual BPM values saved by older versions are preserved; the manual entry box is no longer in the deck UI.

Approximate BPM values are marked with ~. Some music has no reliable estimate, and half/double tempo can occur. Tempo ticks do not establish downbeat phase. Tempo matching uses the known BPM values; it is not beat-phase sync, and tempo changes also change pitch.

## Decks and waveforms

Load a track using its 1 or 2 button, or use A on the controller for a preparation deck. Playing decks reject unconfirmed controller replacement; explicit mouse replacement requires confirmation.

Click a waveform to seek. Drag the stacked waveform or a deck waveform left/right to scrub its playhead. Use the wheel or −/+ controls to zoom. Stacked waveforms share a zoom span; deck overviews have independent zoom and a Fit action. Arrow keys move 50 ms on a focused waveform; Shift+arrow moves one second. SET CUE places the main cue mark at the current position. Three hot cue buttons set or recall marks; Shift-click clears with confirmation.

B, keyboard C, and the on-screen CUE button preview from the cue mark. Release before the configured hold time to return and pause, or hold until the threshold and release to continue playback. General settings changes the threshold. Cue feedback uses a reserved layout slot. Space and Play toggle playback. Tempo and level are vertical controls with a tempo reset.

Effects are active while held; release disengages them. Backspin/brake can stop a deck as part of their effect. The effect/target captured at press is retained for release.

## Controller and mapping

Default Normal assignments:
- A: load selected track into preparation deck; B: cue hold; X: next Sidelist; Y: add to Sidelist.
- D-pad left/right: Folders → Library → Sampler focus. In Sampler, all four directions navigate the 4×4 grid. Left from its first column returns to Library; Shift+left also changes panels. Up/down browses the other focused sections.
- Left X: crossfader; L3: automatic crossfade; Right X/Y: filter/effect mix; R3: selected sampler.
- LB/RB: backspin/brake; Menu: target deck; View: Shift toggle.
- Shift includes explicit deck loading, echo/reverb, tempo matching, and next sample.

Settings → SkyDek Mapper manages both mapping layers and saved named profiles. The live Xbox artwork highlights input. Saved user assignments override defaults. The Xbox Share button is reserved by Windows and is not exposed by XInput. Connection transport is not guessed. Battery is an approximate driver-reported level when available.

Stick-clicks suppress that stick's axes until released and centered. Center both sticks after reconnecting. Crossfader position stays where released. General settings configures stick sensitivity and automatic-crossfade duration.

## Sampler

The performance view contains sixteen pads with filenames and active audio feedback. Drag assigned tiles to move/swap. Sampler settings opens a separate dialog: select a target pad, choose its mode, and browse files through a folder dropdown and search. Select one file and use Assign to selected pad or Add to next empty. An individual file can also be assigned directly.

Restart is the default: a press starts/restarts playback and release does not stop it. Toggle alternates play/stop; Overlap layers up to eight hits per pad; Hold stops on release. Modes save automatically. Stop all samples clears active voices. Assignments reference original file paths; keep the source files in place. Background restoration does not steal pad selection or panel focus.

## Workspace, output, and recording

Drag the separator above the bottom row to resize all three panels' height together. Vertical separators adjust folder/sampler width and the library takes the remaining share. Keyboard arrows also adjust a focused separator. Layout proportions persist locally; General settings can reset them. The toolbar Dark mode switch changes between dark and light appearance and saves that preference.

F11 or the header control toggles fullscreen. General settings selects Windows output devices; applying a different device saves the session and restarts the app. Connect external hardware and refresh the device list. Stop recording before changing outputs.

Choose WAV (32-bit float stereo) or MP3 (320 kbps stereo), then Record to select a destination. Stop REC finalizes the master recording. MP3 encoding runs on the recording worker; no separate encoder installation is required. Closing the window saves and hides to tray; Tray Exit fully exits. Active recording must be stopped first.

## Data and limits

The per-user application directory for com.newground.skydek-dj contains session.json. Library paths, queue, cues, mappings, sampler assignments/modes, analysis results, and preferences persist there. Panel proportions use the WebView's local storage. User audio is read-only.

Analysis streams files without retaining full-song PCM. Playback still decodes full tracks into memory and rejects tracks longer than thirty minutes. Unassigned decoded cache entries are pruned on subsequent decodes. Controller-triggered track loading still runs on the input worker and can delay input during a new decode. Separate headphone cueing, automatic downbeat/phase sync, pitch-preserving time stretch, and stems separation are unavailable.

## Native audio verification

```text
src-tauri/target/release/skydek-dj.exe --verify "C:\\absolute\\verification-output"
```

This generates test tones, checks decoding, renders/records/re-decodes a mix, and runs the native output callback with the master muted. A fixture-deck-1.mp3 in the verification directory also exercises MP3 decoding. These tests do not certify listening quality or extended hardware performance. Runtime logs are written to the supplied directory.
