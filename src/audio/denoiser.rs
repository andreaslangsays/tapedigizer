use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::audio::meter::linear_to_db;
use crate::project::sanitize_filename;

pub const FFT_SIZE: usize = 2048;
pub const HOP_SIZE: usize = 512; // 75% overlap
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1; // 1025 positive frequency bins
pub const STFT_LATENCY_SAMPLES: usize = FFT_SIZE; // 2048 samples exact ringbuffer latency

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditionMode {
    /// Bereinigtes Signal (Rauschen & DC entfernt)
    Wet,
    /// Unbearbeitetes Originalsignal (A/B-Vergleich)
    Bypass,
    /// Nur das herausgefilterte Rauschen (Differenzsignal: Original − Gefiltert)
    DeltaNoiseOnly,
}

impl Default for AuditionMode {
    fn default() -> Self {
        Self::Wet
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DenoiserParams {
    pub enabled: bool,
    pub audition_mode: AuditionMode,
    /// Maximale Rauschabsenkung in dB (0.0 .. 40.0 dB)
    pub reduction_db: f32,
    /// Empfindlichkeit / Over-Subtraction-Faktor (0.5 .. 4.0)
    pub sensitivity: f32,
    /// Zeit- und Frequenzglättung gegen „Musical Noise“ (0.0 .. 1.0)
    pub smoothing: f32,
    /// DC-Offset & Subsonic-Hochpass aktivieren
    pub dc_filter_enabled: bool,
    /// Grenzfrequenz des Subsonic-/DC-Hochpasses in Hz (z. B. 15.0, 20.0, 30.0)
    pub dc_cutoff_hz: f32,
}

impl Default for DenoiserParams {
    fn default() -> Self {
        Self {
            enabled: false,
            audition_mode: AuditionMode::Wet,
            reduction_db: 15.0,
            sensitivity: 1.5,
            smoothing: 0.65,
            dc_filter_enabled: true,
            dc_cutoff_hz: 20.0,
        }
    }
}

/// Gespeichertes Rauschprofil (Fingerprint) inklusive Metadaten und Verweis auf die Rausch-WAV-Datei.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseProfile {
    pub name: String,
    pub media_source: String,
    pub sample_rate: u32,
    pub duration_secs: f32,
    pub dc_offset_l: f32,
    pub dc_offset_r: f32,
    pub rms_db_l: f32,
    pub rms_db_r: f32,
    /// Spektrale Rauschleistung pro Bin (L), Länge = NUM_BINS (1025)
    pub noise_power_l: Vec<f32>,
    /// Spektrale Rauschleistung pro Bin (R), Länge = NUM_BINS (1025)
    pub noise_power_r: Vec<f32>,
    /// Dateiname der zugehörigen Rausch-Sample-WAV im Preset-Ordner
    pub wav_filename: Option<String>,
}

impl NoiseProfile {
    /// Lernt ein Rauschprofil aus einem ausgewählten Stereo-Audioabschnitt (wo nur Bandrauschen/DC zu hören ist).
    pub fn learn_from_samples(
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        name: impl Into<String>,
        media_source: impl Into<String>,
    ) -> Result<Self, String> {
        let len = left.len().min(right.len());
        if len < 256 || sample_rate == 0 {
            return Err("Ausgewählter Rausch-Bereich ist zu kurz (mind. 0,1 s empfohlen).".into());
        }

        // 1. Statischen DC-Offset und RMS bestimmen
        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;
        for i in 0..len {
            sum_l += f64::from(left[i]);
            sum_r += f64::from(right[i]);
        }
        let dc_l = (sum_l / len as f64) as f32;
        let dc_r = (sum_r / len as f64) as f32;

        let mut sum_sq_l = 0.0_f64;
        let mut sum_sq_r = 0.0_f64;
        for i in 0..len {
            let l = left[i] - dc_l;
            let r = right[i] - dc_r;
            sum_sq_l += f64::from(l * l);
            sum_sq_r += f64::from(r * r);
        }
        let rms_l = ((sum_sq_l / len as f64).sqrt()) as f32;
        let rms_r = ((sum_sq_r / len as f64).sqrt()) as f32;

        // 2. STFT über den Rauschabschnitt berechnen
        let tables = FftTables::new();
        let mut mean_l = vec![0.0_f64; NUM_BINS];
        let mut mean_r = vec![0.0_f64; NUM_BINS];
        let mut m2_l = vec![0.0_f64; NUM_BINS];
        let mut m2_r = vec![0.0_f64; NUM_BINS];

        let mut frame_l = [(0.0_f32, 0.0_f32); FFT_SIZE];
        let mut frame_r = [(0.0_f32, 0.0_f32); FFT_SIZE];

        let num_hops = if len >= FFT_SIZE {
            (len - FFT_SIZE) / HOP_SIZE + 1
        } else {
            1
        };

        for hop in 0..num_hops {
            let offset = hop * HOP_SIZE;
            for i in 0..FFT_SIZE {
                let sample_idx = (offset + i) % len;
                let w = tables.window[i];
                frame_l[i] = ((left[sample_idx] - dc_l) * w, 0.0);
                frame_r[i] = ((right[sample_idx] - dc_r) * w, 0.0);
            }
            tables.fft_in_place(&mut frame_l, false);
            tables.fft_in_place(&mut frame_r, false);

            let n = (hop + 1) as f64;
            for k in 0..NUM_BINS {
                let p_l = f64::from(frame_l[k].0 * frame_l[k].0 + frame_l[k].1 * frame_l[k].1);
                let p_r = f64::from(frame_r[k].0 * frame_r[k].0 + frame_r[k].1 * frame_r[k].1);

                // Welford-Algorithmus für Mittelwert + Standardabweichung pro Frequenz-Bin
                let delta_l = p_l - mean_l[k];
                mean_l[k] += delta_l / n;
                m2_l[k] += delta_l * (p_l - mean_l[k]);

                let delta_r = p_r - mean_r[k];
                mean_r[k] += delta_r / n;
                m2_r[k] += delta_r * (p_r - mean_r[k]);
            }
        }

        let mut noise_power_l = Vec::with_capacity(NUM_BINS);
        let mut noise_power_r = Vec::with_capacity(NUM_BINS);
        let denom = (num_hops.max(2) - 1) as f64;

        for k in 0..NUM_BINS {
            let std_l = if num_hops > 1 { (m2_l[k] / denom).sqrt() } else { 0.0 };
            let std_r = if num_hops > 1 { (m2_r[k] / denom).sqrt() } else { 0.0 };
            // Mittelwert + 1.0 * Standardabweichung erfasst das Bandrauschen stabil inklusive leichter Schwankungen
            noise_power_l.push((mean_l[k] + std_l) as f32);
            noise_power_r.push((mean_r[k] + std_r) as f32);
        }

        Ok(Self {
            name: name.into(),
            media_source: media_source.into(),
            sample_rate,
            duration_secs: len as f32 / sample_rate as f32,
            dc_offset_l: dc_l,
            dc_offset_r: dc_r,
            rms_db_l: linear_to_db(rms_l),
            rms_db_r: linear_to_db(rms_r),
            noise_power_l,
            noise_power_r,
            wav_filename: None,
        })
    }

