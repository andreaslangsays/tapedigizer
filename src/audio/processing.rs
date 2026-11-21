//! Streaming processing shared by isolated song playback and file export.
use super::denoiser::{
    AuditionMode, DcSubsonicFilter, DenoiserParams, NoiseProfile, SpectralDenoiser, NUM_BINS,
    STFT_LATENCY_SAMPLES,
};
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
pub struct EqSettings {
    pub enabled: bool,
    pub low_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
    pub low_hz: f32,
    pub mid_hz: f32,
    pub high_hz: f32,
    pub mid_q: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            low_db: 0.0,
            mid_db: 0.0,
            high_db: 0.0,
            low_hz: 150.0,
            mid_hz: 1000.0,
            high_hz: 6000.0,
            mid_q: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicsSettings {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
    pub upward: bool,
}

impl Default for DynamicsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 2.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 3.0,
            makeup_db: 0.0,
            upward: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimiterSettings {
    pub enabled: bool,
    pub ceiling_db: f32,
    pub drive_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            ceiling_db: -0.3,
            drive_db: 0.0,
            release_ms: 60.0,
        }
    }
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
        }
    }
}

impl SongSettings {
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
        std::array::from_fn(|i| !self.bypass && included[i] && self.channels[i].denoiser.enabled)
    }
}

fn linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// A short, sample-rate-independent ramp for controls that directly scale audio.
#[derive(Default)]
struct Gain {
    value: f32,
    target: f32,
    step: f32,
    remaining: usize,
}

impl Gain {
    fn set(&mut self, target: f32, frames: usize) {
        if self.target != target {
            self.target = target;
            self.remaining = frames;
            self.step = (target - self.value) / frames as f32;
        }
    }

    fn reset(&mut self) {
        self.value = self.target;
        self.remaining = 0;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }
}

#[derive(Default)]
struct Biquad {
    // b0, b1, b2, a1, a2 normalized by a0
    coeff: Option<[f64; 5]>,
    z1: f64,
    z2: f64,
}

#[derive(Clone, Copy)]
enum Band {
    LowShelf,
    Peak,
    HighShelf,
}

impl Biquad {
    fn configure(&mut self, band: Band, sr: u32, hz: f32, db: f32, q: f32) {
        if db.abs() < 1e-5 {
            self.coeff = None;
            self.reset();
            return;
        }
        let freq = f64::from(hz).clamp(0.001, f64::from(sr) * 0.49);
        let w = std::f64::consts::TAU * freq / f64::from(sr);
        let (sin, cos) = w.sin_cos();
        let a = 10.0_f64.powf(f64::from(db.clamp(-36.0, 36.0)) / 40.0);
        // RBJ shelving filters with shelf slope 1; the middle band uses Q.
        let beta = (2.0 * a).sqrt() * sin;
        let plus = a + 1.0;
        let minus = a - 1.0;
        let [b0, b1, b2, a0, a1, a2] = match band {
            Band::LowShelf => [
                a * (plus - minus * cos + beta),
                2.0 * a * (minus - plus * cos),
                a * (plus - minus * cos - beta),
                plus + minus * cos + beta,
                -2.0 * (minus + plus * cos),
                plus + minus * cos - beta,
            ],
            Band::HighShelf => [
                a * (plus + minus * cos + beta),
                -2.0 * a * (minus + plus * cos),
                a * (plus + minus * cos - beta),
                plus - minus * cos + beta,
                2.0 * (minus - plus * cos),
                plus - minus * cos - beta,
            ],
            Band::Peak => {
                let alpha = sin / (2.0 * f64::from(q.clamp(0.1, 20.0)));
                [
                    1.0 + alpha * a,
                    -2.0 * cos,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cos,
                    1.0 - alpha / a,
                ]
            }
        };
        self.coeff = Some([b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]);
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let Some([b0, b1, b2, a1, a2]) = self.coeff else {
            return input;
        };
        let x = f64::from(input);
        let y = b0 * x + self.z1;
        self.z1 = b1 * x - a1 * y + self.z2;
        self.z2 = b2 * x - a2 * y;
        y as f32
    }
}

#[derive(Default)]
struct Dynamics {
    enabled: bool,
    below: bool,
    slope: f32,
    threshold: f32,
    knee: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    // Current signed gain change, in dB (makeup is independent).
    gain_db: f32,
}

