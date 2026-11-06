//! Der offene Tag: Editortext, Originaltext, Plattenstand und Konflikte
//! (Spezifikation F7, F9–F13, F15, F16, 12.4).
//!
//! Die Oberfläche bindet den Editor an [`Dokument::text_mut`] und ruft bei Klicks die Methoden
//! auf. Jede Methode gibt eine [`Wirkung`] zurück, die sagt, was die Oberfläche tun soll.
//! Rückfragen (Löschen, Beenden mit Fehler) stellt die Oberfläche und ruft danach den passenden
//! nächsten Schritt auf.
//!
//! Grundsätze aus F16:
//! - Eine externe Version wird **nie stillschweigend überschrieben**: Vor jedem Schreiben wird der
//!   Plattenstand geprüft, bei Abweichung entsteht erst eine Konfliktkopie.
//! - Lokale Eingaben gehen **nie stillschweigend verloren**: Wird die externe Version geladen,
//!   werden lokale Änderungen vorher als Konfliktkopie gesichert.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, NaiveDateTime};

use crate::de;
use crate::storage::{self, LoeschFehler};
use crate::sync::{self, Konflikt};
use crate::template::{self, Template};

/// Wohin der Cursor soll, wenn der Editortext ersetzt wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// Ans Textende und Fokus in den Editor (F7, Schritt 6).
    EndeMitFokus,
    /// Gleiche Zeile und Spalte, gekürzt auf das Textende, Fokus unverändert (F16 B3).
    Behalten,
}

/// Was die Oberfläche nach einer Aktion tun soll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use]
pub struct Wirkung {
    /// `Some`, wenn der Editortext ersetzt wurde.
    pub editor: Option<Cursor>,
    /// Nicht blockierende Hinweise für die Statuszeile.
    pub hinweise: Vec<String>,
    /// Die App hat Dateien im Journal-Ordner geschrieben oder gelöscht. Der Index soll direkt
    /// neu aufgebaut werden, ohne auf die Ordnerüberwachung zu warten (F10, F12).
    pub dateien_geaendert: bool,
}

impl Wirkung {
    fn und(mut self, spaeter: Wirkung) -> Wirkung {
        self.editor = spaeter.editor.or(self.editor);
        self.hinweise.extend(spaeter.hinweise);
        self.dateien_geaendert |= spaeter.dateien_geaendert;
        self
    }
}

/// Nächster Schritt beim Löschen (F12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoeschSchritt {
    /// Kein Tag offen oder kein Ordner: nichts tun.
    Nichts,
    /// Es gibt keine Datei: ohne Rückfrage [`Dokument::auf_vorlage_zuruecksetzen`] aufrufen.
    OhneRueckfrage,
    /// Rückfrage mit diesem Text (Titel „Tagebuch“, Ja/Nein), bei Ja
    /// [`Dokument::in_papierkorb`] aufrufen.
    Rueckfrage(String),
}

/// Text der zweiten Rückfrage, wenn [`Dokument::in_papierkorb`]
/// [`LoeschFehler::PapierkorbNichtVerfuegbar`] liefert (F12, Schritt 5).
pub const RUECKFRAGE_ENDGUELTIG: &str =
    "Der Papierkorb ist für diesen Ordner nicht verfügbar. Eintrag endgültig löschen?";

/// Ein Fehler beim Lesen oder Schreiben der Tagesdatei, mit Pfad.
#[derive(Debug)]
pub struct DateiFehler {
    pub pfad: PathBuf,
    pub fehler: io::Error,
}

impl fmt::Display for DateiFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pfad.display(), self.fehler)
    }
}

impl std::error::Error for DateiFehler {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.fehler)
    }
}

type Ergebnis = Result<Wirkung, DateiFehler>;

/// Zustand des geladenen Tages.
#[derive(Debug, Clone)]
struct Offen {
    /// Editortext direkt nach Laden bzw. Speichern („geändert“ = Text weicht davon ab).
    original: String,
    /// Dateiinhalt beim letzten Laden bzw. Speichern, `None` = keine Datei (F16).
    platte: Option<Vec<u8>>,
    /// Sichtbare Hinweisleiste (F16 C).
    konflikt: Option<Konflikt>,
}

/// Der offene Tag mit Editortext.
pub struct Dokument {
    root: Option<PathBuf>,
    template: Template,
    datum: NaiveDate,
    text: String,
    /// `Some`, sobald ein Tag aus einem Journal-Ordner geladen ist. Ohne Ordner ist der Editor
    /// inaktiv (F1, Schritt 3).
    offen: Option<Offen>,
    uhr: Box<dyn Fn() -> NaiveDateTime + Send>,
}