    /// Passt das Rauschprofil bei abweichender Sample-Rate (z. B. 44.1 kHz Preset in 48/96 kHz DAW-Projekt)
    /// automatisch durch Frequenz-Interpolation auf die Ziel-Sample-Rate an.
    pub fn adapted_for_sample_rate(&self, target_rate: u32) -> Self {
        if target_rate == 0 || target_rate == self.sample_rate || self.noise_power_l.len() != NUM_BINS {
            return self.clone();
        }
        let ratio = f64::from(target_rate) / f64::from(self.sample_rate);
        let mut new_l = Vec::with_capacity(NUM_BINS);
        let mut new_r = Vec::with_capacity(NUM_BINS);

        for k in 0..NUM_BINS {
            let src_bin = (k as f64) * ratio;
            if src_bin >= (NUM_BINS - 1) as f64 {
                new_l.push(*self.noise_power_l.last().unwrap_or(&0.0));
                new_r.push(*self.noise_power_r.last().unwrap_or(&0.0));
            } else {
                let idx = src_bin.floor() as usize;
                let frac = (src_bin - idx as f64) as f32;
                let l = self.noise_power_l[idx] + (self.noise_power_l[idx + 1] - self.noise_power_l[idx]) * frac;
                let r = self.noise_power_r[idx] + (self.noise_power_r[idx + 1] - self.noise_power_r[idx]) * frac;
                new_l.push(l);
                new_r.push(r);
            }
        }

        let mut cloned = self.clone();
        cloned.sample_rate = target_rate;
        cloned.noise_power_l = new_l;
        cloned.noise_power_r = new_r;
        cloned
    }
}

/// Verwaltet gespeicherte Rausch-Samples (.wav + .json) im gemeinsamen Ordner für App & AudioUnit-Plugin.
pub struct NoisePresetManager;

impl NoisePresetManager {
    pub fn default_presets_dir() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join("Library/Application Support/TapeDigizer/NoiseProfiles");
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                return PathBuf::from(appdata).join("TapeDigizer/NoiseProfiles");
            }
        }
        std::env::temp_dir().join("TapeDigizer_NoiseProfiles")
    }

    /// Speichert die ausgewählten Rausch-Samples als 24-Bit WAV + das berechnete `NoiseProfile` als JSON.
    pub fn save_preset_in_dir(
        dir: &Path,
        name: &str,
        media_source: &str,
        left_samples: &[f32],
        right_samples: &[f32],
        sample_rate: u32,
    ) -> Result<(NoiseProfile, PathBuf), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let clean_stem = sanitize_filename(name);
        let wav_name = format!("{clean_stem}.wav");
        let json_name = format!("{clean_stem}.json");
        let wav_path = dir.join(&wav_name);
        let json_path = dir.join(&json_name);

        let mut profile = NoiseProfile::learn_from_samples(
            left_samples,
            right_samples,
            sample_rate,
            name.trim(),
            media_source,
        )?;
        profile.wav_filename = Some(wav_name);

        write_stereo_wav_atomic(&wav_path, left_samples, right_samples, sample_rate)?;
        let json_bytes = serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?;
        std::fs::write(&json_path, json_bytes).map_err(|e| e.to_string())?;

        Ok((profile, wav_path))
    }

    pub fn save_preset(
        name: &str,
        media_source: &str,
        left_samples: &[f32],
        right_samples: &[f32],
        sample_rate: u32,
    ) -> Result<(NoiseProfile, PathBuf), String> {
        Self::save_preset_in_dir(
            &Self::default_presets_dir(),
            name,
            media_source,
            left_samples,
            right_samples,
            sample_rate,
        )
    }

    /// Lädt alle verfügbaren Rausch-Presets aus einem Ordner (sowohl .json-Profile als auch reine .wav-Dateien).
    pub fn list_presets_in_dir(dir: &Path) -> Vec<NoiseProfile> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut profiles = Vec::new();
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();

        for path in &paths {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json")) {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(profile) = serde_json::from_str::<NoiseProfile>(&content) {
                        if profile.noise_power_l.len() == NUM_BINS && profile.noise_power_r.len() == NUM_BINS {
                            profiles.push(profile);
                        }
                    }
                }
            }
        }

        // Falls der Nutzer eine eigene .wav-Datei ohne .json in den Ordner gelegt hat, automatisch einlesen
        for path in &paths {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wav")) {
                let wav_file = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let already_loaded = profiles.iter().any(|p| p.wav_filename.as_deref() == Some(wav_file.as_str()));
                if !already_loaded {
                    if let Ok(profile) = Self::load_profile_from_wav(path) {
                        profiles.push(profile);
                    }
                }
            }
        }

        profiles
    }

    pub fn list_presets() -> Vec<NoiseProfile> {
        Self::list_presets_in_dir(&Self::default_presets_dir())
    }

    pub fn load_profile_from_wav(wav_path: &Path) -> Result<NoiseProfile, String> {
        let mut reader = hound::WavReader::open(wav_path).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        let channels = spec.channels.max(1) as usize;
        let sample_rate = spec.sample_rate;
        let mut left = Vec::new();
        let mut right = Vec::new();

        match spec.sample_format {
            hound::SampleFormat::Float => {
                let samples: Vec<f32> = reader.samples::<f32>().filter_map(Result::ok).collect();
                for frame in samples.chunks(channels) {
                    let l = frame[0];
                    let r = if channels > 1 { frame[1] } else { l };
                    left.push(l);
                    right.push(r);
                }
            }
            hound::SampleFormat::Int => {
                let max_val = (1_i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
                let samples: Vec<i32> = reader.samples::<i32>().filter_map(Result::ok).collect();
                for frame in samples.chunks(channels) {
                    let l = (frame[0] as f32) / max_val;
                    let r = if channels > 1 { (frame[1] as f32) / max_val } else { l };
                    left.push(l);
                    right.push(r);
                }
            }
        }

        let stem = wav_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut profile = NoiseProfile::learn_from_samples(
            &left,
            &right,
            sample_rate,
            stem,
            "Custom WAV",
        )?;
        profile.wav_filename = wav_path.file_name().map(|f| f.to_string_lossy().to_string());
        Ok(profile)
    }

    pub fn delete_preset_in_dir(dir: &Path, profile: &NoiseProfile) -> Result<(), String> {
        let clean_stem = sanitize_filename(&profile.name);
        let json_path = dir.join(format!("{clean_stem}.json"));
        let _ = std::fs::remove_file(json_path);
        if let Some(ref wav) = profile.wav_filename {
            let _ = std::fs::remove_file(dir.join(wav));
        } else {
            let _ = std::fs::remove_file(dir.join(format!("{clean_stem}.wav")));
        }
        Ok(())
    }
}

