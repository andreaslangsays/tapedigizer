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
    sample_sender: Option<Sender<Option<Vec<(f32, f32)>>>>,
    writer_handle: Option<std::thread::JoinHandle<()>>,
    stop_signal: Arc<AtomicBool>,
    
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
            stop_signal: Arc::new(AtomicBool::new(false)),
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
    pub fn poll_auto_stop(&mut self) -> bool {
        if *self.state.read() != RecorderState::Recording {
            return false;
        }

        // Timer limit check
        if let (Some(start), Some(limit)) = (self.start_time, self.timer_limit) {
            if start.elapsed() >= limit {
                self.stop_recording();
                return true;
            }
        }

        // Silence auto-stop check (> 15 seconds of silence)
        if self.silence_autostop_enabled {
            let silence_sec = *self.silence_seconds_detected.read();
            // Only trigger if we have been recording for at least 30 seconds
            if let Some(start) = self.start_time {
                if start.elapsed() > Duration::from_secs(30) && silence_sec >= 15.0 {
                    self.stop_recording();
                    return true;
                }
            }
        }

        false
    }

    /// Starts live monitoring without recording to disk (for level calibration / Einpegeln)
    pub fn start_monitoring(&mut self, device_name: &str, channel_pair: ChannelPair) -> Result<(), String> {
        self.stop_monitoring();
        self.stop_recording();

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
        self.stop_monitoring();
        self.stop_recording();

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

        // Channel for passing stereo samples (L, R) to disk writer thread
        let (tx, rx): (Sender<Option<Vec<(f32, f32)>>>, Receiver<Option<Vec<(f32, f32)>>>) = bounded(256);
        self.sample_sender = Some(tx.clone());
        let stop_signal = Arc::new(AtomicBool::new(false));
        self.stop_signal = stop_signal.clone();

        // Spawn background WAV file writer thread (24-bit PCM)
        let file_path_clone = target_path.clone();
        let recorded_samples_arc = self.recorded_samples.clone();

        let writer_handle = std::thread::spawn(move || {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };

            let Ok(mut writer) = hound::WavWriter::create(&file_path_clone, spec) else {
                eprintln!("Konnte WAV-Datei nicht erstellen: {:?}", file_path_clone);
                return;
            };

            let max_val_24 = 8_388_607.0_f32; // 2^23 - 1

            while let Ok(msg) = rx.recv() {
                match msg {
                    Some(chunk) => {
                        for (l, r) in chunk {
                            let sample_l = (l.clamp(-1.0, 1.0) * max_val_24) as i32;
                            let sample_r = (r.clamp(-1.0, 1.0) * max_val_24) as i32;

                            let _ = writer.write_sample(sample_l);
                            let _ = writer.write_sample(sample_r);
                            recorded_samples_arc.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    None => {
                        // Sentinel: recording ended cleanly, exit loop to finalize!
                        break;
                    }
                }
            }

            if let Err(e) = writer.finalize() {
                eprintln!("Fehler beim Finalisieren der WAV-Datei: {}", e);
            }
        });
        self.writer_handle = Some(writer_handle);

        let meter_arc = self.meter.clone();
        let waveform_arc = self.waveform_data.clone();
        let clipped_arc = self.total_clipped_samples.clone();
        let max_l_arc = self.max_peak_left.clone();
        let max_r_arc = self.max_peak_right.clone();
        let silence_arc = self.silence_seconds_detected.clone();

        let ch_l = channel_pair.left_channel;
        let ch_r = channel_pair.right_channel;

        let err_fn = |err| eprintln!("Audio-Stream Fehler: {}", err);

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
            _ => return Err("Nicht unterstütztes Aufnahme-Format".into()),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self.recording_stream = Some(stream);
        *self.state.write() = RecorderState::Recording;

        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let Some(stream) = self.recording_stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
        if let Some(sender) = self.sample_sender.take() {
            let _ = sender.send(None);
        }
        if let Some(handle) = self.writer_handle.take() {
            let _ = handle.join();
        }
        self.stop_signal.store(true, Ordering::SeqCst);
        if *self.state.read() == RecorderState::Recording {
            *self.state.write() = RecorderState::Finished;
        }
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
fn process_record_f32(
    data: &[f32],
    total_channels: usize,
    ch_l: usize,
    ch_r: usize,
    sample_rate: u32,
    tx: &Sender<Option<Vec<(f32, f32)>>>,
    meter_arc: &Arc<RwLock<MeterLevels>>,
    waveform_arc: &Arc<RwLock<Vec<StereoWaveformPoint>>>,
    clipped_arc: &Arc<AtomicUsize>,
    max_l_arc: &Arc<RwLock<f32>>,
    max_r_arc: &Arc<RwLock<f32>>,
    silence_arc: &Arc<RwLock<f32>>,
) {
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
        let _ = tx.send(Some(chunk));

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
        waveform_arc.write().push(StereoWaveformPoint {
            min_l: -peak_l,
            max_l: peak_l,
            min_r: -peak_r,
            max_r: peak_r,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::device::list_input_devices;
    use crate::analysis::analyze_wav_file;

    #[test]
    fn test_record_and_finalize_lifecycle() {
        let devices = list_input_devices();
        if devices.is_empty() {
            return;
        }
        let dev = &devices[0];
        let pair = ChannelPair {
            label: "Test".into(),
            left_channel: 0,
            right_channel: 0,
        };

        let temp_file = std::env::temp_dir().join("test_lifecycle.wav");
        let mut recorder = AudioRecorder::new();

        let res = recorder.start_recording(&dev.name, pair, temp_file.clone(), None, false);
        if res.is_err() {
            // Permission or device unavailable in headless test environment, gracefully pass
            return;
        }

        std::thread::sleep(Duration::from_millis(300));
        recorder.stop_recording();

        assert!(temp_file.exists());
        let analysis = analyze_wav_file(&temp_file);
        assert!(analysis.is_ok(), "Analysis failed: {:?}", analysis.err());

        let _ = std::fs::remove_file(temp_file);
    }
}
