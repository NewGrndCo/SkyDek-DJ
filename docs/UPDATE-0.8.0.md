# SkyDek DJ 0.8.0 — library analysis and waveform workspace

## Library information before playback

Existing and newly imported tracks are scanned sequentially in the background. Duration comes from decoded frame counts. BPM is an onset-envelope autocorrelation estimate, shown with an approximate marker. Unknown/weak results remain blank instead of inventing a tempo. Previously entered manual BPM values are preserved. Results and individual file errors are saved with the library; missing files do not abort the remaining scan.

The toolbar displays progress and pauses the scan. Clicking Analyze library resumes unprocessed tracks. General settings can rescan the full library. Completed work is checkpointed every ten tracks and at scan completion/pause. Audio samples are streamed through the analyzer and not retained as full decoded tracks. BPM analysis examines up to the first three minutes; duration reads the complete supported file, including long mixes. The 30-minute limit still applies when loading full PCM for playback, not when scanning metadata.

Packet-local decode errors are skipped according to Symphonia's documented recovery contract, bounded by a consecutive-error limit. This prevents an isolated MP3 frame error from rejecting an otherwise decodable song. Duration reflects the frames successfully decoded, so damaged packets can shorten it slightly. Unrecognized formats remain explicit file errors. See the [decoder contract](https://docs.rs/symphonia-core/0.5.5/symphonia_core/codecs/trait.Decoder.html#tymethod.decode).

BPM estimates can select half/double tempo and are less reliable for changing-tempo, sparse, or non-rhythmic music. Waveform ticks indicate estimated tempo, not detected downbeats. This is not beat-phase synchronization or pitch-preserving time stretch. If an audio file changes at the same path, use Rescan all tracks. Files that failed a previous scan can be retried there too.

## Waveforms and cue placement

- Both stacked waveforms and deck waveforms support pointer dragging. Drag left to seek forward or right to seek backward; a click still seeks directly to that time.
- The wheel and −/+ buttons zoom each waveform. The two stacked waveforms share one zoom span. Deck waveforms zoom independently; Fit restores the full-track view.
- Dragging uses a fixed time anchor and animation-frame throttling, rather than compounding delayed playback snapshots. Keyboard Left/Right moves 50 ms, or one second with Shift.
- Loaded tracks request signed min/max peaks and RMS energy at 256-frame resolution, rendered as filled audio envelopes. Main cue and three hot cue markers are shown. No synthetic waveform shapes are substituted.
- Detailed peaks are requested only for loaded tracks and have a bounded frontend cache, rather than putting detailed peak data for the entire library into every live snapshot. Seek remains seek, not vinyl scratching; exact sample-level rendering is not claimed.
- Manual BPM entry is removed; detected BPM remains visible. Tempo is a vertical slider with a reset button.
- Cue preview feedback has an always-reserved height so hold messages do not change deck geometry.

## Panels and sampler organization

The three bottom panels have shared height, controlled by a horizontal separator above them. The two vertical separators adjust folder/sampler widths while the library takes the remaining share. Minimum sizes preserve usable controls. Separators support keyboard arrows, and workspace proportions persist locally. General settings offers Reset panel sizes.

Sampler settings now open in a separate focus-trapped dialog; the pad bank remains the performance view. The editor groups pad selection/mode/file assignment on one side and sound selection on the other. Folder navigation is a dropdown rather than an embedded directory tree. One chosen file uses shared Assign to selected pad / Add to next empty buttons. Lists are paginated to twenty files, and the dialog has one scrolling body, with no nested folder/file scroll containers.

## Verification

- TypeScript/Vite production build.
- 36 Rust tests: existing regressions plus signed peak/RMS data, regular-beat BPM estimates, silence rejection, streaming duration, and cancellation.
- Three Node tests cover anchored waveform dragging, zoomed/clamped seeking, and safe panel proportions from stored values. Run `npm run test:ui` on a current Node release with TypeScript stripping support.
- Strict Rust Clippy check with warnings denied.
- Native output/recording verification and actual desktop review are recorded separately during packaging. These checks do not certify perfect BPM detection or every display/hardware combination.

The final packaged executable passed WAV/MP3 decoding, master recording/re-decoding, and a muted real WASAPI callback check; see `verification/0.8.0/final/runtime-verification.txt`. A previously rejected user MP3 was also decoded successfully with packet recovery. The first desktop preview demonstrated linked top-waveform zoom and pointer seeking. Final deck-height corrections, sampler-dialog layout, and panel resizing still need a visual acceptance pass after restarting into this installer. Physical controller testing was unavailable during this pass. The running preview executable was left untouched; the final installer was built in a separate target directory and copied to the standard bundle location.

This is a local 0.8.0 preview build. The public GitHub release is not automatically changed by preparing this update. User session data was backed up as session-before-0.8.0.json before migration.
