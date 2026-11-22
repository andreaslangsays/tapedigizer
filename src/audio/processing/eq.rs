use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeakEqBand {
    pub hz: f32,
    pub gain_db: f32,
    pub q: f32,
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
    pub peaks: [PeakEqBand; 2],
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
            peaks: [
                PeakEqBand {
                    hz: 350.0,
                    gain_db: 0.0,
                    q: 1.0,
                },
                PeakEqBand {
                    hz: 3500.0,
                    gain_db: 0.0,
                    q: 1.0,
                },
            ],
        }
    }
}

impl EqSettings {
    fn filters(&self) -> [(Band, f32, f32, f32); 5] {
        let [lower, upper] = self.peaks;
        [
            (Band::LowShelf, self.low_hz, self.low_db, 1.0),
            (Band::Peak, lower.hz, lower.gain_db, lower.q),
            (Band::Peak, self.mid_hz, self.mid_db, self.mid_q),
            (Band::Peak, upper.hz, upper.gain_db, upper.q),
            (Band::HighShelf, self.high_hz, self.high_db, 1.0),
        ]
    }

    /// Build the same filters used by playback and export for the EQ graph.
    pub(crate) fn response(&self, sample_rate: u32) -> EqResponse {
        let rate = sample_rate.max(1);
        let coefficients = self.filters().map(|(kind, hz, gain, q)| {
            let mut filter = Biquad::default();
            filter.configure(kind, rate, hz, gain, q);
            filter.coeff
        });
        EqResponse { rate, coefficients }
    }
}

pub(crate) struct EqResponse {
    rate: u32,
    coefficients: [Option<[f64; 5]>; 5],
}

impl EqResponse {
    pub(crate) fn gain_db(&self, hz: f32) -> f32 {
        let angle = std::f64::consts::TAU * f64::from(hz).min(f64::from(self.rate) * 0.5)
            / f64::from(self.rate);
        let (sin, cos) = angle.sin_cos();
        let (sin2, cos2) = (2.0 * angle).sin_cos();
        let power = |a: f64, b: f64, c: f64| {
            (a + b * cos + c * cos2).powi(2) + (b * sin + c * sin2).powi(2)
        };
        self.coefficients
            .iter()
            .flatten()
            .map(|&[b0, b1, b2, a1, a2]| {
                (10.0 * (power(b0, b1, b2) / power(1.0, a1, a2)).log10()) as f32
            })
            .sum()
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
        // RBJ shelving filters with shelf slope 1; peaking filters use Q.
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
pub(super) struct Equalizer([Biquad; 5]);

impl Equalizer {
    pub(super) fn configure(&mut self, settings: &EqSettings, sr: u32, active: bool) {
        for (filter, (band, hz, db, q)) in self.0.iter_mut().zip(settings.filters()) {
            filter.configure(
                band,
                sr,
                hz,
                if settings.enabled && active { db } else { 0.0 },
                q,
            );
        }
    }

    pub(super) fn process(&mut self, mut input: f32) -> f32 {
        for filter in &mut self.0 {
            input = filter.process(input);
        }
        input
    }

    pub(super) fn reset(&mut self) {
        for filter in &mut self.0 {
            filter.reset();
        }
    }
}
