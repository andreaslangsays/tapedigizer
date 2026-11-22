//! Streaming processing shared by isolated song playback and file export.
mod chain;
mod channels;
mod dynamics;
mod eq;
mod gain;
mod legacy_highpass;
mod limiter;
mod mix;
mod restoration;

pub use chain::{EffectChain, EffectStage, MonitorStage};
pub use dynamics::DynamicsSettings;
pub use eq::EqSettings;
#[allow(unused_imports)] // Public library API, also compiled by the desktop binary.
pub use eq::PeakEqBand;
pub use limiter::LimiterSettings;

use self::{
    channels::ChannelTrim, dynamics::DynamicsPair, eq::Equalizer, legacy_highpass::LegacyHighpass,
    limiter::LimiterPair, mix::MainMix, restoration::Restoration,
};
use super::denoiser::{DenoiserParams, NoiseProfile, NUM_BINS, STFT_LATENCY_SAMPLES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Routing {
    #[default]
    Stereo,
    LeftMono,
    RightMono,
    MonoMix,
}

/// File layout requested for this song. Playback always uses the saved routing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SongExportMode {
    #[default]
    AsEdited,
    Stereo,
    SeparateMono,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelSettings {
    pub gain_db: f32,
    pub muted: bool,
    pub invert: bool,
    pub eq: EqSettings,
    pub denoiser: DenoiserParams,
    pub compressor: DynamicsSettings,
    pub expander: DynamicsSettings,
    pub limiter: LimiterSettings,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            muted: false,
            invert: false,
            eq: EqSettings::default(),
            denoiser: DenoiserParams {
                dc_filter_enabled: false,
                ..Default::default()
            },
            compressor: DynamicsSettings::default(),
            expander: DynamicsSettings {
                threshold_db: -40.0,
                ..Default::default()
            },
            limiter: LimiterSettings::default(),
        }
    }
}

impl ChannelSettings {
    pub fn stage_active(&self, stage: EffectStage) -> bool {
        match stage {
            EffectStage::Channels => true,
            EffectStage::Restoration => self.denoiser.enabled || self.denoiser.dc_filter_enabled,
            EffectStage::Equalizer => self.eq.enabled,
            EffectStage::Expander => self.expander.enabled,
            EffectStage::Compressor => self.compressor.enabled,
            EffectStage::Limiter => self.limiter.enabled,
        }
    }

    fn enable_stage(&mut self, stage: EffectStage) {
        match stage {
            EffectStage::Channels => {}
            EffectStage::Restoration => self.denoiser.enabled = true,
            EffectStage::Equalizer => self.eq.enabled = true,
            EffectStage::Expander => self.expander.enabled = true,
            EffectStage::Compressor => self.compressor.enabled = true,
            EffectStage::Limiter => self.limiter.enabled = true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongSettings {
    pub gain_db: f32,
    pub balance_db: f32,
    pub width: f32,
    pub swap_channels: bool,
    pub routing: Routing,
    pub export_mode: SongExportMode,
    pub mix_left: f32,
    pub mix_right: f32,
    pub mono_gain_db: f32,
    pub channels_linked: bool,
    pub stereo_dynamics_linked: bool,
    pub channels: [ChannelSettings; 2],
    pub noise_profile: Option<NoiseProfile>,
    pub legacy_highpass_20hz: bool,
    pub bypass: bool,
    pub effect_chain: EffectChain,
}

impl Default for SongSettings {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            balance_db: 0.0,
            width: 1.0,
            swap_channels: false,
            routing: Routing::Stereo,
            export_mode: SongExportMode::AsEdited,
            mix_left: 0.5,
            mix_right: 0.5,
            mono_gain_db: 0.0,
            channels_linked: true,
            stereo_dynamics_linked: true,
            channels: Default::default(),
            noise_profile: None,
            legacy_highpass_20hz: false,
            bypass: false,
            effect_chain: EffectChain::default(),
        }
    }
}

impl SongSettings {
    pub fn effect_active(&self, stage: EffectStage) -> bool {
        self.effect_chain.enabled(stage)
            && self
                .channels
                .iter()
                .any(|channel| channel.stage_active(stage))
    }

