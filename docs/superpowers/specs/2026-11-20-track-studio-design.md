# Eigenständiger Arbeitsbereich „Track bearbeiten“

Status: Am 21.11.2026 zur Umsetzung freigegeben. Songauswahl oben ohne seitliche Songliste, vollständiger Song-Fokus, alte Projekte, vollständige Speicherung, Mono-Routing sowie Compressor und Expander sind ausdrückliche Nutzeranforderungen. Konkrete Reglerbereiche, Signalreihenfolge und die technische Architektur sind Umsetzungsvorschläge.

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

Empfehlung: Ansatz 1, weil Pegel-, Stereo- und Kanalregler beim Einstellen unmittelbar hörbar sein sollen. Ansatz 1 ist Grundlage der freigegebenen Umsetzung.

## Oberfläche

- Oben: Songauswahl mit Titel, Nummer und Dauer. Der gewählte Song ist eindeutig benannt. Andere Songs erscheinen nur beim Öffnen dieser Auswahl; es gibt keine seitliche Songliste und keine dauerhaft sichtbare Navigation zu Nachbarsongs. Pausen werden nicht als bearbeitbare Songs angeboten.
- Darunter: ausschließlich die Wellenform dieses Songs, lokale Zeitachse ab 00:00, Zoom und Bereichsauswahl. Darstellung passend zum gespeicherten Stereo-/Mono-Ausgabeformat; die Quellkanäle bleiben zur Einzelbearbeitung gezielt inspizierbar. Keine angrenzenden Songs oder Schnittmarken der gesamten Aufnahme.
- Eigener Transport: Wiedergabe/Pause, Stopp, Song-Schleife, Abspielen einer Auswahl und Original/Bearbeitet-Vergleich. Wiedergabe und Suche bleiben auch über Tastatur und Menü innerhalb des Songs.
- Pegelanzeigen zeigen das bearbeitete L/R-Ausgangssignal und Übersteuerungen.
- Die Wellenform nutzt die verfügbare Fensterbreite. Darunter befinden sich klar gegliederte Bearbeitungsmodule für Pegel, Kanal-Routing, Stereobild, Einzelkanäle und Effekte; bei geringer Fensterhöhe sind die Module scrollbar. Der gesamte Arbeitsbereich gehört ausschließlich zum gewählten Song. Die bestehende Mindestfenstergröße 1060 × 700 bleibt nutzbar.
- Bei Auswahlwechsel stoppt die vorherige Wiedergabe. Der neue Song zeigt sofort seine eigenen Einstellungen und seine lokale Position. Zoom und Auswahl werden auf den neuen Song zurückgesetzt.
- Ohne Song erscheint ein eindeutiger Leerzustand mit Projekt-/WAV-Öffnen. Bei einem Projekt ohne aktive Songs wird keine Aufnahmebearbeitung als Ersatz angezeigt.

## Eigene Einstellungen je Song

Bearbeitungsumfang dieses Entwurfs (die konkreten Bedienparameter sind Vorschläge):

