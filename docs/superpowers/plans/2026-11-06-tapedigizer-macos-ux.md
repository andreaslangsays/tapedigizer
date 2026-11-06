# TapeDigizer – Umsetzungsplan für macOS, Track-Marker und Audio

Rust bleibt gesetzt. Der Plan geht von Rust mit eframe/egui für Oberfläche und Wellenform sowie CPAL für Audio aus. Für Systemmenüs und macOS-spezifisches Fensterverhalten ist bei Bedarf eine gezielte Anbindung aus Rust vorgesehen. Bestehende Projekte aus Version 1.2 bleiben lesbar. Ob ältere App-Versionen ein neu gespeichertes Projekt öffnen können, hängt von der Formatentscheidung in Etappe 3 ab und muss vor einer Formatänderung festgelegt werden.

## 1. Ausgangslage messen

- [ ] Einen reproduzierbaren Satz synthetischer Stereo-WAVs für 45 und 60 Minuten bei 44,1 und 48 kHz erzeugen. Zusätzliche kurze Dateien decken bekannte Impulse, Stille und Signale nahe Vollaussteuerung ab. Ein bestehendes 1.2-Projekt mit Titeln und Pausen dient als Kompatibilitätsfixture.
- [ ] Auf einem dokumentierten Mac im Release-Build Öffnen bis zur ersten Wellenform, Spitzen-RAM, CPU im Leerlauf, Reaktionszeit beim Zoomen und Exportdauer erfassen. Hardwareprüfung mit Zoom L-12 separat führen. Geräteabhängige Tests und deterministische Tests trennen.
- Abnahme. Messwerte, Hardware, Dateilängen und Befehle sind dokumentiert und lassen sich in Etappe 5 wiederholen.

## 2. Aufnahme, Speichern und Wiedergabe absichern

- [ ] In `src/audio/recorder.rs` einen eindeutigen Start-/Stopp-Lebenszyklus mit einem Ergebnis des WAV-Writers einführen. Datei-Erstellung, Schreiben, Queue-Überlauf und Finalisieren bis zur Oberfläche melden. Der Audio-Callback muss beim Übergang zum Writer ohne blockierendes Senden auskommen. Bei Rückstau eine unterbrochene Aufnahme eindeutig melden und verwertbare Daten zur Wiederherstellung erhalten.
- [ ] In `src/project.rs` und `src/ui/app.rs` Aufnahmeziele eindeutig vergeben und vorhandene Aufnahmen schützen. Begleit-WAV vor der Projektdatei validiert bereitstellen. Metadaten über eine temporäre Datei im Zielverzeichnis atomar ersetzen. Den aktiven Projektzustand erst nach erfolgreichem Laden umschalten. Rohaufnahme bei fehlgeschlagener Analyse behalten. Vor Neu, Öffnen oder Schließen ungespeicherte Änderungen und laufende Aufnahmen berücksichtigen.
- [ ] In `src/audio/player.rs` Quelle und Ausgabegerät mit unterschiedlichen Sampleraten korrekt abspielen. Wiedergabe, Seek und Bereichsende auf Quellframes beziehen. Ungültige Formate und Gerätefehler an die Oberfläche melden.
- Abnahme. Tests decken Kollisionen, Schreib- und Finalisierungsfehler, unterbrochene Aufnahme, fehlerhaftes Öffnen und 44,1/48 kHz in beiden Richtungen ab. Ein älteres Projekt lässt sich mit zugehöriger WAV öffnen.

## 3. Marker als durchgängigen Editor gestalten