    pub fn set_effect_active(&mut self, stage: EffectStage, active: bool, channel: usize) {
        self.effect_chain.set_enabled(stage, active);
        if active && !self.effect_active(stage) {
            self.channels[channel].enable_stage(stage);
            if self.channels_linked {
                self.channels[1 - channel].enable_stage(stage);
            }
        }
    }

    fn valid_profile(&self) -> Option<&NoiseProfile> {
        self.noise_profile.as_ref().filter(|p| {
            p.sample_rate > 0
                && p.noise_power_l.len() == NUM_BINS
                && p.noise_power_r.len() == NUM_BINS
        })
    }

    fn included_channels(&self) -> [bool; 2] {
        match self.routing {
            Routing::Stereo => [true, true],
            Routing::LeftMono => [true, false],
            Routing::RightMono => [false, true],
            Routing::MonoMix => [self.mix_left > 0.0, self.mix_right > 0.0],
        }
    }

    fn spectral_channels(&self) -> [bool; 2] {
        let included = self.included_channels();
        std::array::from_fn(|i| {
            !self.bypass
                && self.effect_chain.enabled(EffectStage::Restoration)
                && included[i]
                && self.channels[i].denoiser.enabled
        })
    }
}

type Stereo = (f32, f32);
type StereoTaps = (Stereo, Stereo);

#[derive(Clone, Copy)]
pub(crate) struct StageFrame {
    pub before: [f32; 2],
    pub after: [f32; 2],
}

pub struct SongProcessor {
    sample_rate: u32,
    chain: EffectChain,
    active: [bool; EffectStage::ALL.len()],
    trim: [ChannelTrim; 2],
    restoration: [Restoration; 2],
    equalizer: [Equalizer; 2],
    highpass: [LegacyHighpass; 2],
    profile: Option<NoiseProfile>,
    expander: DynamicsPair,
    compressor: DynamicsPair,
    limiter: LimiterPair,
    mix: MainMix,
    stereo_link: bool,
    included: [bool; 2],
    bypass: bool,
    legacy_highpass: bool,
    latency: usize,
}

impl SongProcessor {
    pub fn new(sample_rate: u32, settings: &SongSettings) -> Self {
        let sr = sample_rate.max(1);
        let mut processor = Self {
            sample_rate: sr,
            chain: EffectChain::default(),
            active: Default::default(),
            trim: Default::default(),
            restoration: std::array::from_fn(|i| Restoration::new(i, sr)),
            equalizer: Default::default(),
            highpass: std::array::from_fn(|_| LegacyHighpass::new(sr)),
            profile: None,
            expander: Default::default(),
            compressor: Default::default(),
            limiter: Default::default(),
            mix: MainMix::default(),
            stereo_link: false,
            included: [false; 2],
            bypass: false,
            legacy_highpass: false,
            latency: 0,
        };
        processor.set_settings(settings);
        processor.reset();
        processor
    }

    /// A new order changes the signal that built up each insert's streaming history.
    pub fn requires_reprime(&self, next: &SongSettings) -> bool {
        let spectral = next.spectral_channels();
        let current = self.restoration.each_ref().map(Restoration::is_spectral);
        !self.chain.same_order(&next.effect_chain)
            || spectral != current
            || self.mix.routing != next.routing
            || (self.latency > 0
                && (self.included != next.included_channels()
                    || self.profile.as_ref() != next.valid_profile()))
    }