- Gesamtpegel: −24 bis +12 dB, Ausgangsmeter und Clip-Anzeige.
- Kanal-Routing als dauerhafte Song-Einstellung: **Stereo**, **nur Links → Mono**, **nur Rechts → Mono** und **Links + Rechts → Monomix**. Links/Rechts → Mono verwenden ausschließlich den jeweiligen Quellkanal; der andere Kanal wird nicht beigemischt. Monomix hat eigene, unabhängig einstellbare Mischpegel für Links und Rechts sowie einen gemeinsamen Ausgangspegel. Beide Seiten können einzeln vorgehört und vollständig aus der Mischung genommen werden. Die neutrale Ausgangsmischung ist 50:50: `M = 0.5 * L + 0.5 * R`, sodass zwei identische Kanäle den Pegel nicht verdoppeln. Die Mischpegel können anschließend unabhängig verändert werden; keine automatische Normalisierung darf das gewählte Verhältnis verändern. Der Mono-Ausgangspegel und eine Clip-Anzeige erlauben die Kontrolle der Summenlautstärke. Die Mischpegel sind eigene Routing-Parameter und verändern nicht die zuvor eingestellten Kanal-Bearbeitungspegel. Der Original-Stereo-Song bleibt erhalten und das Routing kann jederzeit zurückgestellt werden.
- Mono ist ein tatsächliches Bearbeitungs- und Ausgabeformat: Mono-Songs werden standardmäßig als einkanalige WAV ausgegeben. Zusätzlich wählt jeder Song zwischen Songformat, expliziter Stereo-WAV (zwei Kanäle) und zwei separaten zeitgleichen Mono-Dateien für L/R. Separate L/R-Dateien enthalten die Einzelkanalbearbeitung und den Song-Pegel vor der Stereo-/Mono-Mischmatrix; gemeinsame Normalisierung und identische Fades erhalten das Verhältnis und die Synchronität. Diese zusätzliche Exportwahl wurde vom Nutzer während der Umsetzung ausdrücklich bestätigt. Beim Abhören über Stereoausgänge wird das Monosignal mittig auf beide Ausgänge gelegt; das ist unabhängig vom einkanaligen Dateiformat.
- Stereobild: Balance, Stereobreite, L/R-Tausch und zusätzlich temporäres Mono-Vorhören. Vorschlag: Breite 0–200 %, Standard 100 %. Unpassende Stereo-Bedienelemente sind bei Mono-Ausgabe deaktiviert oder ausgeblendet.
- L/R getrennt: Kanalpegel, dauerhaftes Stumm und Polaritätsumkehr sowie temporäres Solo. Solo und temporäres Mono-Vorhören dienen nur der Kontrolle und verändern den Export nicht. Ein Parameter-Link erlaubt gemeinsames Einstellen beider Kanäle; getrennte Werte werden beim Entkoppeln nicht unbeabsichtigt überschrieben. In den Modi nur Links/Rechts → Mono beeinflussen Bearbeitungen des verworfenen Kanals den Ausgang nicht.
- Eigene Effekte: Rauschfilter und DC-/Subsonic-Filter sowie ein 3-Band-EQ. Filter-/EQ-Parameter sind bei Bedarf für L und R getrennt einstellbar.
- **Compressor** als eigenes Modul: Threshold, Ratio, Attack, Release, Knee und Makeup-Gain.
- **Expander** als eigenes Modul: Threshold, Ratio, Attack, Release, Knee und Ausgangspegel. Vorgeschlagen sind wählbare Downward- und Upward-Expansion, damit leise Anteile abgesenkt bzw. laute Anteile angehoben werden können. Expansion ist eine bearbeitbare Dynamikfunktion und keine bloße Gate-Schaltfläche.
- Dynamikbearbeitung unterstützt Stereo-Kopplung und unabhängige Kanalbearbeitung. Bei Stereo-Kopplung erhalten beide Kanäle dieselbe Regelverstärkung, damit die Dynamikbearbeitung das Stereobild nicht ungewollt verschiebt. Die Kopplung der Detektoren ist klar vom Parameter-Link unterscheidbar.
- Die Effektstruktur ist modular und um weitere Bearbeitungen erweiterbar; sämtliche angebotenen Module arbeiten tatsächlich auf dem gewählten Song. Neue Effektmodule dürfen weder globale Einstellungen teilen noch andere Songs mit verändern.
- Jedes Effektmodul kann übergangen werden. Original/Bearbeitet ist eine temporäre Abhörfunktion, kein Zurücksetzen gespeicherter Werte.
- Ein Rauschprofil wird ausdrücklich für den ausgewählten Song gelernt oder aus einem vorhandenen Profil übernommen; ein neues Profil ändert andere Songs nicht.
- Ein explizites Zurücksetzen betrifft ausschließlich den ausgewählten Song. Ohne Benutzeraktion werden keine Bearbeitungswerte auf andere Songs kopiert.

## Audiosignalweg und Speicherung

