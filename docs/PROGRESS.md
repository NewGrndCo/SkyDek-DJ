# SkyDek DJ progress ledger

Contract: BUILD-CONTRACT.txt. Visual target: approved-reference.png.
Project is separate from SkyDek Mapper. No VirtualDJ files are in scope.

Architecture decision: CPAL WASAPI output, Symphonia decoding on worker threads, bounded command and snapshot queues, native XInput polling. Audio callback owns both decks and sampler playback. No frontend timers drive playback or controller input.

## Checkpoint — 2026-11-07

Overall: working standalone foundation; full contract INCOMPLETE. No milestone is declared fully accepted where criteria remain untested.

## Subsequent update: 0.2.0

See UPDATE-0.2.0.md. Integrated Settings mapper, persisted Normal/Shift button/axis mappings, native three-second B cue gesture, main cue marks, recursive sampler sound browser with drag/Add, and durable pad assignment are now implemented. Fifteen tests pass. These items supersede the matching unfinished notes in the initial checkpoint below. New UI/physical gesture acceptance is pending installation of 0.2.0.

## Subsequent update: 0.3.0

See UPDATE-0.3.0.md. Corrected Windows folder filtering; retained imported roots; folder trees; first-empty sampler Add; sampler tile moves/swaps; focused library/sampler D-pad navigation and highlighted-pad R3. Eighteen tests pass. Native Windows gesture/hardware acceptance for these changes remains pending installation of 0.3.0. The full build contract is still incomplete.

### Implemented

- Native single-clock two-deck mixing, default WASAPI output, WAV/MP3 decoding off the audio callback.
- Shared mouse/controller commands for playback, volume, crossfader, cue and supported effects.
- Protected playing-deck loads; initial controller load/target behavior corrected.
- Real track overview and stacked waveforms; manual BPM grid and tempo-only matching.
- Eight hot cues per deck, four-beat loops with manual BPM, sixteen one-shot sampler pads.
- Filter, backspin, brake start/stop, echo and initial parallel-comb reverb.
- Post-mixer WAV recording through a bounded queue and separate writer, with finalize-on-stop and overrun detection.
- Library/folder import, search, queue, per-user JSON persistence for library/queue/hot cues.
- Native XInput scans four controller slots; edge-trigger actions; reconnect analog neutral arming.
- Branded desktop layout, Settings, tray Show/Exit, close-to-tray and single-instance handling.
- Separate branded NSIS installer.

### Verification evidence

- TypeScript/Vite production build passed.
- Rust compilation and seven meaningful mixer tests passed: crossfade endpoints, two-deck timing/mix, protected load and cue, sampler stop, hot cue recall/clear, loop bounds, backspin/brake.
- Release --verify exited 0. WAV and MP3 fixture decode passed; a one-second mixed master recording re-decoded with correct frame count; actual Realtek WASAPI callback advanced both decks. Runtime test was muted, so audible quality was not judged.
- Native UI inspected while running; imported library visible, track loaded, real waveform displayed, Settings opened. Mouse Play caused playback time to advance.
- User explicitly confirmed: desktop unlocked, Xbox connected, physical B starts/pauses playback. This is user-confirmed hardware evidence, not an automated listening test.
- Installer built successfully; installation/uninstallation on a fresh Windows environment not tested.

### Remaining contract work

M1: audible two-deck listening/latency testing, click-free ramps, fuller cue-commit behavior, load acknowledgement/state consistency under rapid commands, input sensitivity/calibration UI.

M2: automatic metadata/artwork, browser virtualization/sorting, crates/playlists, missing-file relinking, waveform zoom and corrected grid anchor, full drag/drop, durable cue saves without window close.

M3: persistent sampler bank/gain/loop modes, effect selection/visible active states and adjustable duration, distinct echo-out envelope, broader reverb/DSP tests, recording disk-full/recovery tests. Current effects are initial DSP implementations, not quality-certified DJ effects.

M4: automatic BPM/key analysis and confidence, beat-grid correction/tap tempo, phase sync, pitch-preserving time stretch, real stems engine/model/license/cache and triggers. Stems currently disabled with an explanation; controller trigger presses report unavailable.

M5: controller remapping profiles/import/export, all Shift actions and browsing repeat options, verified Bluetooth/USB transport detection, start-with-Windows UI, headphone routing, output-device/buffer configuration, bounded streaming/cache eviction, crash recovery, installer lifecycle and performance soak tests.

### Risks and deliberate limitations

- Prototype decodes tracks into memory, retains at most 24 decoded tracks, rejects tracks over 30 minutes. Sampler cache retention needs a bounded lifecycle before public release.
- Default float32 Windows output configuration only. Device removal recovery and non-float output are pending.
- Tempo changes alter pitch. Manual grids begin at time zero; phase sync is not implemented.
- Sample assignments are session-only. Session schema migration and corrupt-file recovery remain pending.
- Native controller worker performs load/decode synchronously relative to its own polling thread; audio continues, but controller polling may pause during a load. Separate load worker needed.
- Native effect targets remain on their selected deck; automatic target selection still needs ambiguity/volume-aware refinement.
- Connection reports XInput rather than guessing transport. Battery is approximate XInput level.
- Full transitive redistribution-license audit pending.

### Next action

Preserve this working checkpoint. Test two user-selected songs simultaneously and recording playback. Then repair load acknowledgement/worker scheduling and callback memory lifecycle before expanding stems and sync. Complete the contract incrementally without representing this checkpoint as a finished production application.

### Deliverables

- src/ — React UI
- src-tauri/src/ — mixer, decoder, XInput, recorder, command routing and native verification
- src-tauri/target/release/bundle/nsis/SkyDek DJ_0.1.0_x64-setup.exe
- docs/verification/runtime-verification.txt and generated test audio
- docs/BUILD-CONTRACT.txt and approved-reference.png
- README.md and docs/DEPENDENCIES.md
