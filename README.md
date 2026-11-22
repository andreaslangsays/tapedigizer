# 📼 TapeDigizer

<p align="center">
  <img src="TapeDigizer.iconset/icon_256x256@2x.png" width="128" height="128" alt="TapeDigizer App Icon" />
</p>

<p align="center">
  <strong>Native macOS Studio Console for Digitizing Tapes & Cassettes</strong><br>
  Optimized for multi-channel audio interfaces like the <strong>Zoom LiveTrak L-12</strong>.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0-blue.svg" alt="Version 2.0.0" />
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="MIT License" />
  <img src="https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon%20%2F%20Intel)-darkgrey.svg" alt="macOS" />
  <img src="https://img.shields.io/badge/audio-24--bit%20PCM%20WAV-orange.svg" alt="24-bit WAV" />
  <img src="https://img.shields.io/badge/language-Deutsch%20%7C%20English-purple.svg" alt="i18n" />
</p>

---

## 🇩🇪 Deutsch | Übersicht

**TapeDigizer** ist eine hochpräzise, native macOS-Desktop-App (entwickelt in **Rust** mit hardwarebeschleunigter Benutzeroberfläche), die das Digitalisieren von **Schallplatten (Vinyl LP & Single 45)**, **Kassetten** und **Tonbändern** so einfach, präzise und professionell wie möglich macht.

### Neu in 2.0.0

* **Flexibles Effekt-Rack:** Sechs einzeln schaltbare und umsortierbare Inserts pro Track, ein fester Hauptmix und ein Live-Vergleich von Eingang, Ausgang und Differenzsignal.
* **Nichtdestruktive Bearbeitung:** Track-Verarbeitung bleibt im Projekt editierbar. Änderungen an der Aufnahme erzeugen eine neue WAV-Fassung und lassen die ursprüngliche Datei erhalten.
* **Gemeinsame Bedienelemente:** Potis, Tasten und bündige Panelzeilen verwenden zentrale Komponenten in Silverface und Obsidian.

### Studiokonsole

* **Modernes Dual-Theme Studio-Design (Silverface & Obsidian):** Neues, helles **Silverface**-Mastering-Design als Standard und umschaltbarer **Obsidian**-Dark-Mode direkt in der nativen macOS Unified Titlebar (`Hell` / `Dunkel`).
* **Skalierende Konsole ohne vertikales Scrollen:** Alle Arbeitsbereiche (**Aufnahme**, **Schnitt**, **Einzeltrack**, **Tracks**, **Export**) skalieren dynamisch mit der Fensterhöhe — Wellenformen und Effekt-Rack passen sich an, Trackliste und Exportvorschau werden paginiert. Die Mindestgröße beträgt `1060 × 700 pt`.
* **Integrierte Einzeltrack-Bearbeitung (Song-Studio):** Direkt im Hauptmenü unter **`[ Marker | Tracks | Einzeltrack | Export ]`** (`⌘F`) erreichbar. Pro Song eigene Wellenform, A/B-Vergleich (`Bearbeitet` / `Original`) und eine Effektkette im unteren Rack. Sechs Inserts lassen sich einzeln aktivieren und mit `←` / `→` verschieben. Der Hauptmix sitzt fest am Ende. Beim Vorhören zeigt jede ausgewählte Stufe Eingang, Ausgang und Differenzsignal. Die Kette wird im Projekt gespeichert und beim Export angewendet. Die Quell-WAV bleibt erhalten.
  * **Einzelkanäle:** Kanal-Trim, taktile `MUTE`- und `Ø Phase`-Schalter, L/R-Kopplung und Solo.
  * **Restauration:** Dunkles OLED-Spektral-Display mit Hiss- und Zielkurve, Spectral-Denoiser (`Reduktion`, `Empfindlichkeit`, `Glättung`), Subsonic/DC-Filter und pro-Track Rauschprofil.
  * **5-Band-Equalizer:** Low Shelf, Mittenband und High Shelf plus zwei frei platzierbare Glockenbänder mit Frequenz, Gain und Q. Fünf ziehbare Punkte, `Flat`-Reset und Live-Ausgangsspektrum in der Frequenzgrafik.
  * **Compressor, Expander und Brickwall-Limiter:** Drei separate Inserts mit eigenen Kennlinien. Der **Brickwall-Limiter** bietet `Ceiling` (`-24..0 dB`), `Drive` (`0..+24 dB`), `Release` (`5..500 ms`), `0 ms` Attack und Stereo-Link.
  * **Hauptmix:** Stereo-/Mono-Routing (`Stereo`, `L Mono`, `R Mono`, `Mono-Mix`), Balance, Stereobreite, vertikales OLED-Stereo-LED-Meter und Exportformat pro Song.
