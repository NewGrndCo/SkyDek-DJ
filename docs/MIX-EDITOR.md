# SkyDek DJ Mix Editor — Build 02

Mix Editor replaces the previous Mix Replay workflow. It reconstructs a recorded SkyDek performance across both decks, applies non-destructive automation edits, and exports a new WAV or MP3 without changing the original recording, mix log, or source media.

## Open and import

- **Open** accepts a native `.dek` Mix Editor session.
- **Import old mix log** accepts a Build 01 `.skydek.json`, `.json`, or Markdown file containing a SkyDek JSON block. Import creates an editable in-memory session; it never overwrites the older log.
- **Save As** converts an imported log into a native `.dek` session.
- Double-clicking an associated `.dek` file opens SkyDek DJ and presents that session in Mix Editor. If SkyDek is already running, the single-instance handoff opens the requested session in the existing process.

Build 01 logs do not capture independent EQ bands, loop identity, or individual hot-cue trigger identity. Mix Editor reports that limitation and does not invent missing automation.

## Editing workflow

1. Record a mix normally so SkyDek writes the recording and adjacent `.skydek.json` log.
2. Open **Mix Editor** from the Mix Log panel.
3. Import the older log or open a `.dek` session.
4. Relink any missing deck tracks, samples, or optional original recording.
5. Double-click an automation lane to add a keyframe. Drag a diamond to retime it, or use the Properties panel for precise time and value changes.
6. Select automation or sample events to duplicate or delete them.
7. Drag the ruler playhead or click a deck clip to seek. Recorded log markers are clickable.
8. Build and play the edited preview, then save the `.dek` session or export WAV/MP3.

## Session schema and compatibility

`.dek` is UTF-8 JSON with:

- `format: "skydek.mix"`
- explicit `version`
- session identity, timestamps, and generator metadata
- immutable recorded performance data
- media references and relinks
- non-destructive automation edits
- sample trigger events
- recorded/editor markers
- export settings
- migration notes

Schema version 1 remains compatible with earlier Build 02 beta sessions. Missing optional v1 metadata is deterministically derived when an older session opens. Unknown compatible top-level fields are preserved through the native Rust persistence layer so later builds can extend the format safely. Unsupported schema versions fail with an actionable error instead of being guessed.

Transient UI state—selection, zoom, scroll position, playhead, and current preview playback—is not authoritative session data.

## Source resolution and relinking

Sources resolve in this order:

1. explicit user relink
2. safe relative path stored beside the `.dek`
3. original absolute path
4. safe original relative path
5. matching filename beside the session

Relative paths containing parent traversal are rejected. Missing required deck/sample media disables preview and export until relinked. The original recorded mix is optional because the edited result is reconstructed from deck and sample sources.

When a session is saved, media located under the session folder receives a portable relative reference. Original paths remain available as diagnostics and are never rewritten on disk.

## Supported automation

- dual-deck recorded transport state and track changes
- deck level
- deck tempo/rate
- deck filter
- effect selection and wet mix
- crossfader
- master level
- sampler level
- one-shot sample events and event gain

Automation at the same timestamp uses a stable identifier ordering in both preview and export. Seeking reconstructs state from the canonical session instead of incrementally replaying stale UI state.

## Preview and rendering

Preview and export use the same Rust reconstruction and SkyDek mixer path. Preview is prepared to a temporary WAV and then played by the editor transport; edited sessions must rebuild their preview after a change.

- WAV: 48 kHz stereo, 32-bit float.
- MP3: 48 kHz stereo, 320 kbps through the bundled SkyDek encoder.
- Render progress is reported to the editor.
- Cancellation removes the partial output.
- Final output uses a temporary file and recoverable replacement so a previous valid export is not discarded if finalization fails.
- The session itself remains independent of exported audio.

The renderer supports sessions up to six hours. Individual decoded deck sources retain the desktop build's 30-minute safety limit.

## Keyboard controls

- Space: play/pause prepared preview
- Left/Right: seek 0.25 seconds
- Shift+Left/Right: seek 5 seconds
- Home/End: beginning/end
- Ctrl+Z / Ctrl+Y: undo/redo
- Ctrl+D: duplicate selected automation or sample event
- Ctrl+S / Ctrl+Shift+S: save/save as
- Delete/Backspace: remove selected automation or sample event
- Escape: close, with an unsaved-change warning

## Safety and recovery

Frontend and Rust validation reject malformed JSON, unsupported versions, out-of-order or invalid timestamps, NaN/infinite values, duplicate identifiers, invalid automation ranges, unsafe relative paths, and unreasonable collection sizes. Session saves are atomic and preserve the previous valid `.dek` on failure. Rendering and preview run off the UI thread and cannot run concurrently.

Errors avoid including environment variables or credentials. File paths may be shown when needed for relinking and diagnostics.