impl Dynamics {
    fn configure(&mut self, s: &DynamicsSettings, sr: u32, compressor: bool) {
        let ratio = s.ratio.clamp(1.0, 100.0);
        self.enabled = s.enabled;
        self.below = !compressor && !s.upward;
        self.slope = if compressor {
            1.0 / ratio - 1.0
        } else if self.below {
            1.0 - ratio
        } else {
            ratio - 1.0
        };
        self.threshold = s.threshold_db.clamp(-120.0, 24.0);
        self.knee = s.knee_db.clamp(0.0, 48.0);
        let coefficient = |ms: f32| -(-1000.0 / (sr as f32 * ms.clamp(0.01, 10000.0))).exp_m1();
        self.attack = coefficient(s.attack_ms);
        self.release = coefficient(s.release_ms);
        self.makeup = linear(s.makeup_db.clamp(-48.0, 24.0));
        if !self.enabled {
            self.gain_db = 0.0;
        }
    }

    fn detect(&mut self, level: f32) -> f32 {
        let mut over = 20.0 * level.max(1.0e-9).log10() - self.threshold;
        if self.below {
            over = -over;
        }
        let half = self.knee * 0.5;
        let distance = if over <= -half {
            0.0
        } else if over >= half {
            over
        } else {
            (over + half).powi(2) / (2.0 * self.knee)
        };
        // Bound upward expansion even for extreme ratios and near-silent thresholds.
        let target = (distance * self.slope).clamp(-120.0, 24.0);
        let speed = if target.abs() > self.gain_db.abs() {
            self.attack
        } else {
            self.release
        };
        self.gain_db += (target - self.gain_db) * speed;
        self.gain_db
    }
}

#[derive(Default)]
struct DynamicsPair {
    channels: [Dynamics; 2],
}

impl DynamicsPair {
    fn process(&mut self, frame: &mut [f32; 2], included: [bool; 2], linked: bool) {
        let active = [
            included[0] && self.channels[0].enabled,
            included[1] && self.channels[1].enabled,
        ];
        if !active[0] && !active[1] {
            return;
        }
        let linked = linked && active[0] && active[1];
        let peak = frame[0].abs().max(frame[1].abs());
        let mut gain_db = [0.0_f32; 2];
        for i in 0..2 {
            if active[i] {
                gain_db[i] = self.channels[i].detect(if linked { peak } else { frame[i].abs() });
            }
        }
        if linked {
            // Different channel thresholds/times still produce a common control gain.
            let strongest = if gain_db[0].abs() >= gain_db[1].abs() {
                gain_db[0]
            } else {
                gain_db[1]
            };
            gain_db = [strongest; 2];
        }
        for i in 0..2 {
            if active[i] {
                frame[i] *= linear(gain_db[i]) * self.channels[i].makeup;
            }
        }
    }

    fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.gain_db = 0.0;
        }
    }
}

#[derive(Clone, Copy)]
struct LimiterChannel {
    enabled: bool,
    ceiling: f32,
    drive: f32,
    release: f32,
    gain: f32,
}

impl Default for LimiterChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            ceiling: linear(-0.3),
            drive: 1.0,
            release: 0.01,
            gain: 1.0,
        }
    }
}

impl LimiterChannel {
    fn configure(&mut self, s: &LimiterSettings, sr: u32) {
        self.enabled = s.enabled;
        self.ceiling = linear(s.ceiling_db.clamp(-24.0, 0.0));
        self.drive = linear(s.drive_db.clamp(0.0, 24.0));
        let rel_s = s.release_ms.clamp(5.0, 500.0) * 0.001;
        self.release = 1.0 - (-1.0 / (rel_s * sr.max(1) as f32)).exp();
        if !self.enabled {
            self.gain = 1.0;
        }
    }

