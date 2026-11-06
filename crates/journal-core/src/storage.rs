//! Laden, Speichern und Löschen der Tagesdateien (Spezifikation Kap. 3, F7, F10, F12).
//!
//! - Dateinamen: [`dateiname`], [`klassifiziere`] (3.1, E2, F16 E, 12.3)
//! - Vorhandene Datei eines Tages in ihrer Schreibweise finden: [`tagesdatei_finden`]
//! - Inhalt: rohe Bytes (= Plattenstand, F16) und Editortext mit `\n` ([`text_aus_bytes`],
//!   [`bytes_aus_text`])
//! - Atomar schreiben: [`schreiben`], Löschen: [`in_papierkorb`], [`endgueltig_loeschen`]

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

/// Dateiname für einen neuen Tag, z. B. `2026-11-07.txt`.
pub fn dateiname(datum: NaiveDate) -> String {
    format!("{}.txt", datum.format("%Y-%m-%d"))
}

/// Was eine Datei im Journal-Ordner ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Genau `YYYY-MM-DD.txt` (Endung in beliebiger Schreibweise). `endung_klein` ist `true`
    /// bei `.txt`, das entscheidet auf Dateisystemen mit Groß-/Kleinschreibung (F3, Punkt 4).
    Tagesdatei { endung_klein: bool },
    /// Eigene Konfliktkopie `YYYY-MM-DD.konflikt-….txt` (F16 E).
    Konfliktkopie,
    /// Fängt mit `YYYY-MM-DD` an und endet auf `.txt`, ist aber keines von beiden oben, z. B.
    /// eine Konfliktdatei eines Sync-Dienstes (3.1, 12.3).
    FremderKonflikt,
}

/// Ordnet einen Dateinamen ein. `None` heißt: gehört nicht zum Journal, wird ignoriert.
pub fn klassifiziere(name: &str) -> Option<(NaiveDate, Art)> {
    let datum = datum_am_anfang(name)?;
    let rest = &name[10..];
    let klein = rest.to_ascii_lowercase();
    if !klein.ends_with(".txt") {
        return None;
    }
    let art = if klein == ".txt" {
        Art::Tagesdatei {
            endung_klein: rest == ".txt",
        }
    } else if klein.starts_with(".konflikt-") {
        Art::Konfliktkopie
    } else {
        Art::FremderKonflikt
    };
    Some((datum, art))
}

/// Das Datum eines Dateinamens, der genau `YYYY-MM-DD.txt` ist (Kap. 3.1), sonst `None`.
pub fn datum_aus_name(name: &str) -> Option<NaiveDate> {
    match klassifiziere(name)? {
        (datum, Art::Tagesdatei { .. }) => Some(datum),
        _ => None,
    }
}

/// `YYYY-MM-DD` am Anfang, nur ASCII-Ziffern, gültiges Kalenderdatum.
fn datum_am_anfang(name: &str) -> Option<NaiveDate> {
    let b = name.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let ziffern = |r: std::ops::Range<usize>| -> Option<u32> {
        b[r].iter().try_fold(0u32, |n, &c| {
            c.is_ascii_digit().then(|| n * 10 + u32::from(c - b'0'))
        })
    };
    let jahr = ziffern(0..4)?;
    let monat = ziffern(5..7)?;
    let tag = ziffern(8..10)?;
    NaiveDate::from_ymd_opt(jahr as i32, monat, tag)
}

/// Sucht die vorhandene Datei des Tages in ihrer tatsächlichen Schreibweise (Kap. 3.1).
///
/// Gibt es (auf Dateisystemen mit Groß-/Kleinschreibung) mehrere, gewinnt `.txt` (F3, Punkt 4).
/// Nur normale Dateien zählen.
pub fn tagesdatei_finden(root: &Path, datum: NaiveDate) -> io::Result<Option<PathBuf>> {
    let mut gefunden: Option<(PathBuf, bool)> = None;
    for eintrag in fs::read_dir(root)? {
        let eintrag = eintrag?;
        let name = eintrag.file_name();
        let Some((d, Art::Tagesdatei { endung_klein })) = name.to_str().and_then(klassifiziere)
        else {
            continue;
        };
        if d != datum || !ist_datei(&eintrag) {
            continue;
        }
        if gefunden
            .as_ref()
            .is_none_or(|(_, klein)| !klein && endung_klein)
        {
            gefunden = Some((eintrag.path(), endung_klein));
        }
    }
    Ok(gefunden.map(|(p, _)| p))
}

pub(crate) fn ist_datei(eintrag: &fs::DirEntry) -> bool {
    match eintrag.file_type() {
        Ok(t) if t.is_symlink() => fs::metadata(eintrag.path()).is_ok_and(|m| m.is_file()),
        Ok(t) => t.is_file(),
        Err(_) => false,
    }
}

