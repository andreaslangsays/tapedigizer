# 📼 TapeDigizer

<p align="center">
  <img src="TapeDigizer.iconset/icon_256x256@2x.png" width="128" height="128" alt="TapeDigizer App Icon" />
</p>

<p align="center">
  <strong>Native macOS Studio Console for Digitizing Tapes & Cassettes</strong><br>
  Optimized for multi-channel audio interfaces like the <strong>Zoom LiveTrak L-12</strong>.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.6.0-blue.svg" alt="Version 1.6.0" />
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License" />
  <img src="https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon%20%2F%20Intel)-darkgrey.svg" alt="macOS" />
  <img src="https://img.shields.io/badge/audio-24--bit%20PCM%20WAV-orange.svg" alt="24-bit WAV" />
  <img src="https://img.shields.io/badge/language-Deutsch%20%7C%20English-purple.svg" alt="i18n" />
</p>

---

## 🇩🇪 Deutsch | Übersicht

**TapeDigizer** ist eine hochpräzise, native macOS-Desktop-App (entwickelt in **Rust** mit hardwarebeschleunigter Benutzeroberfläche), die das Digitalisieren von **Schallplatten (Vinyl LP & Single 45)**, **Kassetten** und **Tonbändern** so einfach, präzise und professionell wie möglich macht.

### Highlights in v1.6.0

* **Rausch- & DC-Filter-Studio:** Im Menü **Filter** öffnen (`⌘F`), einen Abschnitt mit reinem Rauschen in der Wellenform markieren und daraus ein Rauschprofil lernen. Beim Vorhören lässt sich zwischen gefiltertem Signal (Wet), Original (A/B) und entferntem Rauschen (Δ) wechseln.
* **Rauschfilter pro Track:** Über das Menü **Filter** lässt sich die Rauschfilterung beim Export für den gewählten Track umschalten oder für alle Tracks gemeinsam ein- oder ausschalten. Der Filter kann auch direkt in den gewählten Track oder die gesamte Aufnahme eingerechnet werden.
* **Audio Unit für macOS:** Der TapeDigizer Denoiser lässt sich als Effekt in Logic Pro und anderen Audio-Unit-fähigen DAWs verwenden.
* **macOS-Oberfläche:** Metal-Rendering über `wgpu`, eine Stereo-Wellenform mit stabiler Höhe beim Ein- und Ausblenden der Schnitt-Lupe und eine sichtbare Versionsnummer.

### ✨ Hauptfunktionen

1. **🎙️ Tab 1: Aufnahme & Einpegeln**
   * **Analoge Audioquellen-Profile:** Vorkonfigurierte Voreinstellungen für **Kassette (Compact Cassette)**, **Tonband (Reel-to-Reel)**, **Vinyl LP (33⅓ RPM)** und **Single (45 RPM)** mit optimierten Standard-Aufnahmezeiten und passenden Stille-Erkennungsparametern.
   * **⏱ Einstellbarer Zeit-Headroom (Aufnahme-Puffer):** Verhindert verfrühtes Abschneiden bei überlangen Kassetten (z. B. C-90 mit reellen 47–49 min). Einstellbar per Schnellwahltasten (`+0m`, `+2m`, `+5m`, `+10m`, `+15m`) oder stufenlosem Schieberegler (Standard: `+5 min` Headroom).
   * **🔇 Hochempfindliche Stille-Erkennung & Erkennung fehlenden Signals (-100 dBFS):** Erkennt echten Signalverlust (wenn das Kassettendeck stoppt oder abschaltet) und schützt vor vorzeitigem Aufnahmeabbruch bei leisen Passagen oder Bandrauschen. Schwellenwert bis **-100 dBFS** stufenlos justierbar inklusive Direktwahl-Button `-100 dB (Kein Signal)`.
   * **Live-Stille-Anzeige:** Während der Aufnahme wird bei Unterschreitung der Schwelle die erkannte Stilledauer in Echtzeit angezeigt (`⏳ Stille: 2.1s / 20s`).
   * **Band & Ablage:** Name des Albums/Bandes und Seite (A/B) direkt vorab festlegen. Automatisch strukturierte Ordner.
   * **Erweiterte Auto-Stopp-Steuerung:** Einstellbare Stille-Schwelle (-100 dBFS bis -30 dBFS), Haltezeit (3 s bis 60 s) und schützende Nachlaufzeit (Run-On 0 s bis 60 s). Ein *Sofort-Stopp*-Button beendet die Aufnahme bei Bedarf unmittelbar ohne Nachlauf.
   * **Automatische Zwischensicherung vor Analyse:** Nach Aufnahmeende wird die 24-Bit-WAV-Datei und das `.tapedigizer`-Projekt sofort auf Disk gesichert.
   * **Zoom LiveTrak L-12 Mehrkanal-Routing:** Automatische Erkennung und Voreinstellung von **Kanäle 11/12 (Stereo Line)** für Kassetten-/Tape-Decks und Phono-Preamps (auch 9/10, Master 13/14 oder diskrete Kanalpaare wählbar).
   * **Responsive Studio LED-Stereokette:** 36 Segmente (-60 bis 0 dBFS) mit Spitzenwert-Ticks, separater numerischer dBFS-Anzeige und klickbarem CLIP-Reset.
   * **Vorab-Pegelcheck & Live-Wellenform:** Pegelkontrolle ohne Aufzeichnung sowie Echtzeit-Zweikanal-Wellenform während der Aufnahme.

