use super::stage_meter::{StageLevels, StageMeter};
use super::spectrum::{OutputSpectrum, SpectrumCapture, SpectrumTap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use crate::audio::processing::{MonitorStage, SongProcessor, SongSettings};
use std::sync::Arc;
use parking_lot::Mutex;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use crate::audio::denoiser::{DenoiserParams, NoiseProfile, SpectralDenoiser, STFT_LATENCY_SAMPLES};

pub struct DecodedAudio {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Clone, Copy, Default)]
struct MonitorFrame {
    output: (f32, f32),
    taps: (f32, f32),
}

impl MonitorFrame {
    fn stereo(output: (f32, f32)) -> Self { Self { output, taps: output } }

    fn audible(self, solo: Option<usize>, mono: bool) -> (f32, f32) {
        let value = match solo {
            Some(0) => self.taps.0,
            Some(1) => self.taps.1,
            _ if mono => 0.5 * (self.output.0 + self.output.1),
            _ => return self.output,
        };
        (value, value)
    }
}

pub(crate) struct PlayerDenoiserState {
    pub params: DenoiserParams,
    song: Option<Box<SongProcessor>>,
    song_settings: Option<SongSettings>,
    stage_meter: Option<StageMeter>,
    pub denoiser: Box<SpectralDenoiser>,
    pub initialized: bool,
    pub cur_idx: usize,
    cur_out: MonitorFrame,
    next_out: MonitorFrame,
    solo: Option<usize>,
    mono_monitor: bool,
}

impl PlayerDenoiserState {
    fn new(sample_rate: u32) -> Self {
        Self {
            params: DenoiserParams::default(),
            song: None,
            song_settings: None,
            stage_meter: None,
            denoiser: Box::new(SpectralDenoiser::new(sample_rate)),
            initialized: false,
            cur_idx: 0,
            cur_out: MonitorFrame::default(),
            next_out: MonitorFrame::default(),
            solo: None,
            mono_monitor: false,
        }
    }

    fn latency(&self) -> usize {
        self.song.as_ref().map_or(STFT_LATENCY_SAMPLES, |song| song.latency())
    }

    fn source_frame(&mut self, sample: usize, left: &[f32], right: &[f32], end: usize) -> MonitorFrame {
        if let Some(song) = &mut self.song {
            // Flush only zeros beyond this song/selection, never a neighboring sample.
            let (l, r) = if sample < end { (left[sample], right[sample]) } else { (0.0, 0.0) };
            let stage = self.stage_meter.as_ref().map(StageMeter::stage);
            let ((output, taps), levels) = song.process_monitored(l, r, stage);
            if let (Some(meter), Some(levels)) = (&mut self.stage_meter, levels) {
                meter.push(levels, song.latency());
            }
            MonitorFrame { output, taps }
        } else {
            let sample = sample.min(end.saturating_sub(1));
            MonitorFrame::stereo(self.denoiser.process_sample(left[sample], right[sample], &self.params))
        }
    }

    fn prime_at(&mut self, idx: usize, left: &[f32], right: &[f32], end: usize) {
        if let Some(song) = &mut self.song { song.reset(); } else { self.denoiser.reset(); }
        let latency = self.latency();
        if let Some(meter) = &mut self.stage_meter {
            meter.reset();
        }
        for sample in idx..idx + latency {
            let _ = self.source_frame(sample, left, right, end);
        }
        // Exclude preroll peaks from the visible interval, retaining delay alignment.
        if let Some(meter) = &mut self.stage_meter {
            meter.take();
        }
        self.cur_out = self.source_frame(idx + latency, left, right, end);
        self.next_out = self.source_frame(idx + latency + 1, left, right, end);
        self.cur_idx = idx;
        self.initialized = true;
    }

    #[inline]
    fn sample_pair_at(&mut self, idx: usize, frac: f32, left: &[f32], right: &[f32], end: usize) -> (f32, f32) {
        if !self.initialized || idx < self.cur_idx {
            self.prime_at(idx, left, right, end);
        }
        while self.cur_idx < idx {
            self.cur_idx += 1;
            self.cur_out = self.next_out;
            self.next_out = self.source_frame(self.cur_idx + self.latency() + 1, left, right, end);
        }
        let current = self.cur_out.audible(self.solo, self.mono_monitor);
        let next = if idx + 1 < end { self.next_out.audible(self.solo, self.mono_monitor) } else { current };
        (current.0 + (next.0 - current.0) * frac,
         current.1 + (next.1 - current.1) * frac)
    }

}