/// Liest die Datei des Tages. `Ok(None)`, wenn es keine gibt.
pub fn laden(root: &Path, datum: NaiveDate) -> io::Result<Option<(PathBuf, Vec<u8>)>> {
    match tagesdatei_finden(root, datum)? {
        Some(pfad) => {
            let bytes = fs::read(&pfad)?;
            Ok(Some((pfad, bytes)))
        }
        None => Ok(None),
    }
}

/// Editortext aus dem Dateiinhalt: BOM entfernen, CRLF zu LF (Kap. 3.2).
///
/// Ungültiges UTF-8 wird durch `�` ersetzt. Solange der Text nicht geändert wird, schreibt die
/// App die Datei nicht, sie bleibt also unverändert.
pub fn text_aus_bytes(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// Dateiinhalt aus dem Editortext: UTF-8 ohne BOM, jede Zeile endet mit CRLF, auch die letzte
/// (Kap. 3.2). Ein leerer Text ergibt eine leere Datei.
pub fn bytes_aus_text(text: &str) -> Vec<u8> {
    let mut s = text.replace("\r\n", "\n").replace('\n', "\r\n");
    if !s.is_empty() && !s.ends_with("\r\n") {
        s.push_str("\r\n");
    }
    s.into_bytes()
}

/// Schreibt den Text des Tages atomar (F10, Empfehlung) und gibt den Pfad zurück.
///
/// Eine vorhandene Datei wird in ihrer Schreibweise überschrieben (T18a), sonst entsteht
/// `YYYY-MM-DD.txt`.
pub fn schreiben(root: &Path, datum: NaiveDate, text: &str) -> io::Result<PathBuf> {
    let ziel = tagesdatei_finden(root, datum)?.unwrap_or_else(|| root.join(dateiname(datum)));
    atomar_schreiben(&ziel, &bytes_aus_text(text))?;
    Ok(ziel)
}

/// Schreibt über eine Temp-Datei im selben Ordner und benennt sie dann um, damit ein
/// Sync-Client nie eine halb geschriebene Datei sieht.
///
/// Die Temp-Datei beginnt mit `.` und endet auf `.tmp`, passt also zu keinem Muster aus
/// [`klassifiziere`]. Eine schreibgeschützte Zieldatei ergibt auf allen Systemen
/// `PermissionDenied` (unter Unix würde das Umbenennen sie sonst einfach ersetzen).
pub fn atomar_schreiben(ziel: &Path, inhalt: &[u8]) -> io::Result<()> {
    if let Ok(m) = fs::metadata(ziel)
        && m.permissions().readonly()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} ist schreibgeschützt", ziel.display()),
        ));
    }
    let ordner = ziel.parent().unwrap_or(Path::new("."));
    let name = ziel.file_name().unwrap_or_default().to_string_lossy();
    let temp = ordner.join(format!(".{name}.{}.tmp", std::process::id()));

    let ergebnis = (|| {
        let mut f = File::create(&temp)?;
        f.write_all(inhalt)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, ziel)
    })();
    if ergebnis.is_err() {
        let _ = fs::remove_file(&temp);
    }
    ergebnis
}

/// Fehler beim Verschieben in den Papierkorb (F12).
#[derive(Debug)]
pub enum LoeschFehler {
    /// Für diesen Ordner gibt es keinen Papierkorb (Netzlaufwerk o. Ä.). Die App fragt dann,
    /// ob endgültig gelöscht werden soll (F12, Schritt 5).
    PapierkorbNichtVerfuegbar(String),
    /// Datei gesperrt, keine Rechte o. Ä. (F12, Schritt 6).
    Sonstiger(io::Error),
}

impl fmt::Display for LoeschFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PapierkorbNichtVerfuegbar(grund) => {
                write!(f, "Der Papierkorb ist nicht verfügbar: {grund}")
            }
            Self::Sonstiger(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoeschFehler {}

/// Verschiebt eine Datei in den Papierkorb des Betriebssystems (F12, E7).
pub fn in_papierkorb(pfad: &Path) -> Result<(), LoeschFehler> {
    // Vorher prüfen, damit „gesperrt / keine Rechte / fehlt“ nicht als „kein Papierkorb“ gilt.
    fs::OpenOptions::new()
        .append(true)
        .open(pfad)
        .map_err(LoeschFehler::Sonstiger)?;
    if ist_netzlaufwerk(pfad) {
        return Err(LoeschFehler::PapierkorbNichtVerfuegbar(
            "Netzlaufwerk".to_owned(),
        ));
    }

    #[allow(unused_mut)]
    let mut kontext = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        // Ohne Finder-Fernsteuerung, die beim ersten Mal eine Berechtigung erfragt.
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        kontext.set_delete_method(DeleteMethod::NsFileManager);
    }
    kontext
        .delete(pfad)
        .map_err(|e| LoeschFehler::PapierkorbNichtVerfuegbar(e.to_string()))
}