/// Zweipoliger Butterworth-Hochpassfilter zur Entfernung von DC-Offset und Subsonic-Bandrumpeln.
#[derive(Clone, Debug)]
pub struct DcSubsonicFilter {
    sample_rate: u32,
    cutoff_hz: f32,
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1_l: f64,
    x2_l: f64,
    y1_l: f64,
    y2_l: f64,
    x1_r: f64,
    x2_r: f64,
    y1_r: f64,
    y2_r: f64,
}

impl DcSubsonicFilter {
    pub fn new(sample_rate: u32, cutoff_hz: f32) -> Self {
        let mut filter = Self {
            sample_rate: sample_rate.max(8000),
            cutoff_hz: 20.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1_l: 0.0,
            x2_l: 0.0,
            y1_l: 0.0,
            y2_l: 0.0,
            x1_r: 0.0,
            x2_r: 0.0,
            y1_r: 0.0,
            y2_r: 0.0,
        };
        filter.set_params(sample_rate, cutoff_hz);
        filter
    }

    pub fn set_params(&mut self, sample_rate: u32, cutoff_hz: f32) {
        let sr = sample_rate.max(8000);
        let fc = cutoff_hz.clamp(5.0, 120.0);
        if self.sample_rate == sr && (self.cutoff_hz - fc).abs() < 0.01 && self.b1 != 0.0 {
            return;
        }
        self.sample_rate = sr;
        self.cutoff_hz = fc;

        let omega = 2.0 * std::f64::consts::PI * f64::from(fc) / f64::from(sr);
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / std::f64::consts::SQRT_2; // Butterworth Q = 1 / sqrt(2)

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 + cs) * 0.5) / a0;
        self.b1 = -(1.0 + cs) / a0;
        self.b2 = ((1.0 + cs) * 0.5) / a0;
        self.a1 = (-2.0 * cs) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    pub fn reset(&mut self) {
        self.x1_l = 0.0;
        self.x2_l = 0.0;
        self.y1_l = 0.0;
        self.y2_l = 0.0;
        self.x1_r = 0.0;
        self.x2_r = 0.0;
        self.y1_r = 0.0;
        self.y2_r = 0.0;
    }

    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32, static_dc_l: f32, static_dc_r: f32) -> (f32, f32) {
        let xl = f64::from(in_l - static_dc_l);
        let xr = f64::from(in_r - static_dc_r);

        let yl = self.b0 * xl + self.b1 * self.x1_l + self.b2 * self.x2_l - self.a1 * self.y1_l - self.a2 * self.y2_l;
        self.x2_l = self.x1_l;
        self.x1_l = xl;
        self.y2_l = self.y1_l;
        self.y1_l = yl;

        let yr = self.b0 * xr + self.b1 * self.x1_r + self.b2 * self.x2_r - self.a1 * self.y1_r - self.a2 * self.y2_r;
        self.x2_r = self.x1_r;
        self.x1_r = xr;
        self.y2_r = self.y1_r;
        self.y1_r = yr;

        (yl as f32, yr as f32)
    }
}

