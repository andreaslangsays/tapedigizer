use super::gain::linear;
use serde::{Deserialize, Serialize};

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
pub(super) struct DynamicsPair {
    channels: [Dynamics; 2],
}

impl DynamicsPair {
    pub(super) fn configure_channel(
        &mut self,
        channel: usize,
        settings: &DynamicsSettings,
        sr: u32,
        compressor: bool,
    ) {
        self.channels[channel].configure(settings, sr, compressor);
    }

    pub(super) fn process(&mut self, frame: &mut [f32; 2], included: [bool; 2], linked: bool) {
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

    pub(super) fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.gain_db = 0.0;
        }
    }
}
