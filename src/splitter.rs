use std::collections::BTreeMap;
use crate::audio::processing::{Routing, SongExportMode, SongProcessor, SongSettings};
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
    pub is_filtered: bool,
    pub processing: Option<SongSettings>,
}

impl TrackInfo {
    #[allow(dead_code)]
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

    pub fn export_filenames_with_artist(&self, artist: Option<&str>) -> Vec<String> {
        let filename = self.export_filename_with_artist(artist);
        if self.processing.as_ref().is_some_and(|s| s.export_mode == SongExportMode::SeparateMono) {
            let stem = filename.strip_suffix(".wav").unwrap_or(&filename);
            vec![format!("{stem} - L.wav"), format!("{stem} - R.wav")]
        } else {
            vec![filename]
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
    /// The immutable WAV to which these positions and settings apply.
    pub(crate) audio_revision: Option<PathBuf>,
    pub(crate) revision_preview_enabled: bool,
    pub markers: Vec<f64>,
    pub custom_titles: Vec<String>,
    pub pause_indices: Vec<usize>,
    pub song_settings: BTreeMap<usize, SongSettings>,
    pub disabled_filter_indices: Vec<usize>,
}

impl MarkerSnapshot {
    pub(crate) fn relocate_audio_revision(&mut self, from: Option<&Path>, to: &Path) {
        if self.audio_revision.as_deref() == from {
            self.audio_revision = Some(to.to_path_buf());
        }
    }

    /// Reindex all segment metadata using the original segment which owns each new interval.
    pub(crate) fn remap_segments(
        &mut self,
        markers: Vec<f64>,
        owners: &[usize],
        detected_pauses: Option<Vec<usize>>,
    ) {
        debug_assert_eq!(markers.len() + 1, owners.len());
        let pauses = detected_pauses.unwrap_or_else(|| {
            owners.iter().enumerate().filter_map(|(index, owner)| {
                self.pause_indices.contains(owner).then_some(index)
            }).collect()
        });
        let mut titles = Vec::new();
        let mut start = 0;
        for segments in owners.chunk_by(|a, b| a == b) {
            let end = start + segments.len();
            if let Some(title) = self.custom_titles.get(segments[0]).filter(|title| !title.is_empty()) {
                // If detection splits off silence, keep the name on the first surviving song.
                let destination = (start..end).find(|i| pauses.binary_search(i).is_err()).unwrap_or(start);
                titles.resize(destination + 1, String::new());
                titles[destination] = title.clone();
            }
            start = end;
        }
        let mut disabled = Vec::new();
        let mut settings = BTreeMap::new();
        for (index, &owner) in owners.iter().enumerate() {
            if self.disabled_filter_indices.contains(&owner) {
                disabled.push(index);
            }
            if let Some(song) = self.song_settings.get(&owner) {
                settings.insert(index, song.clone());
            }
        }
        self.markers = markers;
        self.custom_titles = titles;
        self.pause_indices = pauses;
        self.disabled_filter_indices = disabled;
        self.song_settings = settings;
    }
}

pub struct TrackSplitter {
    pub(crate) audio_revision: Option<PathBuf>,
    pub(crate) revision_preview_enabled: bool,
    pub markers: Vec<f64>, // Split points in seconds
    pub custom_titles: Vec<String>,
    pub pause_indices: Vec<usize>, // Segments marked as excluded pauses (wont be exported)
    pub song_settings: BTreeMap<usize, SongSettings>,
    pub disabled_filter_indices: Vec<usize>, // Segments that will NOT have the filter applied
    pub undo_stack: Vec<MarkerSnapshot>,
    pub redo_stack: Vec<MarkerSnapshot>,
}

impl TrackSplitter {
    pub fn new() -> Self {
        Self {
            audio_revision: None,
            revision_preview_enabled: false,
            song_settings: BTreeMap::new(),
            markers: Vec::new(),
            custom_titles: Vec::new(),
            pause_indices: Vec::new(),
            disabled_filter_indices: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> MarkerSnapshot {
        MarkerSnapshot {
            audio_revision: self.audio_revision.clone(),
            revision_preview_enabled: self.revision_preview_enabled,
            song_settings: self.song_settings.clone(),
            markers: self.markers.clone(),
            custom_titles: self.custom_titles.clone(),
            pause_indices: self.pause_indices.clone(),
            disabled_filter_indices: self.disabled_filter_indices.clone(),
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
            self.restore(prev);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            let current = self.snapshot();
            self.undo_stack.push(current);
            self.restore(next);
            true
        } else {
            false
        }
    }

    pub(crate) fn relocate_audio_revision(&mut self, to: &Path) {
        if self.audio_revision.as_deref() == Some(to) {
            return;
        }
        let from = self.audio_revision.replace(to.to_path_buf());
        for snapshot in self.undo_stack.iter_mut().chain(&mut self.redo_stack) {
            snapshot.relocate_audio_revision(from.as_deref(), to);
        }
    }

    pub(crate) fn history_target(&self, redo: bool) -> Option<&MarkerSnapshot> {
        if redo { self.redo_stack.last() } else { self.undo_stack.last() }
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
        let following = self.song_settings.split_off(&(index + 1));
        self.song_settings.extend(following.into_iter().map(|(i, settings)| (i + 1, settings)));
        if let Some(inherited) = self.song_settings.get(&index).cloned() {
            self.song_settings.insert(index + 1, inherited);
        }
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
        let split_disabled = self.disabled_filter_indices.contains(&index);
        for item in &mut self.disabled_filter_indices {
            if *item > index {
                *item += 1;
            }
        }
        if split_disabled {
            self.disabled_filter_indices.push(index + 1);
            self.disabled_filter_indices.sort_unstable();
        }
        Some(index)
    }

    pub fn remove_marker_at_index(&mut self, index: usize) {
        if index >= self.markers.len() {
            return;
        }
        self.push_undo();
        self.markers.remove(index);
        self.song_settings.remove(&(index + 1));
        let following = self.song_settings.split_off(&(index + 2));
        self.song_settings.extend(following.into_iter().map(|(i, settings)| (i - 1, settings)));
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
        let merged_disabled = !self.is_track_filtered(index) && !self.is_track_filtered(index + 1);
        self.disabled_filter_indices.retain(|&p| p != index + 1 && (p != index || merged_disabled));
        for item in &mut self.disabled_filter_indices {
            if *item > index + 1 {
                *item -= 1;
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
        self.audio_revision = snapshot.audio_revision;
        self.revision_preview_enabled = snapshot.revision_preview_enabled;
        self.song_settings = snapshot.song_settings;
        self.markers = snapshot.markers;
        self.custom_titles = snapshot.custom_titles;
        self.pause_indices = snapshot.pause_indices;
        self.disabled_filter_indices = snapshot.disabled_filter_indices;
    }

    pub fn clear_markers(&mut self) {
        if !self.markers.is_empty() || !self.custom_titles.is_empty() || !self.pause_indices.is_empty() || !self.disabled_filter_indices.is_empty() {
            self.push_undo();
        }
        self.song_settings.retain(|&i, _| i == 0);
        self.markers.clear();
        self.custom_titles.clear();
        self.pause_indices.clear();
        self.disabled_filter_indices.clear();
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

    pub fn is_track_filtered(&self, segment_index: usize) -> bool {
        !self.disabled_filter_indices.contains(&segment_index)
    }

    pub fn set_track_filtered(&mut self, segment_index: usize, filtered: bool) {
        let is_curr = self.is_track_filtered(segment_index);
        if is_curr != filtered {
            self.push_undo();
            if filtered {
                self.disabled_filter_indices.retain(|&idx| idx != segment_index);
            } else if !self.disabled_filter_indices.contains(&segment_index) {
                self.disabled_filter_indices.push(segment_index);
                self.disabled_filter_indices.sort_unstable();
            }
        }
    }

    pub fn toggle_track_filter(&mut self, segment_index: usize) {
        let currently_filtered = self.is_track_filtered(segment_index);
        self.set_track_filtered(segment_index, !currently_filtered);
    }

    pub fn set_all_tracks_filtered(&mut self, total_segments: usize, filtered: bool) {
        self.push_undo();
        if filtered {
            self.disabled_filter_indices.clear();
        } else {
            self.disabled_filter_indices = (0..total_segments).collect();
        }
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
            let is_f = !self.disabled_filter_indices.contains(&i);

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
                is_filtered: is_f,
                processing: self.song_settings.get(&i).cloned(),
            });
        }

        tracks
    }

    pub fn detect_silence_pauses(
        &mut self,
        samples_l: &[f32],
        samples_r: &[f32],
        sample_rate: u32,
        threshold_db: f32,
        min_silence_seconds: f64,
    ) {
        self.detect_pauses(samples_l, samples_r, sample_rate, threshold_db, min_silence_seconds, false);
    }

    pub fn detect_silence_pauses_as_gaps(
        &mut self,
        samples_l: &[f32],
        samples_r: &[f32],
        sample_rate: u32,
        threshold_db: f32,
        min_silence_seconds: f64,
    ) {
        self.detect_pauses(samples_l, samples_r, sample_rate, threshold_db, min_silence_seconds, true);
    }

    fn detect_pauses(
        &mut self,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        threshold_db: f32,
        min_silence_seconds: f64,
        exclude_gaps: bool,
    ) {
        let length = left.len().min(right.len());
        if length == 0 || sample_rate == 0 {
            return;
        }
        let rate = f64::from(sample_rate);
        let window = (sample_rate as usize / 10).max(1);
        let min_windows = (min_silence_seconds / 0.1).ceil() as usize;
        let n_windows = length / window;
        let duration = length as f64 / rate;
        let mut silence = None;
        let mut gaps = Vec::new();
        let mut markers = Vec::new();
        for w in 0..=n_windows {
            let start = w * window;
            let quiet = if w < n_windows {
                let energy: f64 = (start..start + window)
                    .map(|i| {
                        let (l, r) = (f64::from(left[i]), f64::from(right[i]));
                        l * l + r * r
                    })
                    .sum();
                linear_to_db((energy / (2 * window) as f64).sqrt() as f32) < threshold_db
            } else {
                false
            };
            if quiet {
                silence.get_or_insert(w);
            } else if let Some(first) = silence.take() {
                if w - first < min_windows {
                    continue;
                }
                if exclude_gaps {
                    let a = (first * window) as f64 / rate;
                    let b = if w == n_windows { duration } else { start as f64 / rate };
                    if a > 0.4 {
                        markers.push(a);
                    }
                    if b < duration - 0.4 {
                        markers.push(b);
                    }
                    gaps.push((a, b));
                } else if w < n_windows {
                    let center = ((first + (w - first) / 2) * window) as f64 / rate;
                    if center > 0.0 {
                        markers.push(center);
                    }
                }
            }
        }
        let before = self.snapshot();
        // A redetected boundary may move slightly. Match each new song to the old
        // song with the most overlapping audio, preferring the left one on ties.
        let mut owners = Vec::with_capacity(markers.len() + 1);
        let mut pauses = Vec::new();
        let mut start = 0.0;
        for end in markers.iter().copied().chain(std::iter::once(duration)) {
            let mut old = before.markers.partition_point(|&m| m <= start);
            let mut owner = old;
            let mut longest = 0.0;
            let mut position = start;
            while position < end {
                let boundary = before.markers.get(old).copied().unwrap_or(duration).min(end);
                if boundary - position > longest {
                    longest = boundary - position;
                    owner = old;
                }
                position = boundary;
                old += 1;
            }
            if gaps.iter().any(|&(a, b)| start >= a - 0.25 && end <= b + 0.25) {
                pauses.push(owners.len());
            }
            owners.push(owner);
            start = end;
        }
        let mut snapshot = before.clone();
        snapshot.remap_segments(markers, &owners, Some(pauses));
        self.restore(snapshot);
        self.commit_edit(before);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    pub norm_mode: NormalizationMode,
    pub highpass_20hz: bool,
    pub fade_ms: u32,
    pub stereo_balance_db: f32,
    pub artist: Option<String>,
    pub denoiser: Option<(Option<crate::audio::denoiser::NoiseProfile>, crate::audio::denoiser::DenoiserParams)>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            norm_mode: NormalizationMode::None,
            highpass_20hz: false,
            fade_ms: 0,
            stereo_balance_db: 0.0,
            artist: None,
            denoiser: None,
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
    if !matches!(spec.channels, 1 | 2)
        || spec.sample_rate == 0
        || spec.sample_format != hound::SampleFormat::Int
        || !matches!(spec.bits_per_sample, 16 | 24 | 32)
    {
        return Err("Unsupported WAV format".into());
    }
    let duration = reader.duration();
    let rate = f64::from(spec.sample_rate);
    let mut exports = Vec::new();
    let mut names = std::collections::HashSet::new();
    let artist_ref = options.artist.as_deref();
    for track in tracks.iter().filter(|track| !track.is_pause) {
        let start = (track.start_seconds * rate).round() as u32;
        let end = (track.end_seconds * rate).round() as u32;
        if !track.start_seconds.is_finite()
            || !track.end_seconds.is_finite()
            || track.start_seconds < 0.0
            || start >= end
            || end > duration
        {
            return Err(format!("Invalid export range for track {}", track.index));
        }
        let mut paths = Vec::new();
        for filename in track.export_filenames_with_artist(artist_ref) {
            let path = out_dir.join(filename);
            if path.exists() || !names.insert(path.clone()) {
                return Err(format!("Export destination already exists: {}", path.display()));
            }
            paths.push(path);
        }
        let mut processing = track.processing.clone();
        if let Some(song) = &mut processing {
            match song.export_mode {
                SongExportMode::AsEdited => {}
                SongExportMode::Stereo => song.routing = Routing::Stereo,
                SongExportMode::SeparateMono => {
                    song.routing = Routing::Stereo;
                    song.width = 1.0;
                    song.balance_db = 0.0;
                    song.swap_channels = false;
                }
            }
        }
        exports.push(ExportTrack { start, end, paths, processing, legacy_filter: track.is_filtered });
    }
    if exports.is_empty() {
        return Err("No tracks to export".into());
    }

    let track_count = exports.len();
    let report = |finished| {
        if progress(finished, track_count) {
            Ok(())
        } else {
            Err("Export cancelled".to_string())
        }
    };
    report(0)?;
    let has_custom = exports.iter().any(|export| export.processing.is_some());
    let full_scale = (1_i64 << (spec.bits_per_sample - 1)) as f64;
    let gain_for = |peak: f64| (10.0_f64.powf(-0.3 / 20.0) / peak.max(0.0001)).min(10.0);
    let mut gains = vec![1.0; track_count];
    if options.norm_mode == NormalizationMode::PeakAlbum && !has_custom {
        // Original projects normalize against the complete source, including pauses.
        let peak = read_peak(&mut reader, 0, u64::from(duration) * u64::from(spec.channels), || report(0))?;
        gains.fill(gain_for(peak as f64 / full_scale));
    } else if options.norm_mode != NormalizationMode::None {
        let mut album_peak = 0.0_f64;
        for (index, export) in exports.iter().enumerate() {
            let peak = if export.processing.is_some() || options.norm_mode == NormalizationMode::PeakAlbum {
                // Measure the unclipped processed signal, before final fades/quantization.
                let (bl, br) = export.balance(options);
                let mut peak = 0.0_f64;
                render_track(
                    &mut reader,
                    export,
                    options,
                    || report(0),
                    |_, l, r| {
                        peak = peak.max((l * bl).abs()).max((r * br).abs());
                        Ok(())
                    },
                )?;
                peak
            } else {
                let sample_count = u64::from(export.end - export.start) * u64::from(spec.channels);
                read_peak(&mut reader, export.start, sample_count, || report(0))? as f64 / full_scale
            };
            gains[index] = gain_for(peak);
            album_peak = album_peak.max(peak);
        }
        if options.norm_mode == NormalizationMode::PeakAlbum {
            gains.fill(gain_for(album_peak));
        }
    }

    let fade_frames = ((f64::from(options.fade_ms) / 1000.0) * rate).round() as u32;
    let mut staged = Vec::new();
    let mut published = Vec::new();
    let result = (|| {
        for (index, export) in exports.iter().enumerate() {
            let count = export.end - export.start;
            let mono = export.paths.len() == 2
                || export.processing.as_ref().is_some_and(|s| s.routing != Routing::Stereo);
            let output_spec = hound::WavSpec {
                channels: if mono { 1 } else { 2 },
                sample_rate: spec.sample_rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writers = Vec::with_capacity(export.paths.len());
            for path in &export.paths {
                let (temporary, file) = crate::project::unique_file(out_dir, ".export", "partial")?;
                staged.push((temporary, path.clone()));
                writers.push(
                    hound::WavWriter::new(std::io::BufWriter::new(file), output_spec)
                        .map_err(|e| e.to_string())?,
                );
            }
            let (bl, br) = export.balance(options);
            render_track(
                &mut reader,
                export,
                options,
                || report(index),
                |frame, l, r| {
                    let fade = if fade_frames > 0 && count > fade_frames.saturating_mul(2) {
                        f64::from(frame.min(count - 1 - frame).min(fade_frames)) / f64::from(fade_frames)
                    } else {
                        1.0
                    };
                    let encode = |sample: f64, balance: f64| {
                        (sample * 8_388_608.0 * gains[index] * balance * fade)
                            .round()
                            .clamp(-8_388_608.0, 8_388_607.0) as i32
                    };
                    writers[0].write_sample(encode(l, bl)).map_err(|e| e.to_string())?;
                    if let Some(right) = writers.get_mut(1) {
                        right.write_sample(encode(r, br)).map_err(|e| e.to_string())?;
                    } else if !mono {
                        writers[0].write_sample(encode(r, br)).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                },
            )?;
            for writer in writers {
                writer.finalize().map_err(|e| e.to_string())?;
            }
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

struct ExportTrack {
    start: u32,
    end: u32,
    paths: Vec<PathBuf>,
    processing: Option<SongSettings>,
    legacy_filter: bool,
}

impl ExportTrack {
    fn balance(&self, options: &ExportOptions) -> (f64, f64) {
        let db = options.stereo_balance_db.clamp(-12.0, 12.0);
        if self.processing.is_some() || db.abs() < 0.05 {
            (1.0, 1.0)
        } else {
            (10.0_f64.powf(f64::from(db * 0.5) / 20.0), 10.0_f64.powf(f64::from(-db * 0.5) / 20.0))
        }
    }
}

/// Stream precisely the selected source frames. Each pass starts with fresh DSP state.
fn render_track(
    reader: &mut hound::WavReader<impl std::io::Read + std::io::Seek>,
    export: &ExportTrack,
    options: &ExportOptions,
    check: impl Fn() -> Result<(), String>,
    mut emit: impl FnMut(u32, f64, f64) -> Result<(), String>,
) -> Result<(), String> {
    let spec = reader.spec();
    let count = (export.end - export.start) as usize;
    let full_scale = (1_i64 << (spec.bits_per_sample - 1)) as f64;
    reader.seek(export.start).map_err(|e| e.to_string())?;
    let mut samples = reader.samples::<i32>();
    let mut input = || -> Result<(f64, f64), String> {
        let l = f64::from(next_pcm(&mut samples)?) / full_scale;
        let r = if spec.channels == 2 { f64::from(next_pcm(&mut samples)?) / full_scale } else { l };
        Ok((l, r))
    };
    if let Some(settings) = &export.processing {
        let mut dsp = SongProcessor::new(spec.sample_rate, settings);
        let latency = dsp.latency();
        for step in 0..count + latency {
            if step % 16_384 == 0 {
                check()?;
            }
            let (l, r) = if step < count { input()? } else { (0.0, 0.0) };
            let (l, r) = dsp.process_frame(l as f32, r as f32);
            if step >= latency {
                emit((step - latency) as u32, f64::from(l), f64::from(r))?;
            }
        }
        return Ok(());
    }

    // Reproduce legacy global processing, including its held last sample during the STFT flush.
    let mut denoiser = options.denoiser.as_ref().filter(|(_, p)| p.enabled && export.legacy_filter).map(
        |(profile, params)| {
            let mut effect = crate::audio::denoiser::SpectralDenoiser::new(spec.sample_rate);
            effect.set_noise_profile(profile.clone());
            let mut params = params.clone();
            params.audition_mode = crate::audio::denoiser::AuditionMode::Wet;
            (effect, params)
        },
    );
    let use_hp = options.highpass_20hz && denoiser.as_ref().is_none_or(|(_, p)| !p.dc_filter_enabled);
    let alpha = 1.0 / (1.0 + 2.0 * std::f64::consts::PI * 20.0 / f64::from(spec.sample_rate));
    let latency = if denoiser.is_some() { crate::audio::denoiser::STFT_LATENCY_SAMPLES } else { 0 };
    let mut last = (0.0, 0.0);
    let mut previous_input = (0.0, 0.0);
    let mut previous_output = (0.0, 0.0);
    for step in 0..count + latency {
        if step % 16_384 == 0 {
            check()?;
        }
        if step < count {
            last = input()?;
        }
        let (mut l, mut r) = if let Some((effect, params)) = &mut denoiser {
            let (l, r) = effect.process_sample(last.0 as f32, last.1 as f32, params);
            (f64::from(l), f64::from(r))
        } else {
            last
        };
        if step < latency {
            continue;
        }
        if use_hp {
            let filtered = (
                alpha * (previous_output.0 + l - previous_input.0),
                alpha * (previous_output.1 + r - previous_input.1),
            );
            previous_input = (l, r);
            previous_output = filtered;
            (l, r) = filtered;
        }
        emit((step - latency) as u32, l, r)?;
    }
    Ok(())
}

fn next_pcm(samples: &mut impl Iterator<Item = Result<i32, hound::Error>>) -> Result<i32, String> {
    samples.next().ok_or("Incomplete WAV data")?.map_err(|e| e.to_string())
}

fn read_peak(
    reader: &mut hound::WavReader<impl std::io::Read + std::io::Seek>,
    start: u32,
    sample_count: u64,
    check: impl Fn() -> Result<(), String>,
) -> Result<i64, String> {
    reader.seek(start).map_err(|e| e.to_string())?;
    let mut samples = reader.samples::<i32>();
    let mut peak = 0;
    for index in 0..sample_count {
        if index % 16_384 == 0 {
            check()?;
        }
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
    fn leading_detected_pause_keeps_title_on_music() {
        let mut editor = TrackSplitter::new();
        editor.custom_titles.push("My Song".into());
        editor.song_settings.insert(0, SongSettings { gain_db: -2.0, ..Default::default() });
        let before = editor.snapshot();
        let mut audio = vec![0.25; 3000];
        audio[..1000].fill(0.0);
        editor.detect_silence_pauses_as_gaps(&audio, &audio, 1000, -30.0, 0.4);
        let tracks = editor.get_tracks(3.0);
        assert_eq!(tracks.len(), 2);
        assert!(tracks[0].is_pause);
        assert_eq!(tracks[1].title, "My Song");
        assert_ne!(tracks[0].title, "My Song");
        assert_eq!(tracks[1].processing, before.song_settings.get(&0).cloned());
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), before);
    }

    #[test]
    fn adopting_other_song_keeps_legacy_per_track_normalization_identical() {
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let (source, file) =
            crate::project::unique_file(&std::env::temp_dir(), "legacy-per-track", "wav").unwrap();
        let output = source.with_extension("output");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::new(file, spec).unwrap();
        for _ in 0..3200 {
            writer.write_sample(8192_i16).unwrap();
        }
        writer.finalize().unwrap();

        let options = ExportOptions {
            norm_mode: NormalizationMode::PeakPerTrack,
            stereo_balance_db: 12.0,
            ..ExportOptions::default()
        };
        let mut editor = TrackSplitter::new();
        editor.add_marker(0.1, 0.2, 8000);
        let first_frame = |directory: &str, tracks: &[TrackInfo]| {
            let files =
                export_tracks_with_options(&source, output.join(directory), tracks, &options, |_, _| true)
                    .unwrap();
            WavReader::open(&files[0])
                .unwrap()
                .samples::<i32>()
                .take(2)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let legacy = first_frame("legacy", &editor.get_tracks(0.2));
        assert_eq!(legacy[0], 8_388_607);
        editor.song_settings.insert(1, SongSettings::default());
        assert_eq!(first_frame("mixed", &editor.get_tracks(0.2)), legacy);
        std::fs::remove_dir_all(output).unwrap();
        std::fs::remove_file(source).unwrap();
    }

    #[test]
    fn mixed_album_uses_one_gain_from_exported_processed_peaks() {
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let (path, file) = crate::project::unique_file(&std::env::temp_dir(), "mixed-album", "wav").unwrap();
        let output = path.with_extension("output");
        let mut writer = WavWriter::new(
            file,
            WavSpec {
                channels: 1,
                sample_rate: 8_000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .unwrap();
        for level in [4096_i16, 32767_i16, 8192_i16] {
            for _ in 0..800 {
                writer.write_sample(level).unwrap();
            }
        }
        writer.finalize().unwrap();
        let mut editor = TrackSplitter::new();
        editor.markers = vec![0.1, 0.2];
        editor.pause_indices = vec![1];
        editor
            .song_settings
            .insert(2, SongSettings { gain_db: 6.0206, routing: Routing::LeftMono, ..Default::default() });
        let paths =
            export_tracks(&path, &output, &editor.get_tracks(0.3), NormalizationMode::PeakAlbum, |_, _| true)
                .unwrap();
        let first_sample = |p: &Path| WavReader::open(p).unwrap().samples::<i32>().next().unwrap().unwrap();
        let target = (8_388_608.0 * 10.0_f64.powf(-0.3 / 20.0)).round() as i32;
        assert!((first_sample(&paths[0]) - target / 4).abs() <= 2);
        assert!((first_sample(&paths[1]) - target).abs() <= 2);
        std::fs::remove_dir_all(output).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stereo_override_and_two_mono_exports_preserve_channels_and_publish_atomically() {
        use crate::audio::processing::SongExportMode;
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let (path, file) = crate::project::unique_file(&std::env::temp_dir(), "split-stems", "wav").unwrap();
        let output = path.with_extension("output");
        let mut writer = WavWriter::new(
            file,
            WavSpec {
                channels: 2,
                sample_rate: 8_000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..1600 {
            writer.write_sample(4096_i16).unwrap();
            writer.write_sample(16384_i16).unwrap();
        }
        writer.finalize().unwrap();
        let mut editor = TrackSplitter::new();
        let mut settings = SongSettings {
            routing: Routing::LeftMono,
            export_mode: SongExportMode::Stereo,
            swap_channels: true,
            gain_db: -6.0,
            ..Default::default()
        };
        settings.channels[1].invert = true;
        editor.song_settings.insert(0, settings.clone());
        let export = |dir: &Path, tracks: &[TrackInfo]| {
            export_tracks(&path, dir, tracks, NormalizationMode::PeakPerTrack, |_, _| true)
        };
        let target = (8_388_608.0 * 10.0_f64.powf(-0.3 / 20.0)).round() as i32;
        let songs = editor.get_tracks(0.2);
        let stereo = export(&output.join("stereo"), &songs).unwrap();
        assert_eq!(stereo.len(), 1);
        let mut reader = WavReader::open(&stereo[0]).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.duration(), 1600);
        let samples: Vec<i32> = reader.samples().take(2).collect::<Result<_, _>>().unwrap();
        assert_eq!(samples, [-target, (f64::from(target) / 4.0).round() as i32]);

        settings.export_mode = SongExportMode::SeparateMono;
        settings.width = 0.0;
        settings.balance_db = 12.0;
        editor.song_settings.insert(0, settings);
        let songs = editor.get_tracks(0.2);
        let names = songs[0].export_filenames_with_artist(None);
        assert_eq!(names, ["01 - Track 01 - L.wav", "01 - Track 01 - R.wav"]);
        let stems = export(&output.join("stems"), &songs).unwrap();
        assert_eq!(stems.len(), 2);
        for (index, expected) in [(0, (f64::from(target) / 4.0).round() as i32), (1, -target)] {
            let mut reader = WavReader::open(&stems[index]).unwrap();
            assert_eq!(reader.spec().channels, 1);
            assert_eq!(reader.duration(), 1600);
            assert!(reader.samples::<i32>().all(|x| (x.unwrap() - expected).abs() <= 1));
        }
        let conflict = output.join("conflict");
        std::fs::create_dir_all(&conflict).unwrap();
        std::fs::write(conflict.join(&names[1]), b"keep").unwrap();
        assert!(export(&conflict, &songs).is_err());
        assert!(!conflict.join(&names[0]).exists());
        assert_eq!(std::fs::read(conflict.join(&names[1])).unwrap(), b"keep");
        // A destination can also appear after preflight, between staging and publication.
        let raced = output.join("raced");
        assert!(export_tracks(&path, &raced, &songs, NormalizationMode::None, |n, _| {
            if n == 1 {
                std::fs::write(raced.join(&names[1]), b"arrived").unwrap();
            }
            true
        })
        .is_err());
        assert!(!raced.join(&names[0]).exists());
        assert_eq!(std::fs::read(raced.join(&names[1])).unwrap(), b"arrived");
        assert_eq!(std::fs::read_dir(raced).unwrap().count(), 1);
        let cancelled = output.join("cancel");
        assert!(export_tracks(&path, &cancelled, &songs, NormalizationMode::None, |n, _| n == 0).is_err());
        assert_eq!(std::fs::read_dir(cancelled).unwrap().count(), 0);
        std::fs::remove_dir_all(output).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn silence_redetection_matches_existing_songs_and_undoes_as_one_edit() {
        let mut editor = TrackSplitter::new();
        editor.add_marker(1.49, 3.0, 1_000);
        let left = SongSettings { gain_db: -1.0, ..Default::default() };
        let right = SongSettings { gain_db: 2.0, ..Default::default() };
        editor.song_settings = [(0, left.clone()), (1, right.clone())].into();
        let before = editor.snapshot();
        let mut audio = vec![0.25; 3000];
        audio[1000..2000].fill(0.0);
        editor.detect_silence_pauses_as_gaps(&audio, &audio, 1000, -30.0, 0.4);
        assert_eq!(editor.markers, [1.0, 2.0]);
        assert_eq!(editor.pause_indices, [1]);
        assert_eq!(editor.song_settings[&0], left);
        assert_eq!(editor.song_settings[&2], right);
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), before);
    }

    #[test]
    fn song_settings_follow_splits_merges_and_history() {
        let mut editor = TrackSplitter::new();
        editor.add_marker(4.0, 12.0, 48_000);
        let a = SongSettings { gain_db: 2.0, ..Default::default() };
        let b = SongSettings { gain_db: -3.0, ..Default::default() };
        editor.song_settings = [(0, a.clone()), (1, b.clone())].into();
        let before = editor.snapshot();
        editor.add_marker(2.0, 12.0, 48_000);
        let tracks = editor.get_tracks(12.0);
        assert_eq!(tracks[1].processing, Some(a.clone()));
        assert_eq!(tracks[2].processing, Some(b));
        let divided = editor.snapshot();
        editor.remove_marker_at_index(1);
        assert_eq!(editor.song_settings.len(), 2);
        assert_eq!(editor.song_settings[&1], a);
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), divided);
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), before);
        assert!(editor.redo());
        assert_eq!(editor.snapshot(), divided);
        editor.clear_markers();
        assert_eq!(editor.song_settings, [(0, a)].into());
        assert!(editor.undo());
        assert_eq!(editor.snapshot(), divided);
    }

    #[test]
    fn processed_mono_export_only_uses_song_samples_and_settings() {
        use crate::audio::processing::{EffectStage, LimiterSettings, Routing, SongProcessor};
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let rate = 48_000;
        let (path, file) =
            crate::project::unique_file(&std::env::temp_dir(), "mono-song-export", "wav").unwrap();
        let output = path.with_extension("output");
        let mut writer = WavWriter::new(
            file,
            WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: SampleFormat::Int },
        )
        .unwrap();
        let clip: Vec<(i16, i16)> =
            (0..5400).map(|i| ((i % 145 * 15 - 1000) as i16, (i % 99 * 61 - 3000) as i16)).collect();
        for (l, r) in
            [(30000_i16, -30000_i16)].into_iter().chain(clip.iter().copied()).chain([(32000, 32000)])
        {
            writer.write_sample(l).unwrap();
            writer.write_sample(r).unwrap();
        }
        writer.finalize().unwrap();
        let source = std::fs::read(&path).unwrap();
        let mut editor = TrackSplitter::new();
        editor.markers = vec![1.0 / f64::from(rate), (1 + clip.len()) as f64 / f64::from(rate)];
        editor.pause_indices = vec![0, 2];
        let tracks_duration = (clip.len() + 2) as f64 / f64::from(rate);
        let mut song = SongSettings {
            routing: Routing::MonoMix,
            mix_left: 0.25,
            mix_right: 0.875,
            mono_gain_db: -1.5,
            ..Default::default()
        };
        song.effect_chain.move_by(EffectStage::Restoration, 4);
        song.effect_chain.move_by(EffectStage::Limiter, -4);
        for channel in &mut song.channels {
            channel.gain_db = -6.0;
            channel.limiter = LimiterSettings {
                enabled: true,
                ceiling_db: -24.0,
                drive_db: 7.0,
                ..Default::default()
            };
        }
        let options = ExportOptions {
            highpass_20hz: true,
            stereo_balance_db: 10.0,
            denoiser: Some((
                None,
                crate::audio::denoiser::DenoiserParams { enabled: true, ..Default::default() },
            )),
            ..Default::default()
        };
        for spectral in [false, true] {
            song.channels[1].denoiser.enabled = spectral;
            song.noise_profile = Some(
                crate::audio::denoiser::NoiseProfile::learn_from_samples(
                    &[0.001; 256],
                    &[0.002; 256],
                    rate,
                    "test",
                    "cassette",
                )
                .unwrap(),
            );
            editor.song_settings.insert(1, song.clone());
            let tracks = editor.get_tracks(tracks_duration);
            let paths = export_tracks_with_options(
                &path,
                output.join(spectral.to_string()),
                &tracks,
                &options,
                |_, _| true,
            )
            .unwrap();
            let mut reader = WavReader::open(&paths[0]).unwrap();
            assert_eq!(reader.spec().channels, 1);
            let actual: Vec<i32> = reader.samples::<i32>().collect::<Result<_, _>>().unwrap();
            let mut dsp = SongProcessor::new(rate, &song);
            let latency = dsp.latency();
            if spectral {
                assert!(latency > 0);
            }
            let expected: Vec<i32> = clip
                .iter()
                .copied()
                .chain(std::iter::repeat((0, 0)).take(latency))
                .map(|(l, r)| dsp.process_frame(f32::from(l) / 32768.0, f32::from(r) / 32768.0).0)
                .skip(latency)
                .map(|x| (f64::from(x) * 8_388_608.0).round() as i32)
                .collect();
            assert_eq!(actual, expected);
        }
        assert_eq!(std::fs::read(&path).unwrap(), source);
        std::fs::remove_dir_all(output).unwrap();
        std::fs::remove_file(path).unwrap();
    }

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

    #[test]
    fn test_track_filter_toggle_and_undo() {
        let mut editor = TrackSplitter::new();
        editor.markers = vec![10.0, 20.0, 30.0];
        // 4 tracks: 0, 1, 2, 3
        assert!(editor.is_track_filtered(0));
        assert!(editor.is_track_filtered(1));

        // Toggle track 1 off
        editor.toggle_track_filter(1);
        assert!(!editor.is_track_filtered(1));
        assert!(editor.is_track_filtered(0));

        let tracks = editor.get_tracks(40.0);
        assert!(tracks[0].is_filtered);
        assert!(!tracks[1].is_filtered);
        assert!(tracks[2].is_filtered);
        assert!(tracks[3].is_filtered);

        // Undo restores track 1 to filtered
        assert!(editor.undo());
        assert!(editor.is_track_filtered(1));

        // Redo disables it again
        assert!(editor.redo());
        assert!(!editor.is_track_filtered(1));

        // Disable all tracks
        editor.set_all_tracks_filtered(4, false);
        assert!(!editor.is_track_filtered(0));
        assert!(!editor.is_track_filtered(1));
        assert!(!editor.is_track_filtered(2));
        assert!(!editor.is_track_filtered(3));

        // Enable all tracks
        editor.set_all_tracks_filtered(4, true);
        assert!(editor.is_track_filtered(0));
        assert!(editor.is_track_filtered(1));
    }

    #[test]
    fn test_export_tracks_with_per_track_filtering() {
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
        let base = std::env::temp_dir();
        let (source, file) = crate::project::unique_file(&base, "tapedigizer-filter-export-test", "wav").unwrap();
        let output = source.with_extension("filter_output");
        // 1.0s at 48 kHz stereo = 96,000 samples
        let samples: Vec<i32> = (0..96_000).map(|i| if i % 2 == 0 { 100_000 } else { -100_000 }).collect();
        let mut writer = WavWriter::new(file, WavSpec {
            channels: 2, sample_rate: 48_000, bits_per_sample: 24, sample_format: SampleFormat::Int,
        }).unwrap();
        for &sample in &samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        let mut editor = TrackSplitter::new();
        editor.markers = vec![0.5]; // Split at 0.5s into 2 tracks
        // Disable filtering for track 0, leave track 1 enabled
        editor.set_track_filtered(0, false);
        let tracks = editor.get_tracks(1.0);
        assert_eq!(tracks.len(), 2);
        assert!(!tracks[0].is_filtered);
        assert!(tracks[1].is_filtered);

        let mut denoiser_params = crate::audio::denoiser::DenoiserParams::default();
        denoiser_params.enabled = true;
        denoiser_params.reduction_db = 18.0;

        let options = ExportOptions {
            norm_mode: NormalizationMode::None,
            denoiser: Some((None, denoiser_params)),
            ..ExportOptions::default()
        };

        let proceed = |_, _| true;
        let paths = export_tracks_with_options(&source, &output, &tracks, &options, proceed).unwrap();
        assert_eq!(paths.len(), 2);

        let r0: Vec<i32> = WavReader::open(&paths[0]).unwrap().samples::<i32>().map(|s| s.unwrap()).collect();
        let r1: Vec<i32> = WavReader::open(&paths[1]).unwrap().samples::<i32>().map(|s| s.unwrap()).collect();

        // Track 0 was NOT filtered, so samples match original (first 48,000 samples)
        assert_eq!(r0, &samples[..48_000]);
        // Track 1 was filtered by denoiser
        assert_eq!(r1.len(), 48_000);

        // Cleanup
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_dir_all(output);
    }
}