    #[inline]
    fn detect(&mut self, level: f32) -> f32 {
        let target = if level > self.ceiling && level > 1e-9 {
            (self.ceiling / level).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if target < self.gain {
            // Instant brickwall attack prevents overshoots above ceiling.
            self.gain = target;
        } else {
            self.gain += (target - self.gain) * self.release;
        }
        self.gain
    }
}

#[derive(Default)]
struct LimiterPair {
    channels: [LimiterChannel; 2],
}

impl LimiterPair {
    fn process(&mut self, frame: &mut [f32; 2], included: [bool; 2], linked: bool) {
        let active = [
            included[0] && self.channels[0].enabled,
            included[1] && self.channels[1].enabled,
        ];
        if !active[0] && !active[1] {
            return;
        }
        let linked = linked && active[0] && active[1];
        let mut driven = *frame;
        for i in 0..2 {
            if active[i] {
                driven[i] *= self.channels[i].drive;
            }
        }
        let peak = driven[0].abs().max(driven[1].abs());
        let mut gain = [1.0_f32; 2];
        for i in 0..2 {
            if active[i] {
                gain[i] = self.channels[i].detect(if linked { peak } else { driven[i].abs() });
            }
        }
        if linked {
            let strongest = gain[0].min(gain[1]);
            gain = [strongest; 2];
        }
        for i in 0..2 {
            if active[i] {
                let c = self.channels[i].ceiling;
                frame[i] = (driven[i] * gain[i]).clamp(-c, c);
            }
        }
    }

    fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.gain = 1.0;
        }
    }
}

struct Channel {
    side: usize,
    sample_rate: u32,
    input_gain: Gain,
    muted: bool,
    eq: [Biquad; 3],
    spectral: Option<Box<SpectralDenoiser>>,
    noise: DenoiserParams,
    dc: DcSubsonicFilter,
    dc_offset: f32,
    legacy_hp_alpha: f64,
    legacy_hp_history: Option<(f64, f64)>,
    legacy_hp_preroll: usize,
    delay: Vec<f32>,
    delay_pos: usize,
}

impl Channel {
    fn new(side: usize, sr: u32) -> Self {
        Self {
            side,
            sample_rate: sr,
            input_gain: Gain::default(),
            muted: false,
            eq: Default::default(),
            spectral: None,
            noise: DenoiserParams::default(),
            dc: DcSubsonicFilter::new(sr, 20.0),
            dc_offset: 0.0,
            legacy_hp_alpha: 1.0 / (1.0 + std::f64::consts::TAU * 20.0 / f64::from(sr)),
            legacy_hp_history: None,
            legacy_hp_preroll: 0,
            delay: Vec::new(),
            delay_pos: 0,
        }
    }

    fn configure(
        &mut self,
        s: &ChannelSettings,
        profile: Option<&NoiseProfile>,
        profile_changed: bool,
        latency: usize,
        included: bool,
    ) {
        let sr = self.sample_rate;
        self.muted = s.muted;
        self.input_gain.set(
            linear(s.gain_db.clamp(-120.0, 48.0)) * if s.invert { -1.0 } else { 1.0 },
            (sr as usize / 200).max(1),
        );
        let eq = &s.eq;
        for (filter, (band, hz, db)) in self.eq.iter_mut().zip([
            (Band::LowShelf, eq.low_hz, eq.low_db),
            (Band::Peak, eq.mid_hz, eq.mid_db),
            (Band::HighShelf, eq.high_hz, eq.high_db),
        ]) {
            filter.configure(band, sr, hz, if eq.enabled { db } else { 0.0 }, eq.mid_q);
        }
        self.dc.set_params(sr, s.denoiser.dc_cutoff_hz);
        self.dc_offset = profile.map_or(0.0, |p| {
            if self.side == 0 {
                p.dc_offset_l
            } else {
                p.dc_offset_r
            }
        });
        if self.noise.dc_filter_enabled != s.denoiser.dc_filter_enabled {
            self.dc.reset();
        }
        self.noise = s.denoiser.clone();
        // Audition modes belong to live monitoring. Song restoration always renders wet.
        self.noise.audition_mode = AuditionMode::Wet;
        if included && self.noise.enabled {
            let created = self.spectral.is_none();
            let engine = self
                .spectral
                .get_or_insert_with(|| Box::new(SpectralDenoiser::new(sr)));
            if created || profile_changed {
                engine.set_noise_profile(profile.cloned());
            }
        } else {
            self.spectral = None;
        }
        if included && latency > 0 && !self.noise.enabled {
            self.delay.resize(STFT_LATENCY_SAMPLES, 0.0);
        } else {
            self.delay.clear();
            self.delay_pos = 0;
        }
    }

    fn set_legacy_highpass(&mut self, enabled: bool) {
        if enabled != self.legacy_hp_history.is_some() {
            self.legacy_hp_history = enabled.then_some((0.0, 0.0));
            self.legacy_hp_preroll = 0;
        }
    }

