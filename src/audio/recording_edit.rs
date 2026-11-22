use std::io::BufWriter;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::analysis::{analyze_samples, AudioAnalysisReport};
use crate::project::{publish_new_file, unique_file};
use crate::splitter::MarkerSnapshot;

use super::denoiser::{
    crop_trim_audio_to_range, cut_delete_audio_range, AuditionMode, DenoiserParams, NoiseProfile,
    SpectralDenoiser,
};
use super::player::{AudioPlayer, DecodedAudio};
use super::waveform::WaveformCache;

pub(crate) struct PreparedAudio {
    pub path: PathBuf,
    pub decoded: DecodedAudio,
    pub waveform: WaveformCache,
    pub report: AudioAnalysisReport,
}

impl PreparedAudio {
    pub fn read(path: PathBuf) -> Result<Self, String> {
        let decoded = AudioPlayer::decode_file(&path)?;
        Self::new(path, decoded)
    }

    fn new(path: PathBuf, decoded: DecodedAudio) -> Result<Self, String> {
        let report = analyze_samples(&decoded.left, &decoded.right, decoded.sample_rate)?;
        let waveform = WaveformCache::build(&decoded);
        Ok(Self {
            path,
            decoded,
            waveform,
            report,
        })
    }
}

pub(crate) enum RecordingEdit {
    Remove(Range<f64>),
    Keep(Range<f64>),
    Restore {
        track: Option<(usize, Range<f64>)>,
        profile: Option<NoiseProfile>,
        params: DenoiserParams,
    },
}

pub(crate) struct EditedRecording {
    pub audio: PreparedAudio,
    pub markers: MarkerSnapshot,
    pub disable_live_denoiser: bool,
}

impl RecordingEdit {
    pub fn execute(
        self,
        directory: &Path,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        mut markers: MarkerSnapshot,
    ) -> Result<EditedRecording, String> {
        if sample_rate == 0 || left.is_empty() || left.len() != right.len() {
            return Err("Invalid recording audio".into());
        }
        let (left, right, disable_live_denoiser) = match self {
            Self::Remove(range) => {
                let (left, right) = cut_delete_audio_range(
                    left,
                    right,
                    sample_rate,
                    range.start,
                    range.end,
                    &mut markers,
                )?;
                (left, right, false)
            }
            Self::Keep(range) => {
                let (left, right) = crop_trim_audio_to_range(
                    left,
                    right,
                    sample_rate,
                    range.start,
                    range.end,
                    &mut markers,
                )?;
                (left, right, false)
            }
            Self::Restore {
                track,
                profile,
                mut params,
            } => {
                params.enabled = true;
                params.audition_mode = AuditionMode::Wet;
                if let Some((index, range)) = track {
                    let rate = f64::from(sample_rate);
                    let start = (range.start * rate).round() as usize;
                    let end = ((range.end * rate).round() as usize).min(left.len());
                    if start >= end {
                        return Err("Empty track range".into());
                    }
                    let (mut clean_l, mut clean_r) = SpectralDenoiser::process_offline_stereo(
                        &left[start..end],
                        &right[start..end],
                        sample_rate,
                        profile,
                        &params,
                    );
                    let fade = ((rate * 0.005).round() as usize).min(clean_l.len() / 4);
                    for i in 0..fade {
                        let wet = i as f32 / fade as f32;
                        for index in [i, clean_l.len() - 1 - i] {
                            clean_l[index] =
                                left[start + index] * (1.0 - wet) + clean_l[index] * wet;
                            clean_r[index] =
                                right[start + index] * (1.0 - wet) + clean_r[index] * wet;
                        }
                    }
                    let mut edited_l = left.to_vec();
                    let mut edited_r = right.to_vec();
                    edited_l[start..end].copy_from_slice(&clean_l);
                    edited_r[start..end].copy_from_slice(&clean_r);
                    if !markers.disabled_filter_indices.contains(&index) {
                        markers.disabled_filter_indices.push(index);
                        markers.disabled_filter_indices.sort_unstable();
                    }
                    (edited_l, edited_r, false)
                } else {
                    let (left, right) = SpectralDenoiser::process_offline_stereo(
                        left,
                        right,
                        sample_rate,
                        profile,
                        &params,
                    );
                    markers.disabled_filter_indices = (0..=markers.markers.len()).collect();
                    (left, right, true)
                }
            }
        };
        let audio = write_revision(
            directory,
            DecodedAudio {
                left,
                right,
                sample_rate,
            },
        )?;
        markers.audio_revision = Some(audio.path.clone());
        Ok(EditedRecording {
            audio,
            markers,
            disable_live_denoiser,
        })
    }
}

