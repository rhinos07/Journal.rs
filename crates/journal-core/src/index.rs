//! Index der Tagesdateien aufbauen (Spezifikation F3, Dateinamen Kap. 3.1).
//!
//! [`IndexCache`] merkt sich pro Datei Änderungszeit, Größe und Badge. Beim nächsten Aufbau wird
//! nur neu gelesen, was sich geändert hat. Das ist wichtig bei OneDrive „Dateien bei Bedarf“:
//! Jedes Lesen einer Datei, die nur online liegt, lädt sie herunter.
//!
//! Der Aufbau blockiert. Die Oberfläche ruft ihn in einem Hintergrund-Thread auf.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use chrono::NaiveDate;

use crate::badge::{Regel, badge};
use crate::storage::{self, Art};

/// Was der Kalender zu einem Tag anzeigt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tag {
    /// `Some`, wenn die Tagesdatei existiert (grünes Badge, auch bei leerem Text, F6).
    pub badge: Option<String>,
    /// Namen der Konfliktdateien zu diesem Tag: eigene Konfliktkopien (F16 E) und
    /// Konfliktdateien von Sync-Diensten (12.3). Nicht leer bedeutet ⚠ in der Zelle.
    pub konflikte: Vec<String>,
}

/// Ergebnis eines Index-Aufbaus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub tage: BTreeMap<NaiveDate, Tag>,
    /// Dateien, die gerade nicht lesbar waren (z. B. vom Sync-Client gesperrt). Der Tag wird
    /// trotzdem als „Datei vorhanden“ geführt, mit dem zuletzt bekannten oder leerem Badge.
    /// Beim nächsten Aufbau wird erneut gelesen.
    pub lesefehler: Vec<(PathBuf, String)>,
}

