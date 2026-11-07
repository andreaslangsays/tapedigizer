use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

pub struct DecodedAudio {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub sample_rate: u32,
}

pub struct AudioPlayer {
    samples_l: Arc<Vec<f32>>,
    samples_r: Arc<Vec<f32>>,
    sample_rate: u32,
    playhead: Arc<AtomicU64>,
    stop_at_sample: Arc<AtomicUsize>,
    is_playing: Arc<AtomicBool>,
    audition: Arc<Mutex<Option<(f64, usize)>>>,
    stream: Option<Stream>,
    stream_error: Arc<Mutex<Option<String>>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            samples_l: Arc::new(Vec::new()),
            samples_r: Arc::new(Vec::new()),
            sample_rate: 44100,
            playhead: Arc::new(AtomicU64::new(0)),
            stop_at_sample: Arc::new(AtomicUsize::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            audition: Arc::new(Mutex::new(None)),
            stream: None,
            stream_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn decode_file<P: AsRef<Path>>(path: P) -> Result<DecodedAudio, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let buf_reader = std::io::BufReader::with_capacity(1024 * 1024, file);
        let mut reader = hound::WavReader::new(buf_reader).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        if !matches!(spec.channels, 1 | 2)
            || spec.sample_rate == 0
            || spec.sample_format != hound::SampleFormat::Int
            || !matches!(spec.bits_per_sample, 16 | 24 | 32)
        {
            return Err("Unsupported WAV format".into());
        }
        let frames = reader.duration() as usize;
        if frames == 0 {
            return Err("WAV contains no audio".into());
        }
        let mut left = Vec::with_capacity(frames);
        let mut right = Vec::with_capacity(frames);
        let scale = (1_u32 << (spec.bits_per_sample - 1)) as f32;
        let mut samples = reader.samples::<i32>();
        for _ in 0..frames {
            let l = samples.next().ok_or("Incomplete WAV frame")?.map_err(|e| e.to_string())? as f32 / scale;
            let r = if spec.channels == 2 {
                samples.next().ok_or("Incomplete WAV frame")?.map_err(|e| e.to_string())? as f32 / scale
            } else {
                l
            };
            left.push(l);
            right.push(r);
        }
        Ok(DecodedAudio { left, right, sample_rate: spec.sample_rate })
    }

    #[cfg(test)]
    pub(crate) fn load_test_samples(&mut self, audio: DecodedAudio) {
        self.sample_rate = audio.sample_rate;
        self.samples_l = Arc::new(audio.left);
        self.samples_r = Arc::new(audio.right);
    }