    /// Apply a settings snapshot at an audio block boundary. Ordinary parameter
    /// edits retain the streaming histories of inserts that are still active.
    pub fn set_settings(&mut self, settings: &SongSettings) {
        let profile = settings.valid_profile();
        let profile_changed = profile != self.profile.as_ref();
        if profile_changed {
            self.profile = profile.cloned();
        }
        let included = settings.included_channels();
        let bypass_changed = self.bypass != settings.bypass;
        let order_changed = !self.chain.same_order(&settings.effect_chain);
        let enabled_changed =
            |stage| self.chain.enabled(stage) != settings.effect_chain.enabled(stage);
        let reset_expander = enabled_changed(EffectStage::Expander);
        let reset_compressor = enabled_changed(EffectStage::Compressor);
        let reset_limiter = enabled_changed(EffectStage::Limiter);
        let latency = if settings.spectral_channels().contains(&true) {
            STFT_LATENCY_SAMPLES
        } else {
            0
        };
        let ramp = (self.sample_rate as usize / 200).max(1);
        self.bypass = settings.bypass;
        self.legacy_highpass = settings.legacy_highpass_20hz;
        self.stereo_link = settings.stereo_dynamics_linked;
        let restore = settings.effect_chain.enabled(EffectStage::Restoration);
        let eq = settings.effect_chain.enabled(EffectStage::Equalizer);
        for (i, channel) in settings.channels.iter().enumerate() {
            self.trim[i].configure(channel, ramp);
            self.restoration[i].configure(
                &channel.denoiser,
                self.profile.as_ref(),
                profile_changed,
                latency,
                restore && included[i] && !settings.bypass,
            );
            self.equalizer[i].configure(&channel.eq, self.sample_rate, eq);
            self.highpass[i].configure(settings.legacy_highpass_20hz);
            if self.included[i] != included[i]
                || self.latency != latency
                || bypass_changed
                || order_changed
            {
                self.reset_channel(i, latency, &settings.effect_chain);
            }
            self.expander
                .configure_channel(i, &channel.expander, self.sample_rate, false);
            self.compressor
                .configure_channel(i, &channel.compressor, self.sample_rate, true);
            self.limiter
                .configure_channel(i, &channel.limiter, self.sample_rate);
        }
        let reset_detectors = included != self.included || bypass_changed || order_changed;
        if reset_detectors || reset_expander {
            self.expander.reset();
        }
        if reset_detectors || reset_compressor {
            self.compressor.reset();
        }
        if reset_detectors || reset_limiter {
            self.limiter.reset();
        }
        self.chain = settings.effect_chain;
        self.active = std::array::from_fn(|i| {
            let slot = self.chain.slots()[i];
            included
                .iter()
                .zip(&settings.channels)
                .any(|(&used, channel)| used && channel.stage_active(slot.stage))
                && slot.enabled
        });
        self.mix.configure(settings, ramp);
        self.included = included;
        self.latency = latency;
    }

    fn reset_channel(&mut self, i: usize, latency: usize, chain: &EffectChain) {
        self.trim[i].reset();
        self.restoration[i].reset();
        self.equalizer[i].reset();
        let filter_after_restoration =
            chain.position(EffectStage::Equalizer) > chain.position(EffectStage::Restoration);
        self.highpass[i].reset(if filter_after_restoration { latency } else { 0 });
    }

    /// Reset all streaming history at a seek, clip boundary or new render.
    pub fn reset(&mut self) {
        let chain = self.chain;
        for i in 0..2 {
            self.reset_channel(i, self.latency, &chain);
        }
        self.expander.reset();
        self.compressor.reset();
        self.limiter.reset();
        self.mix.reset();
    }

    /// Number of initial output frames to drop; supply this many zeros after
    /// the last source frame to recover the complete clip at the original length.
    pub fn latency(&self) -> usize {
        self.latency
    }

    #[inline]
    pub fn process_frame(&mut self, l: f32, r: f32) -> Stereo {
        self.process_with_monitor_taps(l, r).0
    }

    /// Return the routed output and the two contributions available for solo monitoring.
    #[inline]
    pub fn process_with_monitor_taps(&mut self, l: f32, r: f32) -> StereoTaps {
        self.process_monitored(l, r, None).0
    }

