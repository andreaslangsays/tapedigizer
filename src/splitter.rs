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
    pub fn export_filename(&self) -> String {
        let title = crate::project::sanitize_track_title(&self.title, self.index);
        format!("{:02} - {title}.wav", self.index)
    }

    pub fn export_filename_with_artist(&self, artist: Option<&str>) -> String {
        let title = crate::project::sanitize_track_title(&self.title, self.index);
        if let Some(artist) = artist.map(str::trim).filter(|a| !a.is_empty()) {
            let clean_artist = crate::project::sanitize_filename(artist);
            format!("{:02} - {clean_artist} - {title}.wav", self.index)
        } else {
            format!("{:02} - {title}.wav", self.index)
        }
    }

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
        self.record_undo(self.snapshot());
    }

    fn record_undo(&mut self, snap: MarkerSnapshot) {
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

    pub fn add_marker(&mut self, seconds: f64, total_duration: f64, sample_rate: u32) -> Option<usize> {
        if !seconds.is_finite() || sample_rate == 0 {
            return None;
        }
        let rate = f64::from(sample_rate);
        let frame = (seconds * rate).round();
        let time = frame / rate;
        let index = self.markers.partition_point(|&marker| marker < time);
        if frame < 1.0 || frame >= (total_duration * rate).round()
            || self.markers.get(index) == Some(&time)
        {
            return None;
        }
        self.push_undo();
        self.markers.insert(index, time);
        if index < self.custom_titles.len() {
            self.custom_titles.insert(index + 1, String::new());
        }
        let split_pause = self.pause_indices.contains(&index);
        for pause in &mut self.pause_indices {
            if *pause > index {
                *pause += 1;
            }
        }
        if split_pause {
            self.pause_indices.push(index + 1);
            self.pause_indices.sort_unstable();
        }
        Some(index)
    }

    pub fn remove_marker_at_index(&mut self, index: usize) {
        if index >= self.markers.len() {
            return;
        }
        self.push_undo();
        self.markers.remove(index);
        if index + 1 < self.custom_titles.len() {
            self.custom_titles.remove(index + 1);
        }
        let merged_pause = self.is_pause(index) && self.is_pause(index + 1);
        self.pause_indices.retain(|&p| p != index + 1 && (p != index || merged_pause));
        for pause in &mut self.pause_indices {
            if *pause > index + 1 {
                *pause -= 1;
            }
        }
    }

    /// Move a boundary without changing segment order. The caller owns the undo transaction.
    pub fn preview_marker_move(&mut self, index: usize, seconds: f64, duration: f64, rate: u32) -> bool {
        if index >= self.markers.len() || !seconds.is_finite() || rate == 0 {
            return false;
        }
        let rate = f64::from(rate);
        let first = index.checked_sub(1).map_or(0.0, |i| self.markers[i]);
        let last = self.markers.get(index + 1).copied().unwrap_or(duration);
        let min_frame = (first * rate).round() + 1.0;
        let max_frame = (last * rate).round() - 1.0;
        if min_frame > max_frame {
            return false;
        }
        let next = (seconds * rate).round().clamp(min_frame, max_frame) / rate;
        if next == self.markers[index] {
            return false;
        }
        self.markers[index] = next;
        true
    }

    pub fn move_marker(&mut self, index: usize, seconds: f64, duration: f64, rate: u32) -> bool {
        let before = self.snapshot();
        if !self.preview_marker_move(index, seconds, duration, rate) {
            return false;
        }
        self.commit_edit(before);
        true
    }

    pub fn commit_edit(&mut self, before: MarkerSnapshot) {
        if before != self.snapshot() {
            self.record_undo(before);
        }
    }

    pub fn restore(&mut self, snapshot: MarkerSnapshot) {
        self.markers = snapshot.markers;
        self.custom_titles = snapshot.custom_titles;
        self.pause_indices = snapshot.pause_indices;
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
            let title = self.custom_titles.get(i).filter(|title| !title.is_empty()).cloned().unwrap_or(default_name);

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
                        self.add_marker(center_sec, total_duration, sample_rate);
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
                self.add_marker(*t_start, total_duration, sample_rate);
            }
            if *t_end < total_duration - 0.4 {
                self.add_marker(*t_end, total_duration, sample_rate);
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

#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    pub norm_mode: NormalizationMode,
    pub highpass_20hz: bool,
    pub fade_ms: u32,
    pub stereo_balance_db: f32,
    pub artist: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            norm_mode: NormalizationMode::None,
            highpass_20hz: false,
            fade_ms: 0,
            stereo_balance_db: 0.0,
            artist: None,
        }
    }
}

