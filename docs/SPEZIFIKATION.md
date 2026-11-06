# Trainingstagebuch – Funktionale Spezifikation

Version 1.0 · 06.11.2026 · abgeleitet aus der bestehenden WPF-Implementierung (`Journal/`)

Zweck dieses Dokuments: eine **technologieneutrale** Beschreibung aller relevanten Funktionen,
damit die App in Rust neu implementiert werden kann. Jede Funktion beschreibt das **Ist-Verhalten**
der C#-Version. Wo dieses Verhalten fehlerhaft oder fragwürdig ist, steht ein Kasten
**„Empfehlung Neuimplementierung“**. Ob man ihm folgt, ist eine Entscheidung (siehe Kapitel 9).

Schlüsselwörter: **MUSS** = für Kompatibilität mit vorhandenen Daten bzw. Kernfunktion nötig,
**SOLL** = übernehmen, sofern nicht bewusst anders entschieden, **KANN** = optional.

---

## 1. Überblick

- Desktop-App mit einem Fenster: **Monatskalender** oben, **Texteditor** für den gewählten Tag
  unten, darunter eine **Buttonleiste**.
- Pro Tag existiert höchstens **eine Textdatei** in einem frei wählbaren **Journal-Ordner**
  (typischerweise in OneDrive o. Ä., damit die Dateien synchronisiert werden).
- Der Kalender zeigt für jeden Tag, ob eine Datei existiert, und darin **Kürzel** der erkannten
  Trainingsarten (z. B. `SQ DL`).
- Änderungen im Ordner von außen (Sync) werden automatisch im Kalender sichtbar.
- Es gibt keine Datenbank, keinen Server und keinen eigenen Index. Die Dateien selbst sind die
  einzige Datenquelle.

## 2. Begriffe

| Begriff | Bedeutung |
|---|---|
| Journal-Ordner (`root`) | Vom Benutzer gewählter Ordner, in dem die Tagesdateien liegen |
| Tagesdatei | Datei `<root>/YYYY-MM-DD.txt` |
| Eintrag (`Entry`) | Im Speicher: `{ date: Datum, badge: String }`, ein Element pro erkannter Tagesdatei |
| Badge | Kürzel-String, der in der Kalenderzelle angezeigt wird (darf leer sein) |
| Vorlage | Text, der für einen Tag ohne Datei vorausgefüllt im Editor steht |
| Originaltext | Editorinhalt direkt nach dem Laden eines Tages. Damit wird „geändert“ erkannt |

---

## 3. Datenformat (MUSS – Kompatibilität mit vorhandenen Dateien)

### 3.1 Dateiname
- Schema: `YYYY-MM-DD.txt` (4-stelliges Jahr, 2-stelliger Monat und Tag, mit führenden Nullen).
- Liegt **direkt** im Journal-Ordner, ohne Unterordner.
- **Beim Schreiben** wird immer genau dieser Name verwendet.
- **Beim Einlesen** (Index, F3, entschieden E2) zählt eine Datei **nur** dann als Eintrag, wenn
  ihr vollständiger Name exakt dem Muster `YYYY-MM-DD.txt` entspricht:
  - genau 4 + 2 + 2 Ziffern, getrennt durch `-`, danach `.txt`, ohne weitere Zeichen davor oder
    danach (Regex: `^\d{4}-\d{2}-\d{2}\.txt$`),
  - **Groß-/Kleinschreibung ist egal**: Die Endung zählt in jeder Schreibweise (`.txt`, `.TXT`,
    `.Txt` …). Das gilt für **alle** Dateizugriffe, nicht nur fürs Einlesen:
    - **Laden (F7), Speichern (F10, F15, Auto-Save) und Löschen (F12)** verwenden die vorhandene
      Datei des Tages in ihrer vorhandenen Schreibweise. Liegt dort `2026-11-07.TXT`, wird genau
      diese Datei überschrieben bzw. gelöscht. Es entsteht keine zweite Datei `2026-11-07.txt`.
    - Nur wenn es für den Tag noch **keine** Datei gibt, wird sie neu als `YYYY-MM-DD.txt` (klein)
      angelegt.
    - Unter Windows ist das automatisch so. Auf Dateisystemen, die Groß- und Kleinschreibung
      unterscheiden, muss die App die vorhandene Datei aktiv suchen.
  - das Datum muss ein **gültiges Kalenderdatum** sein (`2024-02-30.txt` zählt nicht),
  - nur normale Dateien zählen, keine Verzeichnisse.

  Alle anderen Dateien werden **ignoriert**, also nicht angezeigt und nicht verändert. Beispiele:

  | Dateiname | Eintrag? |
  |---|---|
  | `2024-01-05.txt` | ja |
  | `2024-01-05.TXT` | ja |
  | `2024-01-05.md` | nein |
  | `2024-01-05.txt.bak` | nein |
  | `2024-01-05-Laptop.txt` (Sync-Konflikt) | nein |
  | `2024-1-5.txt` | nein |
  | `2024-02-30.txt` | nein |

  > **Abweichung von C#:** Dort wird der Name am ersten `.` getrennt und die Endung nicht geprüft,
  > sodass auch `.md`- oder `.txt.bak`-Dateien als Eintrag zählen.
  >
  > **Empfehlung (KANN):** Sync-Konfliktdateien wie `2024-01-05-Laptop.txt` oder
  > `2024-01-05 (1).txt` erkennen und als Warnung anzeigen, damit sie nicht unbemerkt liegen bleiben.

### 3.2 Inhalt
- Reiner Text, **keine Formatierung** (der alte Editor ist zwar ein RichText-Control, gespeichert
  wird aber nur der Klartext).
- Kodierung beim Schreiben: **UTF-8 ohne BOM**.
- Kodierung beim Lesen: UTF-8. Eine BOM SOLL erkannt und ignoriert werden.
- Zeilenumbrüche beim Schreiben: **CRLF** (`\r\n`). Jede Zeile (Absatz) endet mit `\r\n`,
  also auch die letzte.
  > **Empfehlung:** Beim Lesen LF und CRLF akzeptieren. Beim Schreiben CRLF beibehalten, damit
  > ein Diff in OneDrive oder Git nicht jede Zeile als geändert zeigt.
- Es gibt keine Struktur und keine Metadaten im Inhalt. Die Kürzel werden bei jedem Einlesen neu
  aus dem Text berechnet (F4).

---

## 4. Konfiguration

(entschieden E6: **eine Konfigurationsdatei**)

### 4.1 Ort und Format
- Eine Datei `config.toml` im plattformüblichen Konfigurationsverzeichnis des Benutzers.
  Unter Windows: `%APPDATA%\Trainingstagebuch\config.toml`.
- Format: **TOML**, UTF-8. Die Datei ist zum Bearbeiten mit einem normalen Texteditor gedacht.
  Eine Einstellungsoberfläche in der App gibt es nicht (außer der Ordnerwahl, F13).
- Die alte `user.config` der C#-Version wird nicht übernommen. Den Ordner wählt man einmal neu.

### 4.2 Inhalt

| Schlüssel | Typ | Standard | Bedeutung |
|---|---|---|---|
| `root_path` | String (Pfad) | *(leer)* | Journal-Ordner. Leer oder ungültig bedeutet: nicht konfiguriert |
| `template.lines` | Liste von Strings | siehe unten | Zeilen der Vorlage für neue Tage (F8), Platzhalter erlaubt |
| `template.motto.<wochentag>` | String | siehe F8 | Ersetzt `{motto}` |
| `template.uebung.<wochentag>` | String | siehe F8 | Ersetzt `{uebung}` |
| `[[regel]]` | Liste von Tabellen | siehe unten | Kürzel-Regeln (F4), Reihenfolge = Prüf- und Anzeigereihenfolge |
| `regel.kuerzel` | String, Pflicht | – | Angezeigtes Kürzel, z. B. `SQ` |
| `regel.woerter` | Liste von Strings, Pflicht | – | Regel trifft, wenn mindestens eines davon im Text vorkommt |
| `regel.ausser` | Liste von Strings, optional | `[]` | Regel trifft nicht, wenn eines davon im Text vorkommt |

Ist kein einziges `[[regel]]` in der Datei, gilt der **Standard-Regelsatz**. Wer keine Kürzel
möchte, schreibt `regel = []`. Dann zeigt jeder Tag mit Datei ein leeres grünes Badge.