/// Löscht eine Datei endgültig (F12, Schritt 5 nach Rückfrage).
pub fn endgueltig_loeschen(pfad: &Path) -> io::Result<()> {
    fs::remove_file(pfad)
}

#[cfg(windows)]
fn ist_netzlaufwerk(pfad: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};

    let absolut = std::path::absolute(pfad).unwrap_or_else(|_| pfad.to_owned());
    let laufwerk = match absolut.components().next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => return true,
            Prefix::Disk(b) | Prefix::VerbatimDisk(b) => b,
            _ => return false,
        },
        _ => return false,
    };
    let wurzel: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", laufwerk as char))
        .encode_wide()
        .chain(Some(0))
        .collect();
    const DRIVE_REMOTE: u32 = 4;
    // SAFETY: `wurzel` ist ein nullterminierter UTF-16-String, der während des Aufrufs lebt.
    unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wurzel.as_ptr()) == DRIVE_REMOTE
    }
}

#[cfg(not(windows))]
fn ist_netzlaufwerk(_pfad: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    fn namen(root: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    /// `true`, wenn das Dateisystem Groß-/Kleinschreibung unterscheidet.
    fn unterscheidet_schreibweise(root: &Path) -> bool {
        let probe = root.join("probe.TMP");
        fs::write(&probe, "").unwrap();
        let ergebnis = !root.join("probe.tmp").exists();
        fs::remove_file(probe).unwrap();
        ergebnis
    }

    #[test]
    fn dateinamen_aus_3_1() {
        assert_eq!(dateiname(d(2024, 1, 5)), "2024-01-05.txt");
        assert_eq!(datum_aus_name("2024-01-05.txt"), Some(d(2024, 1, 5)));
        assert_eq!(datum_aus_name("2024-01-05.TXT"), Some(d(2024, 1, 5)));
        assert_eq!(datum_aus_name("2024-01-05.Txt"), Some(d(2024, 1, 5)));
        for nein in [
            "2024-01-05.md",
            "2024-01-05.txt.bak",
            "2024-01-05-Laptop.txt",
            "2024-1-5.txt",
            "2024-02-30.txt",
            "x2024-01-05.txt",
            "2024-01-05.txtx",
            "2024-01-05",
            "２０２４-01-05.txt",
            "2024_01_05.txt",
            "",
        ] {
            assert_eq!(datum_aus_name(nein), None, "{nein}");
        }
    }

    #[test]
    fn klassifizierung() {
        let k = |n: &str| klassifiziere(n).map(|(_, a)| a);
        assert_eq!(
            k("2026-11-07.txt"),
            Some(Art::Tagesdatei { endung_klein: true })
        );
        assert_eq!(
            k("2026-11-07.TXT"),
            Some(Art::Tagesdatei {
                endung_klein: false
            })
        );
        assert_eq!(
            k("2026-11-07.konflikt-20261107-101500.txt"),
            Some(Art::Konfliktkopie)
        );
        assert_eq!(
            k("2026-11-07.Konflikt-20261107-101500-2.TXT"),
            Some(Art::Konfliktkopie)
        );
        for fremd in [
            "2026-11-07-Laptop.txt",
            "2026-11-07 (1).txt",
            "2026-11-07 (Konfliktkopie von Laptop).txt",
            "2026-11-07 (conflicted copy 2026-11-07 101500).txt",
            "2026-11-07.sync-conflict-20261107-101500-ABCDEFG.txt",
        ] {
            assert_eq!(k(fremd), Some(Art::FremderKonflikt), "{fremd}");
        }
        for egal in [
            "2026-11-07.md",
            "2026-11-07.txt.bak",
            ".2026-11-07.txt.123.tmp",
            "notiz.txt",
        ] {
            assert_eq!(k(egal), None, "{egal}");
        }
        assert_eq!(
            klassifiziere("2026-11-07 (1).txt").unwrap().0,
            d(2026, 11, 7)
        );
    }

    #[test]
    fn text_und_bytes() {
        assert_eq!(text_aus_bytes(b"a\r\nb\r\n"), "a\nb\n");
        assert_eq!(text_aus_bytes(b"a\nb"), "a\nb");
        assert_eq!(text_aus_bytes(b"\xEF\xBB\xBFa"), "a");
        assert_eq!(bytes_aus_text("a\nb"), b"a\r\nb\r\n");
        assert_eq!(bytes_aus_text("a\nb\n"), b"a\r\nb\r\n");
        assert_eq!(bytes_aus_text("a\r\nb"), b"a\r\nb\r\n");
        assert_eq!(bytes_aus_text("a\n\n"), b"a\r\n\r\n");
        assert_eq!(bytes_aus_text(""), b"");
    }

    #[test]
    fn t12_datei_aus_csharp_bleibt_byte_gleich() {
        let original = "Freitag, 6. November 2026\r\nRückschau halten\r\nÄpfel, Öl, Übung, Maß\r\n";
        let bytes = original.as_bytes();
        assert_eq!(bytes_aus_text(&text_aus_bytes(bytes)), bytes);
    }

    #[test]
    fn neuer_tag_wird_klein_angelegt() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = schreiben(dir.path(), d(2026, 11, 6), "Kniebeugen").unwrap();
        assert_eq!(pfad, dir.path().join("2026-11-06.txt"));
        assert_eq!(fs::read(&pfad).unwrap(), b"Kniebeugen\r\n");
        assert_eq!(
            namen(dir.path()),
            ["2026-11-06.txt"],
            "keine Temp-Datei übrig"
        );
    }

    #[test]
    fn t18a_vorhandene_grossschreibung_bleibt() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-07.TXT"), "alt\r\n").unwrap();

        let gefunden = tagesdatei_finden(dir.path(), d(2026, 11, 7))
            .unwrap()
            .unwrap();
        assert_eq!(gefunden.file_name().unwrap(), "2026-11-07.TXT");

        schreiben(dir.path(), d(2026, 11, 7), "neu").unwrap();
        assert_eq!(namen(dir.path()), ["2026-11-07.TXT"]);
        assert_eq!(
            fs::read(dir.path().join("2026-11-07.TXT")).unwrap(),
            b"neu\r\n"
        );
    }

    #[test]
    fn bei_zwei_schreibweisen_gewinnt_txt() {
        let dir = tempfile::tempdir().unwrap();
        if !unterscheidet_schreibweise(dir.path()) {
            return; // Nur unter Linux (bzw. mit case-sensitivem Dateisystem) möglich.
        }
        fs::write(dir.path().join("2026-11-07.TXT"), "gross").unwrap();
        fs::write(dir.path().join("2026-11-07.txt"), "klein").unwrap();
        fs::write(dir.path().join("2026-11-07.Txt"), "gemischt").unwrap();
        let (pfad, bytes) = laden(dir.path(), d(2026, 11, 7)).unwrap().unwrap();
        assert_eq!(pfad.file_name().unwrap(), "2026-11-07.txt");
        assert_eq!(bytes, b"klein");
    }

    #[test]
    fn andere_dateien_und_ordner_zaehlen_nicht() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-11-07.md"), "x").unwrap();
        fs::write(dir.path().join("2026-11-07-Laptop.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("2026-11-08.txt")).unwrap();
        assert_eq!(laden(dir.path(), d(2026, 11, 7)).unwrap(), None);
        assert_eq!(laden(dir.path(), d(2026, 11, 8)).unwrap(), None);
    }

    #[test]
    fn schreibgeschuetzt_ist_fehler() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("2026-11-07.txt");
        fs::write(&pfad, "alt\r\n").unwrap();
        let mut rechte = fs::metadata(&pfad).unwrap().permissions();
        rechte.set_readonly(true);
        fs::set_permissions(&pfad, rechte.clone()).unwrap();

        let e = schreiben(dir.path(), d(2026, 11, 7), "neu").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&pfad).unwrap(), b"alt\r\n");
        assert_eq!(namen(dir.path()), ["2026-11-07.txt"]);

        #[allow(clippy::permissions_set_readonly_false)]
        rechte.set_readonly(false);
        fs::set_permissions(&pfad, rechte).unwrap();
    }

    #[test]
    fn endgueltig() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("2026-11-07.txt");
        fs::write(&pfad, "x").unwrap();
        endgueltig_loeschen(&pfad).unwrap();
        assert!(!pfad.exists());
    }

    #[test]
    fn papierkorb_fehlende_datei_ist_sonstiger_fehler() {
        let dir = tempfile::tempdir().unwrap();
        let e = in_papierkorb(&dir.path().join("fehlt.txt")).unwrap_err();
        assert!(matches!(e, LoeschFehler::Sonstiger(_)), "{e:?}");
    }

    /// Legt wirklich eine Datei in den Papierkorb des Benutzers, deshalb nur auf Wunsch:
    /// `cargo test -- --ignored papierkorb_echt`
    #[test]
    #[ignore]
    fn papierkorb_echt() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("2000-01-01.txt");
        fs::write(&pfad, "Test aus journal-core, kann gelöscht werden").unwrap();
        in_papierkorb(&pfad).unwrap();
        assert!(!pfad.exists());
    }

    #[cfg(windows)]
    #[test]
    fn unc_pfad_ist_netzlaufwerk() {
        assert!(ist_netzlaufwerk(Path::new(
            r"\\server\freigabe\2026-11-07.txt"
        )));
        assert!(!ist_netzlaufwerk(&std::env::temp_dir()));
    }
}