impl Index {
    pub fn tag(&self, datum: NaiveDate) -> Option<&Tag> {
        self.tage.get(&datum)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stand {
    geaendert: Option<SystemTime>,
    groesse: u64,
}

/// Zwischenspeicher für [`IndexCache::aufbauen`]. Gehört zu einem Regelsatz.
#[derive(Debug, Default)]
pub struct IndexCache {
    regeln: Vec<Regel>,
    dateien: HashMap<PathBuf, (Stand, String)>,
}

impl IndexCache {
    pub fn new(regeln: Vec<Regel>) -> Self {
        Self {
            regeln,
            dateien: HashMap::new(),
        }
    }

    /// Baut den Index für `root` komplett neu auf (F3). Fehler nur, wenn der Ordner selbst nicht
    /// lesbar ist.
    pub fn aufbauen(&mut self, root: &Path) -> io::Result<Index> {
        self.aufbauen_mit(root, |_| {})
    }

    /// Wie [`IndexCache::aufbauen`], meldet aber Zwischenstände, solange Dateien gelesen werden
    /// müssen:
    ///
    /// 1. Gleich nach dem Auflisten des Ordners: Alle Tage mit Datei sind schon enthalten. Tage,
    ///    deren Inhalt noch gelesen werden muss, haben das zuletzt bekannte oder ein leeres Badge.
    ///    Dafür wird keine Datei geöffnet (OneDrive lädt also noch nichts herunter).
    /// 2. Danach etwa alle [`MELDEN_ALLE`] mit den bis dahin gelesenen Badges, die neuesten Tage
    ///    zuerst.
    ///
    /// Muss keine Datei gelesen werden, wird nichts gemeldet. Das Endergebnis ist der Rückgabewert.
    pub fn aufbauen_mit(
        &mut self,
        root: &Path,
        mut melden: impl FnMut(Zwischenstand<'_>),
    ) -> io::Result<Index> {
        let mut index = Index::default();
        // Pro Datum die gewählte Tagesdatei und ob ihre Endung klein ist (F3, Punkt 4).
        let mut tagesdateien: BTreeMap<NaiveDate, (PathBuf, bool)> = BTreeMap::new();

        for eintrag in fs::read_dir(root)? {
            let Ok(eintrag) = eintrag else { continue };
            let name = eintrag.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some((datum, art)) = storage::klassifiziere(name) else {
                continue;
            };
            if !storage::ist_datei(&eintrag) {
                continue;
            }
            match art {
                Art::Tagesdatei { endung_klein } => {
                    let besser = tagesdateien
                        .get(&datum)
                        .is_none_or(|(_, klein)| !klein && endung_klein);
                    if besser {
                        tagesdateien.insert(datum, (eintrag.path(), endung_klein));
                    }
                }
                Art::Konfliktkopie | Art::FremderKonflikt => {
                    index
                        .tage
                        .entry(datum)
                        .or_default()
                        .konflikte
                        .push(name.to_owned());
                }
            }
        }

        for tag in index.tage.values_mut() {
            tag.konflikte.sort();
        }

        // Phase 1: Nur Metadaten. Unveränderte Dateien kommen aus dem Cache, die anderen werden
        // für Phase 2 vorgemerkt.
        let mut neu = HashMap::with_capacity(tagesdateien.len());
        let mut zu_lesen = Vec::new();
        for (datum, (pfad, _)) in tagesdateien {
            let alt = self.dateien.remove(&pfad);
            let badge = match stand(&pfad) {
                Ok(s) => match alt {
                    Some((alt_stand, badge)) if alt_stand == s => {
                        neu.insert(pfad, (s, badge.clone()));
                        badge
                    }
                    alt => {
                        let vorlaeufig = alt.as_ref().map(|(_, b)| b.clone()).unwrap_or_default();
                        zu_lesen.push((datum, pfad, s, alt));
                        vorlaeufig
                    }
                },
                Err(e) => lesefehler(&mut index, &mut neu, pfad, alt, e),
            };
            index.tage.entry(datum).or_default().badge = Some(badge);
        }

        // Phase 2: Inhalte lesen, neueste Tage zuerst.
        let gesamt = zu_lesen.len();
        if gesamt > 0 {
            melden(Zwischenstand {
                index: &index,
                gelesen: 0,
                gesamt,
            });
        }
        let mut zuletzt_gemeldet = Instant::now();
        for (nr, (datum, pfad, s, alt)) in zu_lesen.into_iter().rev().enumerate() {
            let badge = match fs::read(&pfad) {
                Ok(bytes) => {
                    let b = badge(&storage::text_aus_bytes(&bytes), &self.regeln);
                    neu.insert(pfad, (s, b.clone()));
                    b
                }
                Err(e) => lesefehler(&mut index, &mut neu, pfad, alt, e),
            };
            index.tage.entry(datum).or_default().badge = Some(badge);
            if zuletzt_gemeldet.elapsed() >= MELDEN_ALLE && nr + 1 < gesamt {
                melden(Zwischenstand {
                    index: &index,
                    gelesen: nr + 1,
                    gesamt,
                });
                zuletzt_gemeldet = Instant::now();
            }
        }

        self.dateien = neu;
        Ok(index)
    }
}

/// Abstand der Zwischenstände von [`IndexCache::aufbauen_mit`].
pub const MELDEN_ALLE: Duration = Duration::from_millis(250);

/// Zwischenstand eines laufenden Index-Aufbaus.
#[derive(Debug, Clone, Copy)]
pub struct Zwischenstand<'a> {
    /// Alle Tage mit Datei; noch nicht gelesene haben ein vorläufiges Badge.
    pub index: &'a Index,
    /// Bereits gelesene Dateien.
    pub gelesen: usize,
    /// Insgesamt zu lesende Dateien.
    pub gesamt: usize,
}

/// Merkt den Fehler und liefert das zuletzt bekannte Badge (oder leer). Der Cache-Eintrag
/// bekommt einen ungültigen Stand, damit beim nächsten Mal neu gelesen wird.
fn lesefehler(
    index: &mut Index,
    neu: &mut HashMap<PathBuf, (Stand, String)>,
    pfad: PathBuf,
    alt: Option<(Stand, String)>,
    e: io::Error,
) -> String {
    let badge = alt.map(|(_, b)| b).unwrap_or_default();
    index.lesefehler.push((pfad.clone(), e.to_string()));
    let ungueltig = Stand {
        geaendert: None,
        groesse: u64::MAX,
    };
    neu.insert(pfad, (ungueltig, badge.clone()));
    badge
}

fn stand(pfad: &Path) -> io::Result<Stand> {
    let m = fs::metadata(pfad)?;
    Ok(Stand {
        geaendert: m.modified().ok(),
        groesse: m.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs::File;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    fn cache() -> IndexCache {
        IndexCache::new(Config::default().regeln)
    }

    fn badge_am(index: &Index, datum: NaiveDate) -> Option<&str> {
        index.tag(datum).and_then(|t| t.badge.as_deref())
    }

    #[test]
    fn t1_t2_badges() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-02.txt"), "Kreuzheben 3x5\r\n").unwrap();
        fs::write(dir.path().join("2026-11-03.txt"), "Notiz\r\n").unwrap();
        let index = cache().aufbauen(dir.path()).unwrap();
        assert_eq!(badge_am(&index, d(2026, 11, 2)), Some("DL"));
        assert_eq!(badge_am(&index, d(2026, 11, 3)), Some(""));
        assert_eq!(index.tag(d(2026, 11, 4)), None);
        assert!(index.lesefehler.is_empty());
    }

    #[test]
    fn t16_andere_dateien_ohne_badge_und_unveraendert() {
        let dir = tempfile::tempdir().unwrap();
        let dateien = [
            ("2026-11-04.md", "Kniebeugen"),
            ("2026-11-05.txt.bak", "Kniebeugen"),
            ("2026-11-06-Laptop.txt", "Kniebeugen"),
        ];
        for (n, inhalt) in dateien {
            fs::write(dir.path().join(n), inhalt).unwrap();
        }
        let index = cache().aufbauen(dir.path()).unwrap();
        for tag in 4..=6 {
            assert_eq!(badge_am(&index, d(2026, 11, tag)), None);
        }
        for (n, inhalt) in dateien {
            assert_eq!(fs::read_to_string(dir.path().join(n)).unwrap(), inhalt);
        }
        // Die OneDrive-Konfliktdatei wird als Warnung gemeldet (12.3).
        assert_eq!(
            index.tag(d(2026, 11, 6)).unwrap().konflikte,
            ["2026-11-06-Laptop.txt"]
        );
    }

    #[test]
    fn t17_t18_grossschreibung_und_ungueltiges_datum() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-07.TXT"), "Yoga").unwrap();
        fs::write(dir.path().join("2026-02-30.txt"), "Yoga").unwrap();
        let index = cache().aufbauen(dir.path()).unwrap();
        assert_eq!(badge_am(&index, d(2026, 11, 7)), Some("Yo"));
        assert_eq!(index.tage.len(), 1);
    }

    #[test]
    fn t34_konfliktkopie_zeigt_warnung_badge_nur_aus_tagesdatei() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-07.txt"), "Yoga").unwrap();
        fs::write(
            dir.path().join("2026-11-07.konflikt-20261107-101500.txt"),
            "Kniebeugen",
        )
        .unwrap();
        let index = cache().aufbauen(dir.path()).unwrap();
        let tag = index.tag(d(2026, 11, 7)).unwrap();
        assert_eq!(tag.badge.as_deref(), Some("Yo"));
        assert_eq!(tag.konflikte, ["2026-11-07.konflikt-20261107-101500.txt"]);
    }

    #[test]
    fn nur_konfliktkopie_ohne_tagesdatei() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-07 (1).txt"), "x").unwrap();
        let index = cache().aufbauen(dir.path()).unwrap();
        let tag = index.tag(d(2026, 11, 7)).unwrap();
        assert_eq!(tag.badge, None);
        assert_eq!(tag.konflikte.len(), 1);
    }

