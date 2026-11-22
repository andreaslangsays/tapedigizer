// Compatibility filter for projects adopted from the recording export settings.
pub(super) struct LegacyHighpass {
    alpha: f64,
    history: Option<(f64, f64)>,
    preroll: usize,
}

impl LegacyHighpass {
    pub(super) fn new(sr: u32) -> Self {
        Self {
            alpha: 1.0 / (1.0 + std::f64::consts::TAU * 20.0 / f64::from(sr)),
            history: None,
            preroll: 0,
        }
    }

    pub(super) fn configure(&mut self, enabled: bool) {
        if enabled != self.history.is_some() {
            self.history = enabled.then_some((0.0, 0.0));
            self.preroll = 0;
        }
    }

    pub(super) fn reset(&mut self, latency: usize) {
        if let Some(history) = &mut self.history {
            *history = (0.0, 0.0);
            self.preroll = latency;
        }
    }

    pub(super) fn process(&mut self, value: f32) -> f32 {
        let Some((previous_input, previous_output)) = &mut self.history else {
            return value;
        };
        if self.preroll > 0 {
            self.preroll -= 1;
            return value;
        }
        let input = f64::from(value);
        let output = self.alpha * (*previous_output + input - *previous_input);
        *previous_input = input;
        *previous_output = output;
        output as f32
    }
}
