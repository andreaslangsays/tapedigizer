# Eigenständiger Arbeitsbereich „Track bearbeiten“

Status: Entwurf zur Freigabe. Die Anforderungen an den Song-Fokus stammen vom Nutzer; die unten genannten Bedien- und DSP-Details sind Umsetzungsvorschläge.

## Ziel

Ein ausgewählter Song wird zu einem eigenständigen Bearbeitungsobjekt. Wellenform, Zeitachse, Wiedergabe, Pegel, Stereobild, Einzelkanäle und Effekte beziehen sich in diesem Arbeitsbereich ausschließlich auf diesen Song. Jeder Track behält seine eigenen Einstellungen beim Wechseln, Speichern, erneuten Öffnen und Exportieren.

## Befund im vorhandenen Projekt

- `src/ui/app.rs` rendert für Schneiden und Track bearbeiten dieselbe Aufnahme-Wellenform. Die Trackauswahl ändert ein Segment und den globalen Abspielcursor.
- `src/audio/player.rs` verwaltet einen Aufnahme-Player mit einem gemeinsamen Denoiser-Zustand. Ein Wiedergabebereich allein trennt die Bearbeitung nicht.
- `src/splitter.rs` kennt pro Segment Titel, Pausenstatus und Filterfreigabe; die übrigen Exportparameter sind global.
- `src/project.rs` speichert bislang keine individuellen Pegel-, Stereo- oder Effektparameter.

Deshalb müssen Oberfläche, Audiosignalweg, Projektzustand und Export gemeinsam erweitert werden.

## Ansätze

1. **Eigener Song-Arbeitsbereich mit nichtdestruktiver Echtzeit-Bearbeitung (empfohlen).** Ein eigener Wiedergabekontext greift ausschließlich auf die Originalsamples des ausgewählten Songs zu. Pro Song gespeicherte Parameter steuern denselben DSP-Signalweg beim Vorhören und Export. Regler sind direkt hörbar; erforderlich ist eine saubere Trennung von Audiozustand und Oberfläche.
2. **Eigener Song-Arbeitsbereich mit berechneter Vorschau.** Einstellungen werden ebenfalls pro Song gespeichert; Änderungen berechnen zunächst eine neue Vorschau. Der Audio-Callback bleibt einfacher, bei langen Songs und Rauschfilteränderungen entsteht jedoch eine Wartezeit.

Empfehlung: Ansatz 1, weil Pegel-, Stereo- und Kanalregler beim Einstellen unmittelbar hörbar sein sollen. Keine Zustimmung des Nutzers zu dieser technischen Wahl vorausgesetzt.

## Oberfläche

- Links: Songliste mit Titel und Dauer, aktive Auswahl eindeutig hervorgehoben. Pausen werden nicht als bearbeitbare Songs angeboten.
- Mitte: ausschließlich die Stereo-Wellenform dieses Songs, lokale Zeitachse ab 00:00, Zoom und Bereichsauswahl. Keine angrenzenden Songs oder Schnittmarken der gesamten Aufnahme.
- Eigener Transport: Wiedergabe/Pause, Stopp, Song-Schleife, Abspielen einer Auswahl und Original/Bearbeitet-Vergleich. Wiedergabe und Suche bleiben auch über Tastatur und Menü innerhalb des Songs.
- Pegelanzeigen zeigen das bearbeitete L/R-Ausgangssignal und Übersteuerungen.
- Rechts bzw. bei geringer Fensterhöhe scrollbar: Pegel, Stereobild, Einzelkanäle und Effekte. Die bestehende Mindestfenstergröße 1060 × 700 bleibt nutzbar.
- Bei Auswahlwechsel stoppt die vorherige Wiedergabe. Der neue Song zeigt sofort seine eigenen Einstellungen und seine lokale Position. Zoom und Auswahl werden auf den neuen Song zurückgesetzt.
- Ohne Song erscheint ein eindeutiger Leerzustand mit Projekt-/WAV-Öffnen. Bei einem Projekt ohne aktive Songs wird keine Aufnahmebearbeitung als Ersatz angezeigt.

## Eigene Einstellungen je Song

Vorgeschlagene erste Ausstattung:

- Gesamtpegel: −24 bis +12 dB, Ausgangsmeter und Clip-Anzeige.
- Stereobild: Balance, Stereobreite von Mono bis verbreitert, L/R-Tausch und Mono-Vorhören. Vorschlag: Breite 0–200 %, Standard 100 %.
- L/R getrennt: Kanalpegel, Stumm, Solo und Polaritätsumkehr. Solo und Mono-Vorhören dienen nur der Kontrolle und verändern den Export nicht. Ein Link-Schalter erlaubt gemeinsames Einstellen beider Kanäle; getrennte Werte werden beim Entkoppeln nicht unbeabsichtigt überschrieben.
- Eigene Effekte: Rauschfilter und DC-/Subsonic-Filter sowie ein einfacher 3-Band-EQ. Filter-/EQ-Parameter sind bei Bedarf für L und R getrennt einstellbar.
- Jedes Effektmodul kann übergangen werden. Original/Bearbeitet ist eine temporäre Abhörfunktion, kein Zurücksetzen gespeicherter Werte.
- Ein Rauschprofil wird ausdrücklich für den ausgewählten Song gelernt oder aus einem vorhandenen Profil übernommen; ein neues Profil ändert andere Songs nicht.
- Ein explizites Zurücksetzen betrifft ausschließlich den ausgewählten Song. Ohne Benutzeraktion werden keine Bearbeitungswerte auf andere Songs kopiert.

