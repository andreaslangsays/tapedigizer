# 📼 TapeDigizer

<p align="center">
  <img src="TapeDigizer.iconset/icon_256x256@2x.png" width="128" height="128" alt="TapeDigizer App Icon" />
</p>

<p align="center">
  <strong>Native macOS Studio Console for Digitizing Tapes & Cassettes</strong><br>
  Optimized for multi-channel audio interfaces like the <strong>Zoom LiveTrak L-12</strong>.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.2.1-blue.svg" alt="Version 0.2.1" />
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License" />
  <img src="https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon%20%2F%20Intel)-darkgrey.svg" alt="macOS" />
  <img src="https://img.shields.io/badge/audio-24--bit%20PCM%20WAV-orange.svg" alt="24-bit WAV" />
  <img src="https://img.shields.io/badge/language-Deutsch%20%7C%20English-purple.svg" alt="i18n" />
</p>

---

## 🇩🇪 Deutsch | Übersicht

**TapeDigizer** ist eine hochpräzise, native macOS-Desktop-App (entwickelt in **Rust** mit hardwarebeschleunigter Benutzeroberfläche), die das Digitalisieren alter Kassetten und Tonbänder so einfach und professionell wie möglich macht.

### ✨ Hauptfunktionen

1. **🎙️ Tab 1: Aufnahme & Einpegeln**
   * **Zoom LiveTrak L-12 Mehrkanal-Routing:** Erkennt automatisch 14 Kanäle und schlägt standardmäßig **Kanäle 11/12 (Stereo Line)** für dein Kassettendeck vor (auch Kanäle 9/10, Master Mix 13/14 oder diskrete Kanalpaare wählbar).
   * **Echtzeit Studio LED-Kette:** 36-Segment-Stereometer (-60 bis 0 dBFS) mit Spitzenwertanzeige und Übersteuerungs-Warnung (CLIP).
   * **Vorab-Pegelcheck:** Überprüfe und justiere den Eingangspegel ohne Datei-Aufzeichnung.
   * **Unbeaufsichtigte Aufnahme:** Timer für 15, 30, 45 (C-90) oder 60 (C-120) Minuten plus **automatischer Stopp bei Bandende** durch Stilleerkennung.
   * **24-Bit Studio-Qualität:** Verlustfreie, unkomprimierte WAV-Aufnahme mit 144 dB Dynamikbereich.

2. **📊 Tab 2: Pegel-Diagnose & Hardware-Poti-Empfehlung**
   * Automatische Vollauswertung nach der Aufnahme: Spitzenpegel (Peak L/R in dBFS), Headroom und Zählung übersteuerter Samples.
   * **Konkreter Zoom L-12 Gain-Ratgeber:** Sagt dir genau, ob und wie weit das analoge Gain-Poti aufgedreht (+X dB) oder zurückgedreht (-X dB) werden muss.

3. **✂️ Tab 3: Track-Schnitt & Mastering-Export**
   * **Tonband- & Album-Organisation (A- & B-Seite):** Gib den Albumnamen ein und wähle **Seite A** oder **Seite B**. Beim Export werden automatisch strukturierte Unterordner angelegt: `<Zielordner>/<Album-Name>/Seite A/` bzw. `Seite B/`.
   * **Vollwertige Projekt-Verwaltung (`.tapedigizer`):** Speichere (`⌘S`) und lade (`⌘O`) deine Digitalisierungs-Sessions inkl. Audio, Markern, Tracktiteln und Mastering-Einstellungen für unterbrechungsfreies Weiterarbeiten.
   * **Undo & Redo (⌘Z / ⇧⌘Z):** Versehentlich gelöschte oder gesetzte Schnittmarker lassen sich jederzeit mit ⌘Z zurückholen.
   * **Sicherheits-Rückfrage beim Löschen:** Verhindert versehentliches Löschen von Schnittmarkern mit einem Bestätigungs-Dialog.
   * **Optimierte Trackliste & Navigation:**
     * Extra breite Eingabefelder für lange Titel.
     * Schnelle Tastatur-Navigation mit **Enter** oder **Tab** direkt zum nächsten Songtitel.
     * Sichtbare Scrollbar mit Titelanzahlanzeige bei umfangreichen Tracklisten.
     * Einzel-Track-Wiedergabe mit automatischem Stopp am Songende.
   * **Zweikanal-Stereo-Wellenform:** Echte getrennte Spurdarstellung (Kanal 1 links oben, Kanal 2 rechts unten) im Logic Pro Look.
   * **Intuitives Zoomen & Scrollen:**
     * **Option (⌥) + Mausrad:** Butterweiches Hinein- und Herauszoomen (1x bis 25x), zentriert exakt an der Mauszeiger-Position!
     * **Mausrad (ohne Taste):** Horizontales Verschieben nach links und rechts.
     * **Übersicht-Leiste:** Interaktive Miniaturansicht des gesamten Bandes mit verschiebbarem Sichtfenster.
   * **Klare Trennung zwischen Playhead und Schnittmarkern:**
     * **Zeitleiste / Ruler:** Ein Klick oder Ziehen in der Zeitleiste steuert ausschließlich den Playhead (Abspielposition).
     * **Schnitt-Werkzeug (✂️) & Marker-Fähnchen:** Umschaltbar via `[S]` oder Button. Zeigt eine Schnitt-Hilfslinie und setzt gezielt Song-Trenner. Marker-Fähnchen oben lassen sich verschieben oder per Rechtsklick / `Delete` entfernen.
   * **🎧 Innovatives Hover-Vorhören (Audio-Scrubbing):**
     * Wenn aktiv, genügt es, die Maus für ca. 300 ms an einer Stelle der Wellenform ruhig zu halten, um sofort hineinzuhören! Sobald die Maus weiterbewegt wird, pausiert die Wiedergabe. Ideal zum schnellen Auffinden von Songanfängen.
   * **Pausenerkennung & Normalisierung:** Automatische Erkennung von Songpausen und 3 Export-Modi (Original für Logic Pro, Peak-Normalisierung pro Track, Album-Normalisierung).

