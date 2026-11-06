//! Ordnerüberwachung (Spezifikation F5, 12.2).
//!
//! [`Ueberwachung::starten`] überwacht den Journal-Ordner (nicht rekursiv), entprellt die
//! Ereignisse und ruft für jeden Schwung einmal den Rückruf mit einer [`Aenderung`] auf. Der
//! Rückruf läuft in einem Hintergrund-Thread. Die Oberfläche schickt die Änderung z. B. über einen
//! Kanal an den UI-Thread und baut dort den Index neu auf bzw. prüft den offenen Tag
//! ([`crate::dokument::Dokument::pruefen`]).
//!
//! Ausgewertet wird nur, **welche Tage** betroffen sind, nicht die Art des Ereignisses. Die
//! Ereignisse unterscheiden sich je System zu stark (12.2). Ob sich etwas geändert hat, entscheidet
//! später der Vergleich des Inhalts.
//!
//! Beim Ordnerwechsel die alte [`Ueberwachung`] fallen lassen und eine neue starten. Ein bereits
//! entprellter Schwung kann danach noch ankommen; [`Aenderung::root`] zeigt, zu welchem Ordner er
//! gehört.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::NaiveDate;
use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::storage;

/// Wartezeit zum Entprellen (Empfehlung zu F5: 300–500 ms, T8: Badge nach ≤ 1 s).
pub const ENTPRELLEN: Duration = Duration::from_millis(300);

/// Ein Schwung Änderungen im Journal-Ordner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aenderung {
    /// Der überwachte Ordner.
    pub root: PathBuf,
    /// Tage, deren Tagesdatei oder Konfliktdateien sich geändert haben.
    pub tage: BTreeSet<NaiveDate>,
    /// Die Überwachung weiß nicht genau, was passiert ist (Fehler, verlorene Ereignisse, Ordner
    /// selbst geändert). Dann alles neu einlesen und den offenen Tag prüfen.
    pub unbekannt: bool,
}

impl Aenderung {
    /// Muss der offene Tag `datum` geprüft werden (F16 A)?
    pub fn betrifft(&self, datum: NaiveDate) -> bool {
        self.unbekannt || self.tage.contains(&datum)
    }
}

