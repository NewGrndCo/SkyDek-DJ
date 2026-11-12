## SkyDek DJ 0.6.0 — Windows preview

A standalone DJ application for mixing local music with an Xbox controller or a mouse. Includes two decks, waveform seeking, cue points, deck effects, a customizable sound-effects sampler, WAV recording, and saved controller mapping profiles.

### Download

- **SkyDek-DJ-0.6.0-Windows.zip** — recommended: installer, README, brand asset, and third-party notices.
- **SkyDek-DJ-0.6.0-Windows-Setup.exe** — installer only; see the repository README for setup and usage.

Extract the ZIP, exit an older SkyDek instance through its system-tray menu, and run setup. Use your own WAV/MP3 music and samples. Microsoft WebView2 is installed automatically if missing and requires internet access during first setup. loopMIDI, VirtualDJ, and development tools are not required.

### Sampler playback modes

Choose Restart, Toggle, Overlap, or Hold in Sampler settings. Restart is the default: tap to play through, and press again to restart from the beginning. Controller input and mouse activation use the same saved mode.

### Preview status

This is an early preview. Automatic BPM analysis, phase sync, stems separation, and separate headphone cueing are not available. Tempo changes also affect pitch. Test your audio output, controller, and recording before a live performance. The installer is unsigned.

The build passed 33 automated tests, production build/static checks, and native muted audio/recording verification. This does not replace listening and extended hardware testing.

### Installer checksum (SHA-256)

```text
23069c850d3250173ca34b529ab7602484b84de23d3834e66da3761c39b4a634
```

This repository distributes the application installer and documentation. Proprietary SkyDek application source is not included. Third-party notices and applicable dependency source are provided in the ZIP and repository.