- [ ] In `src/splitter.rs` einen gemeinsamen Änderungspfad für Setzen, Verschieben, Löschen, Pause und automatische Vorschläge definieren. Positionen als ganzzahlige Quellframes führen. Auswahl über stabile Marker-Identität und Metadaten über eindeutige Segmentzuordnung halten. Dieselben Grenzen für Drag, Tastatur, Inspector und Snap anwenden. In `src/project.rs` alte Sekundenpositionen nach Laden der WAV übernehmen und die Zuordnung der Titel und Pausen prüfen.
- [ ] Ziehen in `src/ui/waveform_widget.rs` am Mausdruck eindeutig an einen Marker binden. Live-Vorschau während des Zugs, eine Undo-Aktion beim Loslassen, Escape zum Abbrechen. Überlappungen und Grenzfälle zentral behandeln. Beim Teilen behält der linke Abschnitt den bisherigen Titel, beim Zusammenführen wird der linke Titel bevorzugt. Den Umgang mit Pause/Track-Mischungen sichtbar und per Undo umkehrbar gestalten.
- [ ] In `src/ui/app.rs` und `src/i18n.rs` dieselbe Auswahl in Timeline, Trackliste und Inspector anzeigen. Klick auf die Zeitleiste setzt den Abspielkopf. Klick auf das Fähnchen wählt die Schnittgrenze. `M` setzt einen Marker am Abspielkopf, sofern kein Textfeld aktiv ist. Inspector für direkten Zeitcode, Schritte von einem Frame oder Millisekunden, explizites Einrasten und Vorhören beider Seiten nutzen. Pausenerkennung als Vorschau mit Übernehmen und Verwerfen anbieten.
- Abnahme. Ein langer Zug ergibt genau einen Undo-Schritt. Titel und Pausen bleiben bei Einfügen, Löschen, Undo/Redo und Wiederöffnen den richtigen Segmenten zugeordnet. Ein Impuls wird an derselben Framegrenze angezeigt, vorgehört und exportiert. Textfelder behalten ihre normalen Tastaturbefehle.

## 4. Oberfläche und macOS-Bedienung überarbeiten

- [ ] `src/ui/app.rs` nach Aufnahme, Schnitt und Export gliedern. Aufnahme zeigt Quelle, Kanalpaar, große Pegelanzeige, Aufnahmezustand und Dauer. Pegeldiagnose erscheint nach Stopp als Zusammenfassung mit Details bei Bedarf. Im Schnitt bleibt der Transport beim Scrollen sichtbar. Die Wellenform bekommt den größten Platz, Trackliste und Inspector lassen sich ein- und ausblenden. Der Exportdialog zeigt Ziel, Dateinamen, Normalisierung, ausgelassene Pausen und Namenskonflikte vor dem Start.
- [ ] In `src/main.rs`, `src/ui/theme.rs` und `src/i18n.rs` Systemdarstellung, konsistente Typografie, Fokus, Kontraste und Text für Statuszustände umsetzen. Akzentfarben für Auswahl, Aufnahme und Warnungen reservieren. Ablage, Bearbeiten und Ansicht in die macOS-Systemmenüleiste integrieren und mit denselben Befehlen wie die Toolbar verbinden. In einer kleinen Machbarkeitsprüfung VoiceOver und Fokussteuerung der egui-Bedienelemente auf dem Zielsystem prüfen. Bei Lücken gezielt Systemkomponenten aus Rust anbinden.
- Abnahme. Aufnahme, Markerschnitt und Export funktionieren mit Maus, Trackpad und Tastatur auf kleinem Fenster sowie großem Retina-Display. Deutsche und englische Texte, helle und dunkle Darstellung, Fokusreihenfolge und VoiceOver für Kernaktionen sind geprüft.

## 5. Lange Sessions beschleunigen und Release prüfen