## Audiosignalweg und Speicherung

- Die Originalaufnahme bleibt unverändert. Der Song referenziert seinen Start und sein Ende samplegenau innerhalb der Aufnahme.
- Ein eigener Song-Wiedergabekontext verwendet lokale Positionen und darf beim Vorfüllen des Rauschfilters, Resampling oder Ausklingen keine Samples angrenzender Songs lesen. DSP-Zustände werden beim Songwechsel und bei Sprüngen korrekt neu initialisiert.
- Signalfolge: Kanalpegel/Polarität → Rausch- und DC-/Subsonic-Filter → Kanal-EQ → Kanaltausch und Stereobreite/Balance → Gesamtpegel → Ausgang. Abhör-Solo und Mono-Kontrolle folgen ausschließlich im Vorschaupfad.
- Effekte puffern ihre Zustände pro Song. Parameteränderungen werden an Audio-Blockgrenzen übernommen; Synchronisation darf nicht kurzfristig unbearbeitetes Audio ausgeben. Pegeländerungen werden geglättet.
- Der Export verwendet dieselben gespeicherten Song-Parameter und dieselbe DSP-Reihenfolge. Abhör-Solo, Mono-Kontrolle und Originalvergleich werden nicht exportiert.
- Globale Export-Normalisierung und End-Fades bleiben als abschließende, ausdrücklich globale Exportstufe bestehen. Globale Rauschfilter-/Balance-/Subsonic-Einstellungen dürfen die neue Song-Kette nicht versehentlich doppelt anwenden.
- In neuen Projekten beginnen Song-Bearbeitungen neutral. Bestehende Projekte ohne Song-Einstellungen behalten ihren bisherigen Exportklang. Beim bewussten Aktivieren der individuellen Bearbeitung eines bestehenden Songs werden dessen bisher wirksames Rauschprofil, Filterfreigabe, Balance und Subsonic-Einstellungen in die Song-Kette übernommen; diese globalen Stufen entfallen für diesen Song danach, damit sie nicht doppelt wirken. Normalisierung und End-Fades bleiben global. Die Übernahme wird sichtbar gemacht; ein bloßer Tabwechsel verändert keinen Export. Das ist eine vorgeschlagene Migrationsregel, damit der Wechsel keinen überraschenden Klangsprung verursacht.
- Parameter und Rauschprofile werden mit dem Projekt gespeichert; alte Projekte bleiben ladbar. Änderungen fließen in die Erkennung ungespeicherter Änderungen ein.
- Die Zuordnung bleibt bei Umbenennung und Verschiebung von Schnittgrenzen erhalten. Beim Teilen erben beide Teile die bisherigen Einstellungen; beim Zusammenführen bleiben die Einstellungen des linken Songs erhalten. Diese Zusammenführungsregel wird am Vorgang kenntlich gemacht. Undo/Redo stellt auch die zugehörigen Einstellungen wieder her.
- Keine automatische Überschreibung der Original-WAV aus dem neuen Song-Arbeitsbereich. Die bisherigen aufnahmeweiten Einrechnen-Aktionen gehören nicht in diesen Arbeitsbereich.

## Umfang und Abgrenzung

Enthalten sind ein tatsächlicher Song-Editor, hörbare individuelle Bearbeitung, Projektpersistenz und korrekter Export. AU-/VST-Hosting bleibt entsprechend der bisherigen Absprache eine spätere Erweiterung; es gibt keine funktionslosen Plugin-Schaltflächen. Mehrspur-Mixing, Aufnahmeautomationen und DAW-Funktionen sind kein Teil dieses Umbaus.

## Prüfung

- Zwei Songs mit gegensätzlichen Pegel-, Kanal-, Stereo- und Effektwerten bleiben beim Wechseln, Speichern und Wiederöffnen unabhängig.
- Einseitige Testsignale prüfen Kanaltrennung, Polarität, Solo, Balance und Stereo-Matrix; neutrale Parameter erhalten das Signal.
- Songanfang/-ende, Samplegrenzen, Rauschfilterlatenz, Resampling und Schleifen enthalten keine Nachbarsamples.
- Originalvergleich, Solo und Mono-Abhören verändern weder gespeicherte Parameter noch Export.
- Vorschau und Export verwenden nach Berücksichtigung von WAV-Quantisierung und expliziten globalen Exportstufen denselben Signalweg.
- Teilen, Zusammenführen, Undo/Redo und Grenzverschiebungen erhalten die definierte Parameterzuordnung.
- Alte Projekte bleiben klanglich kompatibel; Öffnen oder Wechseln in den Editor allein erzeugt keine Bearbeitung.
- Oberfläche in Deutsch und Englisch bei 1060 × 700 und 1260 × 820 prüfen; vollständige bestehende Testsuite ausführen.

## Dem Implementierer überlassen

Interne Modul- und Typnamen, die konkrete stabile Segmentzuordnung, geeignete Biquad-Implementierung für den EQ, sichere blockweise Parameterübergabe, Meterdarstellung und Detailabstände. Die gemeinsame DSP-Implementierung muss separat testbar sein. Keine zusätzlichen Abhängigkeiten oder externen Plugin-Schnittstellen allein für die neue Oberflächeneinteilung.
