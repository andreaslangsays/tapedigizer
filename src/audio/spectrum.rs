use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};

use parking_lot::Mutex;

use super::denoiser::{FftTables, FFT_SIZE, NUM_BINS};

pub struct OutputSpectrum {
    pub sample_rate: u32,
    pub db: [f32; NUM_BINS],
}

struct SampleBlock {
    channels: [[f32; FFT_SIZE]; 2],
    sample_rate: u32,
}

/// The audio callback publishes the most recent complete block. It can skip
/// publication if the UI is reading, and never waits or performs an FFT.
#[derive(Default)]
pub(super) struct SpectrumTap {
    enabled: AtomicBool,
    latest: Mutex<Option<SampleBlock>>,
}

impl SpectrumTap {
    pub(super) fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::Relaxed) && !enabled {
            self.latest.lock().take();
        }
    }

    pub(super) fn take(&self) -> Option<OutputSpectrum> {
        let block = self.latest.lock().take()?;
        Some(analyze(&block))
    }
}

pub(super) struct SpectrumCapture {
    channels: [[f32; FFT_SIZE]; 2],
    used: usize,
}

impl SpectrumCapture {
    pub(super) fn new() -> Self {
        Self {
            channels: [[0.0; FFT_SIZE]; 2],
            used: 0,
        }
    }

    pub(super) fn reset(&mut self) {
        self.used = 0;
    }

    pub(super) fn push(
        &mut self,
        tap: &SpectrumTap,
        data: &[f32],
        channels: usize,
        sample_rate: u32,
    ) {
        if !tap.enabled.load(Ordering::Relaxed) {
            self.reset();
            return;
        }
        for frame in data.chunks_exact(channels) {
            self.channels[0][self.used] = frame[0];
            self.channels[1][self.used] = frame.get(1).copied().unwrap_or(frame[0]);
            self.used += 1;
            if self.used == FFT_SIZE {
                if let Some(mut slot) = tap.latest.try_lock() {
                    if tap.enabled.load(Ordering::Relaxed) {
                        *slot = Some(SampleBlock {
                            channels: self.channels,
                            sample_rate,
                        });
                    }
                }
                self.reset();
            }
        }
    }
}

fn analyze(block: &SampleBlock) -> OutputSpectrum {
    static TABLES: OnceLock<FftTables> = OnceLock::new();
    let tables = TABLES.get_or_init(FftTables::new);
    let mut fft = [(0.0, 0.0); FFT_SIZE];
    let mut power = [0.0_f32; NUM_BINS];
    for channel in &block.channels {
        for (bin, (&sample, &window)) in fft.iter_mut().zip(channel.iter().zip(&tables.window)) {
            *bin = (sample * window, 0.0);
        }
        tables.fft_in_place(&mut fft, false);
        for (peak, &(real, imaginary)) in power.iter_mut().zip(&fft) {
            *peak = peak.max(real.mul_add(real, imaginary * imaginary));
        }
    }
    // The Hann window halves the coherent gain. The single-sided spectrum
    // doubles positive bins except for DC and Nyquist.
    let db = std::array::from_fn(|index| {
        let factor = if index == 0 || index == FFT_SIZE / 2 {
            2.0
        } else {
            4.0
        };
        let amplitude = power[index].sqrt() * factor / FFT_SIZE as f32;
        (20.0 * amplitude.max(0.000_01).log10()).min(0.0)
    });
    OutputSpectrum {
        sample_rate: block.sample_rate,
        db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_spectrum_preserves_antiphase_stereo_and_drops_stopped_data() {
        let tap = SpectrumTap::default();
        let mut capture = SpectrumCapture::new();
        let tone: Vec<_> = (0..FFT_SIZE)
            .flat_map(|i| {
                let sample =
                    (i as f32 * std::f32::consts::TAU * 32.0 / FFT_SIZE as f32).sin() * 0.5;
                [sample, -sample]
            })
            .collect();
        tap.set_enabled(true);
        capture.push(&tap, &tone[..FFT_SIZE], 2, 48_000);
        assert!(tap.take().is_none());
        capture.push(&tap, &tone[FFT_SIZE..], 2, 48_000);
        let spectrum = tap.take().unwrap();
        assert_eq!(spectrum.sample_rate, 48_000);
        assert!((spectrum.db[32] + 6.02).abs() < 0.02);
        assert!(spectrum.db[100] < -70.0);

        capture.push(&tap, &tone, 2, 48_000);
        tap.set_enabled(false);
        capture.push(&tap, &tone, 2, 48_000);
        assert!(tap.take().is_none());
    }
}