2. **📊 Tab 2: Pegel-Diagnose & Hardware-Poti-Empfehlung**
   * **Analoge Signal-Diagnose:** Spitzenpegel L/R in dBFS, echter Headroom, DC-Offset (Gleichspannungsanteil) sowie Kanalbalance / Balance-Abweichung (L/R Δ dB).
   * **Zoom L-12 Gain-Ratgeber:** Exakte Angabe, wie weit das analoge Gain-Poti am Interface aufgedreht (+X dB) oder zurückgedreht (-X dB) werden muss für optimalen Headroom (-1.5 dBFS).
   * **Stereo-Balance-Hinweis:** Warnt vor ungleicher Tonabnehmer- oder Tonkopf-Justierung.

3. **✂️ Tab 3: Track-Schnitt, Präzisions-Schnitt-Lupe & Mastering-Export**
   * **Überarbeitetes 2-Zonen-Layout:**
     * **Oben (Volle Breite):** Zweikanal-Stereo-Wellenform über die gesamte Fensterbreite für perfekte Übersicht über das gesamte Band / die Platte, mit Marker-Flaggen, Taktlineal und Bereichsauswahl.
     * **Unten Links:** Kompakte, übersichtliche Songliste.
     * **Unten Rechts:** Interaktive Präzisions-Schnitt-Lupe zur exakten Platzierung des Schnittmarkers.
   * **🔍 Neu gestaltete Präzisions-Schnitt-Lupe:**
     * **Zonen-Hervorhebung:** Farbliche Markierung des auslaufenden Tracks (◀ ENDE Txx) und des beginnenden Tracks (START Tyy ▶).
     * **Transienten-Fokussierung (Max. 3):** Die Transienten-Erkennung ist intelligent begrenzt – es werden maximal die **3 signifikantesten Transienten** (stärkste Song-Einsätze) im Umkreis des Markers angezeigt, um Überfrachtung zu vermeiden.
     * **Nulldurchgang-Snap (Zero-Crossing):** Automatisches oder manuelles Einrasten auf den nächsten Nulldurchgang (`∿ Nulldurchgang`), um Knackser und Klicks am Schnittpunkt physikalisch auszuschließen.
     * **Stufenloser Zoom & Feinschnitt:** Schnelles Rein-/Rauszoomen per Mausrad direkt in der Lupe, Nudge-Tasten für Millisekunden- und Sample-Schritte (`±1 Smp`, `±10 ms`).
   * **📝 Kompakte & mächtige Trackliste:**
     * Kompakte Einzeilen-Einträge für maximale Übersicht auch bei über 20 Titeln pro Seite.
     * Großzügiges Titelfeld für mindestens 20–30 Zeichen ohne horizontales Drängeln.
     * **Mehrfach-Titel-Import (Batch-Paste):** Ganze Titellisten (z. B. aus Discogs, Wikipedia oder Bandcamp) mit einem Klick einfügen – Zeile für Zeile automatisch auf die Tracks verteilt!
     * Sofortiges Vorhören jedes Songs (`▶`) und `Weglassen`-Toggle für Rillenpausen oder Bandrauschen.
   * **🎛️ Analog-Mastering beim Export:**
     * **20 Hz Subsonic- / Rumpelfilter:** Entfernt tieffrequentes Platten-Rumpeln (Phono-Rumble) und Gleichstrom-Gleichlaufschwankungen.
     * **Schnittkanten Micro-Fades (5 ms):** Automatische klickfreie Ein- und Ausblendungen an allen Schnittgrenzen.
     * **L/R-Balance-Korrektur:** Gleicht Pegeldifferenzen zwischen linkem und rechtem Kanal automatisch an.
     * **Normalisierung:** 3 Modi (Original für DAW-Nachbearbeitung, Pro-Track Peak-Normalisierung, Album-/Seiten-Normalisierung).
   * **Vollwertiges Projekt-Management & Undo/Redo:** Speichern (`⌘S`), Öffnen (`⌘O`) im `.tapedigizer`-Format, mehrstufiges Undo (`⌘Z`) und Redo (`⇧⌘Z`).

