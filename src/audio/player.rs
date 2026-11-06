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

                    for frame in data.chunks_mut(output_channels) {
                        if pos < total_samples {
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
                            pos = 0;
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
        self.is_playing.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn toggle_play(&self) {
        let current = self.is_playing.load(Ordering::SeqCst);
        self.is_playing.store(!current, Ordering::SeqCst);
    }

    pub fn stop(&mut self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
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
