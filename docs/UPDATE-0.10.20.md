# SkyDek DJ 0.10.20

This official update carries the recent controller, audio, and Mix Editor improvements into SkyDek DJ. The Windows installer and automatic update are signed by the release workflow.

- Microphone input selection, connection status, sample-rate conversion, and bounded buffering.
- Auto DJ's selected-folder source follows the folder selected in the browser.
- Xbox controller mapping labels remain outside the artwork; live feedback and the saved label toggle remain available.
- The beta app is isolated from the official signed updater. The official app continues to use the signed update feed.
- The SkyDek waveform mark is the new application icon. `.dek` Mix Editor projects have a separate document icon on Windows.
- Mix Editor now protects unsaved work, supports local preview and clip-only source replacement, and records finer sample/effect events and a separate microphone stem for new mixes.

The automated checks cover UI logic, Rust behavior, the production build, and packaging. Physical microphone routing, Xbox input, display scaling, long-mix performance, and an update from an older official installation still require hands-on testing. Older mix logs remain readable but cannot reconstruct microphone audio or short events that they never captured.
