# SkyDek DJ 0.3.0

Requested corrections:

- Music imports retain their root folder. The left Browser shows that root and its subfolders containing imported audio. Select a folder to see the tracks directly inside it in the adjacent searchable library. All tracks restores the combined view. Previously imported collections infer a common root until re-imported; source audio is never moved.
- Fixed ordinary Windows path separators and case-insensitive folder comparisons. Selecting a folder does not include similarly named sibling folders or dump descendant tracks into the current folder's list. Empty folders without imported audio are not shown.
- SFX folder browser uses the same directory model. Add now fills the first empty sampler pad. A full bank returns an error and preserves existing sounds. Dragging a sound file onto a particular tile still assigns that tile with replacement confirmation.
- Sampler tiles can be dragged to empty tiles or swapped with occupied tiles. Audio buffers, active voices, highlighted pad, and persistent paths move together. Native WebView drag interception is disabled so HTML tile drag/drop can operate.
- D-pad Left focuses Library; Right focuses Sampler (including older saved mappings using the generic panel action). Up/Down browses only tracks in the current library folder/search, or assigned sampler pads in sampler focus. R3 triggers the highlighted assigned pad. Mouse actions update focus/selection too. Focused panels have a violet outline.
- Existing B cue gesture and crossfader position retention remain.

Verification: production TypeScript build; 18 Rust tests, including exact Windows folder matching, navigation skipping filtered/empty rows, next-free pad allocation, full-bank refusal, pad move/selection/persistence and audio voice continuity. Interactive Windows drag/drop and physical D-pad/R3 workflow still require acceptance after updating. Do not treat engine tests as proof of GUI gestures.

Installation: Exit the old SkyDek DJ instance through its tray menu, stop/finalize any recording, install SkyDek DJ_0.3.0_x64-setup.exe, and reopen. Existing library, cue, mapping and sample-path data is migrated with defaults for the new fields.
