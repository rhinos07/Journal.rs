//! Vorlage für neue Tage (Spezifikation F8).

use chrono::{Datelike, NaiveDate};

use crate::de;

/// Vorlage aus `config.toml` (`[template]`, Kap. 4.2).
///
/// `motto` und `uebung` sind nach Wochentag indiziert, Index 0 = Montag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub lines: Vec<String>,
    pub motto: [String; 7],
    pub uebung: [String; 7],
}

/// Erzeugt den Editortext für einen Tag ohne Datei.
///
/// Jede Zeile aus `lines` wird zu einer Zeile, getrennt durch `\n` (ohne `\n` am Ende).
/// Leere Werte ergeben leere Zeilen. Ist `lines` leer, ist das Ergebnis leer.
pub fn render(template: &Template, datum: NaiveDate) -> String {
    let tag = datum.weekday().num_days_from_monday() as usize;
    let wert = |name: &str| match name {
        "datum_lang" => Some(de::datum_lang(datum)),
        "motto" => Some(template.motto[tag].clone()),
        "uebung" => Some(template.uebung[tag].clone()),
        _ => None,
    };
    template
        .lines
        .iter()
        .map(|zeile| ersetze_platzhalter(zeile, wert))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ersetzt `{name}` in einem Durchgang. Unbekannte Platzhalter bleiben stehen, eingesetzte Werte
/// werden nicht erneut durchsucht.
fn ersetze_platzhalter(zeile: &str, wert: impl Fn(&str) -> Option<String>) -> String {
    let mut ergebnis = String::with_capacity(zeile.len());
    let mut rest = zeile;
    while let Some(start) = rest.find('{') {
        ergebnis.push_str(&rest[..start]);
        let ab_klammer = &rest[start..];
        let ersetzt = ab_klammer
            .find('}')
            .and_then(|ende| wert(&ab_klammer[1..ende]).map(|w| (w, ende)));
        match ersetzt {
            Some((w, ende)) => {
                ergebnis.push_str(&w);
                rest = &ab_klammer[ende + 1..];
            }
            None => {
                ergebnis.push('{');
                rest = &ab_klammer[1..];
            }
        }
    }
    ergebnis.push_str(rest);
    ergebnis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, Config};

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    fn standard() -> Template {
        Config::default().template
    }

    #[test]
    fn t3_freitag() {
        assert_eq!(
            render(&standard(), d(2026, 11, 6)),
            "Freitag, 6. November 2026\nRückschau halten\n5 Werte\nKniebeugen"
        );
    }

    #[test]
    fn leere_werte_ergeben_leere_zeilen() {
        // Montag: keine Übung
        assert_eq!(
            render(&standard(), d(2026, 11, 2)),
            "Montag, 2. November 2026\nDanke sagen\n5 Werte\n"
        );
        // Sonntag: kein Motto
        assert_eq!(
            render(&standard(), d(2026, 11, 8)),
            "Sonntag, 8. November 2026\n\n5 Werte\nKlimmzüge"
        );
    }

    #[test]
    fn leere_vorlage() {
        let mut t = standard();
        t.lines.clear();
        assert_eq!(render(&t, d(2026, 11, 6)), "");
    }

    #[test]
    fn t20_uebung_ueberschrieben() {
        let geparst = config::parse("[template.uebung]\nfreitag = \"Laufen\"\n").expect("gültig");
        let text = render(&geparst.config.template, d(2026, 11, 6));
        assert_eq!(text.lines().nth(3), Some("Laufen"));
    }

    #[test]
    fn t21_eigene_zeilen() {
        let geparst = config::parse(
            "[template]\nlines = [\"{datum_lang}\", \"Gewicht:\", \"Übung: {uebung}\"]\n",
        )
        .expect("gültig");
        assert_eq!(
            render(&geparst.config.template, d(2026, 11, 4)),
            "Mittwoch, 4. November 2026\nGewicht:\nÜbung: Handstand"
        );
    }

    #[test]
    fn platzhalter_sonderfaelle() {
        let w = |name: &str| (name == "a").then(|| "{a}".to_owned());
        assert_eq!(ersetze_platzhalter("x{a}y", w), "x{a}y");
        assert_eq!(ersetze_platzhalter("{a}{a}", w), "{a}{a}");
        assert_eq!(ersetze_platzhalter("{unbekannt} {a", w), "{unbekannt} {a");
        assert_eq!(ersetze_platzhalter("{{a}}", w), "{{a}}");
        assert_eq!(ersetze_platzhalter("ohne", w), "ohne");

        let ja = |name: &str| (name == "a").then(|| "1".to_owned());
        assert_eq!(ersetze_platzhalter("{x {a}}", ja), "{x 1}");
        assert_eq!(ersetze_platzhalter("Übung: {a}!", ja), "Übung: 1!");
    }
}