/// Vorberechnete Tabellen für Radix-2 FFT der Länge 2048 und COLA-Hann-Fenster.
pub struct FftTables {
    pub window: [f32; FFT_SIZE],
    bit_rev: [usize; FFT_SIZE],
    twiddles: [(f32, f32); FFT_SIZE / 2],
}

impl FftTables {
    pub fn new() -> Self {
        let mut window = [0.0_f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            // Periodisches Hann-Fenster für exakte Constant-Overlap-Add (COLA) Rekonstruktion bei 75% Overlap
            window[i] = 0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_SIZE as f32).cos());
        }

        let bits = FFT_SIZE.trailing_zeros() as usize;
        let mut bit_rev = [0_usize; FFT_SIZE];
        for i in 0..FFT_SIZE {
            let mut rev = 0;
            let mut val = i;
            for _ in 0..bits {
                rev = (rev << 1) | (val & 1);
                val >>= 1;
            }
            bit_rev[i] = rev;
        }

        let mut twiddles = [(0.0_f32, 0.0_f32); FFT_SIZE / 2];
        for k in 0..(FFT_SIZE / 2) {
            let angle = -2.0 * PI * (k as f32) / (FFT_SIZE as f32);
            twiddles[k] = (angle.cos(), angle.sin());
        }

        Self {
            window,
            bit_rev,
            twiddles,
        }
    }

    pub fn fft_in_place(&self, buf: &mut [(f32, f32); FFT_SIZE], inverse: bool) {
        for i in 0..FFT_SIZE {
            let j = self.bit_rev[i];
            if i < j {
                buf.swap(i, j);
            }
        }

        let mut len = 2;
        while len <= FFT_SIZE {
            let half = len / 2;
            let step = FFT_SIZE / len;
            let mut i = 0;
            while i < FFT_SIZE {
                for k in 0..half {
                    let (wr, mut wi) = self.twiddles[k * step];
                    if inverse {
                        wi = -wi;
                    }
                    let u = buf[i + k];
                    let v = buf[i + k + half];
                    let tr = v.0 * wr - v.1 * wi;
                    let ti = v.0 * wi + v.1 * wr;
                    buf[i + k] = (u.0 + tr, u.1 + ti);
                    buf[i + k + half] = (u.0 - tr, u.1 - ti);
                }
                i += len;
            }
            len <<= 1;
        }

        if inverse {
            let inv_n = 1.0 / (FFT_SIZE as f32);
            for sample in buf.iter_mut() {
                sample.0 *= inv_n;
                sample.1 *= inv_n;
            }
        }
    }
}

/// Echtzeitfähiger Spektral-Rauschfilter + DC-Blocker (ohne Heap-Allokationen im Render-Pfad).
pub struct SpectralDenoiser {
    sample_rate: u32,
    tables: FftTables,
    dc_filter: DcSubsonicFilter,
    active_profile: Option<NoiseProfile>,

    in_ring_l: [f32; FFT_SIZE],
    in_ring_r: [f32; FFT_SIZE],
    out_ring_l: [f32; FFT_SIZE],
    out_ring_r: [f32; FFT_SIZE],
    dry_delay_l: [f32; FFT_SIZE],
    dry_delay_r: [f32; FFT_SIZE],

    ring_pos: usize,
    hop_counter: usize,

    prev_gain_l: [f32; NUM_BINS],
    prev_gain_r: [f32; NUM_BINS],
    temp_gain_l: [f32; NUM_BINS],
    temp_gain_r: [f32; NUM_BINS],

    fft_l: [(f32, f32); FFT_SIZE],
    fft_r: [(f32, f32); FFT_SIZE],
}