/// Slices the input WAV file into individual tracks and exports them (excluding paused segments)
#[allow(dead_code)]
pub fn export_tracks(
    source_wav_path: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    tracks: &[TrackInfo],
    norm_mode: NormalizationMode,
    progress: impl Fn(usize, usize) -> bool,
) -> Result<Vec<PathBuf>, String> {
    export_tracks_with_options(
        source_wav_path,
        output_dir,
        tracks,
        &ExportOptions {
            norm_mode,
            ..ExportOptions::default()
        },
        progress,
    )
}

/// Slices the input WAV file into individual tracks with analog mastering options
pub fn export_tracks_with_options(
    source_wav_path: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    tracks: &[TrackInfo],
    options: &ExportOptions,
    progress: impl Fn(usize, usize) -> bool,
) -> Result<Vec<PathBuf>, String> {
    let out_dir = output_dir.as_ref();
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let source = std::fs::File::open(source_wav_path).map_err(|e| e.to_string())?;
    let mut reader = hound::WavReader::new(std::io::BufReader::new(source)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if !matches!(spec.channels, 1 | 2) || spec.sample_rate == 0
        || spec.sample_format != hound::SampleFormat::Int
        || !matches!(spec.bits_per_sample, 16 | 24 | 32)
    {
        return Err("Unsupported WAV format".into());
    }
    let channels = u32::from(spec.channels);
    let duration = reader.duration();
    let rate = f64::from(spec.sample_rate);
    let mut exports = Vec::new();
    let mut names = std::collections::HashSet::new();
    let artist_ref = options.artist.as_deref();
    for track in tracks.iter().filter(|track| !track.is_pause) {
        let start = (track.start_seconds * rate).round() as u32;
        let end = (track.end_seconds * rate).round() as u32;
        if !track.start_seconds.is_finite() || !track.end_seconds.is_finite()
            || track.start_seconds < 0.0 || start >= end || end > duration
        {
            return Err(format!("Invalid export range for track {}", track.index));
        }
        let path = out_dir.join(track.export_filename_with_artist(artist_ref));
        if path.exists() || !names.insert(path.clone()) {
            return Err(format!("Export destination already exists: {}", path.display()));
        }
        exports.push((start, end, path));
    }
    if exports.is_empty() {
        return Err("No tracks to export".into());
    }

    let peak_scale = |peak: i64| {
        let full_scale = (1_i64 << (spec.bits_per_sample - 1)) as f64;
        let target = 10.0_f64.powf(-0.3 / 20.0);
        (target / (peak as f64 / full_scale).max(0.0001)).min(10.0)
    };
    let track_count = exports.len();
    let report = |finished| {
        if progress(finished, track_count) { Ok(()) } else { Err("Export cancelled".to_string()) }
    };
    report(0)?;
    let album_gain = if options.norm_mode == NormalizationMode::PeakAlbum {
        peak_scale(read_peak(&mut reader, 0, duration * channels, || report(0))?)
    } else {
        1.0
    };
    let output_spec = hound::WavSpec {
        channels: 2,
        sample_rate: spec.sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let pcm_factor = 2.0_f64.powi(24 - i32::from(spec.bits_per_sample));

    // Stereo balance gains (negative = boost L / cut R, positive = boost R / cut L)
    let bal_db = options.stereo_balance_db.clamp(-12.0, 12.0);
    let (bal_gain_l, bal_gain_r) = if bal_db.abs() < 0.05 {
        (1.0_f64, 1.0_f64)
    } else if bal_db > 0.0 {
        // Right is louder by bal_db -> attenuate R or boost L slightly
        (10.0_f64.powf(f64::from(bal_db * 0.5) / 20.0), 10.0_f64.powf(f64::from(-bal_db * 0.5) / 20.0))
    } else {
        // Left is louder by -bal_db -> attenuate L or boost R
        (10.0_f64.powf(f64::from(bal_db * 0.5) / 20.0), 10.0_f64.powf(f64::from(-bal_db * 0.5) / 20.0))
    };

    let fade_frames = ((f64::from(options.fade_ms) / 1000.0) * rate).round() as u32;
    let hp_alpha = 1.0 / (1.0 + 2.0 * std::f64::consts::PI * 20.0 / rate);

    let mut staged = Vec::new();
    let mut published = Vec::new();
    let result = (|| {
        for (index, (start, end, destination)) in exports.into_iter().enumerate() {
            let count = end - start;
            let gain = if options.norm_mode == NormalizationMode::PeakPerTrack {
                peak_scale(read_peak(&mut reader, start, count * channels, || report(index))?)
            } else {
                album_gain
            };
            let (temporary, file) = crate::project::unique_file(out_dir, ".export", "partial")?;
            staged.push((temporary, destination));
            let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), output_spec)
                .map_err(|e| e.to_string())?;
            reader.seek(start).map_err(|e| e.to_string())?;
            let mut samples = reader.samples::<i32>();

            let mut hp_prev_in_l = 0.0_f64;
            let mut hp_prev_out_l = 0.0_f64;
            let mut hp_prev_in_r = 0.0_f64;
            let mut hp_prev_out_r = 0.0_f64;

            for frame in 0..count {
                if frame % 16_384 == 0 { report(index)?; }
                let raw_l = f64::from(next_pcm(&mut samples)?);
                let raw_r = if channels == 2 { f64::from(next_pcm(&mut samples)?) } else { raw_l };

                let (filt_l, filt_r) = if options.highpass_20hz {
                    let out_l = hp_alpha * (hp_prev_out_l + raw_l - hp_prev_in_l);
                    hp_prev_in_l = raw_l;
                    hp_prev_out_l = out_l;
                    let out_r = hp_alpha * (hp_prev_out_r + raw_r - hp_prev_in_r);
                    hp_prev_in_r = raw_r;
                    hp_prev_out_r = out_r;
                    (out_l, out_r)
                } else {
                    (raw_l, raw_r)
                };

                let fade_env = if fade_frames > 0 && count > fade_frames * 2 {
                    if frame < fade_frames {
                        f64::from(frame) / f64::from(fade_frames)
                    } else if frame >= count - fade_frames {
                        f64::from(count - 1 - frame) / f64::from(fade_frames)
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                let enc_l = (filt_l * pcm_factor * gain * bal_gain_l * fade_env)
                    .round()
                    .clamp(-8_388_608.0, 8_388_607.0) as i32;
                let enc_r = (filt_r * pcm_factor * gain * bal_gain_r * fade_env)
                    .round()
                    .clamp(-8_388_608.0, 8_388_607.0) as i32;

                writer.write_sample(enc_l).map_err(|e| e.to_string())?;
                writer.write_sample(enc_r).map_err(|e| e.to_string())?;
            }
            writer.finalize().map_err(|e| e.to_string())?;
            report(index + 1)?;
        }
        for (temporary, destination) in &staged {
            crate::project::publish_new_file(temporary, destination)
                .map_err(|e| format!("{}: {e}", destination.display()))?;
            published.push(destination.clone());
        }
        Ok::<(), String>(())
    })();
    for (temporary, _) in staged {
        let _ = std::fs::remove_file(temporary);
    }
    if let Err(error) = result {
        for destination in published {
            let _ = std::fs::remove_file(destination);
        }
        return Err(error);
    }
    Ok(published)
}

fn next_pcm(samples: &mut impl Iterator<Item = Result<i32, hound::Error>>) -> Result<i32, String> {
    samples.next().ok_or("Incomplete WAV data")?.map_err(|e| e.to_string())
}

fn read_peak(
    reader: &mut hound::WavReader<impl std::io::Read + std::io::Seek>,
    start: u32,
    sample_count: u32,
    check: impl Fn() -> Result<(), String>,
) -> Result<i64, String> {
    reader.seek(start).map_err(|e| e.to_string())?;
    let mut samples = reader.samples::<i32>();
    let mut peak = 0;
    for index in 0..sample_count {
        if index % 16_384 == 0 { check()?; }
        peak = peak.max(i64::from(next_pcm(&mut samples)?).abs());
    }
    Ok(peak)
}


pub const MAX_MARKER_TRANSIENTS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientPoint {
    pub time_sec: f64,
    pub strength: f32, // 0.0 .. 1.0
}

/// Detects at most `MAX_MARKER_TRANSIENTS` (3) transients around a track marker (`marker_sec`).
/// Prioritizes distinct musical attacks near the marker so the user can cleanly position cut points.
pub fn detect_marker_transients(
    samples_l: &[f32],
    samples_r: &[f32],
    sample_rate: u32,
    marker_sec: f64,
    radius_sec: f64,
    sensitivity: f32,
) -> Vec<TransientPoint> {
    if sample_rate == 0 || samples_l.is_empty() || !marker_sec.is_finite() {
        return Vec::new();
    }
    let total_sec = samples_l.len() as f64 / f64::from(sample_rate);
    let r = radius_sec.clamp(0.15, 4.0);
    let start_sec = (marker_sec - r).max(0.0);
    let end_sec = (marker_sec + r).min(total_sec);
    let mut candidates = detect_transients_in_range(
        samples_l,
        samples_r,
        sample_rate,
        start_sec,
        end_sec,
        sensitivity,
    );
    if candidates.len() <= MAX_MARKER_TRANSIENTS {
        return candidates;
    }

    // Score candidates by onset strength weighted by proximity to the marker
    // (slightly favoring onsets at or shortly after the marker, typical for song starts after a pause)
    candidates.sort_by(|a, b| {
        let dist_a = (a.time_sec - marker_sec).abs();
        let dist_b = (b.time_sec - marker_sec).abs();
        let bias_a = if a.time_sec >= marker_sec - 0.05 { 1.15_f32 } else { 1.0_f32 };
        let bias_b = if b.time_sec >= marker_sec - 0.05 { 1.15_f32 } else { 1.0_f32 };
        let score_a = (a.strength * bias_a) / (1.0 + 0.75 * (dist_a / r) as f32);
        let score_b = (b.strength * bias_b) / (1.0 + 0.75 * (dist_b / r) as f32);
        score_b.total_cmp(&score_a)
    });

    let min_separation_sec = (r * 0.12).clamp(0.08, 0.35);
    let mut selected: Vec<TransientPoint> = Vec::with_capacity(MAX_MARKER_TRANSIENTS);
    for cand in &candidates {
        if selected.len() >= MAX_MARKER_TRANSIENTS {
            break;
        }
        if selected.iter().all(|s| (s.time_sec - cand.time_sec).abs() >= min_separation_sec) {
            selected.push(*cand);
        }
    }
    // If strict separation left fewer than 3 while candidates exist, fill up to 3
    if selected.len() < MAX_MARKER_TRANSIENTS {
        for cand in &candidates {
            if selected.len() >= MAX_MARKER_TRANSIENTS {
                break;
            }
            if !selected.iter().any(|s| (s.time_sec - cand.time_sec).abs() < 0.02) {
                selected.push(*cand);
            }
        }
    }

    selected.sort_by(|a, b| a.time_sec.total_cmp(&b.time_sec));
    selected.truncate(MAX_MARKER_TRANSIENTS);
    selected
}

/// Snaps a target time in seconds to the nearest stereo zero-crossing (or minimum energy point)
/// within `±window_ms` milliseconds to prevent clicks at cut boundaries.
pub fn snap_to_zero_crossing(
    samples_l: &[f32],
    samples_r: &[f32],
    sample_rate: u32,
    target_sec: f64,
    window_ms: f64,
) -> f64 {
    if sample_rate == 0 || samples_l.is_empty() || !target_sec.is_finite() {
        return target_sec;
    }
    let sr = f64::from(sample_rate);
    let n = samples_l.len().min(samples_r.len());
    if n < 2 {
        return target_sec;
    }
    let center = ((target_sec * sr).round() as usize).min(n - 1);
    let radius = ((window_ms.clamp(0.5, 25.0) * 0.001 * sr).round() as usize).max(1);
    let start = center.saturating_sub(radius);
    let end = (center + radius).min(n - 1);

    let mut best_idx = center;
    let mut best_cost = f32::MAX;

    for idx in start..=end {
        let l = samples_l[idx];
        let r = samples_r[idx];
        let amp = l.abs() + r.abs();
        let crosses_zero = idx > 0 && (
            (samples_l[idx - 1] <= 0.0 && l >= 0.0) || (samples_l[idx - 1] >= 0.0 && l <= 0.0) ||
            (samples_r[idx - 1] <= 0.0 && r >= 0.0) || (samples_r[idx - 1] >= 0.0 && r <= 0.0)
        );
        let dist_penalty = ((idx as isize - center as isize).abs() as f32 / radius as f32) * 0.015;
        let zero_bonus = if crosses_zero { 0.35 } else { 1.0 };
        let cost = amp * zero_bonus + dist_penalty;
        if cost < best_cost {
            best_cost = cost;
            best_idx = idx;
        }
    }

    best_idx as f64 / sr
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
    fn moving_marker_previews_commit_as_one_edit_and_respect_neighbors() {
        let mut editor = TrackSplitter::new();
        editor.add_marker(1.0, 4.0, 48_000);
        editor.add_marker(2.0, 4.0, 48_000);
        let before = editor.snapshot();
        let undo_count = editor.undo_stack.len();
        assert!(editor.preview_marker_move(0, 1.25, 4.0, 48_000));
        assert!(editor.preview_marker_move(0, 3.0, 4.0, 48_000));
        assert_eq!((editor.markers[0] * 48_000.0).round() as u64, 95_999);
        editor.commit_edit(before.clone());
        assert_eq!(editor.undo_stack.len(), undo_count + 1);
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), before);
    }

    #[test]
    fn export_preserves_pcm_and_protects_existing_files() {
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let base = std::env::temp_dir();
        let (source, file) = crate::project::unique_file(&base, "tapedigizer-export-test", "wav").unwrap();
        let output = source.with_extension("output");
        let samples = [-8_388_608, 8_388_607, 7, -8, 125_000, -250_000, 2, -1];
        let mut writer = WavWriter::new(file, WavSpec {
            channels: 2, sample_rate: 48_000, bits_per_sample: 24, sample_format: SampleFormat::Int,
        }).unwrap();
        for sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        let editor = TrackSplitter {
            markers: vec![1.0 / 48_000.0, 2.0 / 48_000.0],
            pause_indices: vec![1],
            ..TrackSplitter::new()
        };
        let tracks = editor.get_tracks(1.0);
        let mut tracks = tracks[..3].to_vec();
        tracks[2].end_seconds = 4.0 / 48_000.0;
        let proceed = |_, _| true;
        let original = output.join("original");
        let paths = export_tracks(&source, &original, &tracks, NormalizationMode::None, proceed).unwrap();
        let read = |path: &Path| WavReader::open(path).unwrap().samples::<i32>()
            .collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(read(&paths[0]), &samples[..2]);
        assert_eq!(read(&paths[1]), &samples[4..]);
        assert!(export_tracks(&source, &original, &tracks, NormalizationMode::None, proceed).is_err());
        assert_eq!(read(&paths[0]), &samples[..2]);

        let per_track = export_tracks(&source, output.join("track"), &tracks, NormalizationMode::PeakPerTrack, proceed).unwrap();
        let album = export_tracks(&source, output.join("album"), &tracks, NormalizationMode::PeakAlbum, proceed).unwrap();
        assert!(read(&per_track[1])[1].abs() > 2_000_000);
        assert!(read(&album[1])[1].abs() < 250_000);

        let cancelled = output.join("cancelled");
        let result = export_tracks(&source, &cancelled, &tracks, NormalizationMode::None, |finished, _| finished < 1);
        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(cancelled).unwrap().count(), 0);
        let truncated = output.join("truncated");
        let file = std::fs::OpenOptions::new().write(true).open(&source).unwrap();
        file.set_len(file.metadata().unwrap().len() - 6).unwrap();
        assert!(export_tracks(&source, &truncated, &tracks, NormalizationMode::None, proceed).is_err());
        assert_eq!(std::fs::read_dir(truncated).unwrap().count(), 0);
        std::fs::remove_dir_all(output).unwrap();
        std::fs::remove_file(source).unwrap();
    }

    #[test]
    fn splitting_and_merging_preserves_segment_metadata() {
        let mut editor = TrackSplitter::new();
        editor.add_marker(4.0, 12.0, 48_000);
        editor.custom_titles = vec!["First".into(), "Second".into()];
        editor.toggle_pause(1);
        editor.add_marker(2.0, 12.0, 48_000);
        assert_eq!(editor.get_tracks(12.0)[2].title, "Second");
        assert!(editor.is_pause(2));
        editor.remove_marker_at_index(0);
        assert_eq!(editor.get_tracks(12.0)[1].title, "Second");
        assert!(editor.is_pause(1));
        assert!(editor.undo());
        assert!(editor.is_pause(2));
    }

    #[test]
    fn test_track_splitting_markers() {
        let mut splitter = TrackSplitter::new();
        let total_duration = 300.0; // 5 minutes

        splitter.add_marker(60.0, total_duration, 48_000);
        splitter.add_marker(150.0, total_duration, 48_000);
        splitter.add_marker(210.0, total_duration, 48_000);

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

        splitter.add_marker(60.0, 300.0, 48_000);
        assert_eq!(splitter.markers, vec![60.0]);
        assert!(splitter.can_undo());

        splitter.add_marker(120.0, 300.0, 48_000);
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
        splitter.add_marker(50.0, total_duration, 48_000);
        splitter.add_marker(100.0, total_duration, 48_000);

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
        assert!(splitter.move_marker(0, splitter.markers[0] + 5.0, total_duration, 48_000));
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

    #[test]
    fn test_detect_marker_transients_caps_at_three() {
        let sample_rate = 48000;
        let num_samples = 48000 * 4; // 4 seconds
        let mut samples_l = vec![0.0_f32; num_samples];
        let mut samples_r = vec![0.0_f32; num_samples];

        // Synthesize 8 distinct transients around a marker at 2.0s
        for k in 0..8 {
            let t_sec = 0.8 + k as f64 * 0.3;
            let idx = (t_sec * sample_rate as f64) as usize;
            for i in 0..350 {
                if idx + i < num_samples {
                    let v = 0.85 * (-(i as f32) / 90.0).exp();
                    samples_l[idx + i] = v;
                    samples_r[idx + i] = v;
                }
            }
        }

        let marker_transients = detect_marker_transients(&samples_l, &samples_r, sample_rate, 2.0, 1.8, 1.2);
        assert!(!marker_transients.is_empty());
        assert!(marker_transients.len() <= MAX_MARKER_TRANSIENTS, "Must never return more than 3 transients around a marker");
        assert_eq!(marker_transients.len(), 3);
    }

    #[test]
    fn test_snap_to_zero_crossing() {
        let sample_rate = 10_000;
        // 100 Hz sine wave: period = 100 samples (10ms), zero crossings every 50 samples (5ms)
        let samples: Vec<f32> = (0..1000)
            .map(|i| (2.0 * std::f32::consts::PI * 100.0 * (i as f32) / (sample_rate as f32)).sin())
            .collect();
        // Target at sample 123 (peak is at 125, zero crossing at 100 = 0.010s)
        let snapped = snap_to_zero_crossing(&samples, &samples, sample_rate, 0.0120, 4.0);
        let idx = (snapped * sample_rate as f64).round() as usize;
        assert!(samples[idx].abs() < 0.1, "Snapped sample amplitude {} should be near zero", samples[idx]);
    }
}
