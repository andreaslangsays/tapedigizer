pub(super) fn linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// A short, sample-rate-independent ramp for controls that directly scale audio.
#[derive(Default)]
pub(super) struct Gain {
    value: f32,
    target: f32,
    step: f32,
    remaining: usize,
}

impl Gain {
    pub(super) fn set(&mut self, target: f32, frames: usize) {
        if self.target != target {
            self.target = target;
            self.remaining = frames;
            self.step = (target - self.value) / frames as f32;
        }
    }

    pub(super) fn reset(&mut self) {
        self.value = self.target;
        self.remaining = 0;
    }

    #[inline]
    pub(super) fn next(&mut self) -> f32 {
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
