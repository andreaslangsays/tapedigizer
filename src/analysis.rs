use std::path::Path;
use crate::audio::meter::linear_to_db;

#[derive(Clone, Debug)]
pub enum LevelDiagnosis {
    Optimal {
        peak_db: f32,
        advice: String,
    },
    TooQuiet {
        peak_db: f32,
        recommended_boost_db: f32,
        advice: String,
    },
    Clipped {
        peak_db: f32,
        clipped_samples: usize,
        recommended_cut_db: f32,
        advice: String,
    },
}

#[derive(Clone, Debug)]
pub struct AudioAnalysisReport {
    pub peak_left_db: f32,
    pub peak_right_db: f32,
    pub max_peak_db: f32,
    pub rms_left_db: f32,
    pub rms_right_db: f32,
    pub clipped_samples: usize,
    pub total_samples: usize,
    pub duration_seconds: f64,
    pub diagnosis: LevelDiagnosis,
}

pub fn analyze_wav_file<P: AsRef<Path>>(path: P) -> Result<AudioAnalysisReport, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();

    let max_val = match spec.bits_per_sample {
        16 => 32768.0_f32,
        24 => 8388608.0_f32,
        32 => 2147483648.0_f32,
        _ => 32768.0_f32,
    };

    let mut peak_l: f32 = 0.0;
    let mut peak_r: f32 = 0.0;
    let mut sum_sq_l: f64 = 0.0;
    let mut sum_sq_r: f64 = 0.0;
    let mut clipped_samples: usize = 0;
    let mut frame_count: usize = 0;

    let clip_threshold = 0.999;

    if spec.channels == 2 {
        let mut iter = reader.samples::<i32>();
        while let (Some(Ok(s_l)), Some(Ok(s_r))) = (iter.next(), iter.next()) {
            let fl = (s_l as f32 / max_val).clamp(-1.0, 1.0);
            let fr = (s_r as f32 / max_val).clamp(-1.0, 1.0);

            let abs_l = fl.abs();
            let abs_r = fr.abs();

            if abs_l > peak_l { peak_l = abs_l; }
            if abs_r > peak_r { peak_r = abs_r; }

            if abs_l >= clip_threshold || abs_r >= clip_threshold {
                clipped_samples += 1;
            }

            sum_sq_l += (fl as f64) * (fl as f64);
            sum_sq_r += (fr as f64) * (fr as f64);
            frame_count += 1;
        }
    } else {
        for s in reader.samples::<i32>() {
            if let Ok(sample) = s {
                let f = (sample as f32 / max_val).clamp(-1.0, 1.0);
                let abs = f.abs();
                if abs > peak_l { peak_l = abs; peak_r = abs; }
                if abs >= clip_threshold { clipped_samples += 1; }
                sum_sq_l += (f as f64) * (f as f64);
                sum_sq_r += (f as f64) * (f as f64);
                frame_count += 1;
            }
        }
    }

    if frame_count == 0 {
        return Err("WAV-Datei enthält keine Audio-Samples".to_string());
    }

    let peak_l_db = linear_to_db(peak_l);
    let peak_r_db = linear_to_db(peak_r);
    let max_peak_db = peak_l_db.max(peak_r_db);

    let rms_l = (sum_sq_l / frame_count as f64).sqrt() as f32;
    let rms_r = (sum_sq_r / frame_count as f64).sqrt() as f32;
    let rms_l_db = linear_to_db(rms_l);
    let rms_r_db = linear_to_db(rms_r);

    let duration_seconds = frame_count as f64 / spec.sample_rate as f64;

    // Evaluate Diagnosis
    let target_peak_db = -1.5_f32; // Safe target for cassette/tape transfer

    let diagnosis = if clipped_samples > 0 || max_peak_db >= -0.1 {
        let cut = (max_peak_db - target_peak_db).abs().max(2.0).ceil();
        LevelDiagnosis::Clipped {
            peak_db: max_peak_db,
            clipped_samples,
            recommended_cut_db: cut,
            advice: format!(
                "[WARNUNG] Übersteuerung erkannt!\n\
                Es sind {} Samples an der Clipping-Grenze (0 dBFS) aufgetreten.\n\
                -> EMPFEHLUNG FÜR DAS ZOOM L-12:\n\
                Drehe den Gain-Regler am Audio-Interface um ca. -{:.0} dB zurück \
                und wiederhole die Aufnahme, um digitale Verzerrungen zu vermeiden.",
                clipped_samples, cut
            ),
        }
    } else if max_peak_db < -6.0 {
        let diff = (target_peak_db - max_peak_db).max(1.0);
        let boost = (diff * 0.85).round(); // Conservative boost recommendation
        LevelDiagnosis::TooQuiet {
            peak_db: max_peak_db,
            recommended_boost_db: boost,
            advice: format!(
                "[HINWEIS] Die Aufnahme ist etwas leise ({:.1} dBFS Peak).\n\
                Du hast ca. {:.1} dB ungenutzten Headroom verschenkt.\n\
                -> EMPFEHLUNG FÜR DAS ZOOM L-12:\n\
                Drehe den Gain-Regler am Audio-Interface um ca. +{:.0} dB weiter auf.\n\
                (Optimaler Zielbereich liegt bei ca. -1.5 dBFS)",
                max_peak_db, target_peak_db - max_peak_db, boost
            ),
        }
    } else {
        LevelDiagnosis::Optimal {
            peak_db: max_peak_db,
            advice: format!(
                "[OPTIMAL] Hervorragend ausgesteuert!\n\
                Der maximale Peak liegt bei {:.1} dBFS ohne Übersteuerungen.\n\
                Der analoge Dynamikbereich des Tonträgers wird optimal ausgenutzt. \
                Die Gain-Einstellung am Zoom L-12 ist ideal.",
                max_peak_db
            ),
        }
    };

    Ok(AudioAnalysisReport {
        peak_left_db: peak_l_db,
        peak_right_db: peak_r_db,
        max_peak_db,
        rms_left_db: rms_l_db,
        rms_right_db: rms_r_db,
        clipped_samples,
        total_samples: frame_count,
        duration_seconds,
        diagnosis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &Path, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            let samp = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(samp).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn test_analysis_optimal() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_opt.wav");
        // Peak around -2.0 dBFS (~0.79)
        let samples = vec![0.79_f32; 44100];
        write_test_wav(&test_file, &samples);

        let report = analyze_wav_file(&test_file).unwrap();
        assert_eq!(report.clipped_samples, 0);
        assert!((report.max_peak_db - (-2.0)).abs() < 0.5);
        match report.diagnosis {
            LevelDiagnosis::Optimal { .. } => {},
            _ => panic!("Expected Optimal diagnosis"),
        }
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_analysis_too_quiet() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_quiet.wav");
        // Peak around -14 dBFS (~0.2)
        let samples = vec![0.2_f32; 44100];
        write_test_wav(&test_file, &samples);

        let report = analyze_wav_file(&test_file).unwrap();
        assert_eq!(report.clipped_samples, 0);
        match report.diagnosis {
            LevelDiagnosis::TooQuiet { recommended_boost_db, .. } => {
                assert!(recommended_boost_db >= 9.0 && recommended_boost_db <= 12.0);
            },
            _ => panic!("Expected TooQuiet diagnosis"),
        }
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_analysis_clipped() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_clip.wav");
        // Saturated at 1.0 (clipping)
        let samples = vec![1.0_f32; 44100];
        write_test_wav(&test_file, &samples);

        let report = analyze_wav_file(&test_file).unwrap();
        assert!(report.clipped_samples > 0);
        match report.diagnosis {
            LevelDiagnosis::Clipped { clipped_samples, .. } => {
                assert_eq!(clipped_samples, 44100);
            },
            _ => panic!("Expected Clipped diagnosis"),
        }
        let _ = std::fs::remove_file(test_file);
    }
}
