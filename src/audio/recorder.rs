use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, Stream};
use crossbeam_channel::{bounded, Receiver, Sender};
use crate::audio::device::ChannelPair;
use crate::audio::meter::{linear_to_db, MeterLevels};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RecorderState {
    Idle,
    Monitoring,
    Recording,
    Finalizing,
    Finished,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StereoWaveformPoint {
    pub min_l: f32,
    pub max_l: f32,
    pub min_r: f32,
    pub max_r: f32,
}

pub struct AudioRecorder {
    state: Arc<RwLock<RecorderState>>,
    meter: Arc<RwLock<MeterLevels>>,
    waveform_data: Arc<RwLock<Vec<StereoWaveformPoint>>>,
    
    // Recording controls
    recording_stream: Option<Stream>,
    monitor_stream: Option<Stream>,
    sample_sender: Option<Sender<Vec<(f32, f32)>>>,
    writer_handle: Option<std::thread::JoinHandle<Result<(), String>>>,
    interrupted: Arc<AtomicBool>,
    recording_error: Option<String>,
    
    // Stats
    recorded_samples: Arc<AtomicUsize>,
    total_clipped_samples: Arc<AtomicUsize>,
    max_peak_left: Arc<RwLock<f32>>,
    max_peak_right: Arc<RwLock<f32>>,
    
    start_time: Option<Instant>,
    timer_limit: Option<Duration>,
    silence_autostop_enabled: bool,
    silence_seconds_detected: Arc<RwLock<f32>>,
    
    current_filepath: Option<PathBuf>,
    sample_rate: u32,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RecorderState::Idle)),
            meter: Arc::new(RwLock::new(MeterLevels::default())),
            waveform_data: Arc::new(RwLock::new(Vec::new())),
            recording_stream: None,
            monitor_stream: None,
            sample_sender: None,
            writer_handle: None,
            interrupted: Arc::new(AtomicBool::new(false)),
            recording_error: None,
            recorded_samples: Arc::new(AtomicUsize::new(0)),
            total_clipped_samples: Arc::new(AtomicUsize::new(0)),
            max_peak_left: Arc::new(RwLock::new(0.0)),
            max_peak_right: Arc::new(RwLock::new(0.0)),
            start_time: None,
            timer_limit: None,
            silence_autostop_enabled: false,
            silence_seconds_detected: Arc::new(RwLock::new(0.0)),
            current_filepath: None,
            sample_rate: 48000,
        }
    }

    pub fn state(&self) -> RecorderState {
        *self.state.read()
    }

    pub fn get_meter(&self) -> MeterLevels {
        *self.meter.read()
    }

    pub fn get_waveform_data(&self) -> Vec<StereoWaveformPoint> {
        self.waveform_data.read().clone()
    }

    pub fn with_waveform_data<R>(&self, f: impl FnOnce(&[StereoWaveformPoint]) -> R) -> R {
        let guard = self.waveform_data.read();
        f(&guard)
    }

    pub fn waveform_len(&self) -> usize {
        self.waveform_data.read().len()
    }

    pub fn recorded_samples(&self) -> usize {
        self.recorded_samples.load(Ordering::Relaxed)
    }

    pub fn total_clipped_samples(&self) -> usize {
        self.total_clipped_samples.load(Ordering::Relaxed)
    }

    pub fn max_peak_l_db(&self) -> f32 {
        linear_to_db(*self.max_peak_left.read())
    }

    pub fn max_peak_r_db(&self) -> f32 {
        linear_to_db(*self.max_peak_right.read())
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn current_filepath(&self) -> Option<PathBuf> {
        self.current_filepath.clone()
    }

    pub fn elapsed_duration(&self) -> Duration {
        if let Some(start) = self.start_time {
            start.elapsed()
        } else {
            Duration::ZERO
        }
    }

    pub fn remaining_duration(&self) -> Option<Duration> {
        if let (Some(start), Some(limit)) = (self.start_time, self.timer_limit) {
            let elapsed = start.elapsed();
            if elapsed >= limit {
                Some(Duration::ZERO)
            } else {
                Some(limit - elapsed)
            }
        } else {
            None
        }
    }

    /// Check if auto-stop conditions (timer or silence) have been met
    pub fn poll_auto_stop(&mut self) {
        if self.state() != RecorderState::Recording {
            return;
        }
        let elapsed = self.elapsed_duration();
        let timer_elapsed = self.timer_limit.is_some_and(|limit| elapsed >= limit);
        let end_of_tape = self.silence_autostop_enabled && elapsed > Duration::from_secs(30)
            && *self.silence_seconds_detected.read() >= 15.0;
        if self.interrupted.load(Ordering::Acquire) || timer_elapsed || end_of_tape {
            self.begin_stop();
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state(), RecorderState::Recording | RecorderState::Finalizing)
    }

    /// Starts live monitoring without recording to disk (for level calibration / Einpegeln)
    pub fn start_monitoring(&mut self, device_name: &str, channel_pair: ChannelPair) -> Result<(), String> {
        if self.is_busy() {
            return Err("A recording is active".into());
        }
        self.stop_monitoring();

        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| format!("Gerät '{}' nicht gefunden", device_name))?;

        let default_config = device.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = default_config.sample_rate().0;
        self.sample_rate = sample_rate;
        let total_channels = default_config.channels() as usize;

        let meter_arc = self.meter.clone();
        let ch_l = channel_pair.left_channel;
        let ch_r = channel_pair.right_channel;

        let err_fn = |err| eprintln!("Audio-Stream Fehler: {}", err);

        let stream = match default_config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &default_config.into(),
                move |data: &[f32], _| {
                    process_meter_f32(data, total_channels, ch_l, ch_r, &meter_arc);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &default_config.into(),
                move |data: &[i16], _| {
                    let f32_data: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                    process_meter_f32(&f32_data, total_channels, ch_l, ch_r, &meter_arc);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &default_config.into(),
                move |data: &[u16], _| {
                    let f32_data: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                    process_meter_f32(&f32_data, total_channels, ch_l, ch_r, &meter_arc);
                },
                err_fn,
                None,
            ),
            _ => return Err("Nicht unterstütztes Sample-Format".into()),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self.monitor_stream = Some(stream);
        *self.state.write() = RecorderState::Monitoring;
        Ok(())
    }

    pub fn stop_monitoring(&mut self) {
        if let Some(stream) = self.monitor_stream.take() {
            let _ = stream.pause();
        }
        if *self.state.read() == RecorderState::Monitoring {
            *self.state.write() = RecorderState::Idle;
        }
    }

    /// Starts high quality 24-bit recording to a file
    pub fn start_recording(
        &mut self,
        device_name: &str,
        channel_pair: ChannelPair,
        target_path: PathBuf,
        timer_duration: Option<Duration>,
        enable_silence_autostop: bool,
    ) -> Result<(), String> {
        if self.is_busy() {
            return Err("A recording is active".into());
        }
        self.stop_monitoring();

        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| format!("Gerät '{}' nicht gefunden", device_name))?;

        let default_config = device.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = default_config.sample_rate().0;
        self.sample_rate = sample_rate;
        let total_channels = default_config.channels() as usize;

        // Reset stats
        self.recorded_samples.store(0, Ordering::SeqCst);
        self.total_clipped_samples.store(0, Ordering::SeqCst);
        *self.max_peak_left.write() = 0.0;
        *self.max_peak_right.write() = 0.0;
        self.waveform_data.write().clear();
        *self.silence_seconds_detected.write() = 0.0;
        self.timer_limit = timer_duration;
        self.silence_autostop_enabled = enable_silence_autostop;
        self.start_time = Some(Instant::now());
        self.current_filepath = Some(target_path.clone());

        if channel_pair.left_channel >= total_channels || channel_pair.right_channel >= total_channels {
            return Err("Invalid input channel pair".into());
        }
        if !matches!(default_config.sample_format(), cpal::SampleFormat::F32 | cpal::SampleFormat::I16) {
            return Err("Unsupported input format".into());
        }

        let file = std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&target_path).map_err(|e| format!("{}: {e}", target_path.display()))?;
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)
            .map_err(|e| e.to_string())?;
        let (tx, rx): (Sender<Vec<(f32, f32)>>, Receiver<Vec<(f32, f32)>>) = bounded(256);
        self.sample_sender = Some(tx.clone());
        self.interrupted.store(false, Ordering::Release);
        self.recording_error = None;

        let writer_interrupted = self.interrupted.clone();
        let recorded_samples = self.recorded_samples.clone();
        self.writer_handle = Some(std::thread::spawn(move || {
            let write_result = (|| {
                // Leave room for the WAV header in the RIFF size field.
                const MAX_FRAMES: usize = (u32::MAX as usize - 128) / 6;
                let mut frames = 0;
                let mut checkpoint = Instant::now();
                while let Ok(chunk) = rx.recv() {
                    if frames + chunk.len() > MAX_FRAMES {
                        return Err("WAV size limit reached".to_string());
                    }
                    for (left, right) in chunk {
                        writer.write_sample(to_pcm24(left)).map_err(|e| e.to_string())?;
                        writer.write_sample(to_pcm24(right)).map_err(|e| e.to_string())?;
                        frames += 1;
                    }
                    recorded_samples.store(frames, Ordering::Relaxed);
                    if checkpoint.elapsed() >= Duration::from_secs(5) {
                        writer.flush().map_err(|e| e.to_string())?;
                        checkpoint = Instant::now();
                    }
                }
                Ok::<(), String>(())
            })();
            if write_result.is_err() {
                writer_interrupted.store(true, Ordering::Release);
            }
            let finalize_result = writer.finalize().map_err(|e| e.to_string());
            write_result.and(finalize_result)
        }));

        let meter_arc = self.meter.clone();
        let waveform_arc = self.waveform_data.clone();
        let clipped_arc = self.total_clipped_samples.clone();
        let max_l_arc = self.max_peak_left.clone();
        let max_r_arc = self.max_peak_right.clone();
        let silence_arc = self.silence_seconds_detected.clone();

        let ch_l = channel_pair.left_channel;
        let ch_r = channel_pair.right_channel;

        let callback_interrupted = self.interrupted.clone();
        let stream_interrupted = self.interrupted.clone();
        let err_fn = move |err| {
            stream_interrupted.store(true, Ordering::Release);
            eprintln!("Audio stream: {err}");
        };

        let stream = match default_config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &default_config.into(),
                move |data: &[f32], _| {
                    process_record_f32(
                        data,
                        total_channels,
                        ch_l,
                        ch_r,
                        sample_rate,
                        &tx,
                        &callback_interrupted,
                        &meter_arc,
                        &waveform_arc,
                        &clipped_arc,
                        &max_l_arc,
                        &max_r_arc,
                        &silence_arc,
                    );
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &default_config.into(),
                move |data: &[i16], _| {
                    let f32_data: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                    process_record_f32(
                        &f32_data,
                        total_channels,
                        ch_l,
                        ch_r,
                        sample_rate,
                        &tx,
                        &callback_interrupted,
                        &meter_arc,
                        &waveform_arc,
                        &clipped_arc,
                        &max_l_arc,
                        &max_r_arc,
                        &silence_arc,
                    );
                },
                err_fn,
                None,
            ),
            _ => unreachable!("input format validated above"),
        }
        .map_err(|e| e.to_string())
        .and_then(|stream| {
            stream.play().map_err(|e| e.to_string())?;
            Ok(stream)
        });
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                self.stop_recording();
                if self.recorded_samples() == 0 {
                    let _ = std::fs::remove_file(&target_path);
                }
                return Err(e);
            }
        };
        self.recording_stream = Some(stream);
        *self.state.write() = RecorderState::Recording;

        Ok(())
    }

    pub fn begin_stop(&mut self) {
        if let Some(stream) = self.recording_stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
        self.sample_sender.take();
        if self.state() == RecorderState::Recording {
            *self.state.write() = RecorderState::Finalizing;
        }
    }

    pub fn poll_finished(&mut self) -> bool {
        if self.state() != RecorderState::Finalizing
            || self.writer_handle.as_ref().is_some_and(|writer| !writer.is_finished())
        {
            return false;
        }
        self.finish_writer();
        true
    }

    fn stop_recording(&mut self) {
        self.begin_stop();
        self.finish_writer();
    }

    fn finish_writer(&mut self) {
        if let Some(handle) = self.writer_handle.take() {
            self.recording_error = match handle.join() {
                Ok(Ok(())) if self.interrupted.load(Ordering::Acquire) => {
                    Some("Audio stream interrupted or recording queue overrun".into())
                }
                Ok(result) => result.err(),
                Err(_) => Some("Recording writer stopped unexpectedly".into()),
            };
        }
        if self.state() == RecorderState::Finalizing {
            *self.state.write() = RecorderState::Finished;
        }
    }

    pub fn take_recording_error(&mut self) -> Option<String> {
        self.recording_error.take()
    }

}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.stop_recording();
    }
}

