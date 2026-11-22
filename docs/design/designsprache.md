# Designsprache · Silverface und Obsidian

TapeDigizer übernimmt die Anordnung eines Studiogeräts. Messwerte, Wellenformen und Kennlinien liegen in vertieften Anzeigen. Abgesetzte Modulflächen, dezente Konturen und leicht plastische Tasten und Potis machen die Bedienebenen sichtbar. Dunkle Spektral- und LED-Anzeigen können auch in Silverface stehen.

## Farben und Material

**Silverface** ist beim Start aktiv und kombiniert warmes Silber mit Kupferorange und Petrol. **Obsidian** kombiniert Graphit mit warmem Amber und Phosphorgrün. Die Schaltfläche **Dunkel** bzw. **Hell** in der Titelleiste schaltet das Theme für die laufende Sitzung um. Die Titelleiste, Komponenten und Signalflächen lesen ihre Farbrollen aus dem aktiven egui-Kontext. Die Werte in der Tabelle stammen aus der gemeinsamen [Palette](../../src/ui/theme.rs). OLED-Pegel und Spektrum verwenden in beiden Themes die dunkle Displaypalette.

| Rolle | Silverface | Obsidian |
|---|---|---|
| Fensterfläche | `#E5E2DC` | `#121316` |
| Titelleiste | `#EDEAE4` | `#1A1C22` |
| Modulfläche | `#F9F8F6` | `#1B1D23` |
| Vertiefte Anzeige | `#F5F3EE` | `#0D0E11` |
| Primärtext | `#121417` | `#E3E2E6` |
| Primäraktion und aktives Segment | `#D46A20 → #BD5612` | `#FFC174 → #F59E0B` |
| Kanalakzente L / R | `#C45B18` / `#0E7490` | `#F59E0B` / `#4EDEA3` |
| Aufnahme und Übersteuerung | `#BA1A1A` | `#EF4444` |
| Tastaturfokus | `#0E7490` | `#FFB95F` |

## Layout und Bedienung

- Die Unified Titlebar verbindet die nativen macOS-Fenstertasten mit **Aufnahme | Schnitt**. Unter **Schnitt** erscheint daneben **Marker | Tracks | Einzeltrack | Export**. Theme- und Sprachwechsel stehen rechts.
- Die Aufnahme zeigt Eingangspegel und Aufnahmebefund nebeneinander. Darunter folgen Bandtransport und Live-Wellenform sowie die Einstellbereiche für Quelle, Projekt, Aufnahmedauer und Auto-Stopp.
- Im Einzeltrack bleibt die Titelleistennavigation sichtbar. Darunter gehören Track-Wähler, Transport, Wellenform und Effekt-Rack dem ausgewählten Song. Das Rack zeigt jeweils das gewählte Modul.
- Nachbarpanels in einer Zeile teilen sich die Breite gleichmäßig und haben einen festen Abstand von **12 pt**. Die Zeile richtet sich nach dem höchsten Inhalt, sodass ihre Ober- und Unterkanten auch bei Sprachwechsel und langen Beschriftungen bündig bleiben. Banddeck und Live-Wellenform teilen sich ebenfalls das Raster und die Anzeigehöhe.
- Die Konsole ist ab **1060 × 700 pt** auf ein Layout ohne vertikales Scrollen ausgelegt. Das Startfenster misst **1260 × 820 pt**. Wellenformen und Teile des Racks passen ihre Höhe an. Trackliste und Exportvorschau passen die Zeilenzahl pro Seite an.
- Die gemeinsamen Höhenklassen sind **28 pt** für Primäraktionen und Navigation, **25 pt** für Standardbedienelemente und **22 pt** für kompakte Aktionen. Die Oberfläche lädt SF Pro und SF Mono, sofern die Schriftdateien verfügbar sind. Zahlenanzeigen wie der Timecode nutzen Monospace.

## Prüfkriterien

- Regulärer Text soll mindestens **4,5:1** Kontrast erreichen. Für bedienrelevante grafische Elemente gilt **3:1** als Prüfziel. REC, CLIP, Bypass und aktive Segmente erhalten zusätzlich eine Beschriftung oder Form.
- Potis melden Name, Wert und Wertebereich über AccessKit und unterstützen Tastaturschritte, Direkteingabe und Wertänderungen über Bedienungshilfen. Die vollständige Tastatur- und VoiceOver-Navigation durch den Workflow bleibt ein manuelles Prüfkriterium.
- Im unteren Einzeltrack-Rack verwenden die Insert-Buttons und der Hauptmix dieselbe Breite. Die Akzentfläche markiert das ausgewählte Panel, der Punkt den Aktivzustand. Die kompakte Vergleichsanzeige verwendet die Textfarbe für den Eingang, den Hauptakzent für den Ausgang und Türkis für das Differenzsignal.
- Der Layouttest `console_renders_before_and_after_recording` prüft die Ansichten mit Testdaten in Deutsch und Englisch bei beiden Fenstergrößen auf Überlauf. Er deckt beide Themes, lange Tracktitel und alle sechs Inserts sowie den Hauptmix ab. Visuelle Prüfungen ergänzen ihn für Fokus, Kontrast und Zustandswechsel.
