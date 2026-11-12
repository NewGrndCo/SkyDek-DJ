# SkyDek DJ 0.9.0

Implemented WAV/320kbps MP3 master recording, explicit tempo Sync, effective BPM display, wet-controlled fast backspin, damped stereo reverb, bounded animation-frame waveform prediction, light/dark appearance, spatial 4x4 sampler navigation, original logo centers and larger titles.

Validation: 53 Rust tests passed, one optional external fixture test ignored; 15 frontend logic tests passed; TypeScript/Vite build and strict Clippy passed. Native muted WASAPI/recording check passed. Actual sampler assignment of Bang Lazer Jordan.mp3 passed. Reported six failing SFX files passed decoder/sampler rendering checks. Library rescan decoded795/796 files; remaining file is HTML named .mp3. Added explicit actionable error for web-page files.

Desktop checked dark/light workspace, light settings and sampler editor, recording-format selection, corrected non-overlapping deck controls, and installed0.9.0. Physical controller disconnected: live D-pad verification and audible effects assessment remain pending. Sync command math and limits tested; final two-deck mouse acceptance interrupted by foreground-window availability.

The registered installed executable was0.5.0; installed0.9.0 through NSIS to update the shortcut target. Final installer at src-tauri/target/release/bundle/nsis/. No GitHub publication. MP3 uses static LGPL dependencies: license texts bundled, redistribution/source-relink obligations documented in DEPENDENCIES.md; public release compliance materials remain separate from this local installer.
