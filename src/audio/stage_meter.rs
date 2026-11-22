use super::denoiser::STFT_LATENCY_SAMPLES;
use super::processing::{EffectStage, MonitorStage, StageFrame};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StageLevels {
    pub before: f32,
    pub after: f32,
    pub difference: f32,
}

/// Captures stereo peaks for one insert, before the monitor's solo and
/// device output gain. The restoration reference follows its wet delay.
pub(super) struct StageMeter {
    stage: MonitorStage,
    dry_delay: Box<[[f32; 2]]>,
    position: usize,
    filled: usize,
    peaks: Option<StageLevels>,
}

impl Default for StageMeter {
    fn default() -> Self {
        Self {
            stage: MonitorStage::Mix,
            dry_delay: vec![[0.0; 2]; STFT_LATENCY_SAMPLES].into_boxed_slice(),
            position: 0,
            filled: 0,
            peaks: None,
        }
    }
}

impl StageMeter {
    pub(super) fn stage(&self) -> MonitorStage {
        self.stage
    }

    pub(super) fn select(&mut self, stage: MonitorStage) {
        if self.stage != stage {
            self.stage = stage;
            self.reset();
        }
    }

    pub(super) fn reset(&mut self) {
        self.position = 0;
        self.filled = 0;
        self.peaks = None;
    }

    pub(super) fn push(&mut self, mut frame: StageFrame, latency: usize) {
        if self.stage == MonitorStage::Insert(EffectStage::Restoration) && latency > 0 {
            // Swap the dry frame with the one corresponding to the just emitted
            // wet frame. Ignore the initial history from before selection/seek.
            std::mem::swap(&mut frame.before, &mut self.dry_delay[self.position]);
            self.position = (self.position + 1) % latency;
            if self.filled < latency {
                self.filled += 1;
                return;
            }
        }
        let peaks = self.peaks.get_or_insert_with(StageLevels::default);
        for (before, after) in frame.before.into_iter().zip(frame.after) {
            peaks.before = peaks.before.max(before.abs());
            peaks.after = peaks.after.max(after.abs());
            peaks.difference = peaks.difference.max((after - before).abs());
        }
    }

    pub(super) fn take(&mut self) -> Option<StageLevels> {
        self.peaks.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoration_reference_is_latency_aligned_and_difference_is_phase_sensitive() {
        let mut meter = StageMeter::default();
        meter.select(MonitorStage::Insert(EffectStage::Restoration));
        meter.push(
            StageFrame {
                before: [0.5, 0.75],
                after: [0.0; 2],
            },
            2,
        );
        meter.push(
            StageFrame {
                before: [0.25, 0.5],
                after: [0.0; 2],
            },
            2,
        );
        assert_eq!(meter.take(), None);
        meter.push(
            StageFrame {
                before: [0.125; 2],
                after: [0.5, 0.75],
            },
            2,
        );
        assert_eq!(
            meter.take(),
            Some(StageLevels {
                before: 0.75,
                after: 0.75,
                difference: 0.0
            })
        );
        meter.push(
            StageFrame {
                before: [0.5; 2],
                after: [-0.25, -0.5],
            },
            2,
        );
        assert_eq!(
            meter.take(),
            Some(StageLevels {
                before: 0.5,
                after: 0.5,
                difference: 1.0
            })
        );
        meter.select(MonitorStage::Mix);
        meter.push(
            StageFrame {
                before: [0.125; 2],
                after: [0.25; 2],
            },
            2,
        );
        assert_eq!(
            meter.take(),
            Some(StageLevels {
                before: 0.125,
                after: 0.25,
                difference: 0.125
            })
        );
    }
}