pub struct AudioPlayer {
    samples_l: Arc<Vec<f32>>,
    samples_r: Arc<Vec<f32>>,
    sample_rate: u32,
    clip_range: Option<std::ops::Range<usize>>,
    playhead: Arc<AtomicU64>,
    stop_at_sample: Arc<AtomicUsize>,
    loop_enabled: Arc<AtomicBool>,
    loop_start: Arc<AtomicUsize>,
    output_peaks: Arc<[AtomicU32; 2]>,
    output_clipping: Arc<AtomicBool>,
    spectrum: Option<Arc<SpectrumTap>>,
    is_playing: Arc<AtomicBool>,
    audition: Arc<Mutex<Option<(f64, usize)>>>,
    denoiser_state: Arc<Mutex<PlayerDenoiserState>>,
    stream: Option<Stream>,
    stream_error: Arc<Mutex<Option<String>>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            samples_l: Arc::new(Vec::new()),
            samples_r: Arc::new(Vec::new()),
            sample_rate: 44100,
            clip_range: None,
            playhead: Arc::new(AtomicU64::new(0)),
            stop_at_sample: Arc::new(AtomicUsize::new(0)),
            loop_enabled: Arc::new(AtomicBool::new(false)),
            loop_start: Arc::new(AtomicUsize::new(0)),
            output_peaks: Arc::new([AtomicU32::new(0), AtomicU32::new(0)]),
            output_clipping: Arc::new(AtomicBool::new(false)),
            spectrum: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            audition: Arc::new(Mutex::new(None)),
            denoiser_state: Arc::new(Mutex::new(PlayerDenoiserState::new(44100))),
            stream: None,
            stream_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Share the recording allocation while exposing only the selected song in local time.
    pub fn load_song_clip(&mut self, left: Arc<Vec<f32>>, right: Arc<Vec<f32>>, sample_rate: u32, start: usize, end: usize) -> Result<(), String> {
        if sample_rate == 0 || start >= end || end > left.len() || end > right.len() {
            return Err("Invalid song audio range".into());
        }
        let mut next = Self::new();
        next.sample_rate = sample_rate;
        next.samples_l = left;
        next.samples_r = right;
        next.clip_range = Some(start..end);
        next.spectrum = Some(Arc::new(SpectrumTap::default()));
        {
            let mut state = next.denoiser_state.lock();
            state.denoiser.set_sample_rate(sample_rate);
            state.stage_meter = Some(StageMeter::default());
        }
        *self = next;
        Ok(())
    }

    fn clip_bounds(&self) -> std::ops::Range<usize> {
        self.clip_range.clone().unwrap_or(0..self.samples_l.len())
    }

    pub fn ensure_output(&mut self) -> Result<(), String> {
        if self.stream.is_none() { self.init_output_stream()?; }
        Ok(())
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
        let mut ds = self.denoiser_state.lock();
        ds.denoiser.set_sample_rate(audio.sample_rate);
        ds.initialized = false;
    }

    /// Install audio independently of the availability of a playback device.
    pub fn load_decoded(&mut self, audio: DecodedAudio) {
        let (params, profile) = {
            let state = self.denoiser_state.lock();
            (state.params.clone(), state.denoiser.noise_profile().cloned())
        };
        let mut next = Self::new();
        next.sample_rate = audio.sample_rate;
        next.samples_l = Arc::new(audio.left);
        next.samples_r = Arc::new(audio.right);
        {
            let mut state = next.denoiser_state.lock();
            state.denoiser.set_sample_rate(audio.sample_rate);
            state.denoiser.set_noise_profile(profile);
            state.params = params;
        }
        *self = next;
    }

    pub fn set_song_settings(&self, settings: &SongSettings) {
        let mut state = self.denoiser_state.lock();
        if state.song_settings.as_ref() == Some(settings) { return; }
        if let Some(song) = &mut state.song {
            let reprime = song.requires_reprime(settings);
            song.set_settings(settings);
            if reprime { state.initialized = false; }
        } else {
            state.song = Some(Box::new(SongProcessor::new(self.sample_rate, settings)));
            state.initialized = false;
        }
        state.song_settings = Some(settings.clone());
    }