* **Präzise Drehregler (Potis):** Glatte Aluminium-Potis mit 270°-Farbbogen, sofortigem vertikalem Drag ohne Sprung, logarithmischer Skalierung für Frequenzen & Zeiten (`Hz`/`ms`), Mausrad-/Trackpad-Steuerung, `Shift`-Feinmodus und Doppelklick-Reset bipolarer Potis auf `0`.
* **Analog-Instrumentierung:** Umschaltbare Vintage-VU-Meter (`VU` / `LED`) und animiertes Band-Transport-Deck in der Aufnahme-Ansicht sowie direkter Projekt-/WAV-Laden-Button auf der leeren Schnitt-Startseite.
* **Audio Unit für macOS:** Der TapeDigizer Denoiser lässt sich als Effekt in Logic Pro und anderen Audio-Unit-fähigen DAWs verwenden.

Die [Design-Dokumentation](docs/design/README.md) beschreibt die Farbpaletten, das Konsolenlayout und die Bedienelemente in Version 2.0.0.

### ✨ Hauptfunktionen

Die Studiokonsole hat zwei klar getrennte Hauptbereiche: **Aufnahme** für Quelle, Pegelcheck und Digitalisierung sowie **Schnitt** für bestehende Projekte und WAV-Dateien. Im Schnittbereich stehen **`Marker`**, **`Tracks`**, **`Einzeltrack`** und **`Export`** direkt in der Unified Titlebar zur Verfügung. Lange Track- und Dateilisten werden dynamisch seitenweise angepasst.

1. **🎙️ Aufnahme & Einpegeln**
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

2. **📊 Aufnahmebefund neben den Eingangspegeln**
   * **Analoge Signal-Diagnose:** Spitzenpegel L/R in dBFS, echter Headroom, DC-Offset (Gleichspannungsanteil) sowie Kanalbalance / Balance-Abweichung (L/R Δ dB).
   * **Zoom L-12 Gain-Ratgeber:** Exakte Angabe, wie weit das analoge Gain-Poti am Interface aufgedreht (+X dB) oder zurückgedreht (-X dB) werden muss für optimalen Headroom (-1.5 dBFS).
   * **Stereo-Balance-Hinweis:** Warnt vor ungleicher Tonabnehmer- oder Tonkopf-Justierung.