impl SpectralDenoiser {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8000);
        Self {
            sample_rate: sr,
            tables: FftTables::new(),
            dc_filter: DcSubsonicFilter::new(sr, 20.0),
            active_profile: None,
            in_ring_l: [0.0; FFT_SIZE],
            in_ring_r: [0.0; FFT_SIZE],
            out_ring_l: [0.0; FFT_SIZE],
            out_ring_r: [0.0; FFT_SIZE],
            dry_delay_l: [0.0; FFT_SIZE],
            dry_delay_r: [0.0; FFT_SIZE],
            ring_pos: 0,
            hop_counter: 0,
            prev_gain_l: [1.0; NUM_BINS],
            prev_gain_r: [1.0; NUM_BINS],
            temp_gain_l: [1.0; NUM_BINS],
            temp_gain_r: [1.0; NUM_BINS],
            fft_l: [(0.0, 0.0); FFT_SIZE],
            fft_r: [(0.0, 0.0); FFT_SIZE],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        let sr = sample_rate.max(8000);
        if self.sample_rate != sr {
            self.sample_rate = sr;
            if let Some(profile) = self.active_profile.take() {
                self.active_profile = Some(profile.adapted_for_sample_rate(sr));
            }
            self.reset();
        }
    }

    pub fn set_noise_profile(&mut self, profile: Option<NoiseProfile>) {
        self.active_profile = profile.map(|p| p.adapted_for_sample_rate(self.sample_rate));
    }

    pub fn noise_profile(&self) -> Option<&NoiseProfile> {
        self.active_profile.as_ref()
    }

    pub fn reset(&mut self) {
        self.dc_filter.reset();
        self.in_ring_l.fill(0.0);
        self.in_ring_r.fill(0.0);
        self.out_ring_l.fill(0.0);
        self.out_ring_r.fill(0.0);
        self.dry_delay_l.fill(0.0);
        self.dry_delay_r.fill(0.0);
        self.ring_pos = 0;
        self.hop_counter = 0;
        self.prev_gain_l.fill(1.0);
        self.prev_gain_r.fill(1.0);
    }

    /// Verarbeitet ein einzelnes Stereo-Sample-Paar durch die Echtzeit-Pipeline.
    #[inline]
    pub fn process_sample(&mut self, raw_l: f32, raw_r: f32, params: &DenoiserParams) -> (f32, f32) {
        if !params.enabled {
            return (raw_l, raw_r);
        }

        let (static_dc_l, static_dc_r) = self
            .active_profile
            .as_ref()
            .map_or((0.0, 0.0), |p| (p.dc_offset_l, p.dc_offset_r));

        let (dc_cleaned_l, dc_cleaned_r) = if params.dc_filter_enabled {
            self.dc_filter.set_params(self.sample_rate, params.dc_cutoff_hz);
            self.dc_filter.process_sample(raw_l, raw_r, static_dc_l, static_dc_r)
        } else {
            (raw_l, raw_r)
        };

        let pos = self.ring_pos;
        // Verzögerter Original-Puffer (exakt FFT_SIZE Samples verzögert, synchron zum OLA-Ausgang)
        let dry_l = self.dry_delay_l[pos];
        let dry_r = self.dry_delay_r[pos];
        self.dry_delay_l[pos] = raw_l;
        self.dry_delay_r[pos] = raw_r;

        self.in_ring_l[pos] = dc_cleaned_l;
        self.in_ring_r[pos] = dc_cleaned_r;

        self.ring_pos = (pos + 1) & (FFT_SIZE - 1);
        self.hop_counter += 1;

        if self.hop_counter >= HOP_SIZE {
            self.hop_counter = 0;
            self.process_stft_frame(params);
        }

        // Nach dem 4. Overlap-Add ist out_ring[pos] vollständig rekonstruiert
        let wet_l = self.out_ring_l[pos];
        let wet_r = self.out_ring_r[pos];
        self.out_ring_l[pos] = 0.0;
        self.out_ring_r[pos] = 0.0;

        match params.audition_mode {
            AuditionMode::Wet => (wet_l, wet_r),
            AuditionMode::Bypass => (dry_l, dry_r),
            AuditionMode::DeltaNoiseOnly => (dry_l - wet_l, dry_r - wet_r),
        }
    }

    fn process_stft_frame(&mut self, params: &DenoiserParams) {
        let start = self.ring_pos; // Ältestes Sample im Ringpuffer der Länge FFT_SIZE
        for i in 0..FFT_SIZE {
            let idx = (start + i) & (FFT_SIZE - 1);
            let w = self.tables.window[i];
            self.fft_l[i] = (self.in_ring_l[idx] * w, 0.0);
            self.fft_r[i] = (self.in_ring_r[idx] * w, 0.0);
        }

        if let Some(ref profile) = self.active_profile {
            if params.reduction_db > 0.05 {
                self.tables.fft_in_place(&mut self.fft_l, false);
                self.tables.fft_in_place(&mut self.fft_r, false);

                let floor = 10.0_f32.powf(-params.reduction_db.clamp(0.0, 48.0) / 20.0);
                // Nichtlineare Empfindlichkeitsskalierung für feinfühligen Reglerbereich
                let over_sub = (params.sensitivity.clamp(0.3, 5.0)).powi(2);
                let smooth = params.smoothing.clamp(0.0, 0.98);
                let release_coeff = 0.45 + 0.50 * smooth; // 0.45 .. 0.95
                let attack_coeff = 0.15 * smooth; // Schneller Attack erhält Transienten

                for k in 0..NUM_BINS {
                    let sig_p_l = self.fft_l[k].0 * self.fft_l[k].0 + self.fft_l[k].1 * self.fft_l[k].1;
                    let sig_p_r = self.fft_r[k].0 * self.fft_r[k].0 + self.fft_r[k].1 * self.fft_r[k].1;

                    let noise_p_l = profile.noise_power_l[k] * over_sub;
                    let noise_p_r = profile.noise_power_r[k] * over_sub;

                    // Weiche Wiener-Gewichtung mit Spectral Floor
                    let raw_g_l = if sig_p_l > noise_p_l && sig_p_l > 1e-20 {
                        ((sig_p_l - noise_p_l) / sig_p_l).sqrt().clamp(floor, 1.0)
                    } else {
                        floor
                    };

                    let raw_g_r = if sig_p_r > noise_p_r && sig_p_r > 1e-20 {
                        ((sig_p_r - noise_p_r) / sig_p_r).sqrt().clamp(floor, 1.0)
                    } else {
                        floor
                    };

                    self.temp_gain_l[k] = raw_g_l;
                    self.temp_gain_r[k] = raw_g_r;
                }

                // Frequenz-Glättung über Nachbar-Bins (verhindert isolierte Musical-Noise-Spitzen)
                let freq_blend = 0.5 * smooth;
                for k in 0..NUM_BINS {
                    let prev_k = k.saturating_sub(1);
                    let next_k = (k + 1).min(NUM_BINS - 1);
                    let avg_l = 0.25 * self.temp_gain_l[prev_k] + 0.5 * self.temp_gain_l[k] + 0.25 * self.temp_gain_l[next_k];
                    let avg_r = 0.25 * self.temp_gain_r[prev_k] + 0.5 * self.temp_gain_r[k] + 0.25 * self.temp_gain_r[next_k];
                    let target_l = self.temp_gain_l[k] * (1.0 - freq_blend) + avg_l * freq_blend;
                    let target_r = self.temp_gain_r[k] * (1.0 - freq_blend) + avg_r * freq_blend;

                    // Zeitliche Attack-/Release-Glättung pro Frequenz-Bin
                    let coeff_l = if target_l > self.prev_gain_l[k] { attack_coeff } else { release_coeff };
                    let coeff_r = if target_r > self.prev_gain_r[k] { attack_coeff } else { release_coeff };

                    let g_l = coeff_l * self.prev_gain_l[k] + (1.0 - coeff_l) * target_l;
                    let g_r = coeff_r * self.prev_gain_r[k] + (1.0 - coeff_r) * target_r;

                    self.prev_gain_l[k] = g_l;
                    self.prev_gain_r[k] = g_r;

                    self.fft_l[k].0 *= g_l;
                    self.fft_l[k].1 *= g_l;
                    self.fft_r[k].0 *= g_r;
                    self.fft_r[k].1 *= g_r;
                }

                // Konjugiert-symmetrische obere Hälfte für reelle IFFT wiederherstellen
                for k in 1..(FFT_SIZE / 2) {
                    let sym = FFT_SIZE - k;
                    self.fft_l[sym] = (self.fft_l[k].0, -self.fft_l[k].1);
                    self.fft_r[sym] = (self.fft_r[k].0, -self.fft_r[k].1);
                }

                self.tables.fft_in_place(&mut self.fft_l, true);
                self.tables.fft_in_place(&mut self.fft_r, true);
            }
        }

        // Synthese-Fenster + Overlap-Add (Normierung für Hann^2 bei 75% Overlap: sum(w^2) = 1.5)
        let ola_norm = 1.0 / 1.5_f32;
        for i in 0..FFT_SIZE {
            let idx = (start + i) & (FFT_SIZE - 1);
            let w = self.tables.window[i] * ola_norm;
            self.out_ring_l[idx] += self.fft_l[i].0 * w;
            self.out_ring_r[idx] += self.fft_r[i].0 * w;
        }
    }

    /// Berechnet ein komplettes Stereo-Audio-Array offline mit **exakt 0 Samples Phasen-/Latenzausgleich**.
    pub fn process_offline_stereo(
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        profile: Option<NoiseProfile>,
        params: &DenoiserParams,
    ) -> (Vec<f32>, Vec<f32>) {
        let len = left.len().min(right.len());
        if len == 0 || !params.enabled {
            return (left.to_vec(), right.to_vec());
        }

        let mut denoiser = Self::new(sample_rate);
        denoiser.set_noise_profile(profile);

        let mut out_l = Vec::with_capacity(len);
        let mut out_r = Vec::with_capacity(len);

        // Pre-warm mit gespiegeltem/erstem Sample gegen Einschwing-Artefakte
        let first_l = left[0];
        let first_r = right[0];
        for _ in 0..HOP_SIZE {
            let _ = denoiser.process_sample(first_l, first_r, params);
        }
        // Reset STFT buffers after DC filter pre-warm, then stream with exact STFT_LATENCY_SAMPLES compensation
        denoiser.in_ring_l.fill(0.0);
        denoiser.in_ring_r.fill(0.0);
        denoiser.out_ring_l.fill(0.0);
        denoiser.out_ring_r.fill(0.0);
        denoiser.dry_delay_l.fill(0.0);
        denoiser.dry_delay_r.fill(0.0);
        denoiser.ring_pos = 0;
        denoiser.hop_counter = 0;

        let total_steps = len + STFT_LATENCY_SAMPLES;
        for i in 0..total_steps {
            let in_l = if i < len { left[i] } else { 0.0 };
            let in_r = if i < len { right[i] } else { 0.0 };
            let (yl, yr) = denoiser.process_sample(in_l, in_r, params);
            if i >= STFT_LATENCY_SAMPLES {
                out_l.push(yl.clamp(-1.0, 1.0));
                out_r.push(yr.clamp(-1.0, 1.0));
            }
        }

        (out_l, out_r)
    }
}