4. **🌍 Mehrsprachigkeit & Persistenz**
   * Schneller Sprachwechsel zwischen **Deutsch** und **Englisch** direkt in der oberen Statusleiste.
   * Merkt sich alle Einstellungen (letztes Interface, Kanäle, Export-Ordner, Sprache) automatisch in `~/Library/Application Support/TapeDigizer/config.json`.

---

## 🇬🇧 English | Overview

**TapeDigizer** is a high-precision native macOS desktop application (built with **Rust** and hardware-accelerated GUI) designed to digitize reel-to-reel and cassette tapes effortlessly with studio-grade fidelity.

### ✨ Key Features

1. **🎙️ Tab 1: Recording & Calibration**
   * **Zoom LiveTrak L-12 Multi-channel Routing:** Automatically maps input channels with **Channels 11/12 (Stereo Line)** preselected as standard for tape decks.
   * **Real-time Studio LED Ladder Meters:** 36-segment responsive meters (-60 to 0 dBFS) with peak-hold ticks and digital clip indicators.
   * **Unattended Recording:** Timer presets (15, 30, 45, 60 min) and **automatic silence-detection auto-stop** at the end of the tape.
   * **24-bit Studio Quality:** Pure uncompressed 24-bit PCM WAV.

2. **📊 Tab 2: Level Diagnosis & Hardware Gain Advice**
   * Full scan for Peak L/R, true headroom, and clipped samples.
   * **Zoom L-12 Hardware Advice:** Recommends exact potentiometer adjustments (e.g. *"+4 dB"* or *"-2 dB"*).

3. **✂️ Tab 3: Track Splicing & Mastering Export**
   * **Dual-Channel Stereo Waveform:** Logic Pro studio aesthetic with distinct Left and Right channel lanes.
   * **Smooth Navigation:**
     * **Option (⌥) + Mouse Wheel:** Fluid zooming (1x to 25x) centered around the mouse cursor!
     * **Mouse Wheel:** Smooth horizontal panning across the tape.
     * **Overview Minimap:** Full-tape overview bar with a draggable viewport window.
   * **Distinct Playhead & Splicer Controls:**
     * **Timeline Ruler:** Scrubbing in the ruler sets the Playhead without affecting markers.
     * **Scissor Tool (✂️):** Activate via `[S]` to drop splice markers with live cursor guide lines.
   * **🎧 Stationary Hover Audition:**
     * Hover stationary over any section of the tape for ~300ms to immediately preview the audio!
   * **Export:** One-click batch export with 3 normalization algorithms.

---

## ⌨️ Tastaturkürzel / Keyboard Shortcuts

| Taste / Key | Funktion (DE) | Function (EN) |
|---|---|---|
| **⌘ + S** | Projekt speichern (.tapedigizer) | Save Project (.tapedigizer) |
| **⌘ + O** | Projekt öffnen | Open Project |
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
Das kompilierte Bundle befindet sich unter:
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

# macOS App-Bundle inklusive App-Icon bauen und signieren
./scripts/package_app.sh
```

---

## 📄 Lizenz / License

Veröffentlicht unter der [MIT Lizenz](LICENSE).  
Copyright (c) 2026 Andreas Lang.
