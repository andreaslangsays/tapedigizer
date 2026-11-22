# Elemente und Funktionen · v2.0.0

| Ansicht / Element | Bedienung und Verhalten | Implementierung |
|---|---|---|
| Aufnahme | Aufnahme oder Pegelcheck starten, zwischen analogen **VU**-Zeigern und **LED**-Pegelketten wechseln. Der Aufnahmebefund steht neben den Eingangspegeln. Das Banddeck mit Zählwerk begleitet die Live-Wellenform. | [Konsole](../../src/ui/app.rs), [Meter](../../src/ui/meter_widget.rs), [Banddeck](../../src/ui/tape_deck.rs) |
| Marker | Ohne geladenes Audio führt **Projekt / WAV laden …** in den Schnitt. Mit Audio stehen die vollständige Stereo-Wellenform und darunter **Schnitt-Lupe** bzw. **Aufnahme bearbeiten** zur Verfügung. Die Lupe zeigt Trackgrenzen, Nulldurchgang und bis zu drei Transienten. | [Konsole](../../src/ui/app.rs), [Wellenform](../../src/ui/waveform_widget.rs) |
| Tracks | Titel in kompakten Zeilen bearbeiten oder eine Titelliste einfügen, einzelne Songs vorhören, als Pause auslassen oder für den Rauschfilter markieren. **Track bearbeiten →** öffnet den ausgewählten Song. Lange Listen sind paginiert. | [Trackliste](../../src/ui/app.rs) |
| Einzeltrack | Track oben nach Nummer, Titel und Dauer auswählen. Transport, Schleife, Auswahl und Wellenform verwenden die lokale Zeit ab `00:00`. **Bearbeitet / Original**, Mono-Abhöre und Kanal-Solo sind Abhörfunktionen. **Track exportieren …** öffnet die Exportansicht für den ausgewählten Song. | [Song-Editor](../../src/ui/song_editor.rs), [Player](../../src/audio/player.rs) |
| Export | Zielordner, Audioeinstellungen und paginierte Dateivorschau stehen in einer Ansicht. Die Vorschau zeigt die resultierenden Namen, bei getrenntem Mono beide Dateien. Vorhandene Zieldateien blockieren den Export. Beim Schreiben stehen Fortschritt und Abbruch zur Verfügung. | [Exportansicht](../../src/ui/app.rs), [WAV-Export](../../src/splitter.rs) |

## Gemeinsame UI-Bausteine

| Komponentenfamilie | Einsatz | Zentrale Implementierung |
|---|---|---|
| Tasten und Segmente | Titelleiste, Aufnahme und Transport, Markerwerkzeuge, Trackaktionen, Abhörmodus, Rack und Export | [button.rs](../../src/ui/widgets/button.rs) |
| Karten, Modulflächen und Timecode | Aufnahmeaufbau, Schnitt-Lupe, Trackliste, Einzeltrack-Rack und Export. Nachbarpanels werden als bündige Zeile mit gleichen Spaltenbreiten gezeichnet | [panel.rs](../../src/ui/widgets/panel.rs) |
| Felder, Slider und Auswahllisten | Quelle und Auto-Stopp, Tracktitel, Routing und Exportparameter. Lange Songtitel werden im Titelwähler gekürzt und bei Hover vollständig angezeigt. | [egui-Stil](../../src/ui/theme.rs), [Titelwähler](../../src/ui/widgets/field.rs) |
| Potis und Fader | Kanal-Trim, Restauration, EQ, Dynamik und Mix. Die Fader mischen L und R im Monomix. | [rotary.rs](../../src/ui/widgets/rotary.rs), [fader.rs](../../src/ui/widgets/fader.rs) |
| Signal- und Statusanzeigen | Pegel, Signalverlauf, Zeitlineal, Schnitt-Lupe, CLIP, Fortschritt und DSP-Kennlinien | [Meter](../../src/ui/meter_widget.rs), [Wellenformen](../../src/ui/waveform_widget.rs), [Gemeinsame Zeichenfunktionen](../../src/ui/widgets/paint.rs) |

## WAV-Versionen der Aufnahme

Löschen, Trim und das Einrechnen des Rauschfilters in **Aufnahme bearbeiten** erzeugen eine neue WAV-Version. Die Quellaufnahme bleibt erhalten. TapeDigizer legt die Fassung im Projektordner ab. Bei einer noch ungespeicherten WAV liegt sie im TapeDigizer-Sitzungsordner. Die aktive Fassung bestimmt Wiedergabe, Wellenform, Marker und Export.

**Undo / Redo** wechselt innerhalb der Sitzung zwischen Fassungen und lädt die passenden Marker und Track-Einstellungen. Fehlt eine Datei, bleibt die aktuelle Fassung aktiv. Bereits eingerechnete Rauschfilter werden für die betroffenen Tracks in den globalen Exporteinstellungen deaktiviert. **Projekt speichern** übernimmt den Verweis auf die aktive WAV in die Projektdatei. Frühere Fassungen bleiben als Dateien erhalten und benötigen zusätzlichen Speicherplatz. Die Undo-/Redo-Historie gehört zur laufenden Sitzung. [Aufnahmebearbeitung](../../src/audio/recording_edit.rs) · [Schnitthistorie](../../src/splitter.rs)

## Einzeltrack-Rack

Die Buttons im unteren Rack zeigen den Signalweg von links nach rechts. **●** kennzeichnet einen aktiven Insert, **○** einen umgangenen. Ein Klick öffnet die Einstellungen direkt darunter. **Effekt aktiv** schaltet den ausgewählten Insert für den Track ein oder umgeht ihn mit seinen bisherigen Parametern. **←** und **→** verschieben ihn um eine Position. **Hauptmix** steht fest am rechten Ende.