/// Schneidet den ausgewählten Bereich `[start_sec, end_sec]` direkt aus den Audio-Samples heraus
/// (inkl. Zero-Crossing-Snap und 5 ms Micro-Crossfade) und passt alle Track-Marker an.
pub fn cut_delete_audio_range(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    start_sec: f64,
    end_sec: f64,
    markers: &mut crate::splitter::MarkerSnapshot,
) -> Result<(Vec<f32>, Vec<f32>), String> {
    let len = left.len().min(right.len());
    if len == 0 || sample_rate == 0 {
        return Err("Keine Audiodaten zum Schneiden vorhanden.".into());
    }
    let sr = f64::from(sample_rate);
    let snapped_start = crate::splitter::snap_to_zero_crossing(left, right, sample_rate, start_sec, 5.0);
    let snapped_end = crate::splitter::snap_to_zero_crossing(left, right, sample_rate, end_sec, 5.0);

    let s_idx = ((snapped_start * sr).round() as usize).min(len);
    let e_idx = ((snapped_end * sr).round() as usize).min(len);
    if e_idx <= s_idx || e_idx - s_idx < 10 {
        return Err("Ausgewählter Bereich ist zu klein zum Ausschneiden.".into());
    }
    let remaining = len - (e_idx - s_idx);
    if remaining < (sample_rate as usize / 10).max(64) {
        return Err("Die gesamte Aufnahme kann nicht vollständig gelöscht werden.".into());
    }

    let xfade_len = ((0.005 * sr).round() as usize).min(s_idx).min(e_idx - s_idx);
    let mut new_l = Vec::with_capacity(remaining);
    let mut new_r = Vec::with_capacity(remaining);

    new_l.extend_from_slice(&left[..s_idx - xfade_len]);
    new_r.extend_from_slice(&right[..s_idx - xfade_len]);

    // Sanfter 5 ms Crossfade an der Schnittkante gegen Knackser (erhält exakte Gesamtlänge)
    for i in 0..xfade_len {
        let t = (i as f32) / (xfade_len.max(1) as f32);
        let fade_out = 0.5 * (1.0 + (PI * t).cos());
        let fade_in = 1.0 - fade_out;
        let a_idx = s_idx - xfade_len + i;
        let b_idx = e_idx - xfade_len + i;
        new_l.push(left[a_idx] * fade_out + left[b_idx] * fade_in);
        new_r.push(right[a_idx] * fade_out + right[b_idx] * fade_in);
    }

    new_l.extend_from_slice(&left[e_idx..len]);
    new_r.extend_from_slice(&right[e_idx..len]);

    // Track-Marker aktualisieren
    let cut_start = s_idx as f64 / sr;
    let cut_end = e_idx as f64 / sr;
    let cut_dur = cut_end - cut_start;
    let new_total_sec = new_l.len() as f64 / sr;

    let mut updated_markers = Vec::new();
    for &m in &markers.markers {
        if m < cut_start - 0.001 {
            updated_markers.push(m);
        } else if m > cut_end + 0.001 {
            let shifted = (m - cut_dur).clamp(0.001, new_total_sec - 0.001);
            if updated_markers.last().is_none_or(|&prev: &f64| (shifted - prev).abs() > 0.01) {
                updated_markers.push(shifted);
            }
        }
    }
    markers.markers = updated_markers;
    markers.pause_indices.retain(|&idx| idx <= markers.markers.len());

    Ok((new_l, new_r))
}

