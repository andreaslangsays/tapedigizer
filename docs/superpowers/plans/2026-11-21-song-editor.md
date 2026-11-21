# Song Editor Implementation Plan

> Execute with superpowers:subagent-driven-development for isolated DSP and persistence/export work, and direct integration in the existing UI. Existing uncommitted interface changes must be preserved.

**Goal:** An isolated song editor with upper song selection, realtime processing, independent channels and mono mix, persistent settings and matching export.
**Architecture:** Shared `audio::processing::SongProcessor` consumes serializable per-song `SongSettings`. A dedicated clip player uses original shared audio but exposes only the chosen song. `TrackSplitter` snapshots own per-segment settings; projects migrate optional fields and export uses the same processor.
**Tech Stack:** Existing Rust, egui 0.29, cpal, hound, serde; no new dependencies.
**Spec:** `docs/superpowers/specs/2026-11-20-track-studio-design.md` (approved with mono mixer addition).

## Global Constraints

Upper selection only; no persistent side list. Local time and playback restricted to song samples. All durable settings are saved. Old projects load with legacy behavior until explicit adoption. Mono export is one channel. AU/VST hosting is deferred. Do not revert existing working changes or overwrite source audio.

## Review Focus

- Track switching and keyboard/menu transport must not play or change another track.
- Neighbor samples and filter latency must not leak at clip edges or after seeks.
- Destructive edits and marker changes must preserve or explicitly remap processing state.
- Old project round trips retain legacy metadata; all new audio/export/view state survives save.
- Mono routing ignores discarded channel DSP/detectors; temporary solo/A-B never reaches export.

### Task 1: Shared DSP (delegated)
Files: new `src/audio/processing.rs`, module declaration in `src/audio/mod.rs`.
- [ ] Write failing routing/channel/dynamics/latency tests, run them.
- [ ] Implement `SongSettings`, `Routing`, `ChannelSettings`, `EqSettings`, `DynamicsSettings` (Clone, Debug, PartialEq, serde with defaults).
- [ ] Implement `SongProcessor::new(u32, &SongSettings)`, `set_settings(&SongSettings)`, `reset()`, `latency() -> usize`, `process_frame(f32,f32)->(f32,f32)`.
- [ ] Prove neutral, asymmetric mono mixing, L/R isolation, EQ, compression/expansion and finite extremes.

Contract: SongSettings { gain_db, balance_db, width, swap_channels, routing, mix_left, mix_right, mono_gain_db, channels_linked, stereo_dynamics_linked, channels:[ChannelSettings;2], noise_profile:Option<NoiseProfile>, bypass }. Routing::{Stereo,LeftMono,RightMono,MonoMix}. ChannelSettings {gain_db,muted,invert,eq,denoiser,compressor,expander}. EqSettings {enabled,low_db,mid_db,high_db,low_hz,mid_hz,high_hz,mid_q}. DynamicsSettings {enabled,threshold_db,ratio,attack_ms,release_ms,knee_db,makeup_db,upward}. Neutral defaults and mix_left=mix_right=0.5, width=1.0. Denoiser defaults in ChannelSettings must have enabled=false and dc_filter_enabled=false. All numeric fields f32. Processor output is unclipped float; latency is handled identically by player and export.

### Task 2: Persistence, segment ownership and export (delegated)
Files: `src/project.rs`, `src/splitter.rs`, MarkerSnapshot literal in `src/audio/denoiser.rs`.
- [ ] Write failing old-project, full settings roundtrip, split/merge/undo and processed mono export tests.
- [ ] Add `song_settings:BTreeMap<usize,SongSettings>` to project/splitter/snapshots; `processing:Option<SongSettings>` to TrackInfo. Split inherits; merge retains left; edits and undo preserve maps.
- [ ] Add optional `settings:Option<ProjectSettings>` and `editor_view:SongEditorView` to projects with serde defaults. ProjectSettings has artist_name,media_source,export_highpass_20hz,export_fade_ms,export_balance_db,export_apply_denoiser,denoiser_params,noise_profile. SongEditorView has selected_segment:Option<usize>,zoom:f32,vertical_zoom:f32,scroll:f32,loop_playback:bool (defaults None,1,1,0,false).
- [ ] Export custom songs through SongProcessor, trim/flush latency with zeros within clip, use true mono channel count; bypass legacy global filter/balance/HP for custom songs. Keep legacy export unchanged when no custom songs. Apply final normalization and fades once.
- [ ] Run appropriate tests; report exact exported/persisted behavior.

### Task 3: Isolated realtime player and song UI (primary agent)
Files: `src/audio/player.rs`, new `src/ui/song_editor.rs`, `src/ui/mod.rs`, `src/ui/app.rs`.
- [ ] Write failing clip bounds and playback/DSP parity tests.
- [ ] Add dedicated shared-source clip playback with SongProcessor, independent transport, A/B/solo/mono audition and metering; no dry fallback on parameter-lock contention.
- [ ] Build upper selection, full-width local waveform, selection/zoom and scrollable processing modules for channels, stereo/mono mixer, noise/DC, EQ, compressor and expander.
- [ ] Integrate settings adoption, dirty detection, project save/load, menu/keyboard routing and export metadata. Remove global waveform/destructive controls from song workspace.
- [ ] Verify DE/EN layouts and state isolation.

### Task 4: Verification and delivery
- [ ] Run full cargo test --locked and release build.
- [ ] Review changed code with a fresh reviewer; fix material findings and test.
- [ ] Launch a separately named preview bundle and visually verify recording, isolated song editing, mono mixer and dynamics.
- [ ] Update README and record implementation status.
