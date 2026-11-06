//! Kürzel-Erkennung (Spezifikation F4).

/// Eine Kürzel-Regel aus `config.toml` (`[[regel]]`, Kap. 4.2).
///
/// Die Regel trifft, wenn der Text mindestens eines der `woerter` enthält und keines der
/// `ausser`. Verglichen wird als Teilstring, Groß-/Kleinschreibung ist egal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regel {
    pub kuerzel: String,
    pub woerter: Vec<String>,
    pub ausser: Vec<String>,
}

impl Regel {
    /// Bequemer Konstruktor, vor allem für Tests.
    pub fn new(kuerzel: &str, woerter: &[&str], ausser: &[&str]) -> Self {
        Self {
            kuerzel: kuerzel.to_owned(),
            woerter: woerter.iter().map(|w| (*w).to_owned()).collect(),
            ausser: ausser.iter().map(|w| (*w).to_owned()).collect(),
        }
    }
}

/// Berechnet das Badge für einen Dateiinhalt.
///
/// Ergebnis: die Kürzel der treffenden Regeln in Regel-Reihenfolge, ohne Doppelte, getrennt durch
/// genau ein Leerzeichen. Trifft keine Regel, ist das Ergebnis leer.
///
/// Leere Suchwörter werden ignoriert, Regeln mit leerem Kürzel ebenfalls. Die Konfiguration
/// filtert beides schon beim Laden und meldet es, hier ist es nur eine Absicherung.
pub fn badge(text: &str, regeln: &[Regel]) -> String {
    let text = text.to_lowercase();
    let enthaelt = |wort: &String| {
        let wort = wort.to_lowercase();
        !wort.is_empty() && text.contains(&wort)
    };

    let mut kuerzel: Vec<&str> = Vec::new();
    for regel in regeln {
        if regel.kuerzel.is_empty() || kuerzel.contains(&regel.kuerzel.as_str()) {
            continue;
        }
        if regel.woerter.iter().any(enthaelt) && !regel.ausser.iter().any(enthaelt) {
            kuerzel.push(&regel.kuerzel);
        }
    }
    kuerzel.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn standard(text: &str) -> String {
        badge(text, &Config::default().regeln)
    }

    #[test]
    fn beispiele_aus_f4() {
        assert_eq!(standard("Kniebeugen 5x5, Squat-Rack"), "SQ");
        assert_eq!(standard("Overhead Press 3x5"), "OHP");
        assert_eq!(standard("Bauch, Beinheben"), "B");
        assert_eq!(standard("Kreuzheben, Kniebeugen"), "SQ DL");
        assert_eq!(standard("Bankdrücken, Rücken"), "BP");
        assert_eq!(standard("Brücken"), "R");
        assert_eq!(standard("Klimmzüge"), "Kz");
        assert_eq!(standard("Dips"), "Di");
        assert_eq!(standard("Kaltwasser-Dipping"), "Di");
    }

    #[test]
    fn t1_kreuzheben() {
        assert_eq!(standard("Kreuzheben 3x5"), "DL");
    }

    #[test]
    fn t2_ohne_treffer_leer() {
        assert_eq!(standard("Notiz"), "");
        assert_eq!(standard(""), "");
    }

    #[test]
    fn t11_einmal_sq() {
        assert_eq!(standard("Kniebeuge … Squat"), "SQ");
    }

    #[test]
    fn t11a_overhead_und_bench() {
        assert_eq!(standard("Overhead Press, Bench Press"), "BP OHP");
    }

    #[test]
    fn t11b_bankdruecken_ruecken() {
        assert_eq!(standard("Bankdrücken, Rücken"), "BP");
    }

    #[test]
    fn t11c_klimmzuege() {
        assert_eq!(standard("Klimmzüge 3x8"), "Kz");
    }

    #[test]
    fn t11g_schreibweise_der_regel_egal() {
        let regeln = [Regel::new("SQ", &["KNIEBEUGE"], &[])];
        assert_eq!(badge("kniebeugen", &regeln), "SQ");
    }

    #[test]
    fn press_mit_leerzeichen() {
        assert_eq!(standard("Press 5x5"), "OHP");
        // Ohne folgendes Leerzeichen trifft "press " nicht.
        assert_eq!(standard("Press"), "");
    }

    #[test]
    fn umlaute_gross_geschrieben() {
        assert_eq!(standard("LIEGESTÜTZE"), "Li");
    }

    #[test]
    fn leere_woerter_und_kuerzel_werden_ignoriert() {
        let regeln = [
            Regel::new("X", &[""], &[]),
            Regel::new("", &["notiz"], &[]),
            Regel::new("Y", &["notiz"], &[""]),
        ];
        assert_eq!(badge("Notiz", &regeln), "Y");
    }

    #[test]
    fn keine_regeln_leeres_badge() {
        assert_eq!(badge("Kniebeugen", &[]), "");
    }
}