    pub fn load_decoded(&mut self, audio: DecodedAudio) -> Result<(), String> {
        let mut next = Self::new();
        next.sample_rate = audio.sample_rate;
        next.samples_l = Arc::new(audio.left);
        next.samples_r = Arc::new(audio.right);
        next.init_output_stream()?;
        *self = next;
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn init_output_stream(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "Kein Standard-Ausgabegerät gefunden".to_string())?;

        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let output_channels = config.channels() as usize;
        let source_step = f64::from(self.sample_rate) / f64::from(config.sample_rate().0);

        let audition = self.audition.clone();
        let playhead_arc = self.playhead.clone();
        let stop_at_sample_arc = self.stop_at_sample.clone();
        let is_playing_arc = self.is_playing.clone();
        let samples_l = self.samples_l.clone();
        let samples_r = self.samples_r.clone();
        let total_samples = samples_l.len();

        let error_playing = self.is_playing.clone();
        let stream_error = self.stream_error.clone();
        let error_audition = audition.clone();
        let err_fn = move |err: cpal::StreamError| {
            error_audition.lock().take();
            error_playing.store(false, Ordering::Relaxed);
            *stream_error.lock() = Some(err.to_string());
        };

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    if let Some(mut preview) = audition.try_lock() {
                        if let Some((start, end)) = *preview {
                            let next = render_output(
                                data, output_channels, &samples_l, &samples_r,
                                start, end, source_step,
                            );
                            *preview = (next < end as f64).then_some((next, end));
                            return;
                        }
                    }
                    if !is_playing_arc.load(Ordering::Relaxed) || total_samples == 0 {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                        return;
                    }

                    let start = playhead_arc.load(Ordering::Relaxed);
                    let stop = stop_at_sample_arc.load(Ordering::Relaxed);
                    let end = if stop == 0 { total_samples } else { stop.min(total_samples) };
                    let position = render_output(
                        data, output_channels, &samples_l, &samples_r,
                        f64::from_bits(start), end, source_step,
                    );
                    let end_reached = position >= end as f64;
                    let next = position.min(end as f64).to_bits();
                    if playhead_arc.compare_exchange(start, next, Ordering::Relaxed, Ordering::Relaxed).is_ok()
                        && end_reached
                    {
                        is_playing_arc.store(false, Ordering::Relaxed);
                        stop_at_sample_arc.store(0, Ordering::Relaxed);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn audition_at(&self, seconds: f64) {
        if self.is_playing() || !seconds.is_finite() {
            return;
        }
        let start = (seconds * f64::from(self.sample_rate)).round() as usize;
        let end = start.saturating_add(self.sample_rate as usize * 15).min(self.total_samples());
        *self.audition.lock() = (start < end).then_some((start as f64, end));
    }

    pub fn stop_audition(&self) {
        self.audition.lock().take();
    }

    pub fn is_auditioning(&self) -> bool {
        self.audition.lock().is_some()
    }

    pub fn play(&self) {
        self.stop_audition();
        if self.total_samples() == 0 {
            return;
        }
        if self.current_seconds() >= self.total_seconds() {
            self.seek_seconds(0.0);
        }
        self.stop_at_sample.store(0, Ordering::SeqCst);
        self.is_playing.store(true, Ordering::SeqCst);
    }

    pub fn play_range(&self, start_seconds: f64, end_seconds: f64) {
        if !start_seconds.is_finite() || !end_seconds.is_finite() || start_seconds >= end_seconds {
            return;
        }
        self.stop_audition();
        self.seek_seconds(start_seconds);
        let end_sample = (end_seconds * self.sample_rate as f64).round() as usize;
        let clamped = end_sample.min(self.total_samples());
        self.stop_at_sample.store(clamped, Ordering::SeqCst);
        self.is_playing.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn toggle_range(&self, start: f64, end: f64) {
        if self.is_playing() {
            self.pause();
            return;
        }
        let position = self.current_seconds();
        let resume = if (start..end).contains(&position) { position } else { start };
        self.play_range(resume, end);
    }

    pub fn take_stream_error(&self) -> Option<String> {
        self.stream_error.lock().take()
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
        self.stop_audition();
        self.is_playing.store(false, Ordering::SeqCst);
        self.stop_at_sample.store(0, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
    }

    #[cfg(test)]
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
        self.playhead.store((clamped as f64).to_bits(), Ordering::SeqCst);
    }

    pub fn current_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            f64::from_bits(self.playhead.load(Ordering::Relaxed)) / self.sample_rate as f64
        }
    }

    pub fn total_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples_l.len() as f64 / self.sample_rate as f64
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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

fn render_output(
    data: &mut [f32],
    output_channels: usize,
    left: &[f32],
    right: &[f32],
    mut position: f64,
    end: usize,
    source_step: f64,
) -> f64 {
    for frame in data.chunks_mut(output_channels) {
        frame.fill(0.0);
        if position >= end as f64 {
            continue;
        }
        let index = position.floor() as usize;
        let next = (index + 1).min(end - 1);
        let fraction = (position - index as f64) as f32;
        let left = left[index] + (left[next] - left[index]) * fraction;
        let right = right[index] + (right[next] - right[index]) * fraction;
        if frame.len() >= 2 {
            frame[0] = left;
            frame[1] = right;
        } else {
            frame[0] = (left + right) * 0.5;
        }
        position += source_step;
    }
    position
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_preview_leaves_transport_alone_and_caps_the_range() {
        let mut player = AudioPlayer::new();
        player.sample_rate = 100;
        player.samples_l = Arc::new(vec![0.0; 3000]);
        player.seek_seconds(2.0);
        player.audition_at(7.0);
        assert_eq!(*player.audition.lock(), Some((700.0, 2200)));
        assert_eq!(player.current_seconds(), 2.0);
        assert!(!player.is_playing());
        player.audition_at(28.0);
        assert_eq!(*player.audition.lock(), Some((2800.0, 3000)));
        player.play();
        assert!(player.audition.lock().is_none());
    }

    #[test]
    fn resampling_preserves_time_and_cut_boundary_in_both_directions() {
        for (source_rate, output_rate) in [(44_100, 48_000), (48_000, 44_100)] {
            let source = vec![0.5; source_rate + 1];
            let mut output = vec![0.0; (output_rate + 2) * 2];
            let position = render_output(
                &mut output, 2, &source, &source, 0.0, source_rate,
                source_rate as f64 / output_rate as f64,
            );
            assert!(position >= source_rate as f64);
            assert!(output[..(output_rate - 1) * 2].iter().all(|&v| v == 0.5));
            assert_eq!(&output[(output_rate + 1) * 2..], &[0.0, 0.0]);
        }
    }

    #[test]
    fn scope_transport_pauses_resumes_and_restarts_at_the_end() {
        let mut player = AudioPlayer::new();
        player.sample_rate = 100;
        player.samples_l = Arc::new(vec![0.0; 1000]);
        player.toggle_range(2.0, 5.0);
        assert_eq!(player.current_seconds(), 2.0);
        player.seek_seconds(3.0);
        player.toggle_range(2.0, 5.0);
        assert!(!player.is_playing());
        player.toggle_range(2.0, 5.0);
        assert!(player.is_playing());
        assert_eq!(player.current_seconds(), 3.0);
        assert_eq!(player.stop_boundary_seconds(), Some(5.0));
        player.pause();
        player.seek_seconds(5.0);
        player.toggle_range(2.0, 5.0);
        assert_eq!(player.current_seconds(), 2.0);
    }

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