4. **🌍 Mehrsprachigkeit & Persistenz**
   * Schneller Sprachwechsel zwischen **Deutsch** und **Englisch** direkt in der oberen Statusleiste.
   * Merkt sich alle Einstellungen (letztes Interface, Kanäle, Export-Ordner, Sprache) automatisch in `~/Library/Application Support/TapeDigizer/config.json`.

---

## 🇬🇧 English | Overview

**TapeDigizer** is a high-precision native macOS desktop application (built with **Rust** and hardware-accelerated GUI) designed to digitize **vinyl records (LP 33⅓ & Single 45)**, **cassettes**, and **reel-to-reel tapes** effortlessly with studio-grade fidelity.

### Highlights in v1.6.0

* **Noise & DC Filter Studio:** Open it from the **Filter** menu (`⌘F`), select a noise-only section of the waveform, and learn a noise profile from it. Audition the filtered signal (Wet), the original (A/B), or the removed noise (Δ).
* **Per-track noise filtering:** Use the **Filter** menu to toggle noise reduction at export for the selected track or enable or disable it for all tracks at once. You can also bake the filter into the selected track or the entire recording.
* **Audio Unit for macOS:** Use TapeDigizer Denoiser as an effect in Logic Pro and other Audio Unit hosts.
* **macOS interface:** Metal rendering via `wgpu`, a stereo waveform that keeps its height when toggling the cut magnifier, and a visible app version.

### ✨ Key Features

1. **🎙️ Tab 1: Recording & Calibration**
   * **Analog Media Profiles:** Presets for **Compact Cassette**, **Reel-to-Reel Tape**, **Vinyl LP (33⅓ RPM)**, and **Single (45 RPM)** with customized duration presets and silence detection thresholds.
   * **⏱ Configurable Time Headroom (Buffer):** Prevents premature cutoffs on tapes running longer than nominal length (e.g. C-90 running 47–49 min). Adjustable via quick-preset chips (`+0m`, `+2m`, `+5m`, `+10m`, `+15m`) or free slider (default: `+5 min` safety buffer).
   * **🔇 Ultra-Sensitive Silence & Missing Signal Detection (-100 dBFS):** Detects complete signal loss (when tape deck stops or shuts off) down to **-100 dBFS** to prevent early stops during quiet music passages or tape hiss. Includes `-100 dB (No signal)` one-click preset.
   * **Live Silence Duration Indicator:** Shows detected silence duration in real-time during recording (`⏳ Silence: 2.1s / 20s`).
   * **Configurable Auto-Stop:** Fine-tune silence threshold (-100 dBFS to -30 dBFS), hold time (3s to 60s), and run-on tail (0s to 60s), plus an instant-stop button.
   * **Zoom LiveTrak L-12 Multi-channel Routing:** Preconfigures **Channels 11/12 (Stereo Line)** as standard for tape decks and phono preamps.
   * **Real-time Studio LED Ladder Meters:** 36-segment responsive meters with peak-hold ticks, numeric dBFS readouts, and clickable clip indicators.
   * **Immediate Persistence:** Audio and project files are safely flushed to disk before analysis begins.
   * **24-bit Studio Quality:** Pure uncompressed 24-bit PCM WAV.

2. **📊 Tab 2: Level Diagnosis & Hardware Gain Advice**
   * **Signal Analysis:** Peak L/R (dBFS), headroom, DC-offset detection, and channel balance deviation (L/R Δ dB).
   * **Zoom L-12 Hardware Advice:** Recommends exact potentiometer adjustments (e.g. *"+4 dB"* or *"-2 dB"*).

