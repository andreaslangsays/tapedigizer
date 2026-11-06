use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "de")]
    De,
    #[serde(rename = "en")]
    En,
}

impl Default for Language {
    fn default() -> Self {
        Language::De
    }
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::De => "DE",
            Language::En => "EN",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::De => "Deutsch",
            Language::En => "English",
        }
    }

    // Tape Album & Side Metadata
    pub fn card_tape_project(&self) -> &'static str {
        match self {
            Language::De => "TONBAND- & PROJEKT-VERWALTUNG",
            Language::En => "TAPE & PROJECT MANAGEMENT",
        }
    }
    pub fn label_tape_name(&self) -> &'static str {
        match self {
            Language::De => "Tonband- / Album-Name:",
            Language::En => "Tape / Album Name:",
        }
    }
    pub fn hint_tape_name(&self) -> &'static str {
        match self {
            Language::De => "z.B. Sommerhits 1985 oder Band 04",
            Language::En => "e.g. Summer Hits 1985 or Reel 04",
        }
    }
    pub fn label_tape_side(&self) -> &'static str {
        match self {
            Language::De => "Bandseite:",
            Language::En => "Tape Side:",
        }
    }
    pub fn btn_save_project(&self) -> &'static str {
        match self {
            Language::De => "Projekt speichern (⌘S)",
            Language::En => "Save Project (⌘S)",
        }
    }
    pub fn btn_open_project(&self) -> &'static str {
        match self {
            Language::De => "Projekt öffnen (⌘O)",
            Language::En => "Open Project (⌘O)",
        }
    }
    pub fn btn_undo(&self) -> &'static str {
        match self {
            Language::De => "Rückgängig (⌘Z)",
            Language::En => "Undo (⌘Z)",
        }
    }
    pub fn btn_redo(&self) -> &'static str {
        match self {
            Language::De => "Wiederholen (⇧⌘Z)",
            Language::En => "Redo (⇧⌘Z)",
        }
    }
    pub fn label_export_target_preview(&self) -> &'static str {
        match self {
            Language::De => "Automatischer Ziel-Unterordner:",
            Language::En => "Automatic Target Subfolder:",
        }
    }
    pub fn label_scroll_hint(&self, count: usize) -> String {
        match self {
            Language::De => format!("{} Tracks in der Liste — scrolle mit dem Mausrad für weitere Titel", count),
            Language::En => format!("{} tracks in list — scroll with mouse wheel to see more titles", count),
        }
    }
    pub fn confirm_delete_question(&self, mins: u64, secs: u64, title: &str) -> String {
        match self {
            Language::De => format!("Trenner bei {:02}:{:02} ({}) wirklich entfernen?", mins, secs, title),
            Language::En => format!("Really delete split marker at {:02}:{:02} ({})?", mins, secs, title),
        }
    }
    pub fn btn_confirm_delete(&self) -> &'static str {
        match self {
            Language::De => "Ja, löschen",
            Language::En => "Yes, delete",
        }
    }
    pub fn btn_cancel(&self) -> &'static str {
        match self {
            Language::De => "Abbrechen",
            Language::En => "Cancel",
        }
    }

    // Top Header & Navigation Tabs
    pub fn menu_file(&self) -> &'static str {
        match self {
            Language::De => "Ablage",
            Language::En => "File",
        }
    }
    pub fn menu_new_project(&self) -> &'static str {
        match self {
            Language::De => "Neues Projekt",
            Language::En => "New Project",
        }
    }
    pub fn menu_open_project(&self) -> &'static str {
        match self {
            Language::De => "Projekt öffnen...",
            Language::En => "Open Project...",
        }
    }
    pub fn menu_save_project(&self) -> &'static str {
        match self {
            Language::De => "Projekt speichern",
            Language::En => "Save Project",
        }
    }
    pub fn menu_save_project_as(&self) -> &'static str {
        match self {
            Language::De => "Projekt speichern unter...",
            Language::En => "Save Project As...",
        }
    }
    pub fn menu_reveal_in_finder(&self) -> &'static str {
        match self {
            Language::De => "Aufnahme-Ordner im Finder zeigen",
            Language::En => "Reveal Recording Folder in Finder",
        }
    }
    pub fn menu_edit(&self) -> &'static str {
        match self {
            Language::De => "Bearbeiten",
            Language::En => "Edit",
        }
    }
    pub fn menu_undo(&self) -> &'static str {
        match self {
            Language::De => "Widerrufen",
            Language::En => "Undo",
        }
    }
    pub fn menu_redo(&self) -> &'static str {
        match self {
            Language::De => "Wiederholen",
            Language::En => "Redo",
        }
    }
    pub fn menu_help(&self) -> &'static str {
        match self {
            Language::De => "Hilfe",
            Language::En => "Help",
        }
    }
    pub fn menu_about(&self) -> &'static str {
        match self {
            Language::De => "Über TapeDigizer",
            Language::En => "About TapeDigizer",
        }
    }

    // Tape Metadata Card (Tab 1)
    pub fn card_tape_metadata(&self) -> &'static str {
        match self {
            Language::De => "KASSETTEN-METADATEN & PROJEKT-SPEICHERORT",
            Language::En => "CASSETTE METADATA & PROJECT FOLDER",
        }
    }
    pub fn label_project_folder(&self) -> &'static str {
        match self {
            Language::De => "Projekt-Ordner:",
            Language::En => "Project Folder:",
        }
    }
    pub fn btn_change_folder(&self) -> &'static str {
        match self {
            Language::De => "Ordner wählen...",
            Language::En => "Choose Folder...",
        }
    }
    pub fn hint_auto_saved(&self) -> &'static str {
        match self {
            Language::De => "Automatische Zwischenspeicherung sofort bei Aufnahme-Stopp",
            Language::En => "Auto-saved as project immediately when recording stops",
        }
    }

    pub fn tab_recording(&self) -> &'static str {
        match self {
            Language::De => "1. AUFNAHME & PEGEL",
            Language::En => "1. RECORD & LEVEL",
        }
    }
    pub fn tab_diagnosis(&self) -> &'static str {
        match self {
            Language::De => "2. PEGEL-DIAGNOSE",
            Language::En => "2. LEVEL DIAGNOSIS",
        }
    }
    pub fn tab_split_export(&self) -> &'static str {
        match self {
            Language::De => "3. SCHNITT & EXPORT",
            Language::En => "3. SPLIT & EXPORT",
        }
    }

    // Audio Routing (Tab 1)
    pub fn card_audio_routing(&self) -> &'static str {
        match self {
            Language::De => "AUDIO-ROUTING (INTERFACE & EINGANGSKANÄLE)",
            Language::En => "AUDIO ROUTING (INTERFACE & INPUT CHANNELS)",
        }
    }
    pub fn label_interface(&self) -> &'static str {
        match self {
            Language::De => "Interface:",
            Language::En => "Interface:",
        }
    }
    pub fn label_stereo_input(&self) -> &'static str {
        match self {
            Language::De => "Stereo-Eingang:",
            Language::En => "Stereo Input:",
        }
    }
    pub fn btn_rescan(&self) -> &'static str {
        match self {
            Language::De => "Neu scannen",
            Language::En => "Rescan",
        }
    }
    pub fn label_no_device(&self) -> &'static str {
        match self {
            Language::De => "Kein Gerät",
            Language::En => "No Device",
        }
    }
    pub fn label_select_channel(&self) -> &'static str {
        match self {
            Language::De => "Kanal wählen",
            Language::En => "Select Channel",
        }
    }

    // LED Meter & Level Check (Tab 1)
    pub fn card_led_meters(&self) -> &'static str {
        match self {
            Language::De => "STUDIO LED LADDER METERS (VORAB-CHECK & LIVE-MONITORING)",
            Language::En => "STUDIO LED LADDER METERS (PRE-CHECK & LIVE MONITORING)",
        }
    }
    pub fn meter_instructions(&self) -> &'static str {
        match self {
            Language::De => "Lass das Band an einer lauten Stelle laufen und dreh am Zoom L-12 Gain-Poti bis zur gelben Zone (-3 bis -1 dBFS).",
            Language::En => "Play the tape at a loud section and adjust Zoom L-12 Gain knob into the amber zone (-3 to -1 dBFS).",
        }
    }
    pub fn btn_start_level_check(&self) -> &'static str {
        match self {
            Language::De => "PEGEL-CHECK STARTEN",
            Language::En => "START LEVEL CHECK",
        }
    }
    pub fn btn_stop_level_check(&self) -> &'static str {
        match self {
            Language::De => "CHECK BEENDEN",
            Language::En => "STOP CHECK",
        }
    }

    // Recording Controls (Tab 1)
    pub fn card_recording_control(&self) -> &'static str {
        match self {
            Language::De => "AUFNAHME-STEUERUNG & AUTO-STOPP (UNBEAUFSICHTIGT)",
            Language::En => "RECORDING CONTROL & AUTO-STOP (UNATTENDED)",
        }
    }
    pub fn label_duration_preset(&self) -> &'static str {
        match self {
            Language::De => "Dauer-Voreinstellung:",
            Language::En => "Duration Preset:",
        }
    }
    pub fn preset_unlimited(&self) -> &'static str {
        match self {
            Language::De => "Unbegrenzt",
            Language::En => "Unlimited",
        }
    }
    pub fn checkbox_silence_autostop(&self) -> &'static str {
        match self {
            Language::De => "Auto-Stopp bei Bandende (sobald nach mindestens 30 Sek. 15 Sek. Stille herrscht)",
            Language::En => "Auto-stop at tape end (stops when silence is detected for >= 15s after 30s)",
        }
    }

    // Live Waveform (Tab 1)
    pub fn card_live_waveform(&self) -> &'static str {
        match self {
            Language::De => "LIVE-WELLENFORM (ECHTZEIT-AUFZEICHNUNG)",
            Language::En => "LIVE WAVEFORM (REAL-TIME RECORDING)",
        }
    }
    pub fn live_waveform_idle(&self) -> &'static str {
        match self {
            Language::De => "Bereit zur Aufnahme — die Wellenform baut sich hier live während der Digitalisierung auf.",
            Language::En => "Ready to record — the waveform will build here in real-time as audio is digitized.",
        }
    }
    pub fn live_waveform_monitoring(&self) -> &'static str {
        match self {
            Language::De => "Pegel-Check aktiv — starte die Aufnahme, um das Signal aufzuzeichnen.",
            Language::En => "Level check active — start recording to capture audio.",
        }
    }
    pub fn btn_start_recording(&self) -> &'static str {
        match self {
            Language::De => "REC | AUFNAHME STARTEN (24-BIT WAV)",
            Language::En => "REC | START RECORDING (24-BIT WAV)",
        }
    }
    pub fn btn_stop_recording(&self) -> &'static str {
        match self {
            Language::De => "AUFNAHME STOPPEN & ANALYSIEREN",
            Language::En => "STOP RECORDING & ANALYZE",
        }
    }

    // Tab 2: Diagnosis
    pub fn card_diagnosis(&self) -> &'static str {
        match self {
            Language::De => "POST-RECORDING PEGEL-DIAGNOSE & HARDWARE-REGELUNG",
            Language::En => "POST-RECORDING LEVEL DIAGNOSIS & HARDWARE CALIBRATION",
        }
    }
    pub fn card_knob_title(&self) -> &'static str {
        match self {
            Language::De => "ZOOM L-12 GAIN-POTI EMPFEHLUNG",
            Language::En => "ZOOM L-12 GAIN KNOB RECOMMENDATION",
        }
    }
    pub fn card_stats_title(&self) -> &'static str {
        match self {
            Language::De => "AUFNAHME-STATISTIK",
            Language::En => "RECORDING STATISTICS",
        }
    }
    pub fn label_signal_quality(&self) -> &'static str {
        match self {
            Language::De => "Signal-Qualität:",
            Language::En => "Signal Quality:",
        }
    }
    pub fn label_max_peak(&self) -> &'static str {
        match self {
            Language::De => "Maximaler Peak:",
            Language::En => "Maximum Peak:",
        }
    }
    pub fn label_headroom(&self) -> &'static str {
        match self {
            Language::De => "Headroom:",
            Language::En => "Headroom:",
        }
    }
    pub fn label_clipped_samples(&self) -> &'static str {
        match self {
            Language::De => "Übersteuerte Samples:",
            Language::En => "Clipped Samples:",
        }
    }
    pub fn label_duration(&self) -> &'static str {
        match self {
            Language::De => "Aufnahme-Dauer:",
            Language::En => "Recording Duration:",
        }
    }
    pub fn btn_retry(&self) -> &'static str {
        match self {
            Language::De => "Wiederholen (Neuer Versuch)",
            Language::En => "Retry (New Take)",
        }
    }
    pub fn btn_proceed_to_split(&self) -> &'static str {
        match self {
            Language::De => "Aufnahme behalten & Tracks schneiden ->",
            Language::En => "Keep Recording & Split Tracks ->",
        }
    }

    // Tab 3: Arrange & Tools
    pub fn card_arrange(&self) -> &'static str {
        match self {
            Language::De => "STEREO TRACK-ARRANGE & SCHNITT-MARKER (LOGIC PRO STYLE)",
            Language::En => "STEREO TRACK ARRANGE & SPLIT MARKERS (LOGIC PRO STYLE)",
        }
    }
    pub fn btn_tool_pointer(&self) -> &'static str {
        match self {
            Language::De => "NAVIGIEREN / ABHÖREN [V]",
            Language::En => "NAVIGATE / AUDITION [V]",
        }
    }
    pub fn btn_tool_scissor(&self) -> &'static str {
        match self {
            Language::De => "✂ SCHNITT-WERKZEUG [S]",
            Language::En => "✂ SPLIT TOOL [S]",
        }
    }
    pub fn btn_hover_on(&self) -> &'static str {
        match self {
            Language::De => "🎧 HOVER-VORHÖREN: AN [H]",
            Language::En => "🎧 HOVER PREVIEW: ON [H]",
        }
    }
    pub fn btn_hover_off(&self) -> &'static str {
        match self {
            Language::De => "🎧 HOVER-VORHÖREN: AUS [H]",
            Language::En => "🎧 HOVER PREVIEW: OFF [H]",
        }
    }
    pub fn tip_navigation(&self) -> &'static str {
        match self {
            Language::De => "Tipp: Option + Mausrad = Zoom | Mausrad = Scrollen | Klick in Zeitleiste = Playhead | ✂ Schnitt-Werkzeug oder [M] = Trenner | Übersicht unten = Verschieben",
            Language::En => "Tip: Option + Scroll = Zoom | Scroll = Pan | Click in Ruler = Playhead | ✂ Split Tool or [M] = Split | Overview bar = Move",
        }
    }
    pub fn btn_play(&self) -> &'static str {
        match self {
            Language::De => "PLAY (Leertaste)",
            Language::En => "PLAY (Space)",
        }
    }
    pub fn btn_pause(&self) -> &'static str {
        match self {
            Language::De => "PAUSE",
            Language::En => "PAUSE",
        }
    }
    pub fn btn_stop(&self) -> &'static str {
        match self {
            Language::De => "STOP",
            Language::En => "STOP",
        }
    }
    pub fn btn_add_marker(&self) -> &'static str {
        match self {
            Language::De => "+ Marker an Position [M]",
            Language::En => "+ Marker at Position [M]",
        }
    }
    pub fn btn_auto_detect_pauses(&self) -> &'static str {
        match self {
            Language::De => "Pausen als Trenner erkennen",
            Language::En => "Auto-detect Pauses (Split)",
        }
    }
    pub fn btn_auto_detect_pauses_gaps(&self) -> &'static str {
        match self {
            Language::De => "✂ Pausen ausschneiden & weglassen",
            Language::En => "✂ Cut & Skip Pauses",
        }
    }
    pub fn btn_magnifier_toggle(&self) -> &'static str {
        match self {
            Language::De => "🔍 Lupenansicht [L]",
            Language::En => "🔍 Magnifier View [L]",
        }
    }
    pub fn label_vertical_zoom(&self) -> &'static str {
        match self {
            Language::De => "Pegel-Lupe",
            Language::En => "Gain Scope",
        }
    }
    pub fn label_waveform_height(&self) -> &'static str {
        match self {
            Language::De => "Wellenhöhe",
            Language::En => "Wave Height",
        }
    }
    pub fn card_magnifier_scope(&self) -> &'static str {
        match self {
            Language::De => "🔍 DETAIL-LUPENANSICHT (FOKUS ±1.5s SAMPLE-PRÄZISION)",
            Language::En => "🔍 DETAIL MAGNIFIER (FOCUS ±1.5s SAMPLE PRECISION)",
        }
    }
    pub fn btn_make_pause(&self) -> &'static str {
        match self {
            Language::De => "⏸ Weglassen",
            Language::En => "⏸ Skip Pause",
        }
    }
    pub fn btn_make_track(&self) -> &'static str {
        match self {
            Language::De => "▶ Als Track",
            Language::En => "▶ As Track",
        }
    }
    pub fn badge_pause(&self) -> &'static str {
        match self {
            Language::De => "PAUSE (WEGGELASSEN)",
            Language::En => "PAUSE (SKIPPED)",
        }
    }
    pub fn label_nudge_start(&self) -> &'static str {
        match self {
            Language::De => "Start-Marker verschieben",
            Language::En => "Nudge Start Marker",
        }
    }
    pub fn label_nudge_end(&self) -> &'static str {
        match self {
            Language::De => "End-Marker verschieben",
            Language::En => "Nudge End Marker",
        }
    }
    pub fn tip_nudge_keys(&self) -> &'static str {
        match self {
            Language::De => "Tipp: ◀ / ▶ = Marker verschieben (mit Shift = 1s) | ⇧⌥ + Mausrad = Pegel-Lupe",
            Language::En => "Tip: ◀ / ▶ = Nudge marker (with Shift = 1s) | ⇧⌥ + Scroll = Vertical Amplitude Zoom",
        }
    }
    pub fn btn_clear_markers(&self) -> &'static str {
        match self {
            Language::De => "Alle Marker löschen",
            Language::En => "Clear All Markers",
        }
    }

    // Tab 3: Tracklist Table
    pub fn card_tracklist(&self, count: usize) -> String {
        match self {
            Language::De => format!("TRACKLISTE / SONGS ({} TRACKS)", count),
            Language::En => format!("TRACKLIST / SONGS ({} TRACKS)", count),
        }
    }
    pub fn col_title(&self) -> &'static str {
        match self {
            Language::De => "Titel",
            Language::En => "Title",
        }
    }
    pub fn col_start(&self) -> &'static str {
        match self {
            Language::De => "Start",
            Language::En => "Start",
        }
    }
    pub fn col_end(&self) -> &'static str {
        match self {
            Language::De => "Ende",
            Language::En => "End",
        }
    }
    pub fn col_duration(&self) -> &'static str {
        match self {
            Language::De => "Dauer",
            Language::En => "Duration",
        }
    }
    pub fn col_actions(&self) -> &'static str {
        match self {
            Language::De => "Aktionen",
            Language::En => "Actions",
        }
    }
    pub fn btn_delete(&self) -> &'static str {
        match self {
            Language::De => "Entfernen",
            Language::En => "Delete",
        }
    }

    // Tab 3: Export Drawer
    pub fn card_export(&self) -> &'static str {
        match self {
            Language::De => "EXPORT & MASTERING-OPTIONEN",
            Language::En => "EXPORT & MASTERING OPTIONS",
        }
    }
    pub fn label_export_dir(&self) -> &'static str {
        match self {
            Language::De => "Ziel-Ordner:",
            Language::En => "Export Folder:",
        }
    }
    pub fn btn_choose_folder(&self) -> &'static str {
        match self {
            Language::De => "Ordner wählen...",
            Language::En => "Choose Folder...",
        }
    }
    pub fn label_normalization(&self) -> &'static str {
        match self {
            Language::De => "Normalisierung:",
            Language::En => "Normalization:",
        }
    }
    pub fn norm_original(&self) -> &'static str {
        match self {
            Language::De => "Original belassen (für Logic Pro)",
            Language::En => "Keep Original (for Logic Pro)",
        }
    }
    pub fn norm_per_track(&self) -> &'static str {
        match self {
            Language::De => "Peak auf -0.3 dBFS pro Song",
            Language::En => "Peak at -0.3 dBFS per Song",
        }
    }
    pub fn norm_album(&self) -> &'static str {
        match self {
            Language::De => "Album-Normalisierung (erhält Dynamik)",
            Language::En => "Album Normalization (preserves dynamics)",
        }
    }
    pub fn btn_export_tracks(&self, count: usize) -> String {
        match self {
            Language::De => format!("ALLE {} TRACKS ALS 24-BIT WAV EXPORTIEREN", count),
            Language::En => format!("EXPORT ALL {} TRACKS AS 24-BIT WAV", count),
        }
    }
    pub fn btn_show_in_finder(&self) -> &'static str {
        match self {
            Language::De => "Im Finder anzeigen",
            Language::En => "Show in Finder",
        }
    }
}