- [ ] In `src/ui/app.rs`, `src/analysis.rs` und `src/splitter.rs` Dateioperationen, Analyse, Pausenerkennung und Export aus dem UI-Pfad verlagern. Fortschritt, Fehler und Abbruch klar darstellen. Hintergrundresultate mit Projektkennung versehen, damit ein Projektwechsel keine alten Ergebnisse übernimmt. Exportdateien erst nach erfolgreichem Finalisieren veröffentlichen und Kollisionen vorab behandeln.
- [ ] In `src/ui/waveform_widget.rs` und `src/audio/player.rs` zuerst mehrfaches Dekodieren und die zweite vollständige PCM-Kopie beim Export vermeiden. Min/Max-Pegel mehrstufig cachen und Transienten nur bei geändertem Audio, Bereich oder Empfindlichkeit neu berechnen. Repaint auf laufende oder geänderte Inhalte begrenzen. Falls die RAM-Messung es erfordert, dateibasierten Zugriff mit begrenztem PCM-Cache umsetzen.
- [ ] Exporttests für alle Normalisierungsmodi, exakt zugeschnittene Bereiche, defekte WAVs, ausgeschlossene Pausen und Fehler mitten im Batch ergänzen. Bei vorhandener 24-Bit-Quelle den Modus ohne Normalisierung auf identische PCM-Werte prüfen. Messung aus Etappe 1 wiederholen. Vorläufiges Ziel für den Referenz-Mac ist eine 95. Perzentile unter 33 ms pro UI-Frame beim Zoomen einer 60-Minuten-Datei. Analyse und Export lassen Fenster und Bedienelemente responsiv. Aufnahmeprüfung mit dem L-12 über eine volle Bandseite inklusive Kanalrouting, Geräteabzug und verweigerter Berechtigung.
- [ ] In `scripts/package_app.sh` Paketbau von lokaler Installation und Desktop-Neustarts trennen. Unterstützte macOS-Versionen und Architekturen am tatsächlich installierten Release testen. Für öffentliche Direktverteilung Developer-ID-Signatur, Notarisierung und einen Gatekeeper-Test auf einem sauberen System einplanen. [Apple-Dokumentation zur Notarisierung](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)

## Freigabekriterien

- Neuaufnahme und fehlgeschlagene Dateioperationen gefährden keine bestehende Aufnahme. Stopp, Speichern und Wiederöffnen erhalten Audio und Projektmetadaten.
- Marker, Vorhören und WAV-Export verwenden identische Quellframe-Grenzen. Alte Projekte bestehen den Lade- und Speicher-Rundlauf.
- Eine volle Bandseite lässt sich mit dem L-12 aufnehmen, schneiden und exportieren. Testläufe und Profiling belegen die Ergebnisse auf den ausgewiesenen macOS-Zielen.

## Gemessener Stand

Release-Profil auf einem Apple M1 Pro mit 16 GB RAM unter macOS 26.7.1. Vergleich mit Commit `896fd95` bei identischen synthetischen WAV-Dateien. „Vorbereitung“ umfasst WAV-Dekodierung, Wellenform-Cache und Pegeldiagnose. Der Zoomwert misst die CPU-Zeit für die Wellenform in einem headless-egui-Kontext. Für eine vollständige Messung des Fensters sind ein interaktiver Lauf und eine GPU-Aufzeichnung erforderlich.

| Datei | Vorbereitung vorher → jetzt | Export vorher → jetzt | Zoom CPU p95 vorher → jetzt |
| --- | ---: | ---: | ---: |
| 45 min · 44,1 kHz | 5,41 → 2,61 s | 6,46 → 5,42 s | 0,07 → 0,07 ms |
| 60 min · 48 kHz | 7,35 → 3,91 s | 8,68 → 8,23 s | 0,07 → 0,07 ms |

Die höchste gemessene RSS lag im abschließenden Testlauf bei rund 1,86 GiB. Der Player hält die dekodierte Aufnahme für Wiedergabe und Detailansicht im Speicher. Der Export liest PCM abschnittsweise aus der Quelldatei.

`cargo test --locked` meldete 19 bestandene Tests. Der separate Langzeittest bestand beide Profile. Das arm64-Bundle wurde gebaut und seine Ad-hoc-Signatur geprüft. Die native Menüleiste und der Öffnen-Dialog wurden am gestarteten Bundle beobachtet.

Für die öffentliche Freigabe stehen der vollständige Aufnahme- und Exportlauf mit dem Zoom L-12, Geräteabzug und Berechtigungsfälle, die interaktive Prüfung auf weiteren macOS-Zielsystemen, ein VoiceOver-Durchlauf sowie Developer-ID-Signatur und Notarisierung aus.