impl fmt::Debug for Dokument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dokument")
            .field("root", &self.root)
            .field("datum", &self.datum)
            .field("text", &self.text)
            .field("offen", &self.offen)
            .finish_non_exhaustive()
    }
}

impl Dokument {
    /// Neues Dokument ohne geladenen Tag. Danach [`Dokument::tag_waehlen`] mit heute aufrufen
    /// (F1, Schritt 5).
    pub fn new(template: Template, root: Option<PathBuf>) -> Self {
        Self {
            root,
            template,
            datum: Local::now().date_naive(),
            text: String::new(),
            offen: None,
            uhr: Box::new(|| Local::now().naive_local()),
        }
    }

    /// Ersetzt die Uhr für die Namen von Konfliktkopien (für Tests).
    pub fn mit_uhr(mut self, uhr: impl Fn() -> NaiveDateTime + Send + 'static) -> Self {
        self.uhr = Box::new(uhr);
        self
    }

    // -- Abfragen --------------------------------------------------------------------------

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Der ausgewählte Tag. Ohne Ordner ist das nur die Auswahl im Kalender.
    pub fn datum(&self) -> NaiveDate {
        self.datum
    }

    /// `true`, wenn ein Tag geladen ist und der Editor bearbeitet werden kann.
    pub fn aktiv(&self) -> bool {
        self.offen.is_some()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Für den Editor. Ist [`Dokument::aktiv`] `false`, soll die Oberfläche ihn sperren.
    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    /// Text weicht vom Originaltext ab (F9).
    pub fn geaendert(&self) -> bool {
        self.offen.as_ref().is_some_and(|o| o.original != self.text)
    }

    /// Zu zeigende Hinweisleiste (F16 C).
    pub fn konflikt(&self) -> Option<Konflikt> {
        self.offen.as_ref().and_then(|o| o.konflikt)
    }

    // -- Tag wählen, Ordner wechseln -------------------------------------------------------

    /// Anderen Tag auswählen (F7).
    ///
    /// Speichert zuerst geänderten Text des bisherigen Tages (mit Schutz aus F16 D). Schlägt das
    /// fehl, bleibt der bisherige Tag offen und der Fehler wird zurückgegeben. Die Oberfläche soll
    /// dann die Auswahl im Kalender zurücksetzen und den Fehler zeigen.
    pub fn tag_waehlen(&mut self, datum: NaiveDate) -> Ergebnis {
        if self.root.is_none() {
            self.datum = datum;
            return Ok(Wirkung::default());
        }
        let gespeichert = self.speichern_wenn_geaendert()?;
        let geladen = self.laden(datum)?;
        Ok(gespeichert.und(geladen))
    }

    /// Journal-Ordner wechseln (F13) oder erstmals setzen.
    ///
    /// Geänderter Text wird vorher **im alten Ordner** gespeichert (Empfehlung zu F13). Schlägt
    /// das fehl, bleibt alles beim Alten. Danach wird der ausgewählte Tag aus dem neuen Ordner
    /// geladen.
    pub fn ordner_setzen(&mut self, root: PathBuf) -> Ergebnis {
        let gespeichert = self.speichern_wenn_geaendert()?;
        self.root = Some(root);
        self.offen = None;
        self.text.clear();
        let mut geladen = self.laden(self.datum)?;
        geladen.dateien_geaendert = true;
        Ok(gespeichert.und(geladen))
    }

    /// Lädt den Tag ohne vorheriges Speichern (F7, Schritte 3–5).
    fn laden(&mut self, datum: NaiveDate) -> Ergebnis {
        let root = self.root.clone().expect("laden nur mit Ordner");
        let platte = Self::platte_lesen(&root, datum)?;
        self.datum = datum;
        self.uebernehmen(platte);
        Ok(Wirkung {
            editor: Some(Cursor::EndeMitFokus),
            ..Wirkung::default()
        })
    }

    /// Setzt Editortext, Originaltext und Plattenstand auf `platte` (bzw. die Vorlage).
    fn uebernehmen(&mut self, platte: Option<Vec<u8>>) {
        self.text = match &platte {
            Some(bytes) => storage::text_aus_bytes(bytes),
            None => template::render(&self.template, self.datum),
        };
        self.offen = Some(Offen {
            original: self.text.clone(),
            platte,
            konflikt: None,
        });
    }

    fn platte_lesen(root: &Path, datum: NaiveDate) -> Result<Option<Vec<u8>>, DateiFehler> {
        storage::laden(root, datum)
            .map(|o| o.map(|(_, bytes)| bytes))
            .map_err(|fehler| DateiFehler {
                pfad: root.join(storage::dateiname(datum)),
                fehler,
            })
    }

    // -- Speichern -------------------------------------------------------------------------

    /// Speichern-Button bzw. `Strg+S` (F10), auch „Meine Version speichern“ (F16 C).
    ///
    /// Schreibt immer, auch unveränderten Text (also auch die reine Vorlage). Der Cursor bleibt,
    /// wo er ist.
    pub fn speichern(&mut self) -> Ergebnis {
        if self.offen.is_none() {
            return Ok(Wirkung::default());
        }
        self.schreiben_geschuetzt()
    }

    /// Speichert nur, wenn der Text geändert ist: Auto-Save beim Tageswechsel (F7), Beenden
    /// (F15), nach einigen Sekunden ohne Tastendruck (12.4), vor dem Ordnerwechsel (F13).
    ///
    /// Beim Beenden: Bei einem Fehler zeigt die Oberfläche „Erneut versuchen / Ohne Speichern
    /// beenden / Abbrechen“ (F15, Schritt 4).
    pub fn speichern_wenn_geaendert(&mut self) -> Ergebnis {
        if !self.geaendert() {
            return Ok(Wirkung::default());
        }
        self.schreiben_geschuetzt()
    }

    /// Schreiben mit Schutz aus F16 D.
    fn schreiben_geschuetzt(&mut self) -> Ergebnis {
        let root = self.root.clone().expect("offen nur mit Ordner");
        let datum = self.datum;
        let aktuell = Self::platte_lesen(&root, datum)?;
        let offen = self.offen.as_mut().expect("nur mit offenem Tag");
        let mut wirkung = Wirkung {
            dateien_geaendert: true,
            ..Wirkung::default()
        };

        if aktuell != offen.platte
            && let Some(andere) = &aktuell
        {
            let name = sync::konfliktkopie_schreiben(&root, datum, (self.uhr)(), andere).map_err(
                |fehler| DateiFehler {
                    pfad: root.clone(),
                    fehler,
                },
            )?;
            wirkung.hinweise.push(format!(
                "Konflikt: Die andere Version wurde als {name} gesichert."
            ));
        }

        storage::schreiben(&root, datum, &self.text).map_err(|fehler| DateiFehler {
            pfad: root.join(storage::dateiname(datum)),
            fehler,
        })?;
        offen.platte = Some(storage::bytes_aus_text(&self.text));
        offen.original = self.text.clone();
        offen.konflikt = None;
        Ok(wirkung)
    }

    // -- Rückgängig ------------------------------------------------------------------------

    /// Editor auf den Stand beim letzten Laden bzw. Speichern zurücksetzen (F11). Schreibt nichts.
    pub fn rueckgaengig(&mut self) -> Wirkung {
        let Some(offen) = &self.offen else {
            return Wirkung::default();
        };
        self.text = offen.original.clone();
        Wirkung {
            editor: Some(Cursor::EndeMitFokus),
            ..Wirkung::default()
        }
    }

    // -- Löschen ---------------------------------------------------------------------------

    /// Erster Schritt beim Löschen (F12): Was soll die Oberfläche fragen?
    pub fn loeschen_pruefen(&self) -> Result<LoeschSchritt, DateiFehler> {
        let (Some(root), Some(_)) = (&self.root, &self.offen) else {
            return Ok(LoeschSchritt::Nichts);
        };
        let datei = storage::tagesdatei_finden(root, self.datum).map_err(|fehler| DateiFehler {
            pfad: root.clone(),
            fehler,
        })?;
        Ok(match datei {
            Some(_) => LoeschSchritt::Rueckfrage(format!(
                "Eintrag vom {} in den Papierkorb verschieben?",
                de::datum_lang(self.datum)
            )),
            None => LoeschSchritt::OhneRueckfrage,
        })
    }

    /// Kein Datei vorhanden: Editor ohne Rückfrage auf die Vorlage setzen (F12, Schritt 2).
    pub fn auf_vorlage_zuruecksetzen(&mut self) -> Wirkung {
        if self.offen.is_none() {
            return Wirkung::default();
        }
        self.text = template::render(&self.template, self.datum);
        Wirkung {
            editor: Some(Cursor::EndeMitFokus),
            ..Wirkung::default()
        }
    }

    /// Nach „Ja“: Datei in den Papierkorb (F12, Schritte 4–8). Ungespeicherte Änderungen werden
    /// verworfen.
    ///
    /// Bei [`LoeschFehler::PapierkorbNichtVerfuegbar`] fragt die Oberfläche
    /// [`RUECKFRAGE_ENDGUELTIG`] und ruft bei „Ja“ [`Dokument::endgueltig_loeschen`] auf.
    /// Bei [`LoeschFehler::Sonstiger`] zeigt sie den Fehler, Datei und Editor bleiben unverändert.
    pub fn in_papierkorb(&mut self) -> Result<Wirkung, LoeschFehler> {
        self.datei_entfernen(storage::in_papierkorb)
    }

    /// Nach der zweiten Rückfrage: endgültig löschen (F12, Schritt 5).
    pub fn endgueltig_loeschen(&mut self) -> Result<Wirkung, LoeschFehler> {
        self.datei_entfernen(|pfad| {
            storage::endgueltig_loeschen(pfad).map_err(LoeschFehler::Sonstiger)
        })
    }

    fn datei_entfernen(
        &mut self,
        entfernen: impl FnOnce(&Path) -> Result<(), LoeschFehler>,
    ) -> Result<Wirkung, LoeschFehler> {
        let (Some(root), Some(_)) = (self.root.clone(), &self.offen) else {
            return Ok(Wirkung::default());
        };
        if let Some(pfad) =
            storage::tagesdatei_finden(&root, self.datum).map_err(LoeschFehler::Sonstiger)?
        {
            entfernen(&pfad)?;
        }
        self.uebernehmen(None);
        Ok(Wirkung {
            editor: Some(Cursor::EndeMitFokus),
            hinweise: Vec::new(),
            dateien_geaendert: true,
        })
    }

    // -- Externe Änderungen (F16) ----------------------------------------------------------

    /// Prüft die Datei des offenen Tages auf externe Änderungen (F16 A). Aufrufen bei einem
    /// Ereignis der Ordnerüberwachung für diesen Tag und wenn das Fenster den Fokus bekommt.
    ///
    /// Ist die Datei gerade nicht lesbar (z. B. vom Sync-Client gesperrt), passiert nichts. Der
    /// nächste Auslöser prüft erneut.
    pub fn pruefen(&mut self) -> Wirkung {
        let (Some(root), Some(offen)) = (&self.root, &self.offen) else {
            return Wirkung::default();
        };
        let Ok(aktuell) = Self::platte_lesen(root, self.datum) else {
            return Wirkung::default();
        };
        if aktuell == offen.platte {
            // Gleich, z. B. das Echo des eigenen Speicherns (T33). Eine Leiste von vorher wird
            // hinfällig, wenn die Datei wieder dem Plattenstand entspricht.
            if let Some(o) = self.offen.as_mut() {
                o.konflikt = None;
            }
            return Wirkung::default();
        }

        if self.geaendert() {
            // F16 C: Leiste zeigen, nichts schreiben, Plattenstand bleibt der alte.
            let konflikt = if aktuell.is_some() {
                Konflikt::Geaendert
            } else {
                Konflikt::Geloescht
            };
            if let Some(o) = self.offen.as_mut() {
                o.konflikt = Some(konflikt);
            }
            return Wirkung::default();
        }

        // F16 B: ohne lokale Änderungen automatisch neu laden.
        let hinweis = if aktuell.is_some() {
            "Eintrag wurde von außen aktualisiert."
        } else {
            "Eintrag wurde außerhalb der App gelöscht."
        };
        self.uebernehmen(aktuell);
        Wirkung {
            editor: Some(Cursor::Behalten),
            hinweise: vec![hinweis.to_owned()],
            dateien_geaendert: false,
        }
    }

    /// „Externe Version laden“ bzw. „Löschen übernehmen“ (F16 C): Lokale Änderungen zuerst als
    /// Konfliktkopie sichern, dann die Version von der Platte laden.
    pub fn externe_laden(&mut self) -> Ergebnis {
        let (Some(root), Some(_)) = (self.root.clone(), &self.offen) else {
            return Ok(Wirkung::default());
        };
        let aktuell = Self::platte_lesen(&root, self.datum)?;
        let mut wirkung = Wirkung {
            editor: Some(Cursor::Behalten),
            ..Wirkung::default()
        };
        if self.geaendert() {
            let name = sync::konfliktkopie_schreiben(
                &root,
                self.datum,
                (self.uhr)(),
                &storage::bytes_aus_text(&self.text),
            )
            .map_err(|fehler| DateiFehler {
                pfad: root.clone(),
                fehler,
            })?;
            wirkung
                .hinweise
                .push(format!("Deine Version wurde als {name} gesichert."));
            wirkung.dateien_geaendert = true;
        }
        self.uebernehmen(aktuell);
        Ok(wirkung)
    }
}

#[cfg(test)]
#[allow(unused_must_use)] // In Tests interessiert die Wirkung nicht überall.
mod tests {
    use std::fs;