    #[inline]
    pub(crate) fn process_monitored(
        &mut self,
        l: f32,
        r: f32,
        selected: Option<MonitorStage>,
    ) -> (StereoTaps, Option<StageFrame>) {
        let mut frame = [l, r];
        let mut levels = None;
        if !self.bypass {
            for (sample, included) in frame.iter_mut().zip(self.included) {
                if !included {
                    *sample = 0.0;
                }
            }
            let mut muted = [false; 2];
            // Copying the descriptors keeps the sample loop allocation-free and
            // allows each insert to borrow just its own processing state.
            let chain = self.chain;
            for (index, slot) in chain.slots().iter().enumerate() {
                let observed = selected == Some(MonitorStage::Insert(slot.stage));
                let before = frame;
                if self.active[index] {
                    match slot.stage {
                        EffectStage::Channels => {
                            for (i, sample) in frame.iter_mut().enumerate() {
                                muted[i] = self.trim[i].muted;
                                *sample = self.trim[i].process(*sample);
                            }
                        }
                        EffectStage::Restoration => {
                            let eq_follows = chain
                                .slots()
                                .get(index + 1)
                                .is_some_and(|next| next.stage == EffectStage::Equalizer);
                            let gate_here =
                                !eq_follows || (!self.active[index + 1] && !self.legacy_highpass);
                            for (i, sample) in frame.iter_mut().enumerate() {
                                let restored = self.restoration[i].process(*sample);
                                *sample = if gate_here && muted[i] { 0.0 } else { restored };
                            }
                        }
                        EffectStage::Equalizer => {
                            for (i, sample) in frame.iter_mut().enumerate() {
                                let filtered = self.equalizer[i].process(*sample);
                                *sample = if muted[i] && !self.legacy_highpass {
                                    0.0
                                } else {
                                    filtered
                                };
                            }
                        }
                        EffectStage::Expander => {
                            self.expander
                                .process(&mut frame, self.included, self.stereo_link)
                        }
                        EffectStage::Compressor => {
                            self.compressor
                                .process(&mut frame, self.included, self.stereo_link)
                        }
                        EffectStage::Limiter => {
                            self.limiter
                                .process(&mut frame, self.included, self.stereo_link)
                        }
                    }
                }
                if observed {
                    levels = Some(StageFrame {
                        before,
                        after: frame,
                    });
                }
                // The adopted recording filter remains at its legacy insertion
                // point. EQ bypass and the EQ comparison only affect the EQ.
                if self.legacy_highpass && slot.stage == EffectStage::Equalizer {
                    for (i, sample) in frame.iter_mut().enumerate() {
                        let filtered = self.highpass[i].process(*sample);
                        *sample = if muted[i] { 0.0 } else { filtered };
                    }
                }
            }
        } else if matches!(selected, Some(MonitorStage::Insert(_))) {
            levels = Some(StageFrame {
                before: frame,
                after: frame,
            });
        }
        let taps = self.mix.process(frame);
        if selected == Some(MonitorStage::Mix) {
            let (left, right) = taps.0;
            levels = Some(StageFrame {
                before: frame,
                after: [left, right],
            });
        }
        (taps, levels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.0001, "{a} vs {b}");
    }

    #[test]
    fn saved_insert_order_changes_limiting_before_the_fixed_main_mix() {
        let mut settings = SongSettings {
            gain_db: 6.0206,
            ..Default::default()
        };
        for channel in &mut settings.channels {
            channel.gain_db = 6.0206;
            channel.limiter = LimiterSettings {
                enabled: true,
                ceiling_db: -6.0206,
                ..Default::default()
            };
        }
        let mut legacy = SongProcessor::new(48000, &settings);
        close(legacy.process_frame(0.4, 0.4).0, 1.0);

        let mut saved = serde_json::to_value(settings).unwrap();
        saved["effect_chain"] = serde_json::json!([
            {"stage": "Limiter", "enabled": true},
            {"stage": "Channels", "enabled": true}
        ]);
        let reordered: SongSettings = serde_json::from_value(saved).unwrap();
        let mut processor = SongProcessor::new(48000, &reordered);
        close(processor.process_frame(0.4, 0.4).0, 1.6);
    }

    #[test]
    fn bypass_keeps_insert_parameters_and_taps_follow_the_selected_stage() {
        let mut settings = SongSettings::default();
        settings.channels[0].gain_db = 6.0206;
        settings.channels[1].gain_db = 6.0206;
        let stage = EffectStage::Limiter;
        settings.set_effect_active(stage, true, 0);
        for channel in &mut settings.channels {
            channel.limiter.ceiling_db = -6.0206;
        }
        let tap = Some(MonitorStage::Insert(stage));
        let mut processor = SongProcessor::new(48000, &settings);
        let ((output, _), frame) = processor.process_monitored(0.4, 0.4, tap);
        let frame = frame.unwrap();
        close(frame.before[0], 0.8);
        close(frame.after[0], 0.5);
        close(output.0, 0.5);

        settings.set_effect_active(stage, false, 0);
        processor.set_settings(&settings);
        let ((output, _), frame) = processor.process_monitored(0.4, 0.4, tap);
        let frame = frame.unwrap();
        assert_eq!(frame.before, frame.after);
        close(output.0, 0.8);

        settings.set_effect_active(stage, true, 0);
        assert_eq!(settings.channels[0].limiter.ceiling_db, -6.0206);
        assert!(settings.effect_chain.move_by(stage, -5));
        assert!(!settings.effect_chain.move_by(stage, -1));
        assert!(processor.requires_reprime(&settings));
        processor.set_settings(&settings);
        let ((output, _), frame) = processor.process_monitored(0.4, 0.4, tap);
        let frame = frame.unwrap();
        close(frame.before[0], 0.4);
        close(frame.after[0], 0.4);
        close(output.0, 0.8);
    }