// Processing for Live Metering
fn process_meter_f32(
    data: &[f32],
    total_channels: usize,
    ch_l: usize,
    ch_r: usize,
    meter_arc: &Arc<RwLock<MeterLevels>>,
) {
    let mut peak_l: f32 = 0.0;
    let mut peak_r: f32 = 0.0;
    let mut sum_sq_l: f32 = 0.0;
    let mut sum_sq_r: f32 = 0.0;
    let mut count: usize = 0;

    for frame in data.chunks_exact(total_channels) {
        let l = frame[ch_l.min(total_channels - 1)].abs();
        let r = frame[ch_r.min(total_channels - 1)].abs();

        if l > peak_l { peak_l = l; }
        if r > peak_r { peak_r = r; }

        sum_sq_l += l * l;
        sum_sq_r += r * r;
        count += 1;
    }

    if count == 0 { return; }

    let rms_l = (sum_sq_l / count as f32).sqrt();
    let rms_r = (sum_sq_r / count as f32).sqrt();

    let mut m = meter_arc.write();
    let new_peak_l_db = linear_to_db(peak_l);
    let new_peak_r_db = linear_to_db(peak_r);
    let new_rms_l_db = linear_to_db(rms_l);
    let new_rms_r_db = linear_to_db(rms_r);

    m.peak_l_db = new_peak_l_db;
    m.peak_r_db = new_peak_r_db;
    m.rms_l_db = new_rms_l_db;
    m.rms_r_db = new_rms_r_db;

    // Peak hold decay
    m.peak_hold_l_db = (m.peak_hold_l_db - 0.5).max(new_peak_l_db);
    m.peak_hold_r_db = (m.peak_hold_r_db - 0.5).max(new_peak_r_db);

    if peak_l >= 0.999 || peak_r >= 0.999 {
        m.is_clipping = true;
        m.clip_count += 1;
    }
}