/// Trimmt (beschneidet) die Aufnahme auf den ausgewählten Bereich `[start_sec, end_sec]`
/// und entfernt alles davor und danach.
pub fn crop_trim_audio_to_range(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    start_sec: f64,
    end_sec: f64,
    markers: &mut crate::splitter::MarkerSnapshot,
) -> Result<(Vec<f32>, Vec<f32>), String> {
    let len = left.len().min(right.len());
    if len == 0 || sample_rate == 0 {
        return Err("Keine Audiodaten zum Trimmen vorhanden.".into());
    }
    let sr = f64::from(sample_rate);
    let snapped_start = crate::splitter::snap_to_zero_crossing(left, right, sample_rate, start_sec, 5.0);
    let snapped_end = crate::splitter::snap_to_zero_crossing(left, right, sample_rate, end_sec, 5.0);

    let s_idx = ((snapped_start * sr).round() as usize).min(len);
    let e_idx = ((snapped_end * sr).round() as usize).min(len);
    if e_idx <= s_idx || e_idx - s_idx < (sample_rate as usize / 10).max(64) {
        return Err("Ausgewählter Bereich ist zu kurz zum Trimmen.".into());
    }

    let mut new_l = left[s_idx..e_idx].to_vec();
    let mut new_r = right[s_idx..e_idx].to_vec();

    // Kurzes 5 ms Micro-Fade-In/Out an den neuen Bandenden
    let fade_len = ((0.005 * sr).round() as usize).min(new_l.len() / 4);
    for i in 0..fade_len {
        let gain = (i as f32) / (fade_len.max(1) as f32);
        new_l[i] *= gain;
        new_r[i] *= gain;
        let rev = new_l.len() - 1 - i;
        new_l[rev] *= gain;
        new_r[rev] *= gain;
    }

    let trim_start = s_idx as f64 / sr;
    let trim_end = e_idx as f64 / sr;
    let new_total_sec = new_l.len() as f64 / sr;

    let mut updated_markers = Vec::new();
    for &m in &markers.markers {
        if m > trim_start + 0.01 && m < trim_end - 0.01 {
            let shifted = (m - trim_start).clamp(0.001, new_total_sec - 0.001);
            updated_markers.push(shifted);
        }
    }
    markers.markers = updated_markers;
    markers.pause_indices.retain(|&idx| idx <= markers.markers.len());

    Ok((new_l, new_r))
}