`<wochentag>` ∈ `montag`, `dienstag`, `mittwoch`, `donnerstag`, `freitag`, `samstag`, `sonntag`.

**Platzhalter** in `template.lines`:

| Platzhalter | Ersetzt durch |
|---|---|
| `{datum_lang}` | Langes Datum in der Sprache des Systems, z. B. `Freitag, 6. November 2026` |
| `{motto}` | `template.motto.<wochentag>` des Tages, fehlt der Wert: leer |
| `{uebung}` | `template.uebung.<wochentag>` des Tages, fehlt der Wert: leer |

Unbekannte Platzhalter bleiben als Text stehen. Ein Platzhalter darf irgendwo in einer Zeile stehen,
z. B. `"Übung: {uebung}"`.

**Standarddatei** (wird beim ersten Start angelegt und entspricht genau dem Verhalten der C#-Version):

```toml
# Journal-Ordner (wird von der App beim Ordnerwechsel gesetzt)
root_path = ""

[template]
# Zeilen der Vorlage für neue Tage. Platzhalter: {datum_lang}, {motto}, {uebung}
lines = ["{datum_lang}", "{motto}", "5 Werte", "{uebung}"]

[template.motto]
montag     = "Danke sagen"
dienstag   = "Bombige Zeiten"
mittwoch   = "Phantastische Zukunft"
donnerstag = "Lieber"
freitag    = "Rückschau halten"
samstag    = ""
sonntag    = ""

[template.uebung]
montag     = ""
dienstag   = "Brücken"
mittwoch   = "Handstand"
donnerstag = "Beinheben"
freitag    = "Kniebeugen"
samstag    = "Liegestütze"
sonntag    = "Klimmzüge"

# Kürzel-Regeln für den Kalender. Reihenfolge = Reihenfolge der Anzeige.
# Eine Regel trifft, wenn eines der "woerter" im Text vorkommt und keines der "ausser".
# Teilstring-Suche, Groß-/Kleinschreibung egal. Doppelte Kürzel werden nur einmal angezeigt.
[[regel]]
kuerzel = "SQ"
woerter = ["kniebeuge", "squat"]

[[regel]]
kuerzel = "DL"
woerter = ["kreuzheben", "deadlift"]

[[regel]]
kuerzel = "BP"
woerter = ["bankdrücken", "bench press"]

[[regel]]
kuerzel = "OHP"
woerter = ["press "]
ausser  = ["bench press"]

[[regel]]
kuerzel = "OHP"
woerter = ["overhead press"]

[[regel]]
kuerzel = "PC"
woerter = ["power clean"]

[[regel]]
kuerzel = "Pi"
woerter = ["pistol"]

[[regel]]
kuerzel = "Schw"
woerter = ["schwimmen"]

[[regel]]
kuerzel = "Li"
woerter = ["liegestütz"]

[[regel]]
kuerzel = "B"
woerter = ["bauch", "beinheben"]

[[regel]]
kuerzel = "R"
woerter = ["rücken"]
ausser  = ["drücken"]

[[regel]]
kuerzel = "Kz"
woerter = ["klimmzug", "klimmzüg"]

[[regel]]
kuerzel = "Di"
woerter = ["dip"]

[[regel]]
kuerzel = "Hs"
woerter = ["handstand"]

[[regel]]
kuerzel = "Jo"
woerter = ["joggen"]

[[regel]]
kuerzel = "Ff"
woerter = ["fahrradfahren", "mtb"]

[[regel]]
kuerzel = "Yo"
woerter = ["yoga"]

[[regel]]
kuerzel = "Tri"
woerter = ["trifecta"]

[[regel]]
kuerzel = "Krank"
woerter = ["krank"]
```

### 4.3 Verhalten
- **Laden:** einmal beim Programmstart. Änderungen an der Datei wirken erst nach einem Neustart.
  *(KANN: die Datei überwachen und die Vorlage sofort neu laden.)*
- **Datei fehlt:** Die App legt sie mit den Standardwerten an.
- **Einzelne Schlüssel fehlen:** Für sie gelten die Standardwerte. Die Datei wird deshalb nicht
  umgeschrieben.
- **Datei fehlerhaft** (ungültiges TOML, falscher Typ): Die App zeigt beim Start eine Meldung mit
  Zeile und Fehler, arbeitet mit den Standardwerten weiter und **überschreibt die Datei nicht**.
  Damit gehen die Änderungen des Benutzers nicht verloren. Das Speichern eines neuen `root_path`
  wirkt in diesem Fall nur bis zum Programmende.
- **Schreiben durch die App:** nur `root_path`, und zwar jedes Mal, wenn ein Ordner erfolgreich
  registriert wurde (F1, F13). Dabei MÜSSEN alle anderen Einträge, **Kommentare und Formatierung
  erhalten bleiben**. Die Datei wird gezielt editiert und nicht aus den Einstellungen neu erzeugt.
- Ein Pfad ist **gültig**, wenn er nicht leer ist und als Verzeichnis existiert.
- Die Konfigurationsdatei liegt **lokal und pro Rechner**, weil der Journal-Ordner je Gerät anders
  heißen kann. Wer die Vorlage auf mehreren Rechnern gleich haben will, muss die Datei derzeit selbst
  kopieren.
  *(KANN später: Vorlage optional aus einer Datei im Journal-Ordner lesen, die mitsynchronisiert wird.)*
- Die Kürzel-Regeln (F4) stehen ebenfalls in dieser Datei (entschieden E3). Auch für sie gilt:
  Änderungen wirken nach einem Neustart. Danach wird der Index mit den neuen Regeln komplett neu
  berechnet, auch rückwirkend für alle Tage.

---

## 5. Funktionen

### F1 – Programmstart
1. `root_path` aus der Konfiguration lesen.
2. Ist der Pfad ungültig (siehe Kap. 4), wird er verworfen und der **Ordnerauswahl-Dialog** (F2)
   geöffnet.
3. Ist danach immer noch kein Pfad gesetzt (Dialog abgebrochen), erscheint die Meldung
   „No path to entires“ (sic). Die App startet trotzdem, ohne Ordner. Kalender und Editor bleiben
   leer bzw. inaktiv, bis über F13 ein Ordner gewählt wird.
4. Sonst wird der Ordner **registriert**:
   - `root_path` speichern,
   - Ordnerüberwachung starten (F5),
   - Index aufbauen (F3).
5. Im Kalender wird **heute** ausgewählt und angezeigt, was F6 (Tag laden) auslöst.

### F2 – Ordnerauswahl-Dialog
- Nativer Dialog „Ordner auswählen“ mit dem Titel „Please select a folder.“
- Bei OK wird `root_path` auf den gewählten Ordner gesetzt. Bei Abbruch bleibt der bisherige Wert.
- In C# gibt es den Dialog zweimal. Die zweite Variante im Editor zeigt nach der Auswahl zusätzlich
  eine Bestätigungsmeldung („The selected folder was: …“). Das ist ein Überbleibsel aus einem
  Beispiel und SOLL entfallen.

### F3 – Index aufbauen (`update_entries`)
Eingabe: `root_path`. Ausgabe: Liste von `Entry`, die die Kalenderanzeige komplett ersetzt.

1. Ist `root_path` leer, entsteht eine leere Liste.
2. Für **jede Datei** direkt in `root_path` (nicht rekursiv):
   - Prüfen, ob der Name dem Muster aus 3.1 entspricht (`YYYY-MM-DD.txt`, gültiges Datum).
     Andere Dateien überspringen.
   - Bei einem Treffer den gesamten Inhalt lesen, das Badge mit F4 berechnen und
     `Entry { date, badge }` anhängen.
3. Die neue Liste ersetzt die alte vollständig, danach werden alle Kalenderzellen neu gezeichnet.
4. Pro Datum gibt es höchstens einen Eintrag. Die Ausnahme sind Dateisysteme, die Groß- und
   Kleinschreibung unterscheiden (`…txt` und `…TXT` nebeneinander). Dann gilt die Datei mit
   kleingeschriebener Endung `.txt`.
   *(C#: Hier konnten mehrere Dateien zum selben Datum existieren, z. B. `.txt` und `.md`. Angezeigt
   wurde nur die erste gefundene.)*

> **Empfehlung:** Lesefehler einzelner Dateien (z. B. Datei gerade von OneDrive gesperrt)
> abfangen, die Datei überspringen bzw. später erneut versuchen. In C# bringt so ein Fehler die
> App zum Absturz. Den Index im Hintergrund aufbauen, falls der Ordner groß ist (Jahre × 365 Dateien
> sind unkritisch, aber das Einlesen blockiert in C# die Oberfläche).

**Umgesetzt (Neuimplementierung):** Der Index wird im Hintergrund in zwei Phasen gebaut. Zuerst
wird nur der Ordner aufgelistet, und alle Tage mit Datei erscheinen sofort als grünes Badge, ohne
eine Datei zu öffnen. Das ist wichtig bei OneDrive „Dateien bei Bedarf“, wo jedes Lesen die Datei
herunterlädt. Danach werden die Inhalte gelesen, die neuesten Tage zuerst, und die Kürzel
erscheinen nach und nach. Die Statuszeile zeigt dabei „Lese Einträge … gelesen / gesamt“.
Unveränderte Dateien (gleiche Änderungszeit und Größe) werden nicht erneut gelesen.

### F4 – Kürzel-Erkennung (`parse_badge`)
Entschieden E3: Die Regeln kommen aus der Konfiguration (`[[regel]]`, Kap. 4.2), und **doppelte
Kürzel werden entfernt**. Die Erkennung bleibt eine **Teilstring-Suche** ohne Wortgrenzen.

Eingabe: Dateiinhalt und die Regelliste. Ausgabe: Badge-String.

**Regel** = `{ kuerzel: String, woerter: [String], ausser: [String] }`

1. Inhalt in **Kleinbuchstaben** umwandeln (Unicode, Umlaute bleiben erhalten). `woerter` und
   `ausser` werden ebenfalls klein verglichen. In der Konfiguration ist die Schreibweise also egal.
2. Die Regeln **in der Reihenfolge der Konfiguration** prüfen. Eine Regel **trifft**, wenn
   - der Inhalt **mindestens eines** der `woerter` enthält **und**
   - der Inhalt **keines** der `ausser` enthält (fehlt `ausser` oder ist es leer, entfällt diese
     Bedingung).

   „enthält“ heißt Teilstring irgendwo im gesamten Inhalt. Leerzeichen in einem Suchwort zählen mit,
   z. B. `"press "`.
3. Trifft eine Regel und ist ihr `kuerzel` noch **nicht** im Ergebnis, wird es angehängt. Ist es
   schon drin, passiert nichts. Zwei Kürzel sind gleich, wenn sie zeichengenau übereinstimmen.
4. Ergebnis: die Kürzel in der Reihenfolge ihres ersten Treffers, getrennt durch **genau ein
   Leerzeichen**, ohne Leerzeichen am Anfang oder Ende. Trifft keine Regel, ist das Ergebnis leer.
   Das grüne Badge erscheint trotzdem, weil die Datei existiert (F6).
5. Die Kürzel werden **nicht** in die Tagesdatei geschrieben. Sie werden bei jedem Einlesen neu
   berechnet. Eine geänderte Regel wirkt deshalb nach dem Neustart rückwirkend auf alle Tage.

**Standard-Regelsatz:** Er entspricht der C#-Version, siehe die Standarddatei in Kap. 4.2.
Es gibt zwei Abweichungen: Doppelte Kürzel werden entfernt, und `Kz` erkennt zusätzlich
`klimmzüg`, also auch die Mehrzahl „Klimmzüge“. Dass auch in der Ausgabe dieselbe
Reihenfolge herauskommt, liegt daran, dass Regeln mit gleichem Kürzel direkt hintereinander stehen.

Zum Vergleich die fest codierte Logik der C#-Version:

| # | Bedingung (C#) | Kürzel | Standardregel in `config.toml` |
|---|---|---|---|
| 1–2 | `kniebeuge` · `squat` | `SQ` | `SQ`: woerter `kniebeuge`, `squat` |
| 3–4 | `kreuzheben` · `deadlift` | `DL` | `DL`: `kreuzheben`, `deadlift` |
| 5–6a | `bankdrücken` · `bench press` | `BP` | `BP`: `bankdrücken`, `bench press` |
| 6b | *sonst* `press ` | `OHP` | `OHP`: `press `, ausser `bench press` |
| 7 | `overhead press` | `OHP` | `OHP`: `overhead press` (zweite Regel) |
| 8 | `power clean` | `PC` | `PC` |
| 9 | `pistol` | `Pi` | `Pi` |
| 10 | `schwimmen` | `Schw` | `Schw` |
| 11 | `liegestütz` | `Li` | `Li` |
| 12–13 | `bauch` · `beinheben` | `B` | `B`: `bauch`, `beinheben` |
| 14 | `rücken` und nicht `drücken` | `R` | `R`: `rücken`, ausser `drücken` |
| 15 | `klimmzug` | `Kz` | `Kz`: `klimmzug`, **`klimmzüg`** (neu, damit die Mehrzahl „Klimmzüge“ erkannt wird) |
| 16 | `dip` | `Di` | `Di` |
| 17 | `handstand` | `Hs` | `Hs` |
| 18 | `joggen` | `Jo` | `Jo` |
| 19–20 | `fahrradfahren` · `mtb` | `Ff` | `Ff`: `fahrradfahren`, `mtb` |
| 21 | `yoga` | `Yo` | `Yo` |
| 22 | `trifecta` | `Tri` | `Tri` |
| 23 | `krank` | `Krank` | `Krank` |

**Beispiele mit dem Standard-Regelsatz:**

| Inhalt (Auszug) | C# | Neu |
|---|---|---|
| `Kniebeugen 5x5, Squat-Rack` | `SQ SQ ` | `SQ` |
| `Overhead Press 3x5` | `OHP OHP ` | `OHP` |
| `Bauch, Beinheben` | `B B ` | `B` |
| `Kreuzheben, Kniebeugen` | `SQ DL ` | `SQ DL` (Reihenfolge nach Regeln, nicht nach Text) |
| `Bankdrücken, Rücken` | `BP ` | `BP` (R entfällt wegen „drücken“) |
| `Brücken` (Dienstagsvorlage) | `R ` | `R` (`brücken` enthält `rücken`, siehe E4) |
| `Klimmzüge` (Sonntagsvorlage) | *leer* | `Kz` (über `klimmzüg`) |
| `Dips` / `Kaltwasser-Dipping` | `Di ` | `Di` (Teilstring, gewollt) |

**Fehlerhafte Regeln** (Ergänzung zu Kap. 4.3): Eine Regel mit leerem `kuerzel` oder ohne
nicht-leere `woerter` wird **übersprungen**, und beim Start erscheint eine Meldung. Die übrigen
Regeln gelten weiter. Leere Strings in `woerter` werden ignoriert, weil ein leerer Suchtext sonst
auf jeden Inhalt passen würde.

### F5 – Ordnerüberwachung
- Den Journal-Ordner (nicht rekursiv) auf **Anlegen, Ändern und Löschen** von Dateien überwachen.
  In C# werden Umbenennungen **nicht** behandelt.
- Jedes Ereignis löst **auf dem UI-Thread** einen kompletten Neuaufbau des Index (F3) aus.
- Wird der Ordner gewechselt (F13), überwacht die App ab dann den neuen Ordner.
- C#: Der geöffnete Editor wird nicht neu geladen, nur der Kalender wird aktualisiert.
  **Neu (E5):** Betrifft ein Ereignis die Datei des offenen Tages, wird zusätzlich F16 ausgelöst.

> **Empfehlung:**
> - Umbenennungen ebenfalls behandeln. Sync-Clients schreiben oft eine Temp-Datei und benennen
>   sie dann um.
> - Ereignisse **entprellen** (z. B. 300–500 ms), weil ein einzelner Speichervorgang mehrere
>   Ereignisse auslöst.
> - Beim Ordnerwechsel den alten Watcher sauber beenden. In C# werden die Handler mehrfach
>   registriert.

### F6 – Kalenderanzeige
- **Monatsansicht** mit 7 Spalten und 6 Wochenzeilen (42 Zellen). Tage aus dem Vor- und
  Folgemonat werden ausgegraut angezeigt.
- Der erste Wochentag richtet sich nach dem Gebietsschema (deutsch: **Montag**).
- Kopfzeile: Buttons ◀ und ▶ für Vormonat und Folgemonat, dazu Monat und Jahr (z. B.
  „November 2026“, Schriftgröße 20).
- **Jahres- und Jahrzehntansicht (MUSS, entschieden E8):**
  - Klick auf den Titel der Monatsansicht wechselt in die **Jahresansicht**. Sie zeigt 12 Monate
    im Raster 4 × 3 und im Titel das Jahr.
  - Klick auf den Titel der Jahresansicht wechselt in die **Jahrzehntansicht**. Sie zeigt 12 Jahre
    im Raster 4 × 3: das Jahrzehnt, z. B. 2020–2029, plus je ein ausgegrautes Jahr davor und danach.
    Der Titel lautet z. B. „2020-2029“.
  - ◀ und ▶ blättern in der jeweiligen Ansicht um einen Monat, ein Jahr bzw. ein Jahrzehnt.
  - Klick auf ein Jahr öffnet die Jahresansicht dieses Jahres. Klick auf einen Monat öffnet die
    Monatsansicht dieses Monats.
  - Der Wechsel der Ansicht ändert den **ausgewählten Tag nicht**, F7 wird also nicht ausgelöst.
  - In Jahres- und Jahrzehntansicht gibt es keine Badges.
  - Hervorgehoben werden der aktuelle Monat bzw. das aktuelle Jahr der Anzeige.
- Darunter die Wochentagsnamen **ausgeschrieben** („Montag“ … „Sonntag“). Ist die Spalte dafür zu
  schmal (z. B. bei der Startbreite 500 px), werden sie abgekürzt („Mo“ … „So“).
- **Tageszelle:**
  - oben die Tageszahl,
  - darunter das **Badge**, wenn es für das Datum einen `Entry` gibt: abgerundeter Kasten
    (Radius 4) mit grünem Rahmen `#006000`, Hintergrund `#DFFFF5` und zentriertem Kürzeltext.
    Passt der Text nicht, steht er linksbündig und wird abgeschnitten; der Tooltip der Zelle zeigt
    ihn ganz.
  - **Wichtig:** Das Badge erscheint auch bei leerem Kürzeltext. Der grüne Kasten ist der Hinweis
    „Datei vorhanden“.
  - Ohne Eintrag gibt es keinen Kasten.
- **Zustände der Zelle:**

| Zustand | Darstellung (C#) |
|---|---|
| normal | Kopfbereich `#DFE8F5`, Rahmen `#5D8CC9` 0,5 px |
| heute | Kopfbereich `#F7D275`, Rahmen `#F3B84B` 2 px |
| ausgewählt | Hintergrund `#BADDE9` |
| Maus darüber | Hintergrund `#BADDE9` mit 50 % Deckkraft |
| anderer Monat | Hintergrund `#E9EDF5`, Text `#777777` |

> **Entscheidung (Rust-Version): flacher Stil.** Keine Rahmen, abgerundete Flächen (Radius 8), eine
> Akzentfarbe `#2563EB`:
>
> | Zustand | Darstellung (Rust) |
> |---|---|
> | normal | keine Fläche, Tageszahl oben links `#1F2937` |
> | Wochenende | Fläche `#F3F5F8` (nur im angezeigten Monat) |
> | heute | Tageszahl weiß im gefüllten Kreis `#2563EB` |
> | ausgewählt | Fläche `#E3ECFD`, Rand `#2563EB` 1,5 px |
> | Maus darüber | Fläche `#EEF2F7` |
> | anderer Monat | Tageszahl `#B4BBC6`, Kürzel blasser |
>
> Das Badge wird in seine Kürzel zerlegt, jedes Kürzel ist eine eigene Kapsel („Pill“). Die Farbe
> ergibt sich automatisch aus dem Kürzel (FNV-1a-Hash auf zehn gedämpfte Farbpaare) und ist damit
> auf allen Geräten gleich. Die Pills brechen um, was nicht in die Zelle passt, wird zu „+n“
> zusammengefasst, mindestens ein Kürzel ist immer sichtbar. Ein Eintrag ohne Kürzel („Datei
> vorhanden“) wird als grauer Punkt gezeigt. In Jahres- und Jahrzehntansicht ist das aktuelle
> Feld Akzent-farbig mit Punkt darunter, das angezeigte Feld wie „ausgewählt“.

  Die Farben sind **KANN**. Wichtig ist nur, dass „heute“, „ausgewählt“, „anderer Monat“ und
  „Datei vorhanden“ klar unterscheidbar sind.
- Es ist **genau ein Tag** ausgewählt (Einzelauswahl). Ein Klick auf einen Tag aus dem Nachbarmonat
  wählt ihn aus und blättert zu diesem Monat.
- Tastatur im Kalender: Pfeiltasten wechseln den Tag (Standard-Kalender), siehe Kap. 7.

### F7 – Tag auswählen und laden
Wird ausgelöst, wenn sich der ausgewählte Tag ändert (Klick, Tastatur, Programmstart). Das gilt auch,
wenn sich `root_path` ändert, dann wird der aktuelle Tag neu geladen.

1. Ohne `root_path` oder ohne gültiges Datum passiert nichts.
2. **Auto-Save des vorherigen Tages (F9):** Gab es einen Originaltext und weicht der aktuelle
   Editortext davon ab, wird der Text **ohne Rückfrage** in die Datei des *vorherigen* Tages
   geschrieben. Dabei gilt der Schutz beim Speichern aus F16 D.
   **Neu:** Schlägt das Speichern fehl, bleibt der bisherige Tag offen (Auswahl im Kalender
   zurücksetzen), und es erscheint eine Meldung mit dem Fehler. So geht nichts verloren.
3. Der Editor wird mit der **Vorlage** (F8) für den neuen Tag gefüllt.
4. Existiert `<root>/YYYY-MM-DD.txt`, wird der **komplette** Editorinhalt durch den Dateiinhalt
   ersetzt. Die Vorlage verschwindet dann vollständig.
5. Originaltext = aktueller Editortext, Originaldatum = neuer Tag, Plattenstand = Dateiinhalt
   bzw. „keine Datei“ (F16).
6. Cursor ans **Textende** setzen und dem Editor den **Fokus** geben. (Diese Änderung ist lokal,
   noch nicht committet.)

### F8 – Vorlage für neue Tage
Die Vorlage kommt aus der Konfiguration (entschieden E6, Kap. 4.2). Für jeden Eintrag in
`template.lines` entsteht eine Zeile, die Platzhalter werden für den gewählten Tag ersetzt.

Standard (wie in der C#-Version, dort fest im Code) sind diese Zeilen in dieser Reihenfolge:
1. **Langes Datum** in der Sprache des Systems, z. B. `Freitag, 6. November 2026`
2. **Tagesmotto** nach Wochentag
3. Fester Text `5 Werte`
4. **Tagesübung** nach Wochentag

Standardwerte für Motto und Übung:

| Wochentag | Motto | Übung |
|---|---|---|
| Montag | Danke sagen | *(leer)* |
| Dienstag | Bombige Zeiten | Brücken |
| Mittwoch | Phantastische Zukunft | Handstand |
| Donnerstag | Lieber | Beinheben |
| Freitag | Rückschau halten | Kniebeugen |
| Samstag | *(leer)* | Liegestütze |
| Sonntag | *(leer)* | Klimmzüge |

- Leere Werte ergeben eine **leere Zeile**, die Zeile bleibt also erhalten.
- Ist `template.lines` leer (`[]`), bleibt der Editor bei neuen Tagen leer.
- Eine unveränderte Vorlage wird beim Tageswechsel **nicht** gespeichert (Text = Originaltext).
  Es entsteht also nur dann eine Datei, wenn man etwas ändert oder ausdrücklich speichert (F10).
- **Nebenwirkung:** Wird ein Tag gespeichert, landen die Übungsnamen der Vorlage in der Datei und
  erzeugen Kürzel (F4), auch wenn die Übung nicht gemacht wurde. Beispiele: Freitag → `SQ`,
  Dienstag → `R`, Mittwoch → `Hs`, Donnerstag → `B`, Samstag → `Li`, Sonntag → `Kz`
  (in der C#-Version beim Sonntag noch nicht).

> **Entschieden (E4):** Der Vorlagentext zählt bei der Kürzel-Erkennung **mit**, wie in der
> C#-Version. Die Kürzel-Erkennung (F4) sieht nur den Dateiinhalt und kennt die Vorlage nicht.
> Wer die Kürzel aus der Vorlage nicht möchte, entfernt `{uebung}` aus `template.lines` oder
> passt die Übungen in der Konfiguration an.

### F9 – Bearbeiten und Änderungserkennung
- Freier Klartext-Editor mit mehreren Zeilen, vertikaler Scrollbar und Zeilenumbruch.
- „Geändert“ heißt: der aktuelle Text unterscheidet sich vom Originaltext (einfacher String-Vergleich).
- In der C#-Version wird **nur** gespeichert über:
  - Tageswechsel (F7, Schritt 2, automatisch),
  - Speichern (F10),
  - Wechsel des Journal-Ordners (F13 → F7, mit dem *neuen* Ordner, siehe Hinweis dort).
- **Beim Schließen (entschieden, E1):** siehe F15. In der C#-Version gehen ungespeicherte
  Änderungen hier verloren. Die Neuimplementierung MUSS automatisch speichern.
- **Ist-Verhalten, kritisch:**
  - Wird die Datei des gerade geöffneten Tages extern geändert (Sync von einem anderen Gerät),
    merkt der Editor das nicht. Bei lokaler Änderung und anschließendem Tageswechsel wird die
    externe Version **überschrieben**.
  - **Neu (entschieden E5):** siehe F16. Ohne lokale Änderungen wird automatisch neu geladen,
    bei lokalen Änderungen erscheint eine Hinweisleiste mit Konfliktkopie, und es wird nie
    stillschweigend überschrieben.

> **KANN (nicht entschieden):** nach einigen Sekunden ohne Tastendruck automatisch speichern.

### F10 – Speichern (Button „Haken“, `Strg+S`)
1. Ohne Originaldatum (kein Tag geladen) passiert nichts.
2. Ist kein `root_path` gesetzt, Ordnerauswahl (F2). Bei Abbruch nichts tun.
3. Den Editortext **immer** (auch unverändert, also auch die reine Vorlage) nach
   `<root>/YYYY-MM-DD.txt` des Originaldatums schreiben und eine vorhandene Datei überschreiben.
4. Der gespeicherte Stand gilt als Originaltext und Plattenstand. **Abweichung von C#:** Der Tag
   wird nicht neu geladen, der Cursor bleibt also, wo er ist (in C# sprang er ans Textende).
5. Der Kalender aktualisiert sich über die Ordnerüberwachung (F5), nicht direkt.

> **Neu (E5):** Vor dem Schreiben gilt der Schutz aus F16 D (Plattenstand prüfen, bei Abweichung
> Konfliktkopie).
>
> **Empfehlung:** Atomar schreiben (Temp-Datei im selben Ordner, dann umbenennen), damit der
> Sync-Client nie eine halb geschriebene Datei hochlädt. Danach den Index direkt aktualisieren,
> statt auf den Watcher zu warten.

### F11 – Rückgängig (Button „Pfeil zurück“)
- Den Editorinhalt durch den Originaltext ersetzen, also den Stand beim letzten Laden bzw.
  Speichern. Es wird **nichts** auf die Platte geschrieben.
- Das ist ein **einstufiges** Zurücksetzen, kein Undo-Verlauf. Das normale Rückgängig des Editors
  (`Strg+Z`) gibt es zusätzlich.

### F12 – Löschen (Button „X“)
Entschieden E7: Die Datei kommt **in den Papierkorb** des Betriebssystems und wird nicht
endgültig gelöscht.

1. Ohne Originaldatum oder `root_path` passiert nichts.
2. Existiert keine Datei für den Tag (nur Vorlage oder ungespeicherter Text), wird ohne Rückfrage
   nur der Editor auf die Vorlage zurückgesetzt, so wie bei F11, nur eben auf die Vorlage.
3. Rückfrage (Titel „Tagebuch“): „Eintrag vom `<langes Datum>` in den Papierkorb verschieben?“
   mit **Ja/Nein**. Bei Nein passiert nichts.
4. Die Datei `<root>/YYYY-MM-DD.txt` des Originaldatums in den **Papierkorb** verschieben.
   Ungespeicherte Änderungen im Editor werden **nicht** vorher gespeichert, sie werden verworfen.
5. **Papierkorb nicht verfügbar** (z. B. Netzlaufwerk, oder das Betriebssystem meldet einen
   Fehler): Zweite Rückfrage „Der Papierkorb ist für diesen Ordner nicht verfügbar. Eintrag
   endgültig löschen?“ mit Ja/Nein. Bei Ja wird endgültig gelöscht, bei Nein passiert nichts.
6. **Sonstiger Fehler** (Datei gesperrt, keine Rechte): Meldung mit Fehlertext. Datei und Editor
   bleiben unverändert.
7. Bei Erfolg den Tag neu laden (F7, ohne Auto-Save). Danach steht die Vorlage im Editor.
8. Den Index direkt aktualisieren, das Badge verschwindet. Die Ordnerüberwachung (F5) würde das
   ebenfalls bemerken.

> **Hinweis:** Wiederherstellen geht über den Papierkorb des Betriebssystems, eine eigene
> „Wiederherstellen“-Funktion gibt es in der App nicht. Liegt der Journal-Ordner in OneDrive, landet
> die Löschung zusätzlich im OneDrive-Papierkorb (online).
>
> **Abweichung von C#:** Dort wurde endgültig gelöscht, und die Rückfrage kam auch, wenn gar keine
> Datei existierte.

### F13 – Journal-Ordner ändern (Button „Schraubenschlüssel“)
1. Ordnerauswahl (F2).
2. Ist danach ein `root_path` gesetzt, wird er registriert: Konfiguration speichern, Überwachung
   auf den neuen Ordner, Index neu aufbauen (F3).
3. Der Editor lädt den aktuellen Tag aus dem neuen Ordner (F7).

> **Hinweis Ist-Verhalten:** Ungespeicherte Änderungen werden dabei in den **neuen** Ordner
> geschrieben. Ursache: Der Auto-Save in F7 nutzt den bereits geänderten Pfad.
> **Entschieden:** Vor dem Ordnerwechsel wird im **alten** Ordner gespeichert. Schlägt das fehl,
> bleibt der alte Ordner aktiv.

### F14 – Externer Link (entfällt, Entscheidung E9)
- In der C#-Version steht unten rechts ein Link auf `http://www.joesgoals.com`.
- Er wird **nicht übernommen** (siehe Kap. 8).

### F15 – Programm beenden (neu, Entscheidung E1)
Ausgelöst durch jedes reguläre Beenden: Fenster schließen (X, `Alt+F4`), Abmelden bzw.
Herunterfahren, sofern das Betriebssystem dies meldet.

1. Gibt es einen Originaltext und weicht der Editortext davon ab, wird er **ohne Rückfrage**
   in die Datei des Originaldatums im aktuellen Journal-Ordner gespeichert. Das ist dieselbe Regel
   wie beim Auto-Save in F7, Schritt 2.
2. Ist der Text unverändert (z. B. nur die Vorlage), wird **nichts** geschrieben. Es entsteht
   also keine leere Vorlagendatei.
3. Ist kein Journal-Ordner gesetzt, wird nichts gespeichert und kein Dialog geöffnet.
4. **Fehler beim Speichern** (Datei gesperrt, Ordner nicht erreichbar, keine Schreibrechte):
   Die App bricht das Beenden ab und zeigt eine Meldung mit drei Möglichkeiten:
   „Erneut versuchen“, „Ohne Speichern beenden“ und „Abbrechen“ (die App bleibt offen).
   So gehen Daten nicht unbemerkt verloren.
5. Danach Ordnerüberwachung beenden und das Programm schließen.

> **Hinweis:** Beim harten Beenden (Prozess abschießen, Absturz, Stromausfall) greift F15 nicht.
> Wer das zusätzlich absichern will, kann optional nach einigen Sekunden ohne Tastendruck
> automatisch speichern (KANN, nicht entschieden).

### F16 – Externe Änderungen am offenen Tag (neu, Entscheidung E5)
Ziel: Eine Version, die z. B. per OneDrive von einem anderen Rechner kommt, wird **nie
stillschweigend überschrieben**. Lokale Eingaben gehen ebenfalls **nie stillschweigend verloren**.

**Plattenstand merken:** Zum offenen Tag merkt sich die App den **Plattenstand**, also den
Dateiinhalt (oder einen Hash davon) zum Zeitpunkt des letzten Ladens bzw. Speichern. Gab es keine
Datei, ist der Plattenstand „keine Datei“. Gesetzt wird er in F7 (Laden), F10/F15/Auto-Save
(Speichern) und F12 (Löschen). Verglichen wird immer der **Inhalt**, nicht das Änderungsdatum.
Sync-Clients setzen Zeitstempel nämlich unzuverlässig.

**A) Erkennen.** Auslöser:
- ein Ereignis der Ordnerüberwachung (F5, entprellt) für die Datei des offenen Tages
  (Ändern, Anlegen, Löschen, Umbenennen),
- **zusätzlich (SOLL)**, wenn das Fenster wieder den Fokus bekommt. Damit werden auch verpasste
  Ereignisse erkannt, z. B. bei Netzlaufwerken oder nach dem Ruhezustand.

Die Datei wird neu gelesen und mit dem Plattenstand verglichen. Ist der Inhalt **gleich**, passiert
nichts. Das ist z. B. das Echo des eigenen Speicherns.

**B) Externe Änderung, Editor ohne lokale Änderungen** (Editortext = Originaltext):
1. Den Tag **automatisch neu laden**, ohne Rückfrage: Originaltext und Plattenstand werden zum
   neuen Inhalt.
2. Ist die Datei extern **gelöscht** worden, zeigt der Editor die Vorlage (F8).
3. Die Cursorposition SOLL möglichst erhalten bleiben (gleiche Zeile und Spalte, gekürzt auf das
   Textende). Der Fokus wird nicht verändert.
4. Kurzer, nicht blockierender Hinweis, z. B. in einer Statuszeile: „Eintrag wurde von außen
   aktualisiert.“ (SOLL)

**C) Externe Änderung, Editor mit lokalen Änderungen → Konflikt:**
1. Der Editor bleibt unverändert, man kann weiterschreiben. Es öffnet sich **kein Dialog**.
2. Über dem Editor erscheint eine **Hinweisleiste** (gelb):
   „Dieser Eintrag wurde außerhalb der App geändert.“ mit zwei Buttons:
   - **„Externe Version laden“**: Die lokalen Änderungen werden **zuerst als Konfliktkopie
     gespeichert** (siehe E). Danach wird die externe Version geladen wie in B.
   - **„Meine Version speichern“**: Die **externe Version** wird als Konfliktkopie gespeichert.
     Danach wird die eigene Version normal gespeichert (F10).
3. Die Leiste bleibt sichtbar, bis man einen der Buttons wählt oder bis der Konflikt anders
   aufgelöst wird (Tageswechsel, Speichern, Schließen, siehe D).
4. Wurde die Datei extern **gelöscht**, lautet der Text „Dieser Eintrag wurde außerhalb der App
   gelöscht.“ mit den Buttons „Löschen übernehmen“ (lokale Änderungen als Konfliktkopie sichern,
   danach Vorlage anzeigen) und „Meine Version speichern“ (Datei wird neu angelegt).

**D) Schutz beim Speichern** (gilt für **jedes** Schreiben: Auto-Save beim Tageswechsel (F7),
Speichern (F10), Beenden (F15) und „Meine Version speichern“):
1. Unmittelbar vor dem Schreiben die Datei lesen und mit dem Plattenstand vergleichen.
2. **Gleich:** normal speichern.
3. **Abweichend** (auch dann, wenn A die Änderung noch nicht bemerkt hat):
   - Zuerst den **aktuellen Plattenstand** als Konfliktkopie sichern (E).
   - Dann die eigene Version in die Tagesdatei schreiben.
   - Hinweis (nicht blockierend): „Konflikt: Die andere Version wurde als `<Dateiname>`
     gesichert.“
   - So blockieren Tageswechsel und Beenden nie, und keine Version geht verloren.
   - **Entschieden:** Die eigene, lokal bearbeitete Version **gewinnt** und steht danach in der
     Tagesdatei. Die andere Version liegt als Konfliktkopie daneben.
4. Danach Plattenstand = geschriebener Inhalt, die Hinweisleiste aus C verschwindet.

**E) Konfliktkopie:**
- Ort: der Journal-Ordner. Name: `YYYY-MM-DD.konflikt-YYYYMMDD-HHMMSS.txt`.
  Der erste Teil ist der betroffene Tag, der zweite der Zeitpunkt der Sicherung (Ortszeit).
  Gibt es den Namen schon, wird `-2`, `-3` … angehängt.
- Inhalt: die gesicherte Version unverändert (UTF-8, CRLF wie 3.2).
- Konfliktkopien passen nicht zum Muster `YYYY-MM-DD.txt` (E2). Sie gelten also **nicht** als
  Eintrag und erzeugen keine Kürzel.
- **Sichtbarkeit (SOLL):** Der Index (F3) erkennt Dateien `YYYY-MM-DD.konflikt-*.txt`
  (Groß-/Kleinschreibung egal) und markiert den Tag im Kalender mit einem **⚠** in der Zelle.
  Der Tooltip nennt die Anzahl der Konfliktkopien.
- Aufräumen: Der Benutzer führt die Versionen von Hand zusammen und löscht die Konfliktkopie im
  Dateisystem. Eine Zusammenführung in der App gibt es nicht.

---

## 6. Fensteraufbau

```
┌──────────────────────── Tagebuch ────────────────────────┐
│  ◀ ▶            November 2026                            │
│ Montag Dienstag Mittwoch Donnerstag Freitag Samstag Sonntag│
│ ┌──┐┌──┐┌──┐ …                     (6 Zeilen × 7 Spalten) │
│ │ 2││ 3││ 4│                                              │
│ │SQ││  ││R │  ← Badge (grün) nur wenn Datei existiert     │
│ └──┘└──┘└──┘                                              │
│                                    (Kalender ≥ 400 px hoch)│
├───────────────────────────────────────────────────────────┤
│ Freitag, 6. November 2026                               ▲ │
│ Rückschau halten                                        █ │
│ 5 Werte                                                 │ │
│ Kniebeugen                                              ▼ │
├───────────────────────────────────────────────────────────┤
│ [🔧][✔][↶][✖]                                             │
└───────────────────────────────────────────────────────────┘
```

- Fenstertitel „Tagebuch“, Anwendungs-Icon Kalender.
- Startgröße 500 × 600 px, Größe veränderbar (mit Griff unten rechts).
- Höhen: Kalender mindestens 400 px, der Editor bekommt den restlichen Platz, die Buttonleiste
  ist so hoch wie nötig.
- Buttons mit Icons (15 × 15 px) und Tooltips: „Configuration“, „Save this entry“,
  „Undo changes“, „Remove this entry“.
- Sprache der Oberfläche: gemischt (Tooltips Englisch, Meldungen Deutsch).
  > **Entschieden:** Einheitlich Deutsch. Die Buttons sind beschriftet („Ordner…“, „Speichern“,
  > „Rückgängig“, „Löschen“) statt Icons, mit deutschen Tooltips. Die Statuszeile steht rechts neben
  > den Buttons.

## 7. Tastatur

| Taste | Wirkung |
|---|---|
| `Strg+S` | Speichern (F10), fensterweit |
| Pfeiltasten (Fokus im Kalender) | Tag wechseln (löst F7 aus) |
| `Strg+Z` / `Strg+Y` (Fokus im Editor) | Undo/Redo des Editors |
| `Alt+←` / `Alt+→` | Vortag / Folgetag, auch mit Fokus im Editor |
| `Alt+↑` / `Alt+↓` | eine Woche zurück / vor, auch mit Fokus im Editor |
| `Alt+Pos1` | Heute, auch mit Fokus im Editor |

Pfeiltasten ohne `Alt` wechseln den Tag nur, wenn kein Eingabefeld den Fokus hat (Pfeil hoch/runter =
eine Woche). Nach F7 liegt der Fokus im Editor, deshalb die `Alt`-Varianten. `Strg` heißt unter macOS
`Cmd`.

## 8. Nicht übernehmen

- **Termine/Appointments** (`Appointment`, `AppointmentWindow`, `AppointmentsConverter`,
  Property `Appointments`, zweite ListBox im Zellen-Template): Reste aus einem Beispiel-Kalender,
  ohne Funktion.
- Doppelklick auf eine Kalenderzelle: hat keine Funktion (Code ist auskommentiert).
- Die Meldung „Because you are not using Windows Vista or later …“.
- Der Link auf `http://www.joesgoals.com` (F14, entschieden E9).

## 9. Offene Entscheidungen für die Neuimplementierung

| # | Frage | Ist | Vorschlag |
|---|---|---|---|
| E1 | Speichern beim Schließen? | nein, Datenverlust | ✅ **entschieden: ja, automatisch** (F15) |
| E2 | Welche Dateien zählen als Eintrag? | jede Datei mit Datums-Präfix | ✅ **entschieden: nur `YYYY-MM-DD.txt`** (3.1, F3) |
| E3 | Kürzel deduplizieren / Wortgrenzen? | nein | ✅ **entschieden: deduplizieren, Regeln in `config.toml`**, Teilstring-Suche bleibt (F4, Kap. 4.2) |
| E4 | Vorlagentext bei Kürzeln ignorieren? | nein | ✅ **entschieden: nein, die Vorlage zählt mit (wie bisher)** (F8) |
| E5 | Externe Änderung am offenen Tag | wird ignoriert bzw. überschrieben | ✅ **entschieden: automatisch neu laden bzw. Hinweisleiste mit Konfliktkopie** (F16) |
| E6 | Vorlage konfigurierbar? | fest im Code | ✅ **entschieden: Konfigurationsdatei** (Kap. 4, F8) |
| E7 | Löschen | endgültig | ✅ **entschieden: Papierkorb** (F12) |
| E8 | Jahres-/Jahrzehntansicht | vorhanden | ✅ **entschieden: behalten** (F6) |
| E9 | Link joesgoals.com | vorhanden (defekt) | ✅ **entschieden: entfällt** (F14, Kap. 8) |

## 10. Akzeptanzkriterien (Testfälle)

| # | Gegeben | Wenn | Dann |
|---|---|---|---|
| T1 | Ordner mit `2026-11-02.txt` (Inhalt „Kreuzheben 3x5“) | App startet | Zelle 2.11. zeigt Badge `DL` |
| T2 | Ordner mit `2026-11-03.txt` (Inhalt „Notiz“) | App startet | Zelle 3.11. zeigt leeres grünes Badge |
| T3 | Kein Eintrag am 6.11.2026 (Freitag) | Tag wählen | Editor zeigt die 4 Vorlagenzeilen für Freitag |
| T4 | T3, nichts geändert | anderen Tag wählen | es wird keine Datei `2026-11-06.txt` angelegt |
| T5 | T3, eine Zeile ergänzt | anderen Tag wählen | `2026-11-06.txt` existiert (UTF-8, CRLF) mit Vorlage und Ergänzung, Badge enthält `SQ` |
| T6 | Tag geladen, Text geändert | Rückgängig | Editor zeigt wieder den Stand beim Laden, Datei unverändert |
| T7 | Tag mit Datei | Löschen → Ja | Datei liegt im **Papierkorb** (wiederherstellbar), Editor zeigt Vorlage, Badge verschwindet |
| T8 | App läuft | anderer Prozess legt `2026-11-10.txt` an | Badge am 10.11. erscheint ohne Neustart (≤ 1 s) |
| T9 | App läuft | anderer Prozess löscht eine Tagesdatei | Badge verschwindet |
| T10 | Konfigurierter Ordner existiert nicht mehr | App startet | Ordnerauswahl wird angeboten |
| T11 | Datei mit „Kniebeuge … Squat“ | Index | Badge `SQ` (einmal) |
| T11a | Standardregeln, Datei „Overhead Press, Bench Press“ | Index | Badge `BP OHP` (die erste OHP-Regel entfällt wegen `ausser`, die zweite trifft) |
| T11b | Standardregeln, Datei „Bankdrücken, Rücken“ | Index | Badge `BP` |
| T11c | Standardregeln, Datei „Klimmzüge 3x8“ | Index | Badge `Kz` |
| T11d | `config.toml` ohne `[[regel]]` | App startet | Der Standard-Regelsatz gilt |
| T11e | `config.toml` mit `regel = []` | App startet | Tage mit Datei zeigen ein leeres grünes Badge |
| T11f | Regel mit `kuerzel = ""` oder `woerter = []` | App startet | Meldung, die Regel wird übersprungen, alle anderen gelten |
| T11g | Regel `woerter = ["KNIEBEUGE"]` | Datei „kniebeugen“ | Die Regel trifft, weil die Schreibweise egal ist |
| T12 | Vorhandene Datei aus der C#-Version mit Umlauten | Tag wählen und speichern | Umlaute bleiben korrekt, Datei bleibt byte-gleich, wenn nichts geändert wurde |
| T13 | Tag geladen, Text geändert, nicht gespeichert | Fenster schließen | Datei enthält den geänderten Text, es gibt keine Rückfrage |
| T14 | Tag ohne Datei, Vorlage unverändert | Fenster schließen | Es wird keine Datei angelegt |
| T15 | Text geändert, Datei schreibgeschützt | Fenster schließen | Meldung mit „Erneut versuchen / Ohne Speichern beenden / Abbrechen“, die App bleibt bis zur Wahl offen |
| T16 | Ordner mit `2026-11-04.md`, `2026-11-05.txt.bak`, `2026-11-06-Laptop.txt` | App startet | An diesen Tagen erscheint kein Badge, die Dateien bleiben unverändert |
| T17 | Ordner mit `2026-11-07.TXT` | App startet | Badge am 7.11. erscheint |
| T18 | Ordner mit `2026-02-30.txt` (**ungültiges Datum**, den 30. Februar gibt es nicht) | App startet | Die Datei wird ignoriert, es gibt keinen Absturz |
| T18a | Ordner mit `2026-11-07.TXT` | 7.11. wählen, Text ändern, speichern | `2026-11-07.TXT` enthält den neuen Text, eine zweite Datei `2026-11-07.txt` gibt es nicht |
| T27 | 7.11. offen, nichts geändert | Anderer Prozess ändert `2026-11-07.txt` | Der Editor zeigt nach höchstens 1 s den neuen Inhalt, ohne Rückfrage, mit Hinweis in der Statuszeile |
| T28 | 7.11. offen, lokal Text ergänzt | Anderer Prozess ändert `2026-11-07.txt` | Gelbe Hinweisleiste erscheint, der Editortext bleibt unverändert, es wird nichts geschrieben |
| T29 | T28 | „Meine Version speichern“ | `2026-11-07.txt` enthält die lokale Version, `2026-11-07.konflikt-….txt` die externe |
| T30 | T28 | „Externe Version laden“ | Der Editor zeigt die externe Version, `2026-11-07.konflikt-….txt` enthält die lokale |
| T31 | 7.11. offen, lokal geändert, externe Änderung **nicht** erkannt (Watcher aus) | Tageswechsel | Die eigene Version wird gespeichert, die externe als Konfliktkopie gesichert, Hinweis erscheint, der Tageswechsel wird nicht blockiert |
| T32 | 7.11. offen, lokal geändert | Anderer Prozess löscht `2026-11-07.txt` | Hinweisleiste „… gelöscht“ mit „Löschen übernehmen“ / „Meine Version speichern“ |
| T33 | App speichert 7.11. selbst | Watcher meldet die eigene Änderung | Keine Hinweisleiste, kein Neuladen (Inhalt = Plattenstand) |
| T34 | Ordner enthält `2026-11-07.konflikt-20261107-101500.txt` | App startet | Die Zelle 7.11. zeigt ⚠, das Badge kommt nur aus `2026-11-07.txt` |
| T35 | Fenster im Hintergrund, Datei des offenen Tages extern geändert, Ereignis verpasst | Fenster bekommt den Fokus | Wie T27 bzw. T28 |
| T19 | Keine `config.toml` vorhanden | App startet | Die Datei wird mit den Standardwerten aus 4.2 angelegt |
| T20 | `template.uebung.freitag = "Laufen"` | Neuen Freitag wählen | Die 4. Vorlagenzeile lautet `Laufen` |
| T21 | `template.lines = ["{datum_lang}", "Gewicht:", "Übung: {uebung}"]` | Neuen Mittwoch wählen | Der Editor zeigt drei Zeilen, die dritte lautet `Übung: Handstand` |
| T22 | `config.toml` mit Syntaxfehler | App startet | Meldung mit Zeilenangabe, Standardvorlage aktiv, Datei bleibt unverändert |
| T23 | `config.toml` mit Kommentaren | Ordner über F13 wechseln | Nur `root_path` ändert sich, Kommentare und andere Werte bleiben erhalten |
| T24 | Tag ohne Datei, Text eingetippt | Löschen | Keine Rückfrage, der Editor zeigt wieder die Vorlage |
| T25 | Journal-Ordner auf einem Netzlaufwerk ohne Papierkorb | Löschen → Ja | Zweite Rückfrage „endgültig löschen?“, bei Nein bleibt die Datei erhalten |
| T26 | Monatsansicht, 6.11.2026 ausgewählt | Titel → Titel → 2027 → März | Monatsansicht März 2027, ausgewählt bleibt der 6.11.2026, der Editor ist unverändert |

## 11. Hinweise für die Rust-Umsetzung (unverbindlich)

- Datum und Lokalisierung: `chrono` bzw. `jiff`. Für ausgeschriebene deutsche Wochentags- und
  Monatsnamen `chrono` mit Feature `unstable-locales` oder eine eigene kleine Tabelle.
- Ordnerüberwachung: `notify` (+ `notify-debouncer-mini` zum Entprellen).
- Config-Pfad: `directories` bzw. `dirs`. Lesen: `serde` + `toml`. Für das gezielte Schreiben
  von `root_path` mit erhaltenen Kommentaren: `toml_edit`.
- Papierkorb: `trash` (unterstützt Windows, macOS und Linux).
- Ordnerdialog / Rückfragen: `rfd`.
- GUI: **entschieden: `egui`/`eframe`** (Kap. 12). Die Kalenderzelle
  mit Badge ist ein eigenes Widget, Standard-Datepicker reichen nicht.
- Sinnvolle Aufteilung, damit Kern und Tests ohne GUI auskommen:
  - `storage` (Pfad, Laden, Speichern, Löschen, atomar schreiben),
  - `index` (F3),
  - `badge` (F4, reine Funktion `fn badge(text: &str, regeln: &[Regel]) -> String` → Unit-Tests
    aus Kap. 5/F4 und 10),
  - `template` (F8),
  - `watcher` (F5),
  - `sync` (F16: Konfliktzustand, Konfliktkopie),
  - `dokument` (F7, F9–F13, F15, F16: offener Tag mit Originaltext und Plattenstand, als reine
    Logik ohne GUI testbar),
  - `ui`.

## 12. Plattformen

Entschieden: Die App läuft auf **Windows, macOS und Linux** aus derselben Codebasis.
Oberfläche: **egui/eframe** (entschieden, siehe Kap. 11).

### 12.1 Pfade der Konfiguration

| System | `config.toml` liegt in |
|---|---|
| Windows | `%APPDATA%\Trainingstagebuch\config.toml` |
| macOS | `~/Library/Application Support/Trainingstagebuch/config.toml` |
| Linux | `$XDG_CONFIG_HOME/trainingstagebuch/config.toml`, Standard `~/.config/trainingstagebuch/config.toml` |

### 12.2 Unterschiede, die der Code ausgleichen MUSS

- **Groß-/Kleinschreibung der Dateinamen:** Linux unterscheidet sie, Windows und macOS
  (Standard) nicht. Die App sucht die vorhandene Tagesdatei deshalb immer selbst (3.1, F3)
  und verlässt sich nicht auf das Dateisystem.
- **Ereignisse der Ordnerüberwachung** kommen je System verschieden an (zusammengefasst,
  verzögert, Umbenennen als Paar). Die App wertet deshalb nur aus, *dass* sich etwas geändert
  hat, und vergleicht danach den Inhalt (F16). Den Ereignistyp wertet sie nicht aus.
- **Tastenkürzel:** `Strg` unter Windows und Linux entspricht `Cmd` unter macOS (`Cmd+S`).
- **Papierkorb (F12):** Die Prüfung „Netzlaufwerk, also kein Papierkorb“ über den Laufwerkstyp
  gibt es nur unter Windows. Auf den anderen Systemen gilt ein Fehler beim Verschieben in den
  Papierkorb als „Papierkorb nicht verfügbar“ (F12, Schritt 5).
- **Sprache:** Datums-, Wochentags- und Monatsnamen kommen aus einer festen deutschen Tabelle,
  nicht aus dem Gebietsschema des Systems. Die Oberfläche ist einheitlich deutsch. Erster
  Wochentag ist immer Montag. Dies ersetzt „in der Sprache des Systems“ in F6 und F8.

### 12.3 Sync-Dienste

OneDrive gibt es für Linux nicht offiziell. Die Dateien sind aber überall gleich, daher
funktioniert jeder Dienst, der einen Ordner synchronisiert (OneDrive, Dropbox, Nextcloud,
Syncthing, iCloud Drive …). Die Konfliktdateien der Dienste heißen unterschiedlich. Die
KANN-Warnung aus 3.1 SOLL deshalb mindestens diese Muster erkennen:

| Dienst | Beispiel |
|---|---|
| OneDrive | `2026-11-07-Laptop.txt` |
| Dropbox | `2026-11-07 (Konflikt­kopie von Laptop).txt` |
| Nextcloud | `2026-11-07 (conflicted copy 2026-11-07 101500).txt` |
| Syncthing | `2026-11-07.sync-conflict-20261107-101500-ABCDEFG.txt` |
| allgemein | `2026-11-07 (1).txt` |

Faustregel: Ein Name, der mit `YYYY-MM-DD` beginnt, auf `.txt` endet und **nicht** genau
`YYYY-MM-DD.txt` bzw. eine eigene Konfliktkopie (F16 E) ist, wird als fremde Konfliktdatei
gemeldet.

### 12.4 Beenden ohne Rückmeldung

Wie Herunterfahren und Abmelden beim Programm ankommen, unterscheidet sich je System, und
nicht jedes wird zuverlässig gemeldet (F15). Damit auch dann nichts verloren geht, wird die
KANN-Option aus F9 übernommen (**SOLL**): **Nach 5 Sekunden ohne Tastendruck** wird ein
geänderter Text automatisch gespeichert, nach denselben Regeln wie der Auto-Save in F7
(einschließlich des Schutzes aus F16 D).

### 12.5 Bauen und Ausliefern

- Gebaut und getestet wird über **GitHub Actions** auf Windows, macOS und Linux. Die Tests des
  Kerns laufen auf allen drei Systemen.
- Windows: eine einzelne `.exe` ohne Konsolenfenster, mit eingebettetem Icon und Versionsinfo.
- Linux: eine Binärdatei mit `.desktop`-Datei, optional AppImage oder `.deb`.
- macOS: ein `.app`-Bundle als Universal-Binary für Apple Silicon und Intel. Für die Weitergabe
  an andere Macs ohne Warnung braucht es eine Signatur mit Apple-Entwickler-Account (KANN). Für den Eigenbedarf reicht „Rechtsklick → Öffnen“.
- **Grafik:** eframe mit dem OpenGL-Backend `glow` statt `wgpu`. Die Exe ist damit etwa halb so
  groß (8 statt 16 MB). Voraussetzung ist ein OpenGL-Treiber (auf normalen Rechnern vorhanden,
  fehlt aber z. B. in manchen virtuellen Maschinen ohne Grafiktreiber).
- **Icon:** eigenes Kalender-Icon in den Farben der App (`crates/journal-app/assets/`, erzeugt
  mit `cargo run -p journal-app --example icon`). Das Icon der C#-Version wird wegen unklarer
  Lizenz nicht übernommen.
- **Pakete:** `.github/workflows/release.yml` baut bei einem Tag `v*` die Pakete für alle drei
  Systeme (siehe README).
