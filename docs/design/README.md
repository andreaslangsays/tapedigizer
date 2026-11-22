# TapeDigizer · Design

Diese Dokumentation beschreibt die macOS-Studiokonsole in **Version 2.0.0**. Silverface und Obsidian teilen sich dieselbe Navigation von der Aufnahme über Marker, Tracks und Einzeltrack bis zum Export.

- [Designsprache](designsprache.md) dokumentiert Farbrollen, Bedienelemente und Fensterlayout.
- [Elemente und Funktionen](elemente-und-funktionen.md) beschreibt die Bedienung, den Signalweg und die Zuordnung zum Code.

## Entwurfsreferenzen

- [Obsidian Waveform](../../stitch_analog_audio_digitizer/obsidian_waveform/DESIGN.md)
- Stitch-Studien für [Aufnahme mit VU-Metern](../../stitch_analog_audio_digitizer/aufnahme_pegelkontrolle_vintage_vu_meter/screen.png), [Schnitt](../../stitch_analog_audio_digitizer/schnitt_marker_ausrichtung/screen.png), [Track-DSP](../../stitch_analog_audio_digitizer/track_bearbeitung_dsp_effekte/screen.png) und [Export](../../stitch_analog_audio_digitizer/export_mastering_optionen/screen.png).
- [Plugin-Slot](../../stitch_analog_audio_digitizer/plugin_editor_slot_01_vst3_au/screen.png) als Konzept für späteres AU-/VST3-Hosting. In 2.0.0 läuft der TapeDigizer Denoiser als separate Audio Unit in kompatiblen DAWs.
- [Knobs and Pads · Petr Knoll](https://dribbble.com/shots/15642551-Knobs-and-Pads) und [Multiband Filterbank · Julian Behrens](https://dribbble.com/shots/25263605-Multiband-Filterbank-AIR-Plugin-and-Akai-MPC) als Referenzen für taktile Regler und Schalter.

Die [Track-Studio-Spezifikation vom 20. November](../superpowers/specs/2026-11-20-track-studio-design.md) hält den damaligen Entwurfsstand fest.
