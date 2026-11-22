# Implementierungsplan für das TapeDigizer-Designsystem

**Ziel.** Die Studiokonsole aus TapeDigizer 1.8.0 verwendet gemeinsame, im Rust-Code definierte UI-Bausteine. Eine spätere Änderung an Form, Zeiger oder Skala eines Potis gilt für alle Potis, ohne Anpassungen in den Effektmodulen.

**Vorschlag für die Architektur.** Farben, Schrift und Grundmaße liegen im Theme. Jede Komponentenfamilie besitzt eine zentrale Darstellung und ein gemeinsames Bedienverhalten. Die Ansichten liefern Beschriftung, Wertbindung, Einheit und fachliche Bedeutung. Rust und egui 0.29 bleiben die Grundlage.

## Entscheidung und Umfang

Eine reine Farbpalette wäre schnell zentralisiert, würde aber Änderungen an Form und Zeichnung nur dort erfassen, wo Ansichten bereits denselben Renderer nutzen. Ein externes Theme-Format würde Änderungen ohne neuen Build erlauben, bräuchte aber ein Schema und für neue Formen weiterhin ausführbaren Zeichencode. Vorgeschlagen ist ein internes System mit typisierten Rollen und einem Renderer pro Komponentenfamilie. So lassen sich sowohl einzelne Gestaltungswerte als auch die komplette Darstellung eines Potis an einer Stelle ändern.

Die Migration orientiert sich an der bestehenden 1.8-Oberfläche. Silverface bleibt die Startansicht, Obsidian die dunkle Variante. Die fachlichen Reglerwerte und Eingaberegeln bleiben gleich. Direkte Farbkorrekturen wie das helle Zeitlineal in Obsidian werden als sichtbare Änderungen geprüft. Der Umfang ist die Oberfläche. DSP, Projektformat, Exportverfahren und die lokale Song-Normalisierung gehören zu ihren jeweils eigenen Änderungen.

## Zuständigkeiten im Code

| Datei oder Modul | Verantwortung |
|---|---|
| `src/ui/theme.rs` | `ThemeMode`, Rollenfarben, Typografie, Abstände, gemeinsame Größen und Abbildung auf `egui::Style`. Aktives Theme am `egui::Context` statt im Prozess-Global. |
| `src/ui/widgets/rotary.rs` | Ein öffentlicher Poti-Einstieg, gemeinsame Werteingabe und private Zeichnung. |
| `src/ui/widgets/` | Weitere gemeinsame Familien für Tasten und Segmente, Fader, Eingabefelder und Panelrahmen. Eine Familie wird ausgelagert, sobald ihre Nutzer migriert werden. |
| `src/ui/meter_widget.rs`, `src/ui/waveform_widget.rs` | Signalbezogene Darstellung und Interaktion. Gemeinsame Farben, Anzeigenrahmen und Skalenstile aus Theme und Widgets beziehen. |
| `src/ui/app.rs`, `src/ui/song_editor.rs` | Ansichten, Anordnung und Wertbindungen. Modi und Parameter auswählen und die gemeinsamen Komponenten tatsächlich aufrufen. |

## 1. Inventar und Theme in den Ansichten verankern

- [x] In `docs/design/elemente-und-funktionen.md` pro Komponentenfamilie die zentrale Implementierung und Einsatzorte erfassen. Aufnahme, Marker, Tracks, Einzeltrack und Export abdecken. Standard-egui-Bedienelemente, Sonderanzeigen und bisherige feste Farbwerte mit erfassen.
- [x] `apply_theme(ctx: &egui::Context, mode: ThemeMode)` und `palette(ctx: &egui::Context) -> Palette` in `theme.rs` verwenden. Silverface als Vorgabe für einen neuen Kontext. Die bestehende Theme-Umschaltung in `app.rs` darüber führen. Stil und Rollenfarben kommen aus demselben Modus.
- [x] Alles, was die bisherige prozessweite Palette liest, auf den aufrufenden Kontext umstellen. Einen Test mit zwei Kontexten ergänzen. Das Umschalten des einen auf Obsidian darf die Palette des anderen nicht ändern.
- [ ] Vollständige Vorher-Referenzen bei `1260 × 820 pt` in beiden Themes erfassen. Die bisherige Live-Prüfung und die verlinkten Entwurfsreferenzen waren die Grundlage für den Vergleich der Konsole.

## 2. Potis vollständig über die gemeinsame Komponente führen