3. **✂️ Track-Schnitt, Präzisions-Schnitt-Lupe & Mastering-Export**
   * **Konsolenlayout:** Stereo-Wellenform über die gesamte Fensterbreite mit Schnittmarken, Zeitlineal und Bereichsauswahl. Die Schnitt-Lupe liegt direkt unter der Wellenform. **Aufnahme bearbeiten** öffnet in derselben Ansicht die Werkzeuge für die Gesamtaufnahme. Tracks, Einzeltrack und Export haben eigene Reiter.
   * **🔍 Neu gestaltete Präzisions-Schnitt-Lupe:**
     * **Zonen-Hervorhebung:** Farbliche Markierung des auslaufenden Tracks (◀ ENDE Txx) und des beginnenden Tracks (START Tyy ▶).
     * **Transienten-Fokussierung (Max. 3):** Die Transienten-Erkennung ist intelligent begrenzt – es werden maximal die **3 signifikantesten Transienten** (stärkste Song-Einsätze) im Umkreis des Markers angezeigt, um Überfrachtung zu vermeiden.
     * **Nulldurchgang-Snap (Zero-Crossing):** Automatisches oder manuelles Einrasten auf den nächsten Nulldurchgang (`∿ Nulldurchgang`), um Knackser am Schnittpunkt zu reduzieren.
     * **Stufenloser Zoom & Feinschnitt:** Schnelles Rein-/Rauszoomen per ⌥ + Mausrad direkt in der Lupe, Nudge-Tasten für Millisekunden- und Sample-Schritte (`±1 Smp`, `±10 ms`).
   * **📝 Kompakte & mächtige Trackliste:**
     * Kompakte Einzeilen-Einträge für maximale Übersicht auch bei über 20 Titeln pro Seite.
     * Großzügiges Titelfeld für mindestens 20–30 Zeichen ohne horizontales Drängeln.
     * **Mehrfach-Titel-Import (Batch-Paste):** Ganze Titellisten (z. B. aus Discogs, Wikipedia oder Bandcamp) mit einem Klick einfügen – Zeile für Zeile automatisch auf die Tracks verteilt!
     * Sofortiges Vorhören jedes Songs (`▶`) und `Weglassen`-Toggle für Rillenpausen oder Bandrauschen.
   * **🎛️ Analog-Mastering beim Export:**
     * **20 Hz Subsonic- / Rumpelfilter:** Entfernt tieffrequentes Platten-Rumpeln (Phono-Rumble) und Gleichstrom-Gleichlaufschwankungen.
     * **Schnitt-Fades:** Aus oder `15 ms`, `100 ms`, `300 ms` für Ein- und Ausblendungen an den Trackgrenzen.
     * **L/R-Trim:** Manuelle Pegelkorrektur zwischen linkem und rechtem Kanal für Tracks mit den globalen Exporteinstellungen.
     * **Normalisierung:** 3 Modi (Originalpegel, Peak pro Track bei −0,3 dBFS oder gemeinsamer Gain für die Albumseite).
   * **WAV-Versionen für Aufnahmebearbeitung:** Löschen, Trim und gerenderte Filter erzeugen eine neue Datei. Undo/Redo wechselt WAV-Fassung und Marker gemeinsam. Das Projekt speichert die aktive Fassung.
   * **Vollwertiges Projekt-Management & Undo/Redo:** Speichern (`⌘S`), Öffnen (`⌘O`) im `.tapedigizer`-Format, mehrstufiges Undo (`⌘Z`) und Redo (`⇧⌘Z`).

4. **🌍 Mehrsprachigkeit & Persistenz**
   * Schneller Sprachwechsel zwischen **Deutsch** und **Englisch** direkt in der oberen Statusleiste.
   * Merkt sich alle Einstellungen (letztes Interface, Kanäle, Export-Ordner, Sprache) automatisch in `~/Library/Application Support/TapeDigizer/config.json`.

---

## 🇬🇧 English | Overview

**TapeDigizer** is a high-precision native macOS desktop application (built with **Rust** and hardware-accelerated GUI) designed to digitize **vinyl records (LP 33⅓ & Single 45)**, **cassettes**, and **reel-to-reel tapes** effortlessly with studio-grade fidelity.

### New in 2.0.0

* **Flexible effect rack:** Six independently switchable and reorderable inserts per track, a fixed main mix, and live input, output, and difference signal meters.
* **Nondestructive editing:** Track processing remains editable in the project. Recording edits create a new WAV revision and preserve the original file.
* **Shared controls:** Knobs, buttons, and aligned panel rows use common components in Silverface and Obsidian.

### Studio console