- Die Originalaufnahme bleibt unverändert. Der Song referenziert seinen Start und sein Ende samplegenau innerhalb der Aufnahme.
- Ein eigener Song-Wiedergabekontext verwendet lokale Positionen und darf beim Vorfüllen des Rauschfilters, Resampling oder Ausklingen keine Samples angrenzender Songs lesen. DSP-Zustände werden beim Songwechsel und bei Sprüngen korrekt neu initialisiert.
- Vorgeschlagene Signalfolge: Kanalpegel/Stumm/Polarität → Rausch- und DC-/Subsonic-Filter → Kanal-EQ → Expander → Compressor → gespeichertes Stereo-/Mono-Routing → bei Stereo Kanaltausch und Stereobreite/Balance → Gesamtpegel → Ausgang. Der Modus nur Links/Rechts → Mono verwirft den anderen Kanal vollständig; dessen Einstellungen und Dynamikdetektor dürfen den gewählten Kanal nicht beeinflussen. Abhör-Solo und temporäre Mono-Kontrolle folgen ausschließlich im Vorschaupfad.
- Effekte puffern ihre Zustände pro Song. Parameteränderungen werden an Audio-Blockgrenzen übernommen; Synchronisation darf nicht kurzfristig unbearbeitetes Audio ausgeben. Pegeländerungen werden geglättet.
- Der Export verwendet dieselben gespeicherten Song-Parameter und dieselbe DSP-Reihenfolge einschließlich Kanal-Routing und Dynamik. Abhör-Solo, temporäre Mono-Kontrolle und Originalvergleich werden nicht exportiert. Gespeichertes Mono-Routing und dauerhaftes Stumm dagegen sind ausdrücklich Teil des Exports.
- Globale Export-Normalisierung und End-Fades bleiben als abschließende, ausdrücklich globale Exportstufe bestehen. Globale Rauschfilter-/Balance-/Subsonic-Einstellungen dürfen die neue Song-Kette nicht versehentlich doppelt anwenden.
- In neuen Projekten beginnen Song-Bearbeitungen neutral. Bestehende Projekte ohne Song-Einstellungen behalten ihren bisherigen Exportklang. Beim bewussten Aktivieren der individuellen Bearbeitung eines bestehenden Songs werden dessen bisher wirksames Rauschprofil, Filterfreigabe, Balance und Subsonic-Einstellungen in die Song-Kette übernommen; diese globalen Stufen entfallen für diesen Song danach, damit sie nicht doppelt wirken. Normalisierung und End-Fades bleiben global. Die Übernahme wird sichtbar gemacht; ein bloßer Tabwechsel verändert keinen Export. Das ist eine vorgeschlagene Migrationsregel, damit der Wechsel keinen überraschenden Klangsprung verursacht.
- **Vollständige Speicherung:** Das Projekt enthält für jeden Song alle Pegel-, Kanal-, Routing-, Stereo- und Monomix-Mischpegel einschließlich Mono-Ausgangspegel, EQ-, Rauschfilter-, DC-/Subsonic-, Compressor- und Expander-Einstellungen, Modul-Bypass und gegebenenfalls Modulreihenfolge, Parameter-/Detektorkopplung sowie vollständige benötigte Rauschprofile. Audio-Referenzen und globale Exportoptionen bleiben enthalten. Keine Wirkung darf nur in einem flüchtigen Player- oder UI-Zustand existieren. Zuletzt gewählter Song und dauerhafte Ansichtsoptionen werden ebenfalls gespeichert; laufende Wiedergabe und temporäres Abhör-Solo werden beim Öffnen nicht automatisch aktiviert. Änderungen fließen in die Erkennung ungespeicherter Änderungen ein.
- **Rückwärtskompatibles Laden:** Alle bisher von TapeDigizer erstellten Projektformate bleiben ladbar. Neue Felder sind optional bzw. erhalten neutrale Standardwerte; alte Segmenttitel, Marker, Pausen, Filterfreigaben, Audio-Referenzen und Normalisierung bleiben erhalten. Projektversion und Migration werden ausdrücklich behandelt. Ein altes Projekt kann geöffnet, ergänzt, gespeichert und erneut geöffnet werden, ohne seine bisherigen Inhalte oder die neuen Song-Einstellungen zu verlieren. Abwärtskompatibilität neuer Projektdateien mit alten App-Versionen ist damit nicht zugesichert.
- Die Zuordnung bleibt bei Umbenennung und Verschiebung von Schnittgrenzen erhalten. Beim Teilen erben beide Teile die bisherigen Einstellungen; beim Zusammenführen bleiben die Einstellungen des linken Songs erhalten. Diese Zusammenführungsregel wird am Vorgang kenntlich gemacht. Undo/Redo stellt auch die zugehörigen Einstellungen wieder her.
- Keine automatische Überschreibung der Original-WAV aus dem neuen Song-Arbeitsbereich. Die bisherigen aufnahmeweiten Einrechnen-Aktionen gehören nicht in diesen Arbeitsbereich.

