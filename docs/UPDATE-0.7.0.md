# SkyDek DJ 0.7.0 — workspace refinement

A presentation-focused update to the existing application. Audio rendering, controller input, saved mappings, sampler modes, and session storage are unchanged.

- Matte graphite surfaces replace glossy gradient controls and repeated violet panel borders. Violet is reserved for selected controls and focus; cyan/red retain deck identity.
- Larger track titles, library text, consistent numeric timing, and clearer muted labels create a deliberate information hierarchy.
- Deck effects use a compact two-column group. Decks have restrained color-coded headers and quieter platters, with transport and three hot cues consistently aligned.
- A shorter waveform area and more compact master section leave more room for folders, library rows, and all sixteen sampler pads. Low-height and narrower-window styles reduce density without clipping primary controls.
- The unavailable stems placeholder row is removed from the performance view; General settings still states that stems separation is unavailable.
- Controller help is a collapsed, keyboard-accessible disclosure. The header retains controller and battery status in a shorter form with explanatory hover text.
- Library location shows a readable folder name; its complete source path remains available on hover.
- Sampler selection and active playback remain visually distinct. Mapping dropdowns, modal settings, and assignment panels use the same neutral surface palette.
- Visible keyboard focus and a reduced-motion preference are retained/supported. Native Xbox artwork and live glow geometry are unchanged.

Validation: TypeScript/Vite production build and the existing 33 Rust regression tests. The Windows installer is built separately as 0.7.0. Native visual review covers the empty and loaded-deck workspace and General settings at the current display size. Narrower/shorter styles are implemented but broader display coverage and extended live-performance reliability remain unverified.

The public GitHub 0.6.0 release is not replaced automatically. The source remains private; this local UI update can be reviewed before a separately authorized publication.