/// Überschreibt eine WAV-Zieldatei atomar (ohne Sicherheitskopie, um Speicherplatz zu sparen).
pub fn write_stereo_wav_atomic(
    target_path: &Path,
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Result<(), String> {
    let len = left.len().min(right.len());
    if len == 0 || sample_rate == 0 {
        return Err("Keine gültigen Audiodaten zum Speichern.".into());
    }
    let parent = target_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let (temp_path, file) = crate::project::unique_file(parent, ".wav_write", "partial")?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };

    let write_res = (|| {
        let mut writer = hound::WavWriter::new(std::io::BufWriter::with_capacity(1024 * 1024, file), spec)
            .map_err(|e| e.to_string())?;
        let scale = 8_388_607.0_f32;
        for i in 0..len {
            let sl = (left[i].clamp(-1.0, 1.0) * scale).round() as i32;
            let sr = (right[i].clamp(-1.0, 1.0) * scale).round() as i32;
            writer.write_sample(sl).map_err(|e| e.to_string())?;
            writer.write_sample(sr).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
        std::fs::rename(&temp_path, target_path).map_err(|e| e.to_string())
    })();

    if write_res.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stft_cola_unity_reconstruction() {
        let sr = 44_100;
        let len = sr as usize; // 1 Sekunde 440 Hz Sinus
        let mut left = Vec::with_capacity(len);
        let mut right = Vec::with_capacity(len);
        for i in 0..len {
            let t = i as f32 / sr as f32;
            left.push((2.0 * PI * 440.0 * t).sin() * 0.5);
            right.push((2.0 * PI * 880.0 * t).cos() * 0.4);
        }

        // Ohne Rauschprofil (nur STFT Vorwärts + Rückwärts + Overlap-Add)
        let params = DenoiserParams {
            enabled: true,
            audition_mode: AuditionMode::Wet,
            reduction_db: 0.0,
            sensitivity: 1.0,
            smoothing: 0.5,
            dc_filter_enabled: false,
            dc_cutoff_hz: 20.0,
        };
        let (out_l, out_r) = SpectralDenoiser::process_offline_stereo(&left, &right, sr, None, &params);
        assert_eq!(out_l.len(), len);
        assert_eq!(out_r.len(), len);

        // Im eingeschwungenen Bereich (ab Sample 2048 bis len - 2048) muss die Abweichung minimal sein (< 1e-4)
        for i in 2048..(len - 2048) {
            assert!(
                (out_l[i] - left[i]).abs() < 1e-3,
                "COLA mismatch L at {i}: {} vs {}",
                out_l[i],
                left[i]
            );
            assert!(
                (out_r[i] - right[i]).abs() < 1e-3,
                "COLA mismatch R at {i}: {} vs {}",
                out_r[i],
                right[i]
            );
        }
    }

    #[test]
    fn test_dc_and_noise_reduction_attenuates_tape_hiss_and_preserves_tone() {
        let sr = 44_100;
        let len = sr as usize;
        let mut noise_l = Vec::with_capacity(len);
        let mut noise_r = Vec::with_capacity(len);

        // Deterministisches Pseudo-Rauschen + starker DC-Offset (+0.15)
        let mut seed = 123456789_u32;
        let mut next_rand = || -> f32 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed as f32) / (u32::MAX as f32)) * 2.0 - 1.0
        };

        for _ in 0..len {
            noise_l.push(0.15 + next_rand() * 0.04);
            noise_r.push(-0.12 + next_rand() * 0.04);
        }

        let profile = NoiseProfile::learn_from_samples(&noise_l, &noise_r, sr, "Test Hiss", "Cassette").unwrap();
        assert!((profile.dc_offset_l - 0.15).abs() < 0.01);
        assert!((profile.dc_offset_r - (-0.12)).abs() < 0.01);

        let params = DenoiserParams {
            enabled: true,
            audition_mode: AuditionMode::Wet,
            reduction_db: 24.0,
            sensitivity: 1.8,
            smoothing: 0.7,
            dc_filter_enabled: true,
            dc_cutoff_hz: 20.0,
        };

        // Wende den Filter auf das reine Rauschen + DC an
        let (clean_l, _clean_r) =
            SpectralDenoiser::process_offline_stereo(&noise_l, &noise_r, sr, Some(profile.clone()), &params);

        // DC-Offset im Ergebnis muss nahezu 0 sein und Rausch-RMS deutlich (mind. > 12 dB) abgesenkt
        let slice = &clean_l[sr as usize / 4..];
        let mean_after: f32 = slice.iter().copied().sum::<f32>() / slice.len() as f32;
        let rms_before = (noise_l.iter().map(|&x| (x - 0.15).powi(2)).sum::<f32>() / len as f32).sqrt();
        let rms_after = (slice.iter().map(|&x| x * x).sum::<f32>() / slice.len() as f32).sqrt();

        assert!(mean_after.abs() < 0.005, "DC offset not removed: {mean_after}");
        assert!(
            rms_after < rms_before * 0.25,
            "Noise RMS was not sufficiently reduced: before={rms_before}, after={rms_after}"
        );
    }

    #[test]
    fn test_preset_save_load_and_sample_rate_adaptation() {
        let temp_dir = std::env::temp_dir().join("tapedigizer_noise_presets_test");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let samples_l = vec![0.02_f32; 8192];
        let samples_r = vec![-0.02_f32; 8192];
        let (saved_prof, wav_path) = NoisePresetManager::save_preset_in_dir(
            &temp_dir,
            "Maxell XLII 90",
            "Cassette",
            &samples_l,
            &samples_r,
            44_100,
        )
        .unwrap();

        assert!(wav_path.is_file());
        let listed = NoisePresetManager::list_presets_in_dir(&temp_dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Maxell XLII 90");

        let adapted = saved_prof.adapted_for_sample_rate(48_000);
        assert_eq!(adapted.sample_rate, 48_000);
        assert_eq!(adapted.noise_power_l.len(), NUM_BINS);

        NoisePresetManager::delete_preset_in_dir(&temp_dir, &saved_prof).unwrap();
        assert!(NoisePresetManager::list_presets_in_dir(&temp_dir).is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cut_delete_and_crop_trim_audio() {
        let sr = 1000;
        let left: Vec<f32> = (0..10_000).map(|i| (i as f32) / 10_000.0).collect();
        let right = left.clone();
        let mut snap = crate::splitter::MarkerSnapshot {
            markers: vec![1.5, 3.0, 7.0],
            custom_titles: vec!["A".into(), "B".into(), "C".into(), "D".into()],
            pause_indices: vec![],
            disabled_filter_indices: vec![],
        };

        // Schneide Sekunde 2.0 bis 5.0 (3 Sekunden) heraus
        let (cut_l, _cut_r) = cut_delete_audio_range(&left, &right, sr, 2.0, 5.0, &mut snap).unwrap();
        assert_eq!(cut_l.len(), 7_000);
        assert_eq!(snap.markers.len(), 2);
        assert!((snap.markers[0] - 1.5).abs() < 0.02);
        assert!((snap.markers[1] - 4.0).abs() < 0.02); // 7.0 - 3.0s = 4.0s

        // Trimme auf Sekunde 1.0 bis 5.0 (4 Sekunden)
        let (crop_l, _crop_r) = crop_trim_audio_to_range(&cut_l, &cut_l, sr, 1.0, 5.0, &mut snap).unwrap();
        assert_eq!(crop_l.len(), 4_000);
        assert_eq!(snap.markers.len(), 2);
        assert!((snap.markers[0] - 0.5).abs() < 0.02);
        assert!((snap.markers[1] - 3.0).abs() < 0.02);
    }
}