/// Laufende Überwachung. Fallen lassen beendet sie.
pub struct Ueberwachung {
    root: PathBuf,
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl std::fmt::Debug for Ueberwachung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ueberwachung")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Ueberwachung {
    /// Startet die Überwachung von `root`. `bei_aenderung` wird nur aufgerufen, wenn ein Schwung
    /// mindestens einen Tag betrifft oder unklar ist; Temp-Dateien und fremde Dateien lösen
    /// nichts aus.
    pub fn starten(
        root: &Path,
        bei_aenderung: impl Fn(Aenderung) + Send + 'static,
    ) -> notify_debouncer_full::notify::Result<Self> {
        let root = root.to_owned();
        let root_fuer_rueckruf = root.clone();
        let mut debouncer = new_debouncer(ENTPRELLEN, None, move |ergebnis| {
            let aenderung = auswerten(&root_fuer_rueckruf, ergebnis);
            if aenderung.unbekannt || !aenderung.tage.is_empty() {
                bei_aenderung(aenderung);
            }
        })?;
        debouncer.watch(&root, RecursiveMode::NonRecursive)?;
        Ok(Self {
            root,
            _debouncer: debouncer,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn auswerten(root: &Path, ergebnis: DebounceEventResult) -> Aenderung {
    let mut aenderung = Aenderung {
        root: root.to_owned(),
        ..Aenderung::default()
    };
    let ereignisse = match ergebnis {
        Ok(e) => e,
        Err(_) => {
            aenderung.unbekannt = true;
            return aenderung;
        }
    };
    for ereignis in ereignisse {
        if ereignis.need_rescan() {
            aenderung.unbekannt = true;
            continue;
        }
        // Lesen löst unter Linux Access-Ereignisse aus, auch das Einlesen des Index selbst.
        if matches!(ereignis.kind, EventKind::Access(_)) {
            continue;
        }
        for pfad in &ereignis.paths {
            let Some(name) = pfad.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some((datum, _)) = storage::klassifiziere(name) {
                aenderung.tage.insert(datum);
            } else if ist_root(root, pfad) {
                // Der Ordner selbst wurde verschoben, gelöscht o. Ä.
                aenderung.unbekannt = true;
            }
        }
    }
    aenderung
}

fn ist_root(root: &Path, pfad: &Path) -> bool {
    pfad == root
        || match (root.canonicalize(), pfad.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    /// Wartet bis zu 10 s (macOS meldet träge), bis `datum` gemeldet wurde.
    fn warte_auf(rx: &mpsc::Receiver<Aenderung>, datum: NaiveDate) -> Aenderung {
        let ende = Instant::now() + Duration::from_secs(10);
        while let Some(rest) = ende.checked_duration_since(Instant::now()) {
            if let Ok(a) = rx.recv_timeout(rest)
                && a.betrifft(datum)
            {
                return a;
            }
        }
        panic!("keine Änderung für {datum} gemeldet");
    }

    fn starten(root: &Path) -> (Ueberwachung, mpsc::Receiver<Aenderung>) {
        let (tx, rx) = mpsc::channel();
        let u = Ueberwachung::starten(root, move |a| {
            let _ = tx.send(a);
        })
        .unwrap();
        (u, rx)
    }

    #[test]
    fn t8_t9_anlegen_aendern_loeschen() {
        let dir = tempfile::tempdir().unwrap();
        let (u, rx) = starten(dir.path());
        assert_eq!(u.root(), dir.path());
        let pfad = dir.path().join("2026-11-10.txt");

        fs::write(&pfad, "Joggen").unwrap();
        let a = warte_auf(&rx, d(2026, 11, 10));
        assert_eq!(a.root, dir.path());

        fs::write(&pfad, "Joggen und Yoga").unwrap();
        warte_auf(&rx, d(2026, 11, 10));

        fs::remove_file(&pfad).unwrap();
        warte_auf(&rx, d(2026, 11, 10));
    }

    #[test]
    fn umbenennen_aus_temp_datei() {
        let dir = tempfile::tempdir().unwrap();
        let (_u, rx) = starten(dir.path());
        // So schreiben Sync-Clients (und storage::atomar_schreiben).
        storage::atomar_schreiben(&dir.path().join("2026-11-11.txt"), b"x").unwrap();
        warte_auf(&rx, d(2026, 11, 11));
    }

    #[test]
    fn konfliktkopie_wird_gemeldet() {
        let dir = tempfile::tempdir().unwrap();
        let (_u, rx) = starten(dir.path());
        fs::write(
            dir.path().join("2026-11-12.konflikt-20261112-101500.txt"),
            "x",
        )
        .unwrap();
        warte_auf(&rx, d(2026, 11, 12));
    }

    #[test]
    fn fremde_dateien_loesen_nichts_aus() {
        let dir = tempfile::tempdir().unwrap();
        let (_u, rx) = starten(dir.path());
        fs::write(dir.path().join("notiz.md"), "x").unwrap();
        fs::write(dir.path().join(".2026-11-13.txt.1.tmp"), "x").unwrap();
        // Danach eine echte Änderung als Markierung. Bis dahin darf nur sie gemeldet werden.
        fs::write(dir.path().join("2026-11-14.txt"), "x").unwrap();
        let ende = Instant::now() + Duration::from_secs(10);
        loop {
            let rest = ende
                .checked_duration_since(Instant::now())
                .expect("Markierung nicht gemeldet");
            let Ok(a) = rx.recv_timeout(rest) else {
                continue;
            };
            assert!(!a.unbekannt, "{a:?}");
            assert_eq!(a.tage, BTreeSet::from([d(2026, 11, 14)]), "{a:?}");
            break;
        }
    }

    #[test]
    fn auswerten_ohne_dateisystem() {
        use notify_debouncer_full::DebouncedEvent;
        use notify_debouncer_full::notify::Event;
        use notify_debouncer_full::notify::event::{AccessKind, CreateKind};

        let root = Path::new("/journal");
        let ereignis = |kind, name: &str| {
            DebouncedEvent::new(
                Event::new(kind).add_path(root.join(name)),
                std::time::Instant::now(),
            )
        };
        let a = auswerten(
            root,
            Ok(vec![
                ereignis(EventKind::Create(CreateKind::File), "2026-11-07.txt"),
                ereignis(EventKind::Create(CreateKind::File), "2026-11-08 (1).txt"),
                ereignis(EventKind::Access(AccessKind::Any), "2026-11-09.txt"),
                ereignis(EventKind::Create(CreateKind::File), "notiz.txt"),
            ]),
        );
        assert_eq!(a.tage, BTreeSet::from([d(2026, 11, 7), d(2026, 11, 8)]));
        assert!(!a.unbekannt);
        assert!(a.betrifft(d(2026, 11, 7)));
        assert!(!a.betrifft(d(2026, 11, 9)));

        assert!(auswerten(root, Err(vec![])).unbekannt);
    }

    #[test]
    fn fallen_lassen_beendet_die_ueberwachung() {
        let dir = tempfile::tempdir().unwrap();
        let (u, rx) = starten(dir.path());
        drop(u);
        fs::write(dir.path().join("2026-11-15.txt"), "x").unwrap();
        std::thread::sleep(ENTPRELLEN * 4);
        assert!(rx.try_iter().all(|a| !a.betrifft(d(2026, 11, 15))));
    }
}