    #[test]
    fn legacy_or_partial_insert_lists_always_have_one_slot_per_stage() {
        let legacy: SongSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.effect_chain, EffectChain::default());
        let saved: SongSettings = serde_json::from_value(serde_json::json!({
            "effect_chain": [
                {"stage": "Limiter", "enabled": false},
                {"stage": "Limiter", "enabled": true},
                {"stage": "Channels"}
            ]
        }))
        .unwrap();
        assert_eq!(saved.effect_chain.slots().len(), EffectStage::ALL.len());
        assert_eq!(saved.effect_chain.position(EffectStage::Limiter), 0);
        assert!(!saved.effect_chain.enabled(EffectStage::Limiter));
        assert!(saved.effect_chain.enabled(EffectStage::Channels));
        assert!(saved.effect_chain.enabled(EffectStage::Restoration));
    }

    #[test]
    fn neutral_preserves_float_samples_and_channel_controls_are_independent() {
        let mut s = SongSettings::default();
        let mut p = SongProcessor::new(48000, &s);
        assert_eq!(p.latency(), 0);
        assert_eq!(p.process_frame(1.3, -0.0000003), (1.3, -0.0000003));
        s.channels[0].invert = true;
        s.channels[1].muted = true;
        s.channels[0].gain_db = 6.0206;
        p.set_settings(&s);
        p.reset();
        let (l, r) = p.process_frame(1.3, 0.8);
        close(l, -2.6);
        close(r, 0.0);
    }

    #[test]
    fn mono_weights_are_independent_and_unscaled() {
        let mut s = SongSettings::default();
        s.routing = Routing::MonoMix;
        assert_eq!(
            SongProcessor::new(48000, &s).process_frame(0.2, -0.2),
            (0.0, 0.0)
        );
        s.mix_left = 0.25;
        s.mix_right = 1.0;
        s.mono_gain_db = 6.0206;
        let (l, r) = SongProcessor::new(48000, &s).process_frame(0.6, -0.2);
        close(l, -0.1);
        assert_eq!(l, r);
        s.mix_right = 0.0;
        close(
            SongProcessor::new(48000, &s).process_frame(0.6, -0.2).0,
            0.3,
        );
    }

    fn fast_dynamics() -> DynamicsSettings {
        DynamicsSettings {
            enabled: true,
            threshold_db: -20.0,
            ratio: 2.0,
            attack_ms: 0.1,
            release_ms: 1.0,
            knee_db: 0.0,
            ..Default::default()
        }
    }

    fn settled(s: &SongSettings, input: (f32, f32)) -> (f32, f32) {
        let mut p = SongProcessor::new(48000, s);
        for _ in 0..4800 {
            p.process_frame(input.0, input.1);
        }
        p.process_frame(input.0, input.1)
    }

    #[test]
    fn stereo_detector_link_and_discarded_channels() {
        let mut s = SongSettings::default();
        for c in &mut s.channels {
            c.compressor = fast_dynamics();
        }
        let (l, r) = settled(&s, (0.01, 1.0));
        close(r, 0.316228);
        close(l, 0.00316228);
        s.stereo_dynamics_linked = false;
        close(settled(&s, (0.01, 1.0)).0, 0.01);
        s.stereo_dynamics_linked = true;
        for (route, input) in [
            (Routing::LeftMono, (0.01, 1.0)),
            (Routing::RightMono, (1.0, 0.01)),
        ] {
            s.routing = route;
            let (l, r) = settled(&s, input);
            close(l, 0.01);
            assert_eq!(l, r);
        }
    }