fn write_revision(directory: &Path, mut decoded: DecodedAudio) -> Result<PreparedAudio, String> {
    use std::io::Write;

    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let (temporary, mut file) = unique_file(directory, "audio-edit", "partial")?;
    let destination = temporary.with_extension("wav");
    let result = (|| {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: decoded.sample_rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buffered = BufWriter::with_capacity(1024 * 1024, &mut file);
        let mut writer =
            hound::WavWriter::new(&mut buffered, spec).map_err(|error| error.to_string())?;
        const SCALE: f32 = 8_388_608.0;
        for (left, right) in decoded.left.iter_mut().zip(&mut decoded.right) {
            for sample in [left, right] {
                let pcm = (*sample * SCALE).round().clamp(-SCALE, SCALE - 1.0) as i32;
                writer
                    .write_sample(pcm)
                    .map_err(|error| error.to_string())?;
                // Playback, waveform, and export all use the committed PCM.
                *sample = pcm as f32 / SCALE;
            }
        }
        writer.finalize().map_err(|error| error.to_string())?;
        buffered.flush().map_err(|error| error.to_string())?;
        drop(buffered);
        file.sync_all().map_err(|error| error.to_string())?;
        let audio = PreparedAudio::new(destination, decoded)?;
        publish_new_file(&temporary, &audio.path).map_err(|error| error.to_string())?;
        Ok(audio)
    })();
    let _ = std::fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::player::AudioPlayer;
    use crate::splitter::TrackSplitter;

    #[test]
    fn trim_publishes_a_new_revision_and_preserves_the_source() {
        let (sentinel, _) = unique_file(&std::env::temp_dir(), "tapedigizer-test", "tmp").unwrap();
        let dir = sentinel.with_extension("");
        std::fs::create_dir(&dir).unwrap();
        std::fs::remove_file(sentinel).unwrap();
        let source = dir.join("source.wav");
        let sr = 1000;
        let samples: Vec<_> = (0..1000).map(|i| i as f32 / 1024.0).collect();
        crate::audio::denoiser::write_stereo_wav_atomic(&source, &samples, &samples, sr).unwrap();
        let before = std::fs::read(&source).unwrap();
        let mut splitter = TrackSplitter::new();
        splitter.add_marker(0.5, 1.0, sr);
        let edit = RecordingEdit::Keep(0.2..0.8);
        let edited = edit
            .execute(&dir, &samples, &samples, sr, splitter.snapshot())
            .unwrap();

        assert_ne!(edited.audio.path, source);
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let from_disk = AudioPlayer::decode_file(&edited.audio.path).unwrap();
        assert_eq!(from_disk.left, edited.audio.decoded.left);
        assert_eq!(from_disk.left.len(), 600);
        assert_eq!(edited.markers.markers, [0.3]);
        assert_eq!(
            edited.markers.audio_revision.as_deref(),
            Some(edited.audio.path.as_path())
        );
        let trimmed_bytes = std::fs::read(&edited.audio.path).unwrap();
        let filtered = RecordingEdit::Restore {
            track: Some((0, 0.0..0.3)),
            profile: None,
            params: DenoiserParams {
                dc_filter_enabled: true,
                dc_cutoff_hz: 20.0,
                ..DenoiserParams::default()
            },
        }
        .execute(&dir, &from_disk.left, &from_disk.right, sr, edited.markers)
        .unwrap();
        assert_ne!(filtered.audio.path, edited.audio.path);
        assert_eq!(filtered.audio.decoded.left.len(), 600);
        assert_eq!(filtered.audio.decoded.left[300..], from_disk.left[300..]);
        assert!(filtered.audio.decoded.left[160].abs() < from_disk.left[160].abs() / 2.0);
        assert_eq!(filtered.markers.disabled_filter_indices, [0]);
        assert_eq!(std::fs::read(&edited.audio.path).unwrap(), trimmed_bytes);
        assert_eq!(std::fs::read(&source).unwrap(), before);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