// Processing for Recording to Disk & Waveform Cache
fn to_pcm24(sample: f32) -> i32 {
    (sample * 8_388_608.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32
}

fn process_record_f32(
    data: &[f32],
    total_channels: usize,
    ch_l: usize,
    ch_r: usize,
    sample_rate: u32,
    tx: &Sender<Vec<(f32, f32)>>,
    interrupted: &AtomicBool,
    meter_arc: &Arc<RwLock<MeterLevels>>,
    waveform_arc: &Arc<RwLock<Vec<StereoWaveformPoint>>>,
    clipped_arc: &Arc<AtomicUsize>,
    max_l_arc: &Arc<RwLock<f32>>,
    max_r_arc: &Arc<RwLock<f32>>,
    silence_arc: &Arc<RwLock<f32>>,
) {
    if interrupted.load(Ordering::Acquire) {
        return;
    }
    let mut chunk = Vec::with_capacity(data.len() / total_channels);
    let mut peak_l: f32 = 0.0;
    let mut peak_r: f32 = 0.0;
    let mut sum_sq_l: f32 = 0.0;
    let mut sum_sq_r: f32 = 0.0;
    let mut count = 0;
    let mut local_clips = 0;

    for frame in data.chunks_exact(total_channels) {
        let l_sample = frame[ch_l.min(total_channels - 1)];
        let r_sample = frame[ch_r.min(total_channels - 1)];

        chunk.push((l_sample, r_sample));

        let abs_l = l_sample.abs();
        let abs_r = r_sample.abs();

        if abs_l > peak_l { peak_l = abs_l; }
        if abs_r > peak_r { peak_r = abs_r; }

        if abs_l >= 0.999 || abs_r >= 0.999 {
            local_clips += 1;
        }

        sum_sq_l += abs_l * abs_l;
        sum_sq_r += abs_r * abs_r;
        count += 1;
    }

    if count > 0 {
        if tx.try_send(chunk).is_err() {
            interrupted.store(true, Ordering::Release);
            return;
        }

        if local_clips > 0 {
            clipped_arc.fetch_add(local_clips, Ordering::Relaxed);
        }

        // Update overall max peaks
        {
            let mut ml = max_l_arc.write();
            if peak_l > *ml { *ml = peak_l; }
        }
        {
            let mut mr = max_r_arc.write();
            if peak_r > *mr { *mr = peak_r; }
        }

        // Live meter update
        let rms_l = (sum_sq_l / count as f32).sqrt();
        let rms_r = (sum_sq_r / count as f32).sqrt();

        let mut m = meter_arc.write();
        let new_peak_l_db = linear_to_db(peak_l);
        let new_peak_r_db = linear_to_db(peak_r);
        let new_rms_l_db = linear_to_db(rms_l);
        let new_rms_r_db = linear_to_db(rms_r);

        m.peak_l_db = new_peak_l_db;
        m.peak_r_db = new_peak_r_db;
        m.rms_l_db = new_rms_l_db;
        m.rms_r_db = new_rms_r_db;

        m.peak_hold_l_db = (m.peak_hold_l_db - 0.5).max(new_peak_l_db);
        m.peak_hold_r_db = (m.peak_hold_r_db - 0.5).max(new_peak_r_db);

        if local_clips > 0 {
            m.is_clipping = true;
            m.clip_count += local_clips;
        }

        // Silence tracking: if max RMS < -55 dBFS
        let max_rms_db = new_rms_l_db.max(new_rms_r_db);
        let chunk_duration = count as f32 / sample_rate as f32;
        let mut sil = silence_arc.write();
        if max_rms_db < -55.0 {
            *sil += chunk_duration;
        } else {
            *sil = 0.0;
        }

        // Downsampled stereo waveform point (1 point per chunk)
        // Preview must never make the capture callback wait for rendering.
        if let Some(mut waveform) = waveform_arc.try_write() {
            waveform.push(StereoWaveformPoint {
                min_l: -peak_l,
                max_l: peak_l,
                min_r: -peak_r,
                max_r: peak_r,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::device::list_input_devices;
    use crate::analysis::analyze_wav_file;

    #[test]
    #[ignore = "requires a connected input device and microphone permission"]
    fn test_record_and_finalize_lifecycle() {
        let devices = list_input_devices();
        let dev = devices.iter().find(|device| device.name.contains("L-12"))
            .or_else(|| devices.first()).expect("no audio input device");
        let pair = ChannelPair {
            label: "Test".into(),
            left_channel: if dev.channels >= 12 { 10 } else { 0 },
            right_channel: if dev.channels >= 12 { 11 } else { 0 },
        };

        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temp_file = std::env::temp_dir().join(format!("tapedigizer-lifecycle-{stamp}.wav"));
        let mut recorder = AudioRecorder::new();

        recorder.start_recording(&dev.name, pair, temp_file.clone(), None, false)
            .expect("failed to start hardware recording");

        std::thread::sleep(Duration::from_millis(300));
        recorder.stop_recording();
        assert_eq!(recorder.take_recording_error(), None);

        assert!(temp_file.exists());
        let analysis = analyze_wav_file(&temp_file);
        assert!(analysis.is_ok(), "Analysis failed: {:?}", analysis.err());

        let _ = std::fs::remove_file(temp_file);
    }
}
