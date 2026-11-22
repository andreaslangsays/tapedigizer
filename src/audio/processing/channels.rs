use super::gain::{linear, Gain};
use super::ChannelSettings;

#[derive(Default)]
pub(super) struct ChannelTrim {
    input_gain: Gain,
    pub(super) muted: bool,
}

impl ChannelTrim {
    pub(super) fn configure(&mut self, settings: &ChannelSettings, ramp: usize) {
        self.muted = settings.muted;
        let polarity = if settings.invert { -1.0 } else { 1.0 };
        self.input_gain.set(
            linear(settings.gain_db.clamp(-120.0, 48.0)) * polarity,
            ramp,
        );
    }

    pub(super) fn reset(&mut self) {
        self.input_gain.reset();
    }

    pub(super) fn process(&mut self, input: f32) -> f32 {
        let gain = self.input_gain.next();
        if self.muted {
            0.0
        } else {
            input * gain
        }
    }
}
