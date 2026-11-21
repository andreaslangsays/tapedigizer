# Designsprache · macOS Studio Craft

## Gestaltungsprinzip

TapeDigizer orientiert sich an den **Apple macOS Human Interface Guidelines (Dark Mode)** in Verbindung mit der taktilen Präzision klassischer Studio-Hardware (wie Apple Logic Pro). Eine ruhige, neutrale Graphit- und Aluminium-Basis stellt das Audiosignal in den Mittelpunkt.

Funktionsbereiche werden **flächig** über abgestufte Helligkeits-Ebenen (*Surface Elevation*) und klaren Weißraum voneinander getrennt — ohne `1 px`-Rahmen um Panels oder Track-Zeilen. Tiefe vermittelt Bedienbarkeit: Wellenformen, Pegelmeter, Timecode und Segmented-Mulden sitzen vertieft hinter dunklem Displayglas (innerer Schatten oben, hauchfeiner Lichtsaum unten). Buttons, aktive Segmente, Drehregler und Fader heben sich über leichte vertikale Farbverläufe, oberes Kantenlicht (*Top Specular Highlight*) und sanfte Kontaktschatten plastisch ab.

## Flächen- & Farbsystem (Surface Elevation)

| Ebene / Rolle | Farbe / Verlauf | Verwendung |
|---|---|---|
| Level 0 · Displayglas | `#0B0D10` | Wellenform, Stereo-Meter, Timecode, Segmented-Mulden |
| Level 1 · Fensterfläche | `#16181C` | Grundfläche zwischen Funktions-Bays |
| Level 2 · Unified Toolbar | `#1E2127` | Obere macOS-Kopfzeile (Navigation, Projektaktionen, Dokumenttitel) |
| Level 3 · Surface Bay | `#21252C` | Rahmenlose Funktionsflächen (Aufnahme-Setup, Transport, Schnitt-Lupe, Track-Rack) |
| Level 3b · Listenfläche | `#272C35` | Alternierende Zeilen in der Trackliste |
| Level 4 · Bezel Control | `#3A404B → #2C313A` | Taktile macOS-Buttons und aktive Segmente (leichter vertikaler Verlauf) |
| macOS-Systemakzent | `#4C9BFF → #2678E5` | Primäraktionen, aktive Zustände, Auswahl |
| Fokusring | `#7CC0FF` | Sichtbarer 2-px-Tastaturfokus |
| Text | `#F2F4F8` / `#B8C0CC` / `#8E98A8` | Primär, Sekundär und Skalen (WCAG 2.2 AA ≥ 4,5:1) |
| Signal L / R | `#5CE0B8` / `#89B4FF` | Wellenform-Kanäle und L/R-Kennung |
| Schnitt & Skala | `#F5B952` | Schnittmarken, Auswahlbereich, beleuchtete Rotary-Skala |
| Aufnahme & Alarm | `#E84C52 → #C42B33` | REC-Taste, Aufnahme-Status und CLIP-Anzeige |

## Größenverhältnisse & Geometrie (4/8-pt-Raster)

- **3 feste Höhenklassen**: `30 pt` (Toolbar-Segmented, REC, Transport, Primäraktionen), `26 pt` (Standard-Buttons, Dropdowns, Modul-Segmented), `22 pt` (kompakte Zeilen-Aktionen in der Trackliste).
- **3 feste Eckenradien**: `6 pt` (Buttons & aktive Segmente), `8 pt` (Display-Mulden & Segmented-Tracks), `10 pt` (Surface Bays).
- **Typografie**: Systemschrift (SF Pro) für Bedienelemente (`12–15 pt`), Monospace für Timecode (`15 pt`), dBFS-Werte und Skalen (`11 pt`). Keine System-Emojis in Bedienknöpfen.

## Informationsarchitektur: Zwei Hauptbereiche

Die obere Unified Toolbar schaltet über ein zentrales macOS Segmented Control zwischen den beiden Hauptbereichen um:

1. **Aufnahme**: REC-Taste mit Verlauf, Status/Timecode und Stereo-Eingangsmeter in einer gemeinsamen Surface Bay; darunter die Live-Wellenform im Display-Well und zwei rahmenlose Surface Bays für *Quelle & Projekt* sowie *Aufnahmedauer & Auto-Stopp*.
2. **Schnitt**:
   - **Band-Übersicht**: Schlanke Sub-Navigation als Segmented Control (`Schnitt & Marker` | `Trackliste` | `Export`) plus direkter Einstieg **`Track bearbeiten →`**.
   - **Fokussierter Track-Bereich (Einzeltrack-Bearbeitung)**: Blendet die Band-Navigation vollständig aus und widmet das Fenster ausschließlich dem gewählten Track — mit Fokus-Kopfzeile (`← Zurück zum Schnitt`, Track-Wähler, `Track exportieren …`), lokalem Transport + Ausgangsmeter (`Bearbeitet | Original`), isolierter Song-Wellenform ab `00:00` und einer Processing-Bay in DSP-Signalfluss-Reihenfolge (`Kanäle` → `Restauration` → `Equalizer` → `Dynamik` → `Mix & Ausgabe`) mit taktilen Drehreglern und Hardware-Fadern.

## Barrierefreiheit & Prüfkriterien

- Kontrast von mindestens 4,5:1 für regulären Text und ≥ 3:1 für Regler, Skalen und Fokusringe.
- Kein Zustand wird ausschließlich über Farbe vermittelt (REC, CLIP, Bypass, Solo, Original-Vergleich und aktive Segmente besitzen Text und/oder Form).
- Vollständige Tastaturbedienung und `AccessKit`-Anbindung für Drehregler, Hardware-Fader, Segmented Controls, Bezel-Buttons und das Stereo-Meter.
- Automatisierte Layout-Prüfung (`cargo test`) in Deutsch und Englisch bei `1060 × 700` und `1260 × 820` ohne Überlauf.
