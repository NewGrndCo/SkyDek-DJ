# SkyDek DJ 0.6.0 — sampler playback modes and input stability

## Sampler modes

Select a pad, open **Sampler settings / Add sounds**, and choose **Pad playback mode**. Changes save immediately. Existing pads default to Restart when the new setting is absent. Mode selection applies equally to mouse, keyboard activation of a pad, and whichever controller input is mapped to Sample.

| Mode | Press | Release | Repeated presses |
|---|---|---|---|
| Restart (default) | Start from the beginning | Continue playing | Restart that sample from the beginning |
| Toggle | Play if stopped; stop if playing | No change | Alternate play and stop |
| Overlap | Add another playback voice | Continue playing | Layer independent hits, up to eight per pad |
| Hold | Start from the beginning | Stop | Each hold previews the sample |

Restart, Toggle, and Overlap play to the end without requiring a hold. Overlap is polyphony, not automatic looping. At the eight-voice limit the oldest voice is replaced. Storage and rendering use fixed voice slots, without per-hit allocation in the audio callback. Changing a mode stops the affected pad; Apply this mode to all pads stops all samples and saves the selected mode across all sixteen pads.

Pad highlights now reflect actual engine voice activity. Stop all samples, clearing a pad, and replacing an assignment stop all its overlapping voices. Dragging/swapping pads moves both their voices and saved modes. Existing mapping profiles are retained.

## Input stability repairs

- Saved mappings observed before the update: normal R3 = Sample; normal L3 = Automatic crossfade. These are preserved. Remapping either input to Sample makes it use the selected pad's playback mode.
- Pressing L3 or R3 suppresses that stick's analog axes until the click is released and the stick returns to its dead zone. An incidental nudge while clicking can no longer immediately change the filter/effect mix or cancel an L3 crossfade.
- Stick dead zones now scale continuously from zero. Analog updates compare against the last value actually sent, so gradual small movements accumulate and returning to center is not missed.
- Background restoration of saved samples no longer moves the selected pad, changes the focused browser panel, or overwrites the last command after every file. Pads become triggerable only after their audio has been decoded and its assignment queued. Background restoration skips pads already replaced or cleared by the user.
- Sample press captures the pad and its playback mode atomically. Only Hold-mode controller presses register a stop-on-release action; the other modes have no release stop.

## Verification

- 33 Rust tests, including natural completion after retrigger, toggle, bounded polyphony, stopping all voices, replacement cleanup, mode persistence and swap, silent startup restore, and click/dead-zone input behavior.
- TypeScript/Vite production build and strict Clippy across all targets.
- Packaged Windows NSIS installer and muted native audio callback/recording verification. Physical listening and extended performance testing cannot be certified by the automated tests alone.

This release addresses the reproducible input and playback defects above. It does not claim that software can be guaranteed free of future bugs. Existing limitations in earlier notes, including full-file decoding and track loading on the controller worker, remain outside this sampler-focused update.