* **Modern Dual-Theme Studio Console (Silverface & Obsidian):** Clean light **Silverface** mastering palette by default plus a live **Obsidian** dark mode toggle right in the native macOS Unified Titlebar (`Light` / `Dark`).
* **Responsive Scroll-Free Layout:** All workspaces (**Recording**, **Cut**, **Track FX**, **Tracks**, **Export**) fit the available height at the supported window sizes, starting at `1060 × 700 pt`. Track lists and export previews paginate.
* **Integrated Single-Track Processing (Song Studio):** Built directly into the Editing sub-navigation **`[ Cut | Tracks | Track FX | Export ]`** (`⌘F`). Each song has a local waveform, A/B audition (`Processed` / `Original`), and an insert chain in the lower rack. Enable each of the six inserts independently and reorder them with `←` / `→`. The main mix stays at the end. During playback the selected stage shows input, output, and difference signal peaks. The project stores the chain, and export applies it while preserving the source WAV.
  * **Channels:** Input trim, tactile `MUTE` and `Ø Phase` switches, L/R link, and channel solo.
  * **Restoration:** Dark OLED spectrum display with raw hiss & target curves, spectral denoiser (`Reduction`, `Sensitivity`, `Smoothing`), subsonic/DC filter, and per-track noise profile.
  * **5-Band Equalizer:** Low shelf, mid band, and high shelf plus two freely tunable peak bands with frequency, gain, and Q. Five draggable points, a `Flat` reset, and a live output spectrum in the frequency graph.
  * **Compressor, Expander, and Brickwall Limiter:** Three separate inserts with their own transfer curves. The **Brickwall Limiter** provides `Ceiling` (`-24..0 dB`), `Drive` (`0..+24 dB`), `Release` (`5..500 ms`), `0 ms` attack, and stereo detector linking.
  * **Main mix:** Stereo/mono routing (`Stereo`, `Left mono`, `Right mono`, `Mono mix`), balance, stereo width, vertical OLED stereo LED meter, and per-song export format.
* **Precision Rotary Knobs:** Smooth silver-aluminum knobs with 270° colored arcs, instant vertical drag, logarithmic scaling for frequencies & times (`Hz`/`ms`), mouse-wheel / trackpad scrolling, `Shift` fine mode, and double-click reset to `0` for bipolar knobs.
* **Analog Instrumentation:** Switchable vintage analog VU meters (`VU` / `LED`), animated tape transport deck in Recording, and a primary Load Project / WAV button on the empty Editing start screen.
* **Audio Unit for macOS:** Use TapeDigizer Denoiser as an effect in Logic Pro and other Audio Unit hosts.

The [design documentation](docs/design/README.md) covers the palettes, console layout, and controls in version 2.0.0.

### ✨ Key Features

The console has two distinct workspaces: **Recording** for source setup, level monitoring, and digitization, and **Editing** for existing projects and WAV files. Editing integrates **`Cut`**, **`Tracks`**, **`Track FX`**, and **`Export`** directly in the Unified Titlebar. Long track lists and export previews paginate dynamically to fit the window.

1. **🎙️ Recording & Calibration**
   * **Analog Media Profiles:** Presets for **Compact Cassette**, **Reel-to-Reel Tape**, **Vinyl LP (33⅓ RPM)**, and **Single (45 RPM)** with customized duration presets and silence detection thresholds.
   * **⏱ Configurable Time Headroom (Buffer):** Prevents premature cutoffs on tapes running longer than nominal length (e.g. C-90 running 47–49 min). Adjustable via quick-preset chips (`+0m`, `+2m`, `+5m`, `+10m`, `+15m`) or free slider (default: `+5 min` safety buffer).
   * **🔇 Ultra-Sensitive Silence & Missing Signal Detection (-100 dBFS):** Detects complete signal loss (when tape deck stops or shuts off) down to **-100 dBFS** to prevent early stops during quiet music passages or tape hiss. Includes `-100 dB (No signal)` one-click preset.
   * **Live Silence Duration Indicator:** Shows detected silence duration in real-time during recording (`⏳ Silence: 2.1s / 20s`).
   * **Configurable Auto-Stop:** Fine-tune silence threshold (-100 dBFS to -30 dBFS), hold time (3s to 60s), and run-on tail (0s to 60s), plus an instant-stop button.
   * **Zoom LiveTrak L-12 Multi-channel Routing:** Preconfigures **Channels 11/12 (Stereo Line)** as standard for tape decks and phono preamps.
   * **Real-time Studio LED Ladder Meters:** 36-segment responsive meters with peak-hold ticks, numeric dBFS readouts, and clickable clip indicators.
   * **Immediate Persistence:** Audio and project files are safely flushed to disk before analysis begins.
   * **24-bit Studio Quality:** Pure uncompressed 24-bit PCM WAV.

