# SkyDek DJ 0.10.21

This release adds a universal macOS DMG to the same GitHub release as the Windows installer. The Mac app keeps the same interface and `.dek` project format, includes a universal MP3 recording helper, and declares microphone access for mixing and recording. The SkyDek app and project icons are bundled on both systems.

The Windows and Mac applications use the same signed Tauri update feed. Each platform selects its own installer or updater bundle from the latest release. The Mac DMG includes a drag-to-Applications experience.

The Mac application uses ad-hoc Apple code signing because Apple Developer ID signing and notarization credentials are not configured. macOS may require the user to allow SkyDek DJ under Privacy & Security on first launch. Automated build and test checks do not replace physical Mac controller, microphone, playback, and update testing.