    fn reset(&mut self, latency: usize) {
        if let Some(history) = &mut self.legacy_hp_history {
            *history = (0.0, 0.0);
            // Legacy export starts the one-pole filter at the first aligned frame.
            self.legacy_hp_preroll = latency;
        }
        self.input_gain.reset();
        self.dc.reset();
        if let Some(engine) = &mut self.spectral {
            engine.reset();
        }
        self.delay.fill(0.0);
        self.delay_pos = 0;
        for eq in &mut self.eq {
            eq.reset();
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut v = input * self.input_gain.next();
        if self.muted {
            v = 0.0;
        }
        if let Some(engine) = &mut self.spectral {
            v = if self.side == 0 {
                engine.process_sample(v, 0.0, &self.noise).0
            } else {
                engine.process_sample(0.0, v, &self.noise).1
            };
        } else {
            if self.noise.dc_filter_enabled {
                v = self.dc.process_sample(v, 0.0, self.dc_offset, 0.0).0;
            }
            if !self.delay.is_empty() {
                std::mem::swap(&mut v, &mut self.delay[self.delay_pos]);
                self.delay_pos = (self.delay_pos + 1) % STFT_LATENCY_SAMPLES;
            }
        }
        for eq in &mut self.eq {
            v = eq.process(v);
        }
        if let Some((previous_input, previous_output)) = &mut self.legacy_hp_history {
            if self.legacy_hp_preroll > 0 {
                self.legacy_hp_preroll -= 1;
            } else {
                let input = f64::from(v);
                let output = self.legacy_hp_alpha * (*previous_output + input - *previous_input);
                *previous_input = input;
                *previous_output = output;
                v = output as f32;
            }
        }
        if self.muted {
            0.0
        } else {
            v
        }
    }
}

pub struct SongProcessor {
    sample_rate: u32,
    channels: [Channel; 2],
    profile: Option<NoiseProfile>,
    expander: DynamicsPair,
    compressor: DynamicsPair,
    limiter: LimiterPair,
    stereo_link: bool,
    routing: Routing,
    included: [bool; 2],
    bypass: bool,
    swap: bool,
    width: f32,
    mix: [Gain; 2],
    output: [Gain; 2],
    latency: usize,
}

impl SongProcessor {
    pub fn new(sample_rate: u32, settings: &SongSettings) -> Self {
        let sr = sample_rate.max(1);
        let mut p = Self {
            sample_rate: sr,
            channels: std::array::from_fn(|i| Channel::new(i, sr)),
            profile: None,
            expander: Default::default(),
            compressor: Default::default(),
            limiter: Default::default(),
            stereo_link: false,
            routing: Routing::Stereo,
            included: [false; 2],
            bypass: false,
            swap: false,
            width: 1.0,
            mix: Default::default(),
            output: Default::default(),
            latency: 0,
        };
        p.set_settings(settings);
        p.reset();
        p
    }

    /// Ask before applying a playback edit. Exchanging a dry delay line and
    /// an STFT engine loses queued samples even when the total latency is unchanged.
    pub fn requires_reprime(&self, next: &SongSettings) -> bool {
        let spectral = next.spectral_channels();
        let current = self.channels.each_ref().map(|c| c.spectral.is_some());
        spectral != current
            || self.routing != next.routing
            || (self.latency > 0
                && (self.included != next.included_channels()
                    || self.profile.as_ref() != next.valid_profile()))
    }