    pub fn set_song_monitor(&self, solo: Option<usize>, mono: bool) {
        let mut state = self.denoiser_state.lock();
        state.solo = solo;
        state.mono_monitor = mono;
    }

    pub fn set_looping(&self, enabled: bool) { self.loop_enabled.store(enabled, Ordering::Relaxed); }

    pub fn output_peaks(&self) -> (f32, f32, bool) {
        (f32::from_bits(self.output_peaks[0].load(Ordering::Relaxed)),
         f32::from_bits(self.output_peaks[1].load(Ordering::Relaxed)),
         self.output_clipping.load(Ordering::Relaxed))
    }

    pub(crate) fn take_stage_levels(&self, stage: MonitorStage) -> Option<StageLevels> {
        let mut state = self.denoiser_state.try_lock()?;
        let meter = state.stage_meter.as_mut()?;
        meter.select(stage);
        meter.take()
    }

    pub fn set_spectrum_enabled(&self, enabled: bool) {
        if let Some(tap) = &self.spectrum {
            tap.set_enabled(enabled);
        }
    }

    pub fn take_spectrum(&self) -> Option<OutputSpectrum> {
        self.spectrum.as_ref()?.take()
    }

    pub fn reset_output_clip(&self) { self.output_clipping.store(false, Ordering::Relaxed); }

    pub fn set_denoiser_params(&self, params: DenoiserParams) {
        let mut ds = self.denoiser_state.lock();
        let toggled = ds.params.enabled != params.enabled;
        ds.params = params;
        if toggled {
            ds.initialized = false;
        }
    }

    pub fn set_noise_profile(&self, profile: Option<NoiseProfile>) {
        let mut ds = self.denoiser_state.lock();
        ds.denoiser.set_noise_profile(profile);
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
        let output_rate = config.sample_rate().0;
        let source_step = f64::from(self.sample_rate) / f64::from(output_rate);
        let mut spectrum = self.spectrum.clone().map(|tap| (tap, SpectrumCapture::new()));

        let audition = self.audition.clone();
        let denoiser_arc = self.denoiser_state.clone();
        let playhead_arc = self.playhead.clone();
        let stop_at_sample_arc = self.stop_at_sample.clone();
        let is_playing_arc = self.is_playing.clone();
        let loop_enabled = self.loop_enabled.clone();
        let loop_start = self.loop_start.clone();
        let output_peaks = self.output_peaks.clone();
        let output_clipping = self.output_clipping.clone();
        let samples_l = self.samples_l.clone();
        let samples_r = self.samples_r.clone();
        let clip_range = self.clip_bounds();
        let total_samples = clip_range.len();

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
                    let clip_l = &samples_l[clip_range.clone()];
                    let clip_r = &samples_r[clip_range.clone()];
                    // Contention must never turn a processed block into dry audio.
                    let mut ds_guard = denoiser_arc.lock();
                    let ds_opt = (ds_guard.song.is_some() || ds_guard.params.enabled).then_some(&mut *ds_guard);
                    if let Some(mut preview) = audition.try_lock() {
                        if let Some((start, end)) = *preview {
                            let next = render_output_filtered(
                                data, output_channels, clip_l, clip_r,
                                start, end, source_step, ds_opt,
                            );
                            drop(ds_guard);
                            *preview = (next < end as f64).then_some((next, end));
                            if let Some((tap, capture)) = &mut spectrum {
                                capture.push(tap, data, output_channels, output_rate);
                            }
                            return;
                        }
                    }
                    if !is_playing_arc.load(Ordering::Relaxed) || total_samples == 0 {
                        data.fill(0.0);
                        if let Some(meter) = &mut ds_guard.stage_meter {
                            meter.reset();
                        }
                        if let Some((_, capture)) = &mut spectrum {
                            capture.reset();
                        }
                        output_peaks[0].store(0, Ordering::Relaxed);
                        output_peaks[1].store(0, Ordering::Relaxed);
                        return;
                    }

                    let start = playhead_arc.load(Ordering::Relaxed);
                    let stop = stop_at_sample_arc.load(Ordering::Relaxed);
                    let end = if stop == 0 { total_samples } else { stop.min(total_samples) };
                    let looping = loop_enabled.load(Ordering::Relaxed);
                    let loop_from = loop_start.load(Ordering::Relaxed).min(end.saturating_sub(1));
                    let position = render_output_with_loop(
                        data, output_channels, clip_l, clip_r,
                        f64::from_bits(start), end, source_step, ds_opt,
                        looping.then_some(loop_from),
                    );
                    drop(ds_guard);
                    let mut peaks = [0.0_f32; 2];
                    for frame in data.chunks_mut(output_channels) {
                        for channel in 0..output_channels.min(2) {
                            peaks[channel] = peaks[channel].max(frame[channel].abs());
                            if frame[channel].abs() >= 1.0 { output_clipping.store(true, Ordering::Relaxed); }
                            frame[channel] = frame[channel].clamp(-1.0, 1.0);
                        }
                    }
                    for channel in 0..2 { output_peaks[channel].store(peaks[channel].to_bits(), Ordering::Relaxed); }
                    if let Some((tap, capture)) = &mut spectrum {
                        capture.push(tap, data, output_channels, output_rate);
                    }
                    let end_reached = !looping && position >= end as f64;
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
        self.loop_start.store(0, Ordering::SeqCst);
        self.is_playing.store(true, Ordering::SeqCst);
    }

