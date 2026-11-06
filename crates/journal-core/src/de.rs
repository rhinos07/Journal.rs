//! Deutsche Namen für Wochentage und Monate (Spezifikation Kap. 12.2, „Sprache“).
//!
//! Bewusst eine feste Tabelle statt des Gebietsschemas des Systems, damit die App auf allen
//! Plattformen gleich aussieht.

use chrono::{Datelike, NaiveDate, Weekday};

/// Wochentage, beginnend mit Montag (erster Wochentag im Kalender).
pub const WOCHENTAGE: [&str; 7] = [
    "Montag",
    "Dienstag",
    "Mittwoch",
    "Donnerstag",
    "Freitag",
    "Samstag",
    "Sonntag",
];

/// Monate, Index 0 = Januar.
pub const MONATE: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];

/// Ausgeschriebener Wochentag, z. B. `Freitag`.
pub fn wochentag(tag: Weekday) -> &'static str {
    WOCHENTAGE[tag.num_days_from_monday() as usize]
}

/// Ausgeschriebener Monat für `monat` = 1 … 12.
///
/// # Panics
/// Bei einem Monat außerhalb von 1 … 12.
pub fn monat(monat: u32) -> &'static str {
    MONATE[(monat - 1) as usize]
}

/// Langes Datum, z. B. `Freitag, 6. November 2026` (Platzhalter `{datum_lang}`, F8).
pub fn datum_lang(datum: NaiveDate) -> String {
    format!(
        "{}, {}. {} {}",
        wochentag(datum.weekday()),
        datum.day(),
        monat(datum.month()),
        datum.year()
    )
}

/// Titel der Monatsansicht, z. B. `November 2026` (F6).
pub fn monat_jahr(datum: NaiveDate) -> String {
    format!("{} {}", monat(datum.month()), datum.year())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    #[test]
    fn langes_datum() {
        assert_eq!(datum_lang(d(2026, 11, 6)), "Freitag, 6. November 2026");
        assert_eq!(datum_lang(d(2026, 3, 1)), "Sonntag, 1. März 2026");
        assert_eq!(datum_lang(d(2024, 2, 29)), "Donnerstag, 29. Februar 2024");
    }

    #[test]
    fn titel_monat() {
        assert_eq!(monat_jahr(d(2026, 11, 6)), "November 2026");
    }
}
