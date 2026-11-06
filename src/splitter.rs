use std::path::{Path, PathBuf};
use crate::audio::meter::linear_to_db;

#[derive(Clone, Debug, PartialEq)]
pub enum NormalizationMode {
    None,
    PeakPerTrack, // Each track normalized to -0.3 dBFS
    PeakAlbum,    // All tracks scaled by the same gain factor to keep dynamics intact
}

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub index: usize, // Sequential 1-based index among active (non-pause) tracks
    pub segment_index: usize, // Raw 0-based index of this cut segment
    pub title: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub is_pause: bool,
}

impl TrackInfo {
    pub fn duration_seconds(&self) -> f64 {
        (self.end_seconds - self.start_seconds).max(0.0)
    }

    pub fn duration_formatted(&self) -> String {
        let total_secs = self.duration_seconds().round() as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkerSnapshot {
    pub markers: Vec<f64>,
    pub custom_titles: Vec<String>,
    pub pause_indices: Vec<usize>,
}

pub struct TrackSplitter {
    pub markers: Vec<f64>, // Split points in seconds
    pub custom_titles: Vec<String>,
    pub pause_indices: Vec<usize>, // Segments marked as excluded pauses (wont be exported)
    pub undo_stack: Vec<MarkerSnapshot>,
    pub redo_stack: Vec<MarkerSnapshot>,
}

impl TrackSplitter {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
            custom_titles: Vec::new(),
            pause_indices: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> MarkerSnapshot {
        MarkerSnapshot {
            markers: self.markers.clone(),
            custom_titles: self.custom_titles.clone(),
            pause_indices: self.pause_indices.clone(),
        }
    }

    pub fn push_undo(&mut self) {
        let snap = self.snapshot();
        if self.undo_stack.last() != Some(&snap) {
            self.undo_stack.push(snap);
            if self.undo_stack.len() > 60 {
                self.undo_stack.remove(0);
            }
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            let current = self.snapshot();
            self.redo_stack.push(current);
            self.markers = prev.markers;
            self.custom_titles = prev.custom_titles;
            self.pause_indices = prev.pause_indices;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            let current = self.snapshot();
            self.undo_stack.push(current);
            self.markers = next.markers;
            self.custom_titles = next.custom_titles;
            self.pause_indices = next.pause_indices;
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn add_marker(&mut self, seconds: f64, total_duration: f64) {
        if seconds <= 0.2 || seconds >= total_duration - 0.2 {
            return;
        }
        // Avoid adding duplicate markers very close to existing ones (< 0.5s)
        if !self.markers.iter().any(|&m| (m - seconds).abs() < 0.5) {
            self.push_undo();
            self.markers.push(seconds);
            self.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
    }

    pub fn remove_marker_at_index(&mut self, index: usize) {
        if index < self.markers.len() {
            self.push_undo();
            self.markers.remove(index);
            // Re-adjust pause indices
            self.pause_indices.retain(|&p| p != index);
            for p in &mut self.pause_indices {
                if *p > index {
                    *p -= 1;
                }
            }
        }
    }

    pub fn clear_markers(&mut self) {
        if !self.markers.is_empty() || !self.pause_indices.is_empty() {
            self.push_undo();
        }
        self.markers.clear();
        self.custom_titles.clear();
        self.pause_indices.clear();
    }

    pub fn toggle_pause(&mut self, segment_index: usize) {
        self.push_undo();
        if let Some(pos) = self.pause_indices.iter().position(|&x| x == segment_index) {
            self.pause_indices.remove(pos);
        } else {
            self.pause_indices.push(segment_index);
            self.pause_indices.sort_unstable();
        }
    }

    #[allow(dead_code)]
    pub fn is_pause(&self, segment_index: usize) -> bool {
        self.pause_indices.contains(&segment_index)
    }

    pub fn nudge_marker(&mut self, marker_idx: usize, delta_sec: f64, total_duration: f64) -> bool {
        if marker_idx >= self.markers.len() {
            return false;
        }
        self.push_undo();
        let cur = self.markers[marker_idx];
        let min_bound = if marker_idx == 0 { 0.1 } else { self.markers[marker_idx - 1] + 0.05 };
        let max_bound = if marker_idx + 1 < self.markers.len() {
            self.markers[marker_idx + 1] - 0.05
        } else {
            (total_duration - 0.1).max(0.1)
        };
        let new_val = (cur + delta_sec).clamp(min_bound, max_bound);
        self.markers[marker_idx] = new_val;
        true
    }

    pub fn get_tracks(&self, total_duration: f64) -> Vec<TrackInfo> {
        let mut tracks = Vec::new();
        if total_duration <= 0.1 {
            return tracks;
        }

        let mut points = vec![0.0];
        for &m in &self.markers {
            if m > 0.0 && m < total_duration {
                points.push(m);
            }
        }
        points.push(total_duration);

        let mut active_count = 0;
        for i in 0..points.len() - 1 {
            let start = points[i];
            let end = points[i + 1];
            let is_p = self.pause_indices.contains(&i);

            let export_index = if is_p {
                0
            } else {
                active_count += 1;
                active_count
            };

            let default_name = if is_p {
                format!("Pause {:02}", i + 1)
            } else {
                format!("Track {:02}", export_index)
            };
            let title = self.custom_titles.get(i).cloned().unwrap_or(default_name);

            tracks.push(TrackInfo {
                index: export_index,
                segment_index: i,
                title,
                start_seconds: start,
                end_seconds: end,
                is_pause: is_p,
            });
        }

        tracks
    }

    /// Automatically scans samples to find silent pauses and places markers
    pub fn detect_silence_pauses(
        &mut self,
        samples_l: &[f32],
        samples_r: &[f32],
        sample_rate: u32,
        threshold_db: f32,
        min_silence_seconds: f64,
    ) {
        if samples_l.is_empty() || sample_rate == 0 {
            return;
        }

        self.clear_markers();

        let window_size = (sample_rate as f64 * 0.1) as usize; // 100ms window
        let min_windows = (min_silence_seconds / 0.1).ceil() as usize;

        let total_samples = samples_l.len();
        let total_duration = total_samples as f64 / sample_rate as f64;

        let mut in_silence = false;
        let mut silence_start_win = 0;
        let mut current_silence_len = 0;

        let total_windows = total_samples / window_size;

        for w in 0..total_windows {
            let start_idx = w * window_size;
            let end_idx = (start_idx + window_size).min(total_samples);

            let mut sum_sq = 0.0_f64;
            for i in start_idx..end_idx {
                let sl = samples_l[i] as f64;
                let sr = samples_r[i] as f64;
                sum_sq += sl * sl + sr * sr;
            }

            let rms = ((sum_sq / (2.0 * (end_idx - start_idx) as f64)).sqrt()) as f32;
            let rms_db = linear_to_db(rms);

            if rms_db < threshold_db {
                if !in_silence {
                    in_silence = true;
                    silence_start_win = w;
                    current_silence_len = 1;
                } else {
                    current_silence_len += 1;
                }
            } else {
                if in_silence {
                    if current_silence_len >= min_windows {
                        let center_win = silence_start_win + current_silence_len / 2;
                        let center_sec = (center_win * window_size) as f64 / sample_rate as f64;
                        self.add_marker(center_sec, total_duration);
                    }
                    in_silence = false;
                    current_silence_len = 0;
                }
            }
        }
    }

    /// Automatically scans samples to find silent pauses and marks them as excluded gaps (pause segments)
    pub fn detect_silence_pauses_as_gaps(
        &mut self,
        samples_l: &[f32],
        samples_r: &[f32],
        sample_rate: u32,
        threshold_db: f32,
        min_silence_seconds: f64,
    ) {
        if samples_l.is_empty() || sample_rate == 0 {
            return;
        }

        self.clear_markers();

        let window_size = (sample_rate as f64 * 0.1) as usize; // 100ms window
        let min_windows = (min_silence_seconds / 0.1).ceil() as usize;

        let total_samples = samples_l.len();
        let total_duration = total_samples as f64 / sample_rate as f64;

        let mut in_silence = false;
        let mut silence_start_win = 0;
        let mut current_silence_len = 0;

        let total_windows = total_samples / window_size;
        let mut silence_intervals: Vec<(f64, f64)> = Vec::new();

        for w in 0..total_windows {
            let start_idx = w * window_size;
            let end_idx = (start_idx + window_size).min(total_samples);

            let mut sum_sq = 0.0_f64;
            for i in start_idx..end_idx {
                let sl = samples_l[i] as f64;
                let sr = samples_r[i] as f64;
                sum_sq += sl * sl + sr * sr;
            }

            let rms = ((sum_sq / (2.0 * (end_idx - start_idx) as f64)).sqrt()) as f32;
            let rms_db = linear_to_db(rms);

            if rms_db < threshold_db {
                if !in_silence {
                    in_silence = true;
                    silence_start_win = w;
                    current_silence_len = 1;
                } else {
                    current_silence_len += 1;
                }
            } else {
                if in_silence {
                    if current_silence_len >= min_windows {
                        let t_start = (silence_start_win * window_size) as f64 / sample_rate as f64;
                        let t_end = ((silence_start_win + current_silence_len) * window_size) as f64 / sample_rate as f64;
                        silence_intervals.push((t_start, t_end));
                    }
                    in_silence = false;
                    current_silence_len = 0;
                }
            }
        }

        if in_silence && current_silence_len >= min_windows {
            let t_start = (silence_start_win * window_size) as f64 / sample_rate as f64;
            let t_end = total_duration;
            silence_intervals.push((t_start, t_end));
        }

        // Place markers at starts and ends of silence intervals
        for (t_start, t_end) in &silence_intervals {
            if *t_start > 0.4 {
                self.add_marker(*t_start, total_duration);
            }
            if *t_end < total_duration - 0.4 {
                self.add_marker(*t_end, total_duration);
            }
        }

        // Now identify which segments correspond to silence and mark them in pause_indices
        let tracks = self.get_tracks(total_duration);
        for track in tracks {
            for (t_start, t_end) in &silence_intervals {
                if track.start_seconds >= t_start - 0.25 && track.end_seconds <= t_end + 0.25 {
                    if !self.pause_indices.contains(&track.segment_index) {
                        self.pause_indices.push(track.segment_index);
                    }
                }
            }
        }
        self.pause_indices.sort_unstable();
    }
}

/// Slices the input WAV file into individual tracks and exports them (excluding paused segments)
pub fn export_tracks<P: AsRef<Path>>(
    source_wav_path: P,
    output_dir: P,
    tracks: &[TrackInfo],
    norm_mode: NormalizationMode,
) -> Result<Vec<PathBuf>, String> {
    let exportable_tracks: Vec<&TrackInfo> = tracks.iter().filter(|t| !t.is_pause).collect();
    if exportable_tracks.is_empty() {
        return Err("Keine aktiven Tracks zum Exportieren vorhanden (alle als Pause markiert oder keine Tracks vorhanden).".to_string());
    }

    let src = source_wav_path.as_ref();
    if !src.exists() {
        return Err(format!("Quelldatei '{}' wurde nicht gefunden. Bitte nimm zuerst Audio in Tab 1 auf.", src.display()));
    }

    let out_dir = output_dir.as_ref();
    std::fs::create_dir_all(out_dir).map_err(|e| {
        format!("Zielordner '{}' konnte nicht erstellt werden: {}", out_dir.display(), e)
    })?;

    // Probe writeability
    let probe_file = out_dir.join(".tapedigizer_probe");
    std::fs::write(&probe_file, b"test").map_err(|e| {
        format!("Zielordner '{}' ist schreibgeschützt oder nicht beschreibbar: {}", out_dir.display(), e)
    })?;
    let _ = std::fs::remove_file(&probe_file);

    let mut reader = hound::WavReader::open(src).map_err(|e| format!("Fehler beim Öffnen der Audiodatei: {}", e))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;

    let max_val = match spec.bits_per_sample {
        16 => 32768.0_f32,
        24 => 8388608.0_f32,
        32 => 2147483648.0_f32,
        _ => 32768.0_f32,
    };

    let mut samples_l: Vec<f32> = Vec::new();
    let mut samples_r: Vec<f32> = Vec::new();

    if spec.channels == 2 {
        let mut iter = reader.samples::<i32>();
        while let (Some(Ok(sl)), Some(Ok(sr))) = (iter.next(), iter.next()) {
            samples_l.push(sl as f32 / max_val);
            samples_r.push(sr as f32 / max_val);
        }
    } else {
        for s in reader.samples::<i32>() {
            if let Ok(sample) = s {
                let v = sample as f32 / max_val;
                samples_l.push(v);
                samples_r.push(v);
            }
        }
    }

    let target_peak_lin = 10.0_f32.powf(-0.3 / 20.0); // -0.3 dBFS safe target

    // Calculate album-wide scale if Album Normalization is selected
    let album_scale = if norm_mode == NormalizationMode::PeakAlbum {
        let mut max_p: f32 = 0.0001;
        for &s in samples_l.iter().chain(samples_r.iter()) {
            let abs = s.abs();
            if abs > max_p { max_p = abs; }
        }
        (target_peak_lin / max_p).min(10.0)
    } else {
        1.0
    };

    let out_dir = output_dir.as_ref();
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let mut exported_files = Vec::new();

    for track in exportable_tracks {
        let start_sample = (track.start_seconds * sample_rate as f64).round() as usize;
        let end_sample = ((track.end_seconds * sample_rate as f64).round() as usize).min(samples_l.len());

        if start_sample >= end_sample {
            continue;
        }

        // Clean & sanitize track filename
        let safe_title = crate::project::sanitize_track_title(&track.title, track.index);
        let filename = format!("{:02} - {}.wav", track.index, safe_title);
        let out_path = out_dir.join(filename);

        let track_l = &samples_l[start_sample..end_sample];
        let track_r = &samples_r[start_sample..end_sample];

        // Determine gain scale for this track
        let scale = match norm_mode {
            NormalizationMode::None => 1.0,
            NormalizationMode::PeakAlbum => album_scale,
            NormalizationMode::PeakPerTrack => {
                let mut max_p: f32 = 0.0001;
                for i in 0..track_l.len() {
                    let al = track_l[i].abs();
                    let ar = track_r[i].abs();
                    if al > max_p { max_p = al; }
                    if ar > max_p { max_p = ar; }
                }
                (target_peak_lin / max_p).min(10.0)
            }
        };

        // Write 24-bit WAV file
        let out_spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&out_path, out_spec).map_err(|e| e.to_string())?;
        let max_val_24 = 8_388_607.0_f32;

        for i in 0..track_l.len() {
            let sl = (track_l[i] * scale).clamp(-1.0, 1.0);
            let sr = (track_r[i] * scale).clamp(-1.0, 1.0);

            let samp_l = (sl * max_val_24) as i32;
            let samp_r = (sr * max_val_24) as i32;

            writer.write_sample(samp_l).map_err(|e| e.to_string())?;
            writer.write_sample(samp_r).map_err(|e| e.to_string())?;
        }

        writer.finalize().map_err(|e| e.to_string())?;
        exported_files.push(out_path);
    }

    Ok(exported_files)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientPoint {
    pub time_sec: f64,
    pub strength: f32, // 0.0 .. 1.0
}

/// Detects transient onsets (sharp attacks, drum hits, vocal starts) in a specific time range of audio.
pub fn detect_transients_in_range(
    samples_l: &[f32],
    samples_r: &[f32],
    sample_rate: u32,
    start_sec: f64,
    end_sec: f64,
    sensitivity: f32,
) -> Vec<TransientPoint> {
    if sample_rate == 0 || samples_l.is_empty() || end_sec <= start_sec {
        return Vec::new();
    }

    let sr = sample_rate as f64;
    let s_idx = ((start_sec * sr).max(0.0) as usize).min(samples_l.len());
    let e_idx = ((end_sec * sr).max(0.0) as usize).min(samples_l.len());
    if e_idx <= s_idx {
        return Vec::new();
    }

    // Analysis frame: ~5ms hop (240 samples @ 48kHz), ~10ms window (480 samples @ 48kHz)
    let hop = (sample_rate as usize / 200).max(64);
    let win = hop * 2;
    let slice_l = &samples_l[s_idx..e_idx];
    let slice_r = &samples_r[s_idx..e_idx];
    let n_samples = slice_l.len();
    if n_samples < win {
        return Vec::new();
    }

    let n_frames = (n_samples - win) / hop + 1;
    let mut energies = Vec::with_capacity(n_frames);

    for f in 0..n_frames {
        let offset = f * hop;
        let mut sum_sq = 0.0_f32;
        let l_sub = &slice_l[offset..offset + win];
        let r_sub = &slice_r[offset..offset + win];
        for (&sl, &sr) in l_sub.iter().zip(r_sub.iter()) {
            sum_sq += sl * sl + sr * sr;
        }
        let rms = (sum_sq / (win as f32 * 2.0)).sqrt();
        energies.push(rms);
    }

    // Onset strength: half-wave rectified energy difference
    let mut onsets = Vec::with_capacity(energies.len());
    onsets.push(0.0_f32);
    for i in 1..energies.len() {
        let diff = energies[i] - energies[i - 1];
        onsets.push(diff.max(0.0));
    }

    // Moving average of onsets with adaptive thresholding
    let sens = sensitivity.clamp(0.2, 5.0);
    let thresh_multiplier = 1.5 / sens;
    let min_energy_floor = 0.003 / sens;
    let refractory_hops = ((sample_rate as usize * 30) / (1000 * hop)).max(2); // ~30ms refractory period

    let mut transients = Vec::new();
    let mut last_peak_hop = 0_usize;

    for i in 2..onsets.len().saturating_sub(2) {
        if energies[i] < min_energy_floor {
            continue;
        }
        let o_curr = onsets[i];
        if o_curr <= 0.001 {
            continue;
        }

        // Local moving average
        let start_k = i.saturating_sub(4);
        let end_k = (i + 4).min(onsets.len() - 1);
        let avg: f32 = onsets[start_k..=end_k].iter().sum::<f32>() / (end_k - start_k + 1) as f32;
        let thresh = avg * thresh_multiplier + 0.003;

        if o_curr > thresh && o_curr >= onsets[i - 1] && o_curr >= onsets[i + 1] {
            if i >= last_peak_hop + refractory_hops || last_peak_hop == 0 {
                let time_sec = start_sec + ((i * hop) as f64 / sr);
                let strength = (o_curr / (o_curr + 0.04)).clamp(0.1, 1.0);
                transients.push(TransientPoint { time_sec, strength });
                last_peak_hop = i;
            }
        }
    }

    transients
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_splitting_markers() {
        let mut splitter = TrackSplitter::new();
        let total_duration = 300.0; // 5 minutes

        splitter.add_marker(60.0, total_duration);
        splitter.add_marker(150.0, total_duration);
        splitter.add_marker(210.0, total_duration);

        let tracks = splitter.get_tracks(total_duration);
        assert_eq!(tracks.len(), 4);

        assert_eq!(tracks[0].start_seconds, 0.0);
        assert_eq!(tracks[0].end_seconds, 60.0);
        assert_eq!(tracks[0].duration_formatted(), "01:00");

        assert_eq!(tracks[1].start_seconds, 60.0);
        assert_eq!(tracks[1].end_seconds, 150.0);
        assert_eq!(tracks[1].duration_formatted(), "01:30");

        assert_eq!(tracks[2].start_seconds, 150.0);
        assert_eq!(tracks[2].end_seconds, 210.0);
        assert_eq!(tracks[2].duration_formatted(), "01:00");

        assert_eq!(tracks[3].start_seconds, 210.0);
        assert_eq!(tracks[3].end_seconds, 300.0);
        assert_eq!(tracks[3].duration_formatted(), "01:30");
    }

    #[test]
    fn test_silence_detection_pauses() {
        let mut splitter = TrackSplitter::new();
        let sample_rate = 1000; // 1000 Hz for fast test
        let mut samples = Vec::new();

        // 2 seconds of loud sound (0.5 amplitude)
        samples.extend(vec![0.5_f32; 2000]);
        // 2 seconds of silence (0.0001 amplitude -> -80 dB)
        samples.extend(vec![0.0001_f32; 2000]);
        // 2 seconds of loud sound (0.5 amplitude)
        samples.extend(vec![0.5_f32; 2000]);

        splitter.detect_silence_pauses(&samples, &samples, sample_rate, -40.0, 1.5);

        assert_eq!(splitter.markers.len(), 1);
        let marker = splitter.markers[0];
        // The silence was between 2.0s and 4.0s, center around 3.0s
        assert!((marker - 3.0).abs() < 0.2);
    }

    #[test]
    fn test_undo_redo_markers() {
        let mut splitter = TrackSplitter::new();
        assert!(!splitter.can_undo());
        assert!(!splitter.can_redo());

        splitter.add_marker(60.0, 300.0);
        assert_eq!(splitter.markers, vec![60.0]);
        assert!(splitter.can_undo());

        splitter.add_marker(120.0, 300.0);
        assert_eq!(splitter.markers, vec![60.0, 120.0]);

        // Remove marker 0 (at 60.0)
        splitter.remove_marker_at_index(0);
        assert_eq!(splitter.markers, vec![120.0]);

        // Undo removal -> restores [60.0, 120.0]
        assert!(splitter.undo());
        assert_eq!(splitter.markers, vec![60.0, 120.0]);
        assert!(splitter.can_redo());

        // Redo removal -> returns to [120.0]
        assert!(splitter.redo());
        assert_eq!(splitter.markers, vec![120.0]);
    }

    #[test]
    fn test_toggle_pause_and_nudge() {
        let mut splitter = TrackSplitter::new();
        let total_duration = 200.0;
        splitter.add_marker(50.0, total_duration);
        splitter.add_marker(100.0, total_duration);

        // Segment 1 (50.0 .. 100.0) marked as pause
        splitter.toggle_pause(1);
        assert!(splitter.is_pause(1));

        let tracks = splitter.get_tracks(total_duration);
        assert_eq!(tracks.len(), 3);
        assert!(!tracks[0].is_pause);
        assert_eq!(tracks[0].index, 1);
        assert!(tracks[1].is_pause);
        assert_eq!(tracks[1].index, 0); // pause track
        assert!(!tracks[2].is_pause);
        assert_eq!(tracks[2].index, 2); // next active track is #2!

        // Test nudge
        assert!(splitter.nudge_marker(0, 5.0, total_duration));
        assert!((splitter.markers[0] - 55.0).abs() < 0.001);
    }

    #[test]
    fn test_detect_transients() {
        let sample_rate = 48000;
        let num_samples = 48000 * 2; // 2 seconds
        let mut samples_l = vec![0.0_f32; num_samples];
        let mut samples_r = vec![0.0_f32; num_samples];

        // Synthesize a silent track with two sharp transient bursts at 0.5s and 1.2s
        let t1_idx = 48000 / 2; // 0.5s
        for i in 0..400 {
            samples_l[t1_idx + i] = 0.8 * (- (i as f32) / 100.0).exp();
            samples_r[t1_idx + i] = 0.8 * (- (i as f32) / 100.0).exp();
        }

        let t2_idx = (48000.0 * 1.2) as usize; // 1.2s
        for i in 0..400 {
            samples_l[t2_idx + i] = 0.9 * (- (i as f32) / 100.0).exp();
            samples_r[t2_idx + i] = 0.9 * (- (i as f32) / 100.0).exp();
        }

        let transients = detect_transients_in_range(&samples_l, &samples_r, sample_rate, 0.0, 2.0, 1.0);
        assert!(transients.len() >= 2);
        assert!((transients[0].time_sec - 0.5).abs() < 0.05);
        assert!((transients[1].time_sec - 1.2).abs() < 0.05);
    }
}
