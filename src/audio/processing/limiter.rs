use super::gain::linear;
use serde::{Deserialize, Serialize};

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
pub(super) struct LimiterPair {
    channels: [LimiterChannel; 2],
}

impl LimiterPair {
    pub(super) fn configure_channel(
        &mut self,
        channel: usize,
        settings: &LimiterSettings,
        sr: u32,
    ) {
        self.channels[channel].configure(settings, sr);
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

    pub(super) fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.gain = 1.0;
        }
    }
}