    /// Apply a settings snapshot at an audio block boundary. Changes to ordinary
    /// controls retain overlap, filter and dynamics histories.
    pub fn set_settings(&mut self, s: &SongSettings) {
        let profile = s.valid_profile();
        let profile_changed = profile != self.profile.as_ref();
        if profile_changed {
            self.profile = profile.cloned();
        }
        let included = s.included_channels();
        let bypass_changed = self.bypass != s.bypass;
        let reset_detectors = included != self.included || bypass_changed;
        let entering_mono = self.routing == Routing::Stereo && s.routing != Routing::Stereo;
        let latency = if s.spectral_channels().contains(&true) {
            STFT_LATENCY_SAMPLES
        } else {
            0
        };
        self.routing = s.routing;
        self.bypass = s.bypass;
        self.stereo_link = s.stereo_dynamics_linked;
        self.swap = s.swap_channels && !s.bypass;
        self.width = if s.bypass {
            1.0
        } else {
            s.width.clamp(0.0, 2.0)
        };
        let ramp = (self.sample_rate as usize / 200).max(1);
        for i in 0..2 {
            let c = &s.channels[i];
            let use_channel = included[i] && !s.bypass;
            self.channels[i].configure(
                c,
                self.profile.as_ref(),
                profile_changed,
                latency,
                use_channel,
            );
            self.channels[i].set_legacy_highpass(s.legacy_highpass_20hz);
            if self.included[i] != included[i] || self.latency != latency || bypass_changed {
                self.channels[i].reset(latency);
            }
            self.expander.channels[i].configure(&c.expander, self.sample_rate, false);
            self.compressor.channels[i].configure(&c.compressor, self.sample_rate, true);
            self.limiter.channels[i].configure(&c.limiter, self.sample_rate);
            let out_db = if s.bypass {
                0.0
            } else {
                let route_gain = if s.routing == Routing::Stereo {
                    // Same correction convention as the legacy export balance.
                    s.balance_db.clamp(-48.0, 48.0) * if i == 0 { 0.5 } else { -0.5 }
                } else {
                    s.mono_gain_db.clamp(-120.0, 48.0)
                };
                s.gain_db.clamp(-120.0, 48.0) + route_gain
            };
            self.output[i].set(linear(out_db), ramp);
            if entering_mono {
                self.output[i].reset();
            }
            self.mix[i].set(
                if i == 0 { s.mix_left } else { s.mix_right }.clamp(0.0, 4.0),
                ramp,
            );
        }
        self.included = included;
        self.latency = latency;
        if reset_detectors {
            self.compressor.reset();
            self.expander.reset();
            self.limiter.reset();
        }
    }

    /// Reset all streaming history at a seek, clip boundary or new render.
    pub fn reset(&mut self) {
        for i in 0..2 {
            self.channels[i].reset(self.latency);
            self.output[i].reset();
            self.mix[i].reset();
        }
        self.expander.reset();
        self.compressor.reset();
        self.limiter.reset();
    }

    /// Number of initial output frames to drop; supply this many zeros after
    /// the last source frame to recover the complete clip at the original length.
    pub fn latency(&self) -> usize {
        self.latency
    }

    #[inline]
    pub fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.process_with_monitor_taps(l, r).0
    }

    /// Return the routed output and the two contributions available for solo monitoring.
    #[inline]
    pub fn process_with_monitor_taps(&mut self, l: f32, r: f32) -> ((f32, f32), (f32, f32)) {
        let mut frame = [l, r];
        if !self.bypass {
            for (i, v) in frame.iter_mut().enumerate() {
                *v = if self.included[i] {
                    self.channels[i].process(*v)
                } else {
                    0.0
                };
            }
            self.expander
                .process(&mut frame, self.included, self.stereo_link);
            self.compressor
                .process(&mut frame, self.included, self.stereo_link);
            self.limiter
                .process(&mut frame, self.included, self.stereo_link);
        }
        let mut contributions = None;
        match self.routing {
            Routing::Stereo => {
                if self.swap {
                    frame.swap(0, 1);
                }
                // Skip the matrix at unity to preserve the original float bits.
                if self.width != 1.0 {
                    let adjust = (frame[0] - frame[1]) * (0.5 * (self.width - 1.0));
                    frame[0] += adjust;
                    frame[1] -= adjust;
                }
            }
            Routing::LeftMono => {
                contributions = Some((frame[0], 0.0));
                frame[1] = frame[0];
            }
            Routing::RightMono => {
                contributions = Some((0.0, frame[1]));
                frame[0] = frame[1];
            }
            Routing::MonoMix => {
                let left = frame[0] * self.mix[0].next();
                let right = frame[1] * self.mix[1].next();
                contributions = Some((left, right));
                frame = [left + right; 2];
            }
        }
        let gains = (self.output[0].next(), self.output[1].next());
        let output = (frame[0] * gains.0, frame[1] * gains.1);
        let taps = contributions.map_or(output, |(left, right)| (left * gains.0, right * gains.1));
        (output, taps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.0001, "{a} vs {b}");
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
            assert!(l.abs() <= ceiling_lin + 1e-5, "left {l} exceeded ceiling {ceiling_lin}");
            assert!(r.abs() <= ceiling_lin + 1e-5, "right {r} exceeded ceiling {ceiling_lin}");
        }
        // Stereo-Link erhält das Pegelverhältnis (R ≈ 0.5 * L) auch beim Limiting!
        assert!(max_l > ceiling_lin * 0.95);
        assert!((max_r / max_l - 0.5).abs() < 0.05, "ratio was {}", max_r / max_l);
    }
}