    pub fn play_range(&self, start_seconds: f64, end_seconds: f64) {
        self.start_range(start_seconds, start_seconds, end_seconds);
    }

    fn start_range(&self, loop_start: f64, position: f64, end: f64) {
        if !loop_start.is_finite() || !position.is_finite() || !end.is_finite() {
            return;
        }
        let rate = f64::from(self.sample_rate);
        let first = (loop_start * rate).round() as usize;
        let last = ((end * rate).round() as usize).min(self.total_samples());
        if first >= last { return; }
        let resume = ((position * rate).round() as usize).clamp(first, last - 1);
        self.stop_audition();
        let mut state = self.denoiser_state.lock();
        state.initialized = false;
        self.playhead.store((resume as f64).to_bits(), Ordering::SeqCst);
        self.loop_start.store(first, Ordering::SeqCst);
        self.stop_at_sample.store(last, Ordering::SeqCst);
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
        self.start_range(start, resume, end);
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
        let mut state = self.denoiser_state.lock();
        state.initialized = false;
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
            self.total_samples() as f64 / self.sample_rate as f64
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn total_samples(&self) -> usize {
        self.clip_bounds().len()
    }

    pub fn get_samples_l(&self) -> Arc<Vec<f32>> {
        self.samples_l.clone()
    }

    pub fn get_samples_r(&self) -> Arc<Vec<f32>> {
        self.samples_r.clone()
    }
}

#[cfg(test)]
fn render_output(
    data: &mut [f32],
    output_channels: usize,
    left: &[f32],
    right: &[f32],
    position: f64,
    end: usize,
    source_step: f64,
) -> f64 {
    render_output_filtered(data, output_channels, left, right, position, end, source_step, None)
}

fn render_output_filtered(
    data: &mut [f32],
    output_channels: usize,
    left: &[f32],
    right: &[f32],
    position: f64,
    end: usize,
    source_step: f64,
    denoiser: Option<&mut PlayerDenoiserState>,
) -> f64 {
    render_output_with_loop(data, output_channels, left, right, position, end, source_step, denoiser, None)
}

fn render_output_with_loop(
    data: &mut [f32], output_channels: usize, left: &[f32], right: &[f32],
    mut position: f64, end: usize, source_step: f64,
    mut denoiser: Option<&mut PlayerDenoiserState>, loop_from: Option<usize>,
) -> f64 {
    for frame in data.chunks_mut(output_channels) {
        frame.fill(0.0);
        if position >= end as f64 {
            if let Some(start) = loop_from.filter(|&start| start < end) {
                position = start as f64 + (position - end as f64) % (end - start) as f64;
                if let Some(state) = denoiser.as_deref_mut() { state.initialized = false; }
            } else { continue; }
        }
        let index = position.floor() as usize;
        let next = (index + 1).min(end - 1);
        let fraction = (position - index as f64) as f32;
        let (out_l, out_r) = if let Some(ds) = denoiser.as_deref_mut() {
            ds.sample_pair_at(index, fraction, left, right, end)
        } else {
            let l = left[index] + (left[next] - left[index]) * fraction;
            let r = right[index] + (right[next] - right[index]) * fraction;
            (l, r)
        };
        if frame.len() >= 2 {
            frame[0] = out_l;
            frame[1] = out_r;
        } else {
            frame[0] = (out_l + out_r) * 0.5;
        }
        position += source_step;
    }
    if let Some(start) = loop_from.filter(|&start| start < end) {
        if position >= end as f64 {
            position = start as f64 + (position - end as f64) % (end - start) as f64;
            if let Some(state) = denoiser { state.initialized = false; }
        }
    }
    position
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_downsampling_keeps_dynamics_history() {
        let signal = vec![0.8; 4096];
        let mut settings = SongSettings::default();
        for channel in &mut settings.channels {
            channel.compressor.enabled = true;
            channel.compressor.threshold_db = -30.0;
            channel.compressor.ratio = 4.0;
        }
        let mut player = AudioPlayer::new();
        player.load_song_clip(Arc::new(signal.clone()), Arc::new(signal.clone()), 192000, 0, signal.len()).unwrap();
        player.set_song_settings(&settings);
        let mut state = player.denoiser_state.lock();
        let mut downsampled = [0.0; 1024];
        render_output_filtered(&mut downsampled, 2, &signal, &signal, 0.0, signal.len(), 4.0, Some(&mut state));
        let mut reference = SongProcessor::new(192000, &settings);
        for (i, sample) in signal.iter().take(2048).enumerate() {
            let output = reference.process_frame(*sample, *sample);
            if i % 4 == 0 {
                assert!((downsampled[i / 2] - output.0).abs() < 0.000001);
            }
        }
    }

    #[test]
    fn reordering_the_live_chain_preserves_position_and_updates_the_stage_meter() {
        use crate::audio::processing::{EffectStage, LimiterSettings};
        let source = Arc::new(vec![0.4; 32]);
        let mut player = AudioPlayer::new();
        player
            .load_song_clip(source.clone(), source.clone(), 48_000, 0, source.len())
            .unwrap();
        let mut settings = SongSettings::default();
        for channel in &mut settings.channels {
            channel.gain_db = 6.0206;
            channel.limiter = LimiterSettings {
                enabled: true,
                ceiling_db: -6.0206,
                ..Default::default()
            };
        }
        let mut position = 0.0;
        let stage = MonitorStage::Insert(EffectStage::Limiter);
        assert!(player.take_stage_levels(stage).is_none());
        for (expected, limiter_input) in [(0.5, 0.8), (0.8, 0.4)] {
            player.set_song_settings(&settings);
            let mut buffer = [0.0; 8];
            {
                let mut state = player.denoiser_state.lock();
                position = render_output_filtered(
                    &mut buffer, 2, &source, &source, position, source.len(), 1.0, Some(&mut state),
                );
            }
            assert!(buffer.iter().all(|sample| (sample - expected).abs() < 0.0001));
            let levels = player.take_stage_levels(stage).unwrap();
            assert!((levels.before - limiter_input).abs() < 0.0001);
            assert!((levels.difference - (limiter_input - 0.5_f32).max(0.0)).abs() < 0.0001);
            settings.effect_chain.move_by(EffectStage::Limiter, -5);
        }
        assert_eq!(position, 8.0);
    }

    #[test]
    fn mono_monitor_folds_processed_stereo_and_solo_uses_weighted_taps() {
        let mut player = AudioPlayer::new();
        player.load_song_clip(Arc::new(vec![0.5; 8]), Arc::new(vec![0.25; 8]), 44100, 0, 8).unwrap();
        let mut settings = SongSettings { balance_db: 12.0, mono_gain_db: 12.0, ..Default::default() };
        player.set_song_settings(&settings);
        player.set_song_monitor(None, true);
        let mut buffer = [0.0; 2];
        {
            let mut state = player.denoiser_state.lock();
            render_output_filtered(&mut buffer, 2, &[0.5; 8], &[0.25; 8], 0.0, 8, 1.0, Some(&mut state));
        }
        let expected = (0.5 * 10_f32.powf(0.3) + 0.25 * 10_f32.powf(-0.3)) * 0.5;
        assert!((buffer[0] - expected).abs() < 0.000001);
        assert_eq!(buffer[0], buffer[1]);

        settings.routing = crate::audio::processing::Routing::MonoMix;
        settings.mix_left = 0.0;
        settings.mix_right = 0.8;
        player.set_song_settings(&settings);
        player.set_song_monitor(Some(0), false);
        let mut state = player.denoiser_state.lock();
        render_output_filtered(&mut buffer, 2, &[0.5; 8], &[0.25; 8], 0.0, 8, 1.0, Some(&mut state));
        assert_eq!(buffer, [0.0; 2]);
        assert_eq!(state.song_settings.as_ref(), Some(&settings));
    }

    #[test]
    fn song_channel_filter_edit_reprimes_without_changing_total_latency() {
        let source = Arc::new(vec![0.5; 6000]);
        let mut player = AudioPlayer::new();
        player.load_song_clip(source.clone(),source.clone(),44100,0,6000).unwrap();
        let mut settings = SongSettings::default();
        settings.channels[0].denoiser.enabled = true;
        settings.channels[0].denoiser.reduction_db = 0.0;
        settings.channels[1].denoiser.reduction_db = 0.0;
        player.set_song_settings(&settings);
        let mut position = 0.0;
        for enabled in [false,true,false] {
            settings.channels[1].denoiser.enabled = enabled;
            player.set_song_settings(&settings);
            let mut data = [0.0; 64];
            let mut state = player.denoiser_state.lock();
            position = render_output_filtered(&mut data,2,&source,&source,position,6000,1.0,Some(&mut state));
            assert!(data.iter().all(|&sample| (sample-0.5).abs()<0.0001), "channel change muted a block: {data:?}");
        }
    }

    #[test]
    fn song_playback_processes_mono_mix_and_wraps_without_neighbor_audio() {
        use crate::audio::processing::{Routing, SongSettings};
        let mut player = AudioPlayer::new();
        player.load_song_clip(Arc::new(vec![0.99,0.2,0.4,-0.99]), Arc::new(vec![-0.99,0.4,0.2,0.99]),100,1,3).unwrap();
        let settings = SongSettings { routing: Routing::MonoMix, mix_left: 0.25, mix_right: 0.75, ..Default::default() };
        player.set_song_settings(&settings);
        let mut output = [0.0; 10];
        let mut ds = player.denoiser_state.lock();
        let position = render_output_with_loop(&mut output, 2, &[0.2,0.4], &[0.4,0.2], 0.0, 2, 1.0, Some(&mut ds), Some(0));
        for (sample, expected) in output.iter().zip([0.35,0.35,0.25,0.25,0.35,0.35,0.25,0.25,0.35,0.35]) {
            assert!((sample-expected).abs()<0.00001, "{output:?}");
        }
        assert_eq!(position,1.0);
    }

    #[test]
    fn song_clip_uses_local_time_and_never_reads_neighbor_samples() {
        let mut player = AudioPlayer::new();
        let left = Arc::new(vec![0.9, 0.9, 0.2, 0.3, 0.4, -0.9]);
        let right = Arc::new(vec![-0.9, -0.9, -0.2, -0.3, -0.4, 0.9]);
        player.load_song_clip(left, right, 100, 2, 5).unwrap();
        assert_eq!(player.total_samples(), 3);
        assert_eq!(player.total_seconds(), 0.03);
        player.seek_seconds(99.0);
        assert_eq!(player.current_seconds(), 0.03);
        let mut output = vec![1.0; 10];
        let range = player.clip_bounds();
        render_output(&mut output, 2, &player.samples_l[range.clone()], &player.samples_r[range], 0.0, 3, 1.0);
        assert_eq!(output, vec![0.2, -0.2, 0.3, -0.3, 0.4, -0.4, 0.0, 0.0, 0.0, 0.0]);
        assert!(player.load_song_clip(Arc::new(vec![0.0; 3]), Arc::new(vec![0.0; 2]), 100, 0, 3).is_err());
    }

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