2. **📊 Recording Analysis beside the Input Meters**
   * **Signal Analysis:** Peak L/R (dBFS), headroom, DC-offset detection, and channel balance deviation (L/R Δ dB).
   * **Zoom L-12 Hardware Advice:** Recommends exact potentiometer adjustments (e.g. *"+4 dB"* or *"-2 dB"*).

3. **✂️ Track Splicing, Precision Scope & Mastering Export**
   * **Full-Width Stereo Waveform:** High-resolution waveform occupying full window width for clear navigation and overview.
   * **Precision Splice Scope (Schnitt-Lupe):**
     * Distinct track zone shading (◀ END Txx vs START Tyy ▶).
     * **3-Transient Limit:** Restricts transient markers to the top 3 most prominent musical onsets around the splice point to prevent visual clutter.
     * **Zero-Crossing Snap:** Magnetically aligns cuts to sample zero-crossings (`∿ Zero-Crossing`) to reduce clicks and pops.
     * Zoomable down to individual samples with micro-nudge controls (`±1 Smp`, `±10 ms`).
   * **Compact Tracklist & Batch Title Import:**
     * Single-line compact track rows showing >= 20 characters comfortably.
     * **Batch-Paste:** Paste complete tracklists from Discogs, Wikipedia, or Bandcamp to populate titles automatically.
   * **Recording revisions:** Range deletion, cropping, and rendered filters create a new WAV. Undo and redo switch the WAV and its markers together during the session. Save Project stores the active revision.
   * **Analog Mastering Export Options:**
     * **20 Hz Subsonic / Rumble Filter:** Removes turntable rumble and subsonic DC flutter.
     * **Cut fades:** Off, `15 ms`, `100 ms`, or `300 ms` at track starts and ends.
     * **L/R trim:** Manual channel balance adjustment for tracks using the global export settings.
     * **Normalization:** Original level, per-track peak normalization to −0.3 dBFS, or shared gain for the album side.

---

## ⌨️ Tastaturkürzel / Keyboard Shortcuts

| Taste / Key | Funktion (DE) | Function (EN) |
|---|---|---|
| **⌘ + S** | Projekt speichern (.tapedigizer) | Save Project (.tapedigizer) |
| **⌘ + O** | Projekt öffnen | Open Project |
| **⌘ + F** | Song-Studio öffnen | Open Song Studio |
| **⌘ + Z** | Letzte Aktion rückgängig machen (Undo) | Undo last marker action |
| **⇧ + ⌘ + Z** / **⌘ + Y** | Wiederherstellen (Redo) | Redo marker action |
| **Tab** / **Enter** | Zum nächsten Tracktitel springen | Jump to next track title |
| **Leertaste / Space** | Wiedergabe starten / pausieren | Toggle Play / Pause |
| **⌥ + Mausrad** | Hinein- / Herauszoomen (auf Cursor) | Zoom in / out (centered on mouse) |
| **Horizontal** | Wellenform verschieben | Pan the waveform |
| **H** | Hover-Vorhören an- / ausschalten | Toggle Hover Audition on / off |
| **S** oder **C** | Schnitt-Werkzeug (✂️) umschalten | Toggle Split / Scissor Tool |
| **V** oder **Esc** | Zurück zum Zeiger / Navigieren | Return to Pointer / Select Tool |
| **M** | Marker an Abspielposition setzen | Drop marker at playhead |
| **Delete / ⌫** | Ausgewählten Marker löschen (⌘Z macht rückgängig) | Delete selected marker (⌘Z to undo) |

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
