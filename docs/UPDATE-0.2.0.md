# SkyDek DJ 0.2.0

## Requested changes

- Settings → SkyDek Mapper: integrated editor for all Xbox buttons/triggers and four stick axes, separate Normal/Shift layers, validated choices, Save mappings, Restore defaults.
- Default Normal B: cue preview from a fixed main mark. Release before three seconds returns to the mark and pauses. Release after three seconds continues playback. Native audio sample clock measures the hold. SET CUE changes the main mark. Mouse CUE and keyboard C use the same gesture.
- Cue release remains bound to its original deck and assignment even if the target/layer/map changes while held. Disconnect cancels without continuing playback. Keyboard blur cancels preview.
- SAMPLER → Open SFX folder: recursive WAV/MP3 sound library, search, pagination, folder Refresh, drag-to-pad, and destination selector/Add. Existing Assign file picker retained.
- Sampler pad paths and folder survive application restarts. Files are decoded on a background worker during assignment/restore. Raw sound browser data is fetched only on refresh instead of copying it on every transport update.

## Validation

- Rust tests: 15 passed, including cue tap, exact three-second commit, sub-threshold release, disconnect cancellation, deck isolation, mapping validation/JSON roundtrip, old-session migration, recursive sound scanning, pad assignment/play command and persistence.
- TypeScript and production frontend build passed.
- Windows NSIS package built as SkyDek DJ_0.2.0_x64-setup.exe.
- New physical B gesture, interactive mapping edits, and HTML drag-to-pad still require user acceptance in the updated application. Prior 0.1.0 user-confirmed B play/pause is historical evidence only; it is not evidence for the new gesture.

## Install

Exit the currently running SkyDek DJ using its tray Exit option (closing the window only hides it). Stop/finalize a recording first if one is active. Install 0.2.0, then reopen. Existing library/session data is retained.

This update does not complete the remaining full build contract. Real stems, automatic beat analysis, and the other previously documented unfinished features remain pending.
