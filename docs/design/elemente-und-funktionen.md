# Elemente und Funktionen

| Element | Aufgabe und Verhalten | Bestehender Code |
|---|---|---|
| Arbeitsbereich | Kompakter Wechsel zwischen Aufnahme und Schnitt. Aktiver Bereich ist beschriftet und sichtbar eingerastet. | `src/ui/app.rs`, `src/ui/theme.rs` |
| Transport | Aufnahme starten oder stoppen, Wiedergabe, Stopp, Schleife und Zeitcode. REC und CLIP sind eigene Zustände. | `src/ui/app.rs`, `src/ui/song_editor.rs` |
| Signaldisplay | Live-, Projekt- oder Song-Wellenform je nach Arbeitsbereich. Anzeigen für L/R oder Mono, Zeitlineal, Position und Auswahl. | `src/ui/waveform_widget.rs` |
| Pegelanzeige | Eingangs- oder Ausgangspegel je nach Ansicht. Kalibrierte dBFS-Skala mit Peak und beschrifteter CLIP-Anzeige. | `src/ui/meter_widget.rs` |
| Fader | Links und Rechts im Monomix unabhängig mischen. Skala und Zahlenwert bleiben beim Einstellen sichtbar. Werte lassen sich per Tastatur ändern und direkt eingeben. | `src/ui/song_editor.rs` |
| Drehregler | Kontinuierliche Parameter wie Ausgangspegel, Stereobreite oder Schwellenwert. Zeiger und Skalenbogen zeigen die Stellung, die Zahl darunter den exakten Wert. Tastatur und Direkteingabe stehen neben Ziehen zur Verfügung. | `src/ui/theme.rs`, `src/ui/song_editor.rs` |
| Schalter und Reiter | Einrastende Zustände für Kanalwahl, Module, Bypass und Abhörmodi. Nur relevante Parameter sind eingeblendet. Kurzzeitiges Solo und Originalvergleich sind als Abhörzustände gekennzeichnet. | `src/ui/song_editor.rs` |
| Nachrichten | Hinweise und Fehler stehen am betroffenen Modul oder direkt unter dem Transport. Kritische Meldungen enthalten Text, selbst wenn die Farbe nicht wahrgenommen wird. | `src/ui/app.rs` |
| Export | Zielformat und Umfang explizit benennen. Für zwei Monospuren werden beide Dateinamen gezeigt. | `src/ui/song_editor.rs`, `src/ui/app.rs` |

Die Gestaltung verwendet vorhandene Song- und Projekteinstellungen. Ein neuer Regler muss dieselben Wertebereiche, Einheiten und Abhör- oder Exportregeln übernehmen. Die Vorschau simuliert nur Bedienelemente und Signalwerte. Sie nimmt kein Audio auf und schreibt keine Dateien.

Cut-Magnifier und Wellenform verwenden denselben Displayrahmen. Im Dynamikmodul teilen sich Compressor und Expander eine Frontplatte und werden über zwei Tasten gewechselt. Die jeweiligen Parameter bleiben beim Wechsel erhalten.
