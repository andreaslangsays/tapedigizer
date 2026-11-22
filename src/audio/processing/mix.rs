use super::gain::{linear, Gain};
use super::{Routing, SongSettings};

#[derive(Default)]
pub(super) struct MainMix {
    pub(super) routing: Routing,
    swap: bool,
    width: f32,
    mix: [Gain; 2],
    output: [Gain; 2],
}

impl MainMix {
    pub(super) fn configure(&mut self, s: &SongSettings, ramp: usize) {
        let entering_mono = self.routing == Routing::Stereo && s.routing != Routing::Stereo;
        self.routing = s.routing;
        self.swap = s.swap_channels && !s.bypass;
        self.width = if s.bypass {
            1.0
        } else {
            s.width.clamp(0.0, 2.0)
        };
        for i in 0..2 {
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
    }

    pub(super) fn reset(&mut self) {
        for gain in self.output.iter_mut().chain(&mut self.mix) {
            gain.reset();
        }
    }

    pub(super) fn process(&mut self, mut frame: [f32; 2]) -> ((f32, f32), (f32, f32)) {
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