3. **✂️ Tab 3: Track Splicing, Precision Scope & Mastering Export**
   * **Full-Width Stereo Waveform:** High-resolution waveform occupying full window width for clear navigation and overview.
   * **Precision Splice Scope (Schnitt-Lupe):**
     * Distinct track zone shading (◀ END Txx vs START Tyy ▶).
     * **3-Transient Limit:** Restricts transient markers to the top 3 most prominent musical onsets around the splice point to prevent visual clutter.
     * **Zero-Crossing Snap:** Magnetically aligns cuts to sample zero-crossings (`∿ Zero-Crossing`) to eliminate clicks and pops.
     * Zoomable down to individual samples with micro-nudge controls (`±1 Smp`, `±10 ms`).
   * **Compact Tracklist & Batch Title Import:**
     * Single-line compact track rows showing >= 20 characters comfortably.
     * **Batch-Paste:** Paste complete tracklists from Discogs, Wikipedia, or Bandcamp to populate titles automatically.
   * **Analog Mastering Export Options:**
     * **20 Hz Subsonic / Rumble Filter:** Removes turntable rumble and subsonic DC flutter.
     * **Boundary Micro-Fades (5 ms):** Transparent click-free fades at track heads and tails.
     * **L/R Balance Compensation:** Rebalances channel level imbalances.
     * **Normalization:** Raw DAW mode, per-track peak normalization, and album side loudness matching.

---

## ⌨️ Tastaturkürzel / Keyboard Shortcuts

| Taste / Key | Funktion (DE) | Function (EN) |
|---|---|---|
| **⌘ + S** | Projekt speichern (.tapedigizer) | Save Project (.tapedigizer) |
| **⌘ + O** | Projekt öffnen | Open Project |
| **⌘ + F** | Rausch- & DC-Filter-Studio ein- oder ausblenden | Toggle Noise & DC Filter Studio |
| **⌘ + Z** | Letzte Aktion rückgängig machen (Undo) | Undo last marker action |
| **⇧ + ⌘ + Z** / **⌘ + Y** | Wiederherstellen (Redo) | Redo marker action |
| **Tab** / **Enter** | Zum nächsten Tracktitel springen | Jump to next track title |
| **Leertaste / Space** | Wiedergabe starten / pausieren | Toggle Play / Pause |
| **⌥ + Mausrad** | Hinein- / Herauszoomen (auf Cursor) | Zoom in / out (centered on mouse) |
| **Mausrad / Scroll** | Horizontal nach links / rechts scrollen | Pan horizontally left / right |
| **H** | Hover-Vorhören an- / ausschalten | Toggle Hover Audition on / off |
| **S** oder **C** | Schnitt-Werkzeug (✂️) umschalten | Toggle Split / Scissor Tool |
| **V** oder **Esc** | Zurück zum Zeiger / Navigieren | Return to Pointer / Select Tool |
| **M** | Marker an Abspielposition setzen | Drop marker at playhead |
| **Delete / ⌫** | Ausgewählten Marker löschen (mit Bestätigung) | Delete selected marker (with prompt) |

---

## 🚀 Installation & Start

### 🍏 macOS App Bundle
Nach dem lokalen Build befindet sich das App-Bundle im Projektordner unter:
```bash
TapeDigizer.app
```
* **Per Doppelklick öffnen:** Direkt im Finder doppelklicken oder in deinen Programme-Ordner (`/Applications`) ziehen.
* **Per Terminal starten:**
  ```bash
  open TapeDigizer.app
  ```

### 🛠️ Aus dem Quellcode bauen & paketieren
Voraussetzung: Rust (`cargo`) ist installiert.
```bash
# Repository klonen
git clone git@github.com:andreaslangsays/tapedigizer.git
cd tapedigizer

# Tests ausführen
cargo test

# Lokales macOS App-Bundle bauen und ad hoc signieren
./scripts/package_app.sh
```

Das Skript erzeugt `TapeDigizer.app` und `TapeDigizerDenoiser-Installer.pkg` im Projektordner und installiert die Audio Unit unter `~/Library/Audio/Plug-Ins/Components/`. Für die separaten Hardwareprüfungen und das reproduzierbare Release-Profil:

```bash
cargo test --locked test_list_devices -- --ignored --nocapture
cargo test --locked test_record_and_finalize_lifecycle -- --ignored --nocapture
cargo test --locked --release profile_long_session -- --ignored --nocapture
```

Der Aufnahmetest benötigt ein verfügbares Eingabegerät und Mikrofonberechtigung. Das Profil erzeugt synthetische Aufnahmen und benötigt bis zu 1 GiB freien Speicherplatz für temporäre Exporte.

---

## 📄 Lizenz / License

Veröffentlicht unter der [MIT Lizenz](LICENSE).  
Copyright (c) 2026 Andreas Lang.
