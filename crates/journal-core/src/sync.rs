//! Konflikte mit externen Änderungen (Spezifikation F16).
//!
//! Hier liegen der Konfliktzustand für die Hinweisleiste und das Schreiben von Konfliktkopien.
//! Die Abläufe (wann geprüft, gesichert und geladen wird) stehen in [`crate::dokument`].

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};

/// Konflikt am offenen Tag, solange die Hinweisleiste sichtbar ist (F16 C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Konflikt {
    /// Die Datei wurde außerhalb der App geändert oder angelegt.
    Geaendert,
    /// Die Datei wurde außerhalb der App gelöscht.
    Geloescht,
}

impl Konflikt {
    /// Text der gelben Hinweisleiste.
    pub fn meldung(self) -> &'static str {
        match self {
            Self::Geaendert => "Dieser Eintrag wurde außerhalb der App geändert.",
            Self::Geloescht => "Dieser Eintrag wurde außerhalb der App gelöscht.",
        }
    }

    /// Beschriftung des Buttons, der die externe Version übernimmt
    /// ([`crate::dokument::Dokument::externe_laden`]).
    pub fn button_extern(self) -> &'static str {
        match self {
            Self::Geaendert => "Externe Version laden",
            Self::Geloescht => "Löschen übernehmen",
        }
    }

    /// Beschriftung des Buttons, der die eigene Version speichert
    /// ([`crate::dokument::Dokument::speichern`]).
    pub fn button_eigen(self) -> &'static str {
        "Meine Version speichern"
    }
}

/// Name einer Konfliktkopie: `YYYY-MM-DD.konflikt-YYYYMMDD-HHMMSS.txt`, ab `nr` 2 mit `-nr`
/// vor der Endung (F16 E).
pub fn konfliktkopie_name(datum: NaiveDate, zeit: NaiveDateTime, nr: u32) -> String {
    let zusatz = if nr > 1 {
        format!("-{nr}")
    } else {
        String::new()
    };
    format!(
        "{}.konflikt-{}{zusatz}.txt",
        datum.format("%Y-%m-%d"),
        zeit.format("%Y%m%d-%H%M%S")
    )
}

/// Schreibt `inhalt` unverändert als neue Konfliktkopie in `root` und gibt den Dateinamen zurück.
/// Eine vorhandene Datei wird nie überschrieben.
pub fn konfliktkopie_schreiben(
    root: &Path,
    datum: NaiveDate,
    zeit: NaiveDateTime,
    inhalt: &[u8],
) -> io::Result<String> {
    for nr in 1..=999 {
        let name = konfliktkopie_name(datum, zeit, nr);
        let pfad = root.join(&name);
        match OpenOptions::new().write(true).create_new(true).open(&pfad) {
            Ok(mut f) => {
                let ergebnis = f.write_all(inhalt).and_then(|()| f.sync_all());
                if let Err(e) = ergebnis {
                    drop(f);
                    let _ = std::fs::remove_file(&pfad);
                    return Err(e);
                }
                return Ok(name);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "zu viele Konfliktkopien mit demselben Zeitstempel",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Art, klassifiziere};

    fn zeit() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 7)
            .unwrap()
            .and_hms_opt(10, 15, 0)
            .unwrap()
    }

    #[test]
    fn name() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        assert_eq!(
            konfliktkopie_name(d, zeit(), 1),
            "2026-11-07.konflikt-20261107-101500.txt"
        );
        assert_eq!(
            konfliktkopie_name(d, zeit(), 2),
            "2026-11-07.konflikt-20261107-101500-2.txt"
        );
        // Konfliktkopien gelten nie als Tagesdatei (E2).
        let (datum, art) = klassifiziere(&konfliktkopie_name(d, zeit(), 3)).unwrap();
        assert_eq!((datum, art), (d, Art::Konfliktkopie));
    }

    #[test]
    fn schreiben_haengt_nummer_an() {
        let dir = tempfile::tempdir().unwrap();
        let d = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        let a = konfliktkopie_schreiben(dir.path(), d, zeit(), b"eins").unwrap();
        let b = konfliktkopie_schreiben(dir.path(), d, zeit(), b"zwei").unwrap();
        assert_eq!(a, "2026-11-07.konflikt-20261107-101500.txt");
        assert_eq!(b, "2026-11-07.konflikt-20261107-101500-2.txt");
        assert_eq!(std::fs::read(dir.path().join(a)).unwrap(), b"eins");
        assert_eq!(std::fs::read(dir.path().join(b)).unwrap(), b"zwei");
    }
}