    use super::*;
    use crate::config::Config;
    use crate::index::IndexCache;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    const KONFLIKT: &str = "2026-11-07.konflikt-20261107-101500.txt";

    fn uhr() -> NaiveDateTime {
        d(2026, 11, 7).and_hms_opt(10, 15, 0).unwrap()
    }

    struct Umgebung {
        dir: tempfile::TempDir,
        doc: Dokument,
    }

    impl Umgebung {
        fn neu() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let doc =
                Dokument::new(Config::default().template, Some(dir.path().to_owned())).mit_uhr(uhr);
            Self { dir, doc }
        }

        fn pfad(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }

        fn schreibe(&self, name: &str, inhalt: &str) {
            fs::write(self.pfad(name), inhalt).unwrap();
        }

        fn lies(&self, name: &str) -> String {
            fs::read_to_string(self.pfad(name)).unwrap()
        }

        fn namen(&self) -> Vec<String> {
            let mut v: Vec<_> = fs::read_dir(self.dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            v.sort();
            v
        }

        /// 7.11. öffnen, die Datei hat den Inhalt `inhalt`.
        fn offen_mit(inhalt: &str) -> Self {
            let mut u = Self::neu();
            u.schreibe("2026-11-07.txt", inhalt);
            let w = u.doc.tag_waehlen(d(2026, 11, 7)).unwrap();
            assert_eq!(w.editor, Some(Cursor::EndeMitFokus));
            u
        }
    }