- [x] `rotary_colored_sized` aus `theme.rs` als Ausgangspunkt nehmen. Ziehen, Mausrad, Shift-Feinmodus, Pfeiltasten, Doppelklick bipolarer Regler, AccessKit und Direkteingabe einschließlich bisheriger Skalierung beibehalten. Die Zeichnung von Bogen, Zeiger, Kappe, Schatten und Fokus als privaten Teil in `widgets/rotary.rs` führen.
- [x] Einen Einstieg `rotary(ui: &mut egui::Ui, value: &mut f32, spec: RotarySpec<'_>)` bereitstellen. `RotarySpec` enthält Label, Wertebereich, Einheit, semantischen Akzent und eine benannte Größenvariante. Die Größenvariante bestimmt die verfügbare Fläche. Form, Bogen und Zeiger werden zentral relativ dazu berechnet.
- [x] Sämtliche Poti-Aufrufe in Kanal, Restauration, EQ, Dynamik und Mix migrieren. Konkrete Farben aus den Aufrufen entfernen. Die bisherigen sichtbaren Akzente über semantische Rollen abbilden. Nach der Umstellung die drei bisherigen Poti-Einstiege entfernen und per Quelltextsuche prüfen, dass alle produktiven Aufrufe auf die neue Komponente führen.
- [x] `rotary_accepts_accessibility_values_and_keyboard_steps` und `rotary_responds_to_vertical_pointer_drag` auf den neuen Einstieg umstellen. Einen gezielten Test für einen positiven logarithmischen und einen bipolaren Wertebereich ergänzen, falls die bestehenden Tests die Skalierung nicht abdecken.
- [x] Den zentralen Zeiger im lokalen Prüfbuild sichtbar verändern. Der Render-Test hat den geänderten Zeichenstrich in Kanal, Restauration, EQ, Compressor, Expander, Limiter und Mix in beiden Themes nachgewiesen. Der reguläre Build verwendet den 1.8-Zeiger.

## 3. Weitere Komponenten im gesamten Workflow einsetzen

- [x] Tasten und Segmentsteuerung aus `theme.rs` als gemeinsame Komponenten führen. Aufnahme und Transport, Schnittnavigation, Trackaktionen und Export auf die passenden Varianten umstellen. Für standardmäßige Felder und Auswahllisten den gemeinsamen egui-Stil verwenden.
- [x] Fader, Modulrahmen und Statusanzeigen an ihren vorhandenen Einsatzorten migrieren. Für Ausrichtung und Zielgrößen dieselben Varianten in den Ansichten verwenden. Fachliche Inhalte wie CLIP, REC und Fehler weiterhin als Text erkennbar machen.
- [x] In Pegelanzeigen, Wellenformen, Lupe und DSP-Kennlinien die festen Farben für gemeinsame Rollen durch Theme-Farben ersetzen. Wellenformhintergrund, Zeitlineal, Kanalmarkierung und Overlay jeweils zusammen prüfen. Das absichtlich dunkle Spektraldisplay als eigene Farbrolle führen.
- [x] In den fünf Arbeitsansichten nach geerbten Alternativ-Widgets und verstreuten Darstellungswerten suchen. Gleichartige Verwendungen auf den gemeinsamen Baustein umstellen. Fachspezifische Zeichnung wie EQ-Kurve und Bandtransport in ihren jeweiligen Modulen belassen.

- [x] Nachbarpanels in gleich breite Spalten mit bündigen Ober- und Unterkanten setzen. `panel_row` ermittelt die gemeinsame Höhe im selben Frame. Aufnahme-Setup, Einstieg und Effekt-Rack verwenden diese Zeile. Banddeck und Live-Wellenform erhalten dieselbe Spaltenbreite und Anzeigehöhe.

## 4. Abnahme und Dokumentation

- [x] Den bestehenden Test `console_renders_before_and_after_recording` für beide Themes ausführen und dafür die Theme-Schleife ergänzen. Abdecken sind Deutsch und Englisch, `1060 × 700 pt` und `1260 × 820 pt`, alle Arbeitsansichten und Effektmodule. `cargo test --locked` muss ohne Fehler laufen.
- [x] Die reguläre App mit synthetischer WAV live prüfen. Der macOS-Prüfbuild zeigte Aufnahme und Marker in Silverface sowie Einzeltrack mit dunklem Zeitlineal und Exportvorschau in Obsidian. Mindestgröße, lange Tracktitel, deaktivierte Module, Regler-Drag, Tastatur und Wertänderung über AccessKit wurden automatisiert geprüft.
- [x] `docs/design/designsprache.md` und `docs/design/elemente-und-funktionen.md` mit den tatsächlichen Modulpfaden aktualisieren. Am Poti dokumentieren, wo Form, Zeiger und Farben geändert werden und wo Einheit, Wertebereich sowie Bindung an den DSP definiert sind.

**Fertig ist die Migration**, wenn die regulären App-Ansichten das System verwenden und eine Änderung allein im Poti-Renderer über alle Poti-Einsatzorte sichtbar ist. Der Prüfbuild weist das exemplarisch nach. Presets oder ein Theme-Editor zur Laufzeit wären ein späterer eigener Schritt.
