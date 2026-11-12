# SkyDek DJ 0.5.0

## Controls

- Deck effects and samples are momentary: press to start, release to stop. Controller releases retain the original deck/effect or pad even if the target, selected pad, Shift layer, or mapping changes during the hold. Disconnect cancels held commands. Effects expose their actual active state to the UI and highlight the corresponding Audio FX button. A completed brake/backspin can stop the deck as part of that effect.
- Mouse/touch/Space/Enter on effect and sampler buttons use the same gated behavior. Lost pointer capture, blur, and unmount release held UI commands.
- General settings saves cue hold time from 0.1 to 10 seconds. Default remains 3 seconds. This applies to controller, keyboard C, and on-screen CUE. Short holds return to cue; holds reaching the configured duration continue playback on release.
- L3 defaults to Automatic crossfade. General settings configures the full-width transition duration (0.1–10 seconds). Manual stick/slider movement cancels the ramp. Existing unassigned L3 defaults are migrated once; later user remapping is retained.
- D-pad left/right moves focus across Folders, Library, Sampler. Up/down changes the folder (and its visible tracks), library track, or occupied sampler pad. Mapper reassignments remain authoritative; D-pad defaults are used only when mapped to panel/browse actions.
- Three hot cue buttons are displayed. Existing saved extra cue slots are preserved in session data to avoid data loss.

## Mapper and display

- Mapper settings include the original white Xbox controller artwork with aligned circular button glows, directional stick glows, and live trigger/shoulder feedback. Clicking an artwork control focuses its assignment.
- Named mapping profiles can be saved, selected, and deleted. Save mappings updates the active profile and takes effect immediately. Profile operations validate both layers, persist to the existing session file, and roll back in-memory profile state if persistence fails. At least one profile is retained.
- Mapping select menus use explicit dark option backgrounds and light text. The header crops the decorative outer border from the existing brand image; the canonical asset is unchanged.
- The header shows XInput connection and approximate battery level when supplied by the driver. If not reported, it explicitly says so. It does not invent Bluetooth/USB identification or a percentage.
- Header fullscreen control and F11 toggle native fullscreen.

## Sampler

- Default view shows only the 16 sound pads, their filenames, and bottom controls. Select a pad and open Sampler settings / Add sounds to replace the pad panel with folder browsing and assignment tools. Back to sound pads returns to performance mode.
- Add fills the next empty pad. To selected pad assigns to the chosen tile (with replacement confirmation). A selected pad can be cleared from the assignment panel. Pads remain draggable for reordering/swapping in performance mode. Filenames are normalized correctly for Windows paths and shown with wrapping and full hover text.

## Audio output

- General settings lists Windows output devices, including external interfaces exposed through WASAPI. Refresh after connecting an interface. Apply output & restart saves the session and restarts SkyDek using the chosen output. This is an explicit restart, not seamless live device switching. Recording must be stopped first. The selection persists; Windows default remains an option.
- The engine selects a supported float32 configuration if the device default format is different. Missing selected devices produce a recoverable settings error instead of silently claiming they are connected. Device names are used as CPAL's enumeration identifiers; identical device names cannot currently be distinguished.

## Validation and limits

27 Rust tests cover mixer behavior, release isolation, configurable cue timing, auto crossfade/manual takeover, folder traversal, profile persistence/switch/delete and mapping validation, sampler persistence and existing functions. TypeScript/Vite build and strict Clippy checks are run before packaging. Native verification uses muted generated audio; listening quality and physical controller interaction still require user testing. Actual external-interface playback is not certified without that interface.

The existing session was backed up next to session.json as session-before-0.5.0.json before upgrading. The application identifier and existing library storage remain unchanged. Existing full-build limitations (stems separation, phase sync, full-file decode memory use, controller-worker decode delays) remain documented in earlier release notes.