    const VORLAGE_FREITAG: &str =
        "Freitag, 6. November 2026\nRückschau halten\n5 Werte\nKniebeugen";

    // -- F7 / F8 / F9 --------------------------------------------------------------------------

    #[test]
    fn t3_t4_vorlage_unveraendert_keine_datei() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        assert_eq!(u.doc.text(), VORLAGE_FREITAG);
        assert!(!u.doc.geaendert());

        let w = u.doc.tag_waehlen(d(2026, 11, 7)).unwrap();
        assert!(!w.dateien_geaendert);
        assert!(u.namen().is_empty());
    }

    #[test]
    fn t5_ergaenzte_vorlage_wird_beim_tageswechsel_gespeichert() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        u.doc.text_mut().push_str("\nKniebeugen 5x5 mit 80 kg");
        let w = u.doc.tag_waehlen(d(2026, 11, 7)).unwrap();
        assert!(w.dateien_geaendert);

        let bytes = fs::read(u.pfad("2026-11-06.txt")).unwrap();
        assert_eq!(
            bytes,
            format!("{VORLAGE_FREITAG}\nKniebeugen 5x5 mit 80 kg\n")
                .replace('\n', "\r\n")
                .as_bytes()
        );
        let index = IndexCache::new(Config::default().regeln)
            .aufbauen(u.dir.path())
            .unwrap();
        let badge = index.tag(d(2026, 11, 6)).unwrap().badge.as_deref().unwrap();
        assert!(badge.split(' ').any(|k| k == "SQ"), "{badge}");
    }

    #[test]
    fn datei_ersetzt_vorlage_vollstaendig() {
        let mut u = Umgebung::neu();
        u.schreibe("2026-11-06.txt", "Nur das\r\n");
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        assert_eq!(u.doc.text(), "Nur das\n");
    }

    #[test]
    fn ohne_ordner_passiert_nichts() {
        let mut doc = Dokument::new(Config::default().template, None);
        let w = doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        assert_eq!(w, Wirkung::default());
        assert!(!doc.aktiv());
        assert_eq!(doc.datum(), d(2026, 11, 6));
        assert_eq!(doc.speichern().unwrap(), Wirkung::default());
        assert_eq!(doc.loeschen_pruefen().unwrap(), LoeschSchritt::Nichts);
    }

    #[test]
    fn tageswechsel_mit_speicherfehler_bleibt_auf_altem_tag() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("neu");
        sperren(&u.pfad("2026-11-07.txt"), true);

        assert!(u.doc.tag_waehlen(d(2026, 11, 8)).is_err());
        assert_eq!(u.doc.datum(), d(2026, 11, 7));
        assert_eq!(u.doc.text(), "alt\nneu");
        assert!(u.doc.geaendert());

        sperren(&u.pfad("2026-11-07.txt"), false);
    }

    // -- F10 / F11 -----------------------------------------------------------------------------

    #[test]
    fn f10_speichert_auch_unveraenderte_vorlage_cursor_bleibt() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        let w = u.doc.speichern().unwrap();
        assert_eq!(w.editor, None);
        assert!(w.dateien_geaendert);
        assert_eq!(
            u.lies("2026-11-06.txt"),
            format!("{VORLAGE_FREITAG}\n").replace('\n', "\r\n")
        );
        assert!(!u.doc.geaendert());
    }

    #[test]
    fn t6_rueckgaengig() {
        let mut u = Umgebung::offen_mit("Stand\r\n");
        u.doc.text_mut().push_str("mehr");
        let w = u.doc.rueckgaengig();
        assert_eq!(w.editor, Some(Cursor::EndeMitFokus));
        assert_eq!(u.doc.text(), "Stand\n");
        assert!(!u.doc.geaendert());
        assert_eq!(u.lies("2026-11-07.txt"), "Stand\r\n");
    }

    #[test]
    fn t18a_grossgeschriebene_datei_wird_ueberschrieben() {
        let mut u = Umgebung::neu();
        u.schreibe("2026-11-07.TXT", "alt\r\n");
        u.doc.tag_waehlen(d(2026, 11, 7)).unwrap();
        u.doc.text_mut().push_str("neu");
        u.doc.speichern().unwrap();
        assert_eq!(u.namen(), ["2026-11-07.TXT"]);
        assert_eq!(u.lies("2026-11-07.TXT"), "alt\r\nneu\r\n");
    }

    #[test]
    fn t12_unveraendert_speichern_bleibt_byte_gleich() {
        let original = "Freitag\r\nÄpfel, Öl, Übung, Maß\r\n";
        let mut u = Umgebung::offen_mit(original);
        u.doc.speichern().unwrap();
        u.doc.tag_waehlen(d(2026, 11, 8)).unwrap();
        assert_eq!(
            fs::read(u.pfad("2026-11-07.txt")).unwrap(),
            original.as_bytes()
        );
        assert_eq!(u.namen(), ["2026-11-07.txt"]);
    }

    // -- F12 -----------------------------------------------------------------------------------

    #[test]
    fn t24_ohne_datei_kein_rueckfrage_vorlage() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        u.doc.text_mut().push_str("getippt");
        assert_eq!(
            u.doc.loeschen_pruefen().unwrap(),
            LoeschSchritt::OhneRueckfrage
        );
        let w = u.doc.auf_vorlage_zuruecksetzen();
        assert_eq!(w.editor, Some(Cursor::EndeMitFokus));
        assert_eq!(u.doc.text(), VORLAGE_FREITAG);
        assert!(u.namen().is_empty());
    }

    #[test]
    fn t7_loeschen_mit_rueckfrage_danach_vorlage() {
        let mut u = Umgebung::neu();
        u.schreibe("2026-11-06.txt", "Kniebeugen\r\n");
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        u.doc.text_mut().push_str("ungespeichert");
        assert_eq!(
            u.doc.loeschen_pruefen().unwrap(),
            LoeschSchritt::Rueckfrage(
                "Eintrag vom Freitag, 6. November 2026 in den Papierkorb verschieben?".into()
            )
        );
        // Der echte Papierkorb wird in storage::tests::papierkorb_echt getestet.
        let w = u.doc.endgueltig_loeschen().unwrap();
        assert!(w.dateien_geaendert);
        assert!(u.namen().is_empty());
        assert_eq!(u.doc.text(), VORLAGE_FREITAG);
        assert!(!u.doc.geaendert());
        assert!(
            IndexCache::new(Config::default().regeln)
                .aufbauen(u.dir.path())
                .unwrap()
                .tage
                .is_empty()
        );
    }

    // -- F13 -----------------------------------------------------------------------------------

    #[test]
    fn f13_aenderungen_landen_im_alten_ordner() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("neu");
        let neu = tempfile::tempdir().unwrap();
        fs::write(neu.path().join("2026-11-07.txt"), "anderer Ordner\r\n").unwrap();

        let w = u.doc.ordner_setzen(neu.path().to_owned()).unwrap();
        assert!(w.dateien_geaendert);
        assert_eq!(w.editor, Some(Cursor::EndeMitFokus));
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nneu\r\n");
        assert_eq!(u.doc.text(), "anderer Ordner\n");
        assert_eq!(
            fs::read_to_string(neu.path().join("2026-11-07.txt")).unwrap(),
            "anderer Ordner\r\n"
        );
    }

    #[test]
    fn f1_ordner_spaeter_setzen_laedt_ausgewaehlten_tag() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Dokument::new(Config::default().template, None);
        doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        doc.ordner_setzen(dir.path().to_owned()).unwrap();
        assert!(doc.aktiv());
        assert_eq!(doc.text(), VORLAGE_FREITAG);
    }

    // -- F15 -----------------------------------------------------------------------------------

    #[test]
    fn t13_beenden_speichert_ohne_rueckfrage() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("neu");
        u.doc.speichern_wenn_geaendert().unwrap();
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nneu\r\n");
    }

    #[test]
    fn t14_beenden_mit_unveraenderter_vorlage_legt_keine_datei_an() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        let w = u.doc.speichern_wenn_geaendert().unwrap();
        assert_eq!(w, Wirkung::default());
        assert!(u.namen().is_empty());
    }

    #[test]
    fn t15_beenden_mit_schreibschutz_meldet_fehler_und_erneut_versuchen() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("neu");
        sperren(&u.pfad("2026-11-07.txt"), true);
        let e = u.doc.speichern_wenn_geaendert().unwrap_err();
        assert_eq!(e.fehler.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            u.doc.geaendert(),
            "Text bleibt für „Erneut versuchen“ erhalten"
        );

        sperren(&u.pfad("2026-11-07.txt"), false);
        u.doc.speichern_wenn_geaendert().unwrap();
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nneu\r\n");
    }

    // -- F16 -----------------------------------------------------------------------------------

    #[test]
    fn t27_extern_geaendert_ohne_lokale_aenderung_wird_neu_geladen() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.schreibe("2026-11-07.txt", "vom Laptop\r\n");
        let w = u.doc.pruefen();
        assert_eq!(w.editor, Some(Cursor::Behalten));
        assert_eq!(w.hinweise, ["Eintrag wurde von außen aktualisiert."]);
        assert_eq!(u.doc.text(), "vom Laptop\n");
        assert!(!u.doc.geaendert());
        assert_eq!(u.doc.konflikt(), None);
    }

    #[test]
    fn extern_geloescht_ohne_lokale_aenderung_zeigt_vorlage() {
        let mut u = Umgebung::neu();
        u.schreibe("2026-11-06.txt", "x\r\n");
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        fs::remove_file(u.pfad("2026-11-06.txt")).unwrap();
        let w = u.doc.pruefen();
        assert_eq!(w.hinweise, ["Eintrag wurde außerhalb der App gelöscht."]);
        assert_eq!(u.doc.text(), VORLAGE_FREITAG);
    }

    #[test]
    fn t28_konflikt_zeigt_leiste_und_schreibt_nichts() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        u.schreibe("2026-11-07.txt", "extern\r\n");
        let w = u.doc.pruefen();
        assert_eq!(w, Wirkung::default());
        assert_eq!(u.doc.konflikt(), Some(Konflikt::Geaendert));
        assert_eq!(u.doc.text(), "alt\nlokal");
        assert_eq!(u.namen(), ["2026-11-07.txt"]);
        assert_eq!(u.lies("2026-11-07.txt"), "extern\r\n");
    }

    #[test]
    fn t29_meine_version_speichern() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        u.schreibe("2026-11-07.txt", "extern\r\n");
        let _ = u.doc.pruefen();

        let w = u.doc.speichern().unwrap();
        assert_eq!(
            w.hinweise,
            [format!(
                "Konflikt: Die andere Version wurde als {KONFLIKT} gesichert."
            )]
        );
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nlokal\r\n");
        assert_eq!(u.lies(KONFLIKT), "extern\r\n");
        assert_eq!(u.doc.konflikt(), None);
    }

    #[test]
    fn t30_externe_version_laden() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        u.schreibe("2026-11-07.txt", "extern\r\n");
        let _ = u.doc.pruefen();

        let w = u.doc.externe_laden().unwrap();
        assert_eq!(w.editor, Some(Cursor::Behalten));
        assert_eq!(
            w.hinweise,
            [format!("Deine Version wurde als {KONFLIKT} gesichert.")]
        );
        assert_eq!(u.doc.text(), "extern\n");
        assert!(!u.doc.geaendert());
        assert_eq!(u.doc.konflikt(), None);
        assert_eq!(u.lies(KONFLIKT), "alt\r\nlokal\r\n");
        assert_eq!(u.lies("2026-11-07.txt"), "extern\r\n");
    }

    #[test]
    fn t31_nicht_bemerkte_aenderung_beim_tageswechsel() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        u.schreibe("2026-11-07.txt", "extern\r\n");
        // Kein pruefen(): Die Überwachung hat nichts gemeldet.
        let w = u.doc.tag_waehlen(d(2026, 11, 8)).unwrap();
        assert_eq!(u.doc.datum(), d(2026, 11, 8));
        assert_eq!(w.hinweise.len(), 1);
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nlokal\r\n");
        assert_eq!(u.lies(KONFLIKT), "extern\r\n");
    }

    #[test]
    fn t32_extern_geloescht_mit_lokaler_aenderung() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        fs::remove_file(u.pfad("2026-11-07.txt")).unwrap();
        let _ = u.doc.pruefen();
        let k = u.doc.konflikt().unwrap();
        assert_eq!(k, Konflikt::Geloescht);
        assert_eq!(k.button_extern(), "Löschen übernehmen");
        assert_eq!(k.button_eigen(), "Meine Version speichern");
    }

    #[test]
    fn t32_loeschen_uebernehmen_sichert_lokale_aenderung() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        fs::remove_file(u.pfad("2026-11-07.txt")).unwrap();
        let _ = u.doc.pruefen();

        u.doc.externe_laden().unwrap();
        assert_eq!(u.namen(), [KONFLIKT]);
        assert_eq!(u.lies(KONFLIKT), "alt\r\nlokal\r\n");
        assert!(u.doc.text().starts_with("Samstag, 7. November 2026"));
    }

    #[test]
    fn t32_meine_version_legt_geloeschte_datei_neu_an() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        fs::remove_file(u.pfad("2026-11-07.txt")).unwrap();
        let _ = u.doc.pruefen();

        let w = u.doc.speichern().unwrap();
        assert!(w.hinweise.is_empty(), "nichts zu sichern");
        assert_eq!(u.namen(), ["2026-11-07.txt"]);
        assert_eq!(u.lies("2026-11-07.txt"), "alt\r\nlokal\r\n");
    }

    #[test]
    fn t33_echo_des_eigenen_speicherns() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("neu");
        u.doc.speichern().unwrap();
        u.doc.text_mut().push_str(" weiter");
        assert_eq!(u.doc.pruefen(), Wirkung::default());
        assert_eq!(u.doc.konflikt(), None);
        assert_eq!(u.doc.text(), "alt\nneu weiter");
    }

    #[test]
    fn leiste_verschwindet_wenn_datei_wieder_dem_plattenstand_entspricht() {
        let mut u = Umgebung::offen_mit("alt\r\n");
        u.doc.text_mut().push_str("lokal");
        u.schreibe("2026-11-07.txt", "extern\r\n");
        let _ = u.doc.pruefen();
        assert!(u.doc.konflikt().is_some());
        u.schreibe("2026-11-07.txt", "alt\r\n");
        let _ = u.doc.pruefen();
        assert_eq!(u.doc.konflikt(), None);
    }

    #[test]
    fn extern_angelegt_waehrend_vorlage_offen() {
        let mut u = Umgebung::neu();
        u.doc.tag_waehlen(d(2026, 11, 6)).unwrap();
        u.doc.text_mut().push_str("\nlokal");
        u.schreibe("2026-11-06.txt", "vom Handy\r\n");
        let _ = u.doc.pruefen();
        assert_eq!(u.doc.konflikt(), Some(Konflikt::Geaendert));
        u.doc.speichern().unwrap();
        assert_eq!(
            u.lies("2026-11-06.konflikt-20261107-101500.txt"),
            "vom Handy\r\n"
        );
    }

    fn sperren(pfad: &Path, schreibgeschuetzt: bool) {
        let mut rechte = fs::metadata(pfad).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        rechte.set_readonly(schreibgeschuetzt);
        fs::set_permissions(pfad, rechte).unwrap();
    }
}