    #[test]
    fn single_active_compressor_ignores_other_channel_even_with_detector_link_on() {
        for side in 0..2 {
            let mut s = SongSettings::default();
            s.channels[side].compressor = fast_dynamics();
            let input = if side == 0 { (0.01, 1.0) } else { (1.0, 0.01) };
            assert_eq!(settled(&s, input), input);
        }
    }

    #[test]
    fn playback_reprime_keeps_transition_between_dry_delay_and_stft_continuous() {
        let mut s = SongSettings::default();
        s.channels[0].denoiser.enabled = true;
        let mut p = SongProcessor::new(44100, &s);
        let input = (0.5, 0.5);
        for _ in 0..p.latency() {
            p.process_frame(input.0, input.1);
        }
        for enabled in [true, false] {
            s.channels[1].denoiser.enabled = enabled;
            let reprime = p.requires_reprime(&s);
            p.set_settings(&s);
            if reprime {
                p.reset();
                for _ in 0..p.latency() {
                    p.process_frame(input.0, input.1);
                }
            }
            for _ in 0..p.latency() + 1 {
                let (l, r) = p.process_frame(input.0, input.1);
                close(l, input.0);
                close(r, input.1);
            }
        }
        // Ordinary gain edits must retain their ramp and all filter histories.
        s.gain_db = 1.0;
        assert!(!p.requires_reprime(&s));
    }

    #[test]
    fn expansion_attenuates_below_or_boosts_above_threshold() {
        let mut s = SongSettings::default();
        s.channels[0].expander = fast_dynamics();
        close(settled(&s, (0.01, 0.0)).0, 0.001);
        close(settled(&s, (0.5, 0.0)).0, 0.5);
        s.channels[0].expander.upward = true;
        close(settled(&s, (0.5, 0.0)).0, 2.5);
        close(settled(&s, (0.01, 0.0)).0, 0.01);
    }

    #[test]
    fn one_sided_spectral_filter_aligns_both_edges_and_preserves_history_on_gain_edit() {
        let mut s = SongSettings::default();
        s.channels[0].denoiser.enabled = true;
        s.channels[0].denoiser.reduction_db = 0.0;
        let mut p = SongProcessor::new(48000, &s);
        let latency = p.latency();
        assert_eq!(latency, super::super::denoiser::STFT_LATENCY_SAMPLES);
        for i in 0..latency + 37 {
            let v = match i {
                0 => 0.5,
                36 => -0.3,
                _ => 0.0,
            };
            if i == 20 {
                s.channels[1].gain_db = 4.0;
                p.set_settings(&s);
            }
            let (l, r) = p.process_frame(v, v);
            let expected = match i.checked_sub(latency) {
                Some(0) => 0.5,
                Some(36) => -0.3,
                _ => 0.0,
            };
            close(l, expected);
            if i < latency + 36 {
                close(r, expected);
            } else {
                assert!(r < -0.3 && r > -0.45, "smoothed {r}");
            }
        }
    }

