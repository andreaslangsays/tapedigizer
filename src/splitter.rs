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
}
