# SkyDek DJ 0.9.1 — Windows Preview

A cleaner two-deck workspace with Xbox and mouse controls, library analysis, draggable/zoomable waveforms, customizable mappings and sixteen sound-effect pads.

Since 0.7.0:

- Background duration and estimated BPM analysis; explicit tempo SYNC.
- WAV and 320 kbps MP3 master recording.
- Faster FX Mix-controlled backspin and reworked stereo reverb.
- MP3 damaged-packet recovery and clearer errors for non-audio downloads.
- Smoother waveform animation, larger deck titles and SkyDek vinyl branding.
- Dark/light appearance, linked panel resizing and a separate sampler editor.
- Four-direction sampler grid navigation and stable cue feedback.

0.9.1 packages MP3 encoding as a separate, bundled utility. No extra installation or visible console is needed. Its full source and locked vendored dependencies are included; the proprietary DJ application source remains private.

Download the **Windows-Setup.exe** to install, or the complete **Windows.zip** for the installer, guide, notices and utility source. Exit SkyDek through its tray menu before updating. VirtualDJ and loopMIDI are not required.

Verification: application and frontend regression tests, encoder protocol tests, and installed WAV/MP3 recording/re-decoding and muted native output checks. Physical controller/live listening checks were not performed during this packaging pass.

Preview limits: estimated BPM can be half/double tempo; Sync matches tempo without beat-phase alignment or pitch preservation. Stems separation and separate headphone cueing remain unavailable. Installer is unsigned; checksums accompany the download.
