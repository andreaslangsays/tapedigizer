use crate::audio::denoiser::{
    AuditionMode, DcSubsonicFilter, DenoiserParams, NoiseProfile, SpectralDenoiser,
    STFT_LATENCY_SAMPLES,
};

pub(super) struct Restoration {
    side: usize,
    sample_rate: u32,
    spectral: Option<Box<SpectralDenoiser>>,
    noise: DenoiserParams,
    dc: DcSubsonicFilter,
    dc_offset: f32,
    delay: Vec<f32>,
    delay_pos: usize,
}

impl Restoration {
    pub(super) fn new(side: usize, sr: u32) -> Self {
        Self {
            side,
            sample_rate: sr,
            spectral: None,
            noise: DenoiserParams::default(),
            dc: DcSubsonicFilter::new(sr, 20.0),
            dc_offset: 0.0,
            delay: Vec::new(),
            delay_pos: 0,
        }
    }
    pub(super) fn configure(
        &mut self,
        params: &DenoiserParams,
        profile: Option<&NoiseProfile>,
        profile_changed: bool,
        latency: usize,
        active: bool,
    ) {
        let sr = self.sample_rate;
        self.dc.set_params(sr, params.dc_cutoff_hz);
        self.dc_offset = profile.map_or(0.0, |p| {
            if self.side == 0 {
                p.dc_offset_l
            } else {
                p.dc_offset_r
            }
        });
        if self.noise.dc_filter_enabled != (active && params.dc_filter_enabled) {
            self.dc.reset();
        }
        self.noise = params.clone();
        self.noise.enabled &= active;
        self.noise.dc_filter_enabled &= active;
        // Audition modes belong to live monitoring. Song restoration always renders wet.
        self.noise.audition_mode = AuditionMode::Wet;
        if active && self.noise.enabled {
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
        if active && latency > 0 && !self.noise.enabled {
            self.delay.resize(STFT_LATENCY_SAMPLES, 0.0);
        } else {
            self.delay.clear();
            self.delay_pos = 0;
        }
    }

    pub(super) fn is_spectral(&self) -> bool {
        self.spectral.is_some()
    }

    pub(super) fn reset(&mut self) {
        self.dc.reset();
        if let Some(engine) = &mut self.spectral {
            engine.reset();
        }
        self.delay.fill(0.0);
        self.delay_pos = 0;
    }

    pub(super) fn process(&mut self, mut v: f32) -> f32 {
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
        v
    }
}