    #[test]
    fn ordner_werden_ignoriert_unterordner_nicht_durchsucht() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("2026-11-07.txt")).unwrap();
        fs::create_dir(dir.path().join("archiv")).unwrap();
        fs::write(dir.path().join("archiv").join("2026-11-08.txt"), "x").unwrap();
        let index = cache().aufbauen(dir.path()).unwrap();
        assert!(index.tage.is_empty());
    }

    #[test]
    fn t8_t9_aenderungen_beim_naechsten_aufbau() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = cache();
        assert!(c.aufbauen(dir.path()).unwrap().tage.is_empty());

        let pfad = dir.path().join("2026-11-10.txt");
        fs::write(&pfad, "Joggen").unwrap();
        assert_eq!(
            badge_am(&c.aufbauen(dir.path()).unwrap(), d(2026, 11, 10)),
            Some("Jo")
        );

        fs::write(&pfad, "Joggen und Yoga").unwrap();
        assert_eq!(
            badge_am(&c.aufbauen(dir.path()).unwrap(), d(2026, 11, 10)),
            Some("Jo Yo")
        );

        fs::remove_file(&pfad).unwrap();
        assert!(c.aufbauen(dir.path()).unwrap().tage.is_empty());
        assert!(c.dateien.is_empty());
    }

    #[test]
    fn unveraenderte_dateien_werden_nicht_neu_gelesen() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("2026-11-10.txt");
        fs::write(&pfad, "Joggen").unwrap();
        let mut c = cache();
        c.aufbauen(dir.path()).unwrap();

        // Inhalt ändern, aber Größe und Änderungszeit gleich lassen: Der Cache greift.
        let zeit = fs::metadata(&pfad).unwrap().modified().unwrap();
        fs::write(&pfad, "Yoga!!").unwrap();
        File::options()
            .write(true)
            .open(&pfad)
            .unwrap()
            .set_modified(zeit)
            .unwrap();
        assert_eq!(
            badge_am(&c.aufbauen(dir.path()).unwrap(), d(2026, 11, 10)),
            Some("Jo")
        );
    }

    #[test]
    fn neue_regeln_neuer_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-10.txt"), "Joggen").unwrap();
        let mut c = IndexCache::new(vec![Regel::new("Lauf", &["joggen"], &[])]);
        assert_eq!(
            badge_am(&c.aufbauen(dir.path()).unwrap(), d(2026, 11, 10)),
            Some("Lauf")
        );
    }

    #[test]
    fn zwischenstand_kommt_vor_dem_lesen_mit_allen_tagen() {
        let dir = tempfile::tempdir().unwrap();
        for (name, inhalt) in [
            ("2026-11-01.txt", "Joggen"),
            ("2026-11-02.txt", "Yoga"),
            ("2026-11-03.txt", "Kniebeugen"),
        ] {
            fs::write(dir.path().join(name), inhalt).unwrap();
        }
        let mut c = cache();
        let mut meldungen = Vec::new();
        let index = c
            .aufbauen_mit(dir.path(), |z| {
                meldungen.push((z.index.clone(), z.gelesen, z.gesamt))
            })
            .unwrap();

        let (erster, gelesen, gesamt) = &meldungen[0];
        assert_eq!((*gelesen, *gesamt), (0, 3));
        assert_eq!(erster.tage.len(), 3, "alle Tage sofort da");
        assert!(erster.tage.values().all(|t| t.badge.as_deref() == Some("")));
        assert_eq!(badge_am(&index, d(2026, 11, 1)), Some("Jo"));
        assert_eq!(badge_am(&index, d(2026, 11, 3)), Some("SQ"));

        // Zweiter Lauf: alles im Cache, nichts zu lesen, keine Meldung.
        let mut gemeldet = false;
        let index2 = c.aufbauen_mit(dir.path(), |_| gemeldet = true).unwrap();
        assert!(!gemeldet);
        assert_eq!(index2, index);
    }

    #[test]
    fn zwischenstand_zeigt_bis_zum_lesen_das_alte_badge() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("2026-11-01.txt");
        fs::write(&pfad, "Joggen").unwrap();
        let mut c = cache();
        c.aufbauen(dir.path()).unwrap();
        fs::write(&pfad, "Joggen und Yoga").unwrap();

        let mut vorlaeufig = None;
        let index = c
            .aufbauen_mit(dir.path(), |z| {
                vorlaeufig.get_or_insert(badge_am(z.index, d(2026, 11, 1)).map(str::to_owned));
            })
            .unwrap();
        assert_eq!(vorlaeufig, Some(Some("Jo".to_owned())));
        assert_eq!(badge_am(&index, d(2026, 11, 1)), Some("Jo Yo"));
    }

    #[test]
    fn fehlender_ordner_ist_fehler() {
        let dir = tempfile::tempdir().unwrap();
        assert!(cache().aufbauen(&dir.path().join("fehlt")).is_err());
    }
}