## Umfang und Abgrenzung

Enthalten sind ein tatsächlicher Song-Editor mit oberer Auswahl ohne seitliche Songliste, hörbare individuelle Bearbeitung, Mono-/Stereo-Routing, Einzelkanäle, EQ, Restauration, Compressor, Expander, vollständige Projektpersistenz und korrekter Mono-/Stereo-Export. „Sämtliche Möglichkeiten“ wird als modular erweiterbarer Bearbeitungsbereich verstanden; eine unbegrenzte Sammlung aller existierenden Audioeffekte wird nicht als abgeschlossene Lieferung behauptet. AU-/VST-Hosting bleibt entsprechend der bisherigen Absprache eine spätere Erweiterung; es gibt keine funktionslosen Plugin-Schaltflächen. Mehrspur-Mixing, Aufnahmeautomationen und DAW-Funktionen sind kein Teil dieses Umbaus.

## Prüfung

- Zwei Songs mit gegensätzlichen Pegel-, Kanal-, Stereo- und Effektwerten bleiben beim Wechseln, Speichern und Wiederöffnen unabhängig.
- Einseitige Testsignale prüfen Kanaltrennung, Polarität, Solo, dauerhaftes Stumm, Balance und Stereo-Matrix; neutrale Parameter erhalten das Signal.
- Nur Links → Mono und nur Rechts → Mono enthalten ausschließlich den gewählten Kanal. Das verworfene Signal und seine Effekte/Detektoren beeinflussen die Ausgabe nicht. Monomix aus identischen Signalen erhält in der neutralen 50:50-Einstellung den Pegel; gegenphasige Signale werden entsprechend ihrer Summe behandelt. Unterschiedliche L/R-Mischpegel werden mit eindeutig unterscheidbaren Testsignalen geprüft, ebenso das vollständige Herausnehmen einer Seite und die unabhängige Summenlautstärke. Die Mischung bleibt beim Songwechsel sowie Speichern und Wiederöffnen erhalten und stimmt in Vorschau und Export überein. Mono-WAVs haben tatsächlich einen Kanal, Stereo-WAVs zwei.
- Compressor und Expander werden mit definierten Pegelsprüngen und Signalen oberhalb/unterhalb der Schwelle geprüft: neutraler Bypass, korrekte Verstärkung, Attack/Release, Stereo-Kopplung und unabhängige Kanalparameter. Keine NaN-/Inf-Ausgabe oder unkontrollierte Verstärkung bei Stille und extremen zulässigen Werten.
- Songanfang/-ende, Samplegrenzen, Rauschfilterlatenz, Resampling und Schleifen enthalten keine Nachbarsamples.
- Originalvergleich, Solo und Mono-Abhören verändern weder gespeicherte Parameter noch Export.
- Vorschau und Export verwenden nach Berücksichtigung von WAV-Quantisierung und expliziten globalen Exportstufen denselben Signalweg.
- Teilen, Zusammenführen, Undo/Redo und Grenzverschiebungen erhalten die definierte Parameterzuordnung.
- Ladeprüfungen mit repräsentativen alten Projektdateien sowie Öffnen–Speichern–Wiederöffnen prüfen sämtliche alten Metadaten und alle neuen Song-Parameter einschließlich Mono-Routing, Dynamik und Rauschprofilen. Alte Projekte bleiben klanglich kompatibel; Öffnen oder Wechseln in den Editor allein erzeugt keine Bearbeitung.
- Oberfläche in Deutsch und Englisch bei 1060 × 700 und 1260 × 820 prüfen; vollständige bestehende Testsuite ausführen.

## Dem Implementierer überlassen

Interne Modul- und Typnamen, die konkrete stabile Segmentzuordnung, geeignete Biquad-Implementierung für den EQ, numerisch stabile Dynamikprozessoren mit klar beschrifteten Reglerbereichen, sichere blockweise Parameterübergabe, Meterdarstellung und Detailabstände. Die gemeinsame DSP-Implementierung muss separat testbar sein. Keine zusätzlichen Abhängigkeiten oder externen Plugin-Schnittstellen allein für die neue Oberflächeneinteilung.