Bei der Wiedergabe zeigen **Vor**, **Nach** und **Δ** die Stereo-Spitzenpegel am Eingang, am Ausgang und im Differenzsignal der ausgewählten Stufe in dBFS. **Δ** misst den Betrag von Ausgang minus Eingang und macht auch Änderungen bei gleichem Spitzenpegel sichtbar. Bei spektraler Restauration wird der Eingangspegel um die Filterlatenz ausgerichtet. Der A/B-Schalter **Bearbeitet / Original** vergleicht weiterhin den gesamten Track. [Rack-Bedienung](../../src/ui/effect_rack.rs), [Messung](../../src/audio/stage_meter.rs)

| Modul | Bedienelemente |
|---|---|
| Kanäle | Links/Rechts, Kanal-Trim, Mute, Phasenumkehr, Reglerkopplung und Kanal-Solo. |
| Restauration | Rauschprofil pro Track, Spektralanzeige, Denoiser mit Reduktion, Empfindlichkeit und Glättung sowie Subsonic-/DC-Filter. |
| Equalizer | Fünf Bänder mit ziehbaren Punkten und **Flat**-Reset. Low Shelf, Mittenband und High Shelf werden um zwei Glockenbänder mit **20 Hz bis 20 kHz**, **±18 dB** und **Q 0,2 bis 8** ergänzt. Ein Klick auf einen Punkt oder Reiter wählt die zugehörigen Potis. Beim Vorhören zeigt die Kurve das Live-Spektrum des hörbaren Ausgangssignals in dBFS. |
| Expander | Eigene Kennlinie und Parameter für Downward- oder Upward-Expansion. |
| Compressor | Eigene Kennlinie und Parameter für Threshold, Ratio, Knee, Attack, Release und Makeup. |
| Limiter | Brickwall mit Sample-Peak-Grenze, 0 ms Attack, **Ceiling** von −24 bis 0 dB, **Drive** von 0 bis +24 dB und **Release** von 5 bis 500 ms. Spätere Stufen können den Pegel weiter verändern. |
| Hauptmix | Stereo mit Balance, Breite und Kanaltausch oder Mono aus L, R bzw. beiden Kanälen mit getrennten Mischfadern. Der Song-Pegel und das vertikale LED-Meter sitzen in der Ausgangssektion. Zur Ausgabe stehen Songformat, Stereo-WAV und zwei getrennte Mono-WAVs zur Auswahl. |

**Regler L/R koppeln** überträgt Parameteränderungen auf den zweiten Kanal. In den drei Dynamik-Inserts verknüpft **Detektoren koppeln** die Pegelerkennung.

Potis zeigen ihre Position auf einem 270°-Bogen. Senkrechtes Ziehen, Mausrad oder Trackpad, Pfeiltasten und numerische Eingabe ändern den Wert. **Shift** erlaubt feinere Schritte. Doppelklick setzt bipolare Potis auf null. Positive Bereiche mit großer Spannweite, etwa Frequenz und Release, sind logarithmisch skaliert. [Reglerimplementierung](../../src/ui/widgets/rotary.rs)

Für eine globale Neugestaltung der Potis werden Form, Skalenbogen und Zeiger in `paint_rotary` bearbeitet. `RotaryAccent` ordnet den fachlichen Akzenten Theme-Farben zu. `RotarySize` steuert die Größenvarianten. Label, Wertebereich, Einheit und DSP-Wertbindung liegen an den Aufrufstellen im [Song-Editor](../../src/ui/song_editor.rs).

Songs können bestehende Projekteinstellungen übernehmen. **Eigenständig bearbeiten** oder eine Änderung im Rack speichert die Reihenfolge, den Bypass-Zustand und die Parameter für den Song. Änderungen lassen sich in der Sitzung rückgängig machen. Projekte ohne gespeicherte Kette laden die bisherige Reihenfolge **Kanäle → Restauration → Equalizer → Expander → Compressor → Limiter → Hauptmix**. Wiedergabe und Export nutzen dieselbe Kette und lesen die Quell-WAV unverändert. Exportformat, Normalisierung und Schnitt-Fades können das fertige Ergebnis zusätzlich beeinflussen. [Song-Editor](../../src/ui/song_editor.rs), [Projektformat](../../src/project.rs)

Die Projektdatei speichert die Insert-Reihenfolge in `SongSettings.effect_chain`. Der `SongProcessor` besitzt die laufenden DSP-Zustände und ruft die Module in dieser Reihenfolge auf. Expander und Compressor verwenden denselben Dynamikalgorithmus mit getrennten Zuständen. Der Hauptmix verarbeitet das Ergebnis anschließend und liefert auch die Signale für die Solo-Abhöre. Eine Reihenfolgeänderung initialisiert die Streaming-Zustände neu. Die spektrale Restauration richtet beide Kanäle innerhalb ihrer eigenen Stufe zeitlich aus. [Kettenmodell](../../src/audio/processing/chain.rs), [Signalverarbeitung](../../src/audio/processing/mod.rs), [Hauptmix](../../src/audio/processing/mix.rs)

## Song-Normalisierung

Unter **Einzeltrack → Hauptmix → Ausgangspegel** misst **Normalisieren** den Sample-Spitzenpegel des Songs mit seiner Verarbeitung und setzt den Song-Pegel in Richtung des eingestellten Ziels. Der Zielbereich reicht von **−6 bis 0 dBFS**, mit **−0,3 dBFS** als Vorgabe. Der Song-Pegel ist auf **−24 bis +12 dB** begrenzt. Die Normalisierung im Export bleibt eine nachgelagerte Einstellung für die WAV-Ausgabe. [Song-Editor](../../src/ui/song_editor.rs), [WAV-Export](../../src/splitter.rs)
