use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

pub struct AudioPlayer {
    samples_l: Arc<Vec<f32>>,
    samples_r: Arc<Vec<f32>>,
    sample_rate: u32,
    playhead: Arc<AtomicUsize>,
    stop_at_sample: Arc<AtomicUsize>,
    is_playing: Arc<AtomicBool>,
    stream: Option<Stream>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            samples_l: Arc::new(Vec::new()),
            samples_r: Arc::new(Vec::new()),
            sample_rate: 44100,
            playhead: Arc::new(AtomicUsize::new(0)),
            stop_at_sample: Arc::new(AtomicUsize::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            stream: None,
        }
    }

    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        self.stop();

        let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        self.sample_rate = spec.sample_rate;

        let mut l = Vec::new();
        let mut r = Vec::new();

        let max_val = match spec.bits_per_sample {
            16 => 32768.0,
            24 => 8388608.0,
            32 => 2147483648.0,
            _ => 32768.0,
        };

        if spec.channels == 2 {
            let mut iter = reader.samples::<i32>();
            while let (Some(Ok(s_l)), Some(Ok(s_r))) = (iter.next(), iter.next()) {
                l.push(s_l as f32 / max_val);
                r.push(s_r as f32 / max_val);
            }
        } else if spec.channels == 1 {
            for s in reader.samples::<i32>() {
                if let Ok(sample) = s {
                    let val = sample as f32 / max_val;
                    l.push(val);
                    r.push(val);
                }
            }
        }

        self.samples_l = Arc::new(l);
        self.samples_r = Arc::new(r);
        self.playhead.store(0, Ordering::SeqCst);

        self.init_output_stream()?;
        Ok(())
    }

    fn init_output_stream(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "Kein Standard-Ausgabegerät gefunden".to_string())?;

        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let output_channels = config.channels() as usize;

        let playhead_arc = self.playhead.clone();
        let stop_at_sample_arc = self.stop_at_sample.clone();
        let is_playing_arc = self.is_playing.clone();
        let samples_l = self.samples_l.clone();
        let samples_r = self.samples_r.clone();
        let total_samples = samples_l.len();

        let err_fn = |err| eprintln!("Player Stream Fehler: {}", err);

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    if !is_playing_arc.load(Ordering::Relaxed) || total_samples == 0 {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                        return;
                    }

                    let mut pos = playhead_arc.load(Ordering::Relaxed);
                    let stop_sample = stop_at_sample_arc.load(Ordering::Relaxed);

                    for frame in data.chunks_mut(output_channels) {
                        if pos < total_samples && (stop_sample == 0 || pos < stop_sample) {
                            let sl = samples_l[pos];
                            let sr = samples_r[pos];

                            if frame.len() >= 2 {
                                frame[0] = sl;
                                frame[1] = sr;
                                for extra in &mut frame[2..] {
                                    *extra = 0.0;
                                }
                            } else if !frame.is_empty() {
                                frame[0] = (sl + sr) * 0.5;
                            }
                            pos += 1;
                        } else {
                            for s in frame.iter_mut() {
                                *s = 0.0;
                            }
                            is_playing_arc.store(false, Ordering::Relaxed);
                            stop_at_sample_arc.store(0, Ordering::Relaxed);
                        }
                    }

                    playhead_arc.store(pos, Ordering::Relaxed);
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn play(&self) {
        self.stop_at_sample.store(0, Ordering::SeqCst);
        self.is_playing.store(true, Ordering::SeqCst);
    }

    pub fn play_range(&self, start_seconds: f64, end_seconds: f64) {
        self.seek_seconds(start_seconds);
        let end_sample = (end_seconds * self.sample_rate as f64).round() as usize;
        let clamped = end_sample.min(self.total_samples());
        self.stop_at_sample.store(clamped, Ordering::SeqCst);
        self.is_playing.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.stop_at_sample.store(0, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn toggle_play(&self) {
        let current = self.is_playing.load(Ordering::SeqCst);
        if current {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn stop(&mut self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.stop_at_sample.store(0, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
    }

    pub fn stop_boundary_seconds(&self) -> Option<f64> {
        let stop = self.stop_at_sample.load(Ordering::Relaxed);
        if stop > 0 && self.sample_rate > 0 {
            Some(stop as f64 / self.sample_rate as f64)
        } else {
            None
        }
    }

    pub fn seek_seconds(&self, seconds: f64) {
        let sample = (seconds * self.sample_rate as f64).round() as usize;
        let clamped = sample.min(self.total_samples());
        self.playhead.store(clamped, Ordering::SeqCst);
    }

    pub fn current_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.playhead.load(Ordering::Relaxed) as f64 / self.sample_rate as f64
        }
    }

    pub fn total_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples_l.len() as f64 / self.sample_rate as f64
        }
    }

    pub fn total_samples(&self) -> usize {
        self.samples_l.len()
    }

    pub fn get_samples_l(&self) -> Arc<Vec<f32>> {
        self.samples_l.clone()
    }

    pub fn get_samples_r(&self) -> Arc<Vec<f32>> {
        self.samples_r.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_range_bounds() {
        let mut player = AudioPlayer::new();
        player.sample_rate = 44100;
        player.samples_l = Arc::new(vec![0.1; 44100 * 10]); // 10 seconds of audio
        player.samples_r = Arc::new(vec![0.1; 44100 * 10]);

        player.play_range(2.0, 5.0);
        assert!(player.is_playing());
        assert_eq!(player.current_seconds(), 2.0);
        assert_eq!(player.stop_boundary_seconds(), Some(5.0));

        player.pause();
        assert!(!player.is_playing());
        assert_eq!(player.stop_boundary_seconds(), None);

        player.play();
        assert!(player.is_playing());
        assert_eq!(player.stop_boundary_seconds(), None); // continuous play has no stop bound
    }
}