    #[test]
    fn additional_peaks_load_neutral_and_apply_from_saved_settings() {
        let old: EqSettings = serde_json::from_str(r#"{"enabled":true,"mid_db":-3.0}"#).unwrap();
        assert_eq!(old.mid_db, -3.0);
        assert_eq!(old.peaks.map(|band| band.gain_db), [0.0, 0.0]);
        let mut settings = SongSettings::default();
        settings.channels[0].eq.enabled = true;
        for (slot, hz, gain_db) in [(0, 500.0, 6.0206), (1, 6000.0, -6.0206)] {
            let eq = &mut settings.channels[0].eq;
            eq.peaks = EqSettings::default().peaks;
            eq.peaks[slot] = PeakEqBand {
                hz,
                gain_db,
                q: 1.4,
            };
            close(eq.response(48_000).gain_db(hz), gain_db);
            let restored: SongSettings =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(settings, restored);
            let mut processor = SongProcessor::new(48_000, &restored);
            for i in 0..4800 {
                let sample = (i as f32 * std::f32::consts::TAU * hz / 48_000.0).sin() * 0.1;
                let (left, right) = processor.process_frame(sample, sample);
                if i >= 2400 {
                    close(left, sample * if slot == 0 { 2.0 } else { 0.5 });
                    close(right, sample);
                }
            }
        }
    }

    #[test]
    fn tone_at_mid_eq_center_gets_requested_gain_only_on_enabled_channel() {
        let mut s = SongSettings::default();
        s.channels[0].eq.enabled = true;
        s.channels[0].eq.mid_db = 6.0206;
        let mut p = SongProcessor::new(48000, &s);
        for i in 0..2400 {
            let sample = (i as f32 * std::f32::consts::TAU / 48.0).sin() * 0.1;
            let (l, r) = p.process_frame(sample, sample);
            if i > 1200 {
                close(l, sample * 2.0);
                close(r, sample);
            }
        }
    }

    #[test]
    fn adopted_legacy_highpass_matches_one_pole_step_across_edits_and_reset() {
        let mut s = SongSettings {
            legacy_highpass_20hz: true,
            ..Default::default()
        };
        let mut p = SongProcessor::new(48000, &s);
        let pole = 1.0 / (1.0 + std::f64::consts::TAU * 20.0 / 48000.0);
        for i in 1..2000 {
            if i == 700 {
                s.channels_linked = false;
                p.set_settings(&s);
            }
            let expected = pole.powi(i) as f32;
            let (l, r) = p.process_frame(0.5, -0.25);
            assert!((l - 0.5 * expected).abs() < 1.0e-6);
            assert!((r + 0.25 * expected).abs() < 1.0e-6);
        }
        p.reset();
        close(p.process_frame(0.5, -0.25).0, 0.5 * pole as f32);
        s.bypass = true;
        p.set_settings(&s);
        assert_eq!(p.process_frame(0.5, -0.25), (0.5, -0.25));
    }

    #[test]
    fn dc_only_has_no_latency_and_rejects_constant_input() {
        let mut s = SongSettings::default();
        s.channels[1].denoiser.dc_filter_enabled = true;
        assert_eq!(SongProcessor::new(48000, &s).latency(), 0);
        let (l, r) = settled(&s, (0.1, 0.1));
        close(l, 0.1);
        close(r, 0.0);
    }

    #[test]
    fn changing_master_gain_ramps_without_jumping_or_resetting() {
        let mut s = SongSettings::default();
        let mut p = SongProcessor::new(48000, &s);
        s.gain_db = 6.0206;
        p.set_settings(&s);
        let first = p.process_frame(1.0, 1.0).0;
        assert!(first > 1.0 && first < 1.1);
        for _ in 0..4800 {
            p.process_frame(1.0, 1.0);
        }
        close(p.process_frame(1.0, 1.0).0, 2.0);
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::gain::linear;
    use super::*;

    #[test]
    fn stereo_balance_does_not_follow_a_live_switch_to_mono() {
        let mut s = SongSettings {
            balance_db: 12.0,
            ..Default::default()
        };
        let mut p = SongProcessor::new(48000, &s);
        p.process_frame(0.3, 0.8);
        s.routing = Routing::RightMono;
        p.set_settings(&s);
        assert_eq!(p.process_frame(0.3, 0.8), (0.8, 0.8));
    }

    #[test]
    fn balance_swap_and_width_in_stereo_and_saved_bypass_in_mono() {
        let mut s = SongSettings {
            swap_channels: true,
            balance_db: 12.0412,
            width: 0.0,
            ..Default::default()
        };
        let mut p = SongProcessor::new(48000, &s);
        let (l, r) = p.process_frame(0.75, 0.25);
        assert!((l - 1.0).abs() < 0.0001 && (r - 0.25).abs() < 0.0001);
        s.balance_db = 0.0;
        s.width = 1.0;
        let mut p = SongProcessor::new(48000, &s);
        assert_eq!(p.process_frame(0.75, 0.25), (0.25, 0.75));
        s.routing = Routing::RightMono;
        s.channels[1].muted = true;
        s.bypass = true;
        s.gain_db = 12.0;
        let mut p = SongProcessor::new(48000, &s);
        assert_eq!(p.process_frame(0.75, 0.25), (0.25, 0.25));
    }

    #[test]
    fn compressor_attack_release_and_soft_knee_are_audible() {
        let mut s = SongSettings::default();
        s.channels[0].compressor = DynamicsSettings {
            enabled: true,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 8.0,
            ..Default::default()
        };
        let mut p = SongProcessor::new(48000, &s);
        let attack_start = p.process_frame(1.0, 0.0).0;
        for _ in 0..4800 {
            p.process_frame(1.0, 0.0);
        }
        let attack_end = p.process_frame(1.0, 0.0).0;
        assert!(attack_start > 0.95 && (attack_end - 0.1778).abs() < 0.001);
        let release_start = p.process_frame(0.1, 0.0).0;
        for _ in 0..48000 {
            p.process_frame(0.1, 0.0);
        }
        let soft_knee = p.process_frame(0.1, 0.0).0;
        assert!(release_start < 0.025 && (soft_knee - 0.09173).abs() < 0.001);
    }

    #[test]
    fn spectral_reduction_uses_the_matching_profile_side() {
        let mut s = SongSettings::default();
        let bins = super::super::denoiser::NUM_BINS;
        s.noise_profile = Some(NoiseProfile {
            name: "noise".into(),
            media_source: String::new(),
            sample_rate: 48000,
            duration_secs: 1.0,
            dc_offset_l: 0.0,
            dc_offset_r: 0.0,
            rms_db_l: -60.0,
            rms_db_r: -60.0,
            noise_power_l: vec![0.0; bins],
            noise_power_r: vec![1.0e12; bins],
            wav_filename: None,
        });
        s.channels[0].denoiser.enabled = true;
        s.channels[1].denoiser.enabled = true;
        s.channels[1].denoiser.reduction_db = 30.0;
        s.channels[1].denoiser.smoothing = 0.0;
        let mut p = SongProcessor::new(48000, &s);
        let mut peaks = [0.0_f32; 2];
        for i in 0..12000 {
            let v = (i as f32 * 0.1).sin() * 0.4;
            let (l, r) = p.process_frame(v, v);
            if i > 11500 {
                peaks[0] = peaks[0].max(l.abs());
                peaks[1] = peaks[1].max(r.abs());
            }
        }
        assert!(peaks[0] > 0.39 && peaks[1] < 0.02, "{peaks:?}");
    }

    #[test]
    fn extreme_ratio_stays_finite_and_silence_stays_silent() {
        let mut s = SongSettings::default();
        s.channels[0].expander = DynamicsSettings {
            enabled: true,
            threshold_db: -96.0,
            ratio: 100.0,
            upward: true,
            makeup_db: 24.0,
            attack_ms: 0.0,
            release_ms: 0.0,
            ..Default::default()
        };
        let mut p = SongProcessor::new(8000, &s);
        assert_eq!(p.process_frame(0.0, 0.0), (0.0, 0.0));
        for _ in 0..10 {
            let v = p.process_frame(1.0, 0.0).0;
            assert!(v.is_finite() && v <= 1000.0);
        }
    }

    #[test]
    fn limiter_enforces_ceiling_and_stereo_link() {
        let mut s = SongSettings::default();
        s.stereo_dynamics_linked = true;
        for ch in &mut s.channels {
            ch.limiter = LimiterSettings {
                enabled: true,
                ceiling_db: -6.0,
                drive_db: 12.0,
                release_ms: 50.0,
            };
        }
        let ceiling_lin = linear(-6.0);
        let mut p = SongProcessor::new(48_000, &s);
        let mut max_l = 0.0_f32;
        let mut max_r = 0.0_f32;
        for i in 0..2048 {
            let l_in = (i as f32 * 0.05).sin() * 0.9;
            let r_in = l_in * 0.5;
            let (l, r) = p.process_frame(l_in, r_in);
            max_l = max_l.max(l.abs());
            max_r = max_r.max(r.abs());
            assert!(
                l.abs() <= ceiling_lin + 1e-5,
                "left {l} exceeded ceiling {ceiling_lin}"
            );
            assert!(
                r.abs() <= ceiling_lin + 1e-5,
                "right {r} exceeded ceiling {ceiling_lin}"
            );
        }
        // Stereo-Link erhält das Pegelverhältnis (R ≈ 0.5 * L) auch beim Limiting!
        assert!(max_l > ceiling_lin * 0.95);
        assert!(
            (max_r / max_l - 0.5).abs() < 0.05,
            "ratio was {}",
            max_r / max_l
        );
    }
}
