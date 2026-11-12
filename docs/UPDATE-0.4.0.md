# SkyDek DJ 0.4.0 — stability repairs

Inspected the controller worker, command routing, audio mixer/decoder, recording verification, sampler assignment and persistence, folder browsing, waveform rendering, and mapper settings. Existing architecture and user session files are preserved. This is a focused repair release, not a claim that the full build contract is complete.

## Repairs

- Crossfader stick movement now sends relative changes to the audio engine, which accumulates them against its authoritative position. Previously each input poll calculated an absolute position from delayed snapshots. The crossfader remains where released.
- Trigger actions use 14% press / 6% release hysteresis to prevent repeated actions around one threshold.
- New tracks start at normal playback speed instead of inheriting a previous tempo adjustment. Seeking cancels brake/backspin and active loops so they do not pull the playhead away from a clicked location.
- Both stacked waveforms and deck overview waveforms support click-to-seek. Click the desired time, then SET CUE to store the cue there. Stacked waveform clicks use the visible 16-second window; overviews use the entire track.
- Sampler pads now have Clear controls. Clearing stops that voice, removes its saved assignment, and frees a slot for Add. Add still fills the first empty pad; the bank contains 16 pads. Drag onto a specific tile to replace it or drag tiles to swap them.
- Decoded cache entries no longer accumulate until a fixed 24/48-entry limit blocks new tracks or sounds. Entries not assigned to decks or sampler pads are pruned on subsequent decodes.
- Filter coefficients are computed when the filter changes instead of using exponential calculations for every audio sample.
- Old audio snapshots are ignored when saving cue data for a newly loaded track, preventing the previous track's cue data from being copied to the new track.
- Startup repairs invalid or zero crossfader sensitivity values.
- Settings → SkyDek Mapper includes a live input monitor with button states, stick direction plots, trigger meters, and connection status. It polls independently only while the mapper panel is mounted and does not generate playback commands.
- Rust static analysis warnings were cleaned up without replacing working subsystems.

## Verification

- React/TypeScript production build.
- 21 Rust tests, including relative crossfader accumulation, new-track speed reset, seek/cue behavior, sampler clearing/reassignment, existing cue gestures, navigation, mapping validation, and sampler persistence.
- Strict Clippy check across all targets, with warnings treated as errors.
- Packaged Windows NSIS installer. Native output/recording verification results are stored in `docs/verification/0.4.0/runtime-verification.txt` when run on this machine.

## Practical limits

The original symptom could have multiple causes: new-track speed inheritance is fixed, but this does not prove all audible slowdowns are eliminated. Audible quality, extended-session stability, physical controller navigation, and drag/drop must still be exercised interactively. The native verification uses muted output and generated test tones; it cannot certify what the user hears.

Large audio is still decoded entirely into memory. Track decoding triggered by a controller load still runs in the controller worker, and some audio callback memory lifecycle operations remain work for a later architecture pass. Automatic beat-grid analysis, phase sync, pitch-preserving stretch, and real stems separation remain outside this repair release. Do not enable or advertise them as complete.

Install the 0.4.0 package after exiting the previous instance from its tray menu. Closing its window alone leaves it running. The application identifier is unchanged so existing library, mapping, and sampler configuration can be reused.
