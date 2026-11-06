//! Konfigurationsdatei `config.toml` (Spezifikation Kap. 4, Pfade Kap. 12.1).
//!
//! - [`parse`] liest einen TOML-Text und füllt fehlende Schlüssel mit den Standardwerten.
//! - [`laden`] liest die Datei vom Platz und legt sie bei Bedarf mit [`STANDARD_CONFIG`] an.
//! - [`Geladen::root_path_speichern`] und [`Geladen::design_speichern`] ändern gezielt nur ihren
//!   Schlüssel, Kommentare und Formatierung bleiben erhalten.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::badge::Regel;
use crate::template::Template;

/// Inhalt der Datei, die beim ersten Start angelegt wird (Kap. 4.2).
pub const STANDARD_CONFIG: &str = r#"# Journal-Ordner (wird von der App beim Ordnerwechsel gesetzt)
root_path = ""

# Design: "system" (wie das Betriebssystem), "hell" oder "dunkel".
# Lässt sich auch unten rechts in der App umschalten.
design = "system"

# Keine Kürzel gewünscht? Dann alle [[regel]]-Blöcke unten löschen und hier,
# also VOR der Zeile [template], diese Zeile eintragen:
# regel = []

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
"#;

/// Die wirksamen Einstellungen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Journal-Ordner. `None`, wenn leer. Ob er existiert, prüft [`Config::gueltiger_root`].
    pub root_path: Option<PathBuf>,
    pub design: Design,
    pub template: Template,
    /// Gültige Kürzel-Regeln in Datei-Reihenfolge.
    pub regeln: Vec<Regel>,
}

impl Default for Config {
    fn default() -> Self {
        aus_roh(standard_roh(), &mut Vec::new())
    }
}

impl Config {
    /// Der Journal-Ordner, sofern gesetzt und als Verzeichnis vorhanden (Kap. 4.3).
    pub fn gueltiger_root(&self) -> Option<&Path> {
        self.root_path.as_deref().filter(|p| p.is_dir())
    }
}

/// Ergebnis von [`parse`]: Einstellungen und Hinweise zu übersprungenen Regeln.
#[derive(Debug, Clone)]
pub struct Geparst {
    pub config: Config,
    pub warnungen: Vec<String>,
}

/// Die Datei ist kein gültiges TOML oder hat falsche Typen.
/// Helles oder dunkles Erscheinungsbild der App.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Design {
    /// Wie das Betriebssystem.
    #[default]
    System,
    Hell,
    Dunkel,
}

impl Design {
    pub const ALLE: [Design; 3] = [Design::System, Design::Hell, Design::Dunkel];

    /// Wert in `config.toml`.
    pub fn name(self) -> &'static str {
        match self {
            Design::System => "system",
            Design::Hell => "hell",
            Design::Dunkel => "dunkel",
        }
    }

    fn aus_name(name: &str) -> Option<Design> {
        Design::ALLE
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFehler {
    /// Zeile (ab 1), sofern bekannt.
    pub zeile: Option<usize>,
    pub meldung: String,
}

impl fmt::Display for ConfigFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.zeile {
            Some(z) => write!(f, "Fehler in Zeile {z}: {}", self.meldung),
            None => write!(f, "{}", self.meldung),
        }
    }
}

impl std::error::Error for ConfigFehler {}

/// Liest einen TOML-Text. Fehlende Schlüssel bekommen die Standardwerte.
pub fn parse(text: &str) -> Result<Geparst, ConfigFehler> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let roh: Roh = toml::from_str(text).map_err(|e| ConfigFehler {
        zeile: e.span().map(|s| zeile_von(text, s.start)),
        meldung: e.message().trim().to_owned(),
    })?;
    let mut warnungen = Vec::new();
    let config = aus_roh(&roh, &mut warnungen);
    Ok(Geparst { config, warnungen })
}

/// Plattformüblicher Pfad der Konfigurationsdatei (Kap. 12.1).
pub fn standard_pfad() -> Option<PathBuf> {
    let basis = directories::BaseDirs::new()?;
    let ordner = if cfg!(any(windows, target_os = "macos")) {
        "Trainingstagebuch"
    } else {
        "trainingstagebuch"
    };
    Some(basis.config_dir().join(ordner).join("config.toml"))
}

/// Ergebnis von [`laden`].
#[derive(Debug, Clone)]
pub struct Geladen {
    pub pfad: PathBuf,
    pub config: Config,
    /// Hinweise zu übersprungenen Regeln u. Ä., beim Start anzeigen.
    pub warnungen: Vec<String>,
    /// Gesetzt, wenn die Datei fehlerhaft ist. Dann gelten die Standardwerte.
    pub fehler: Option<ConfigFehler>,
    /// `false`, wenn die Datei fehlerhaft oder nicht lesbar war. Dann wird sie nie geschrieben.
    beschreibbar: bool,
}

/// Lädt die Konfiguration (Kap. 4.3). Schlägt nie fehl, Probleme stehen in `fehler` bzw.
/// `warnungen`.
pub fn laden(pfad: &Path) -> Geladen {
    let mut geladen = Geladen {
        pfad: pfad.to_owned(),
        config: Config::default(),
        warnungen: Vec::new(),
        fehler: None,
        beschreibbar: false,
    };

    match fs::read_to_string(pfad) {
        Ok(text) => match parse(&text) {
            Ok(g) => {
                geladen.config = g.config;
                geladen.warnungen = g.warnungen;
                geladen.beschreibbar = true;
            }
            Err(e) => geladen.fehler = Some(e),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => match anlegen(pfad) {
            Ok(()) => geladen.beschreibbar = true,
            Err(e) => geladen.warnungen.push(format!(
                "Die Konfigurationsdatei {} konnte nicht angelegt werden: {e}",
                pfad.display()
            )),
        },
        Err(e) => {
            geladen.fehler = Some(ConfigFehler {
                zeile: None,
                meldung: format!("Datei nicht lesbar: {e}"),
            })
        }
    }
    geladen
}

fn anlegen(pfad: &Path) -> io::Result<()> {
    if let Some(ordner) = pfad.parent() {
        fs::create_dir_all(ordner)?;
    }
    fs::write(pfad, STANDARD_CONFIG)
}

impl Geladen {
    /// Setzt `root_path` im Speicher und, wenn möglich, in der Datei (Kap. 4.3).
    ///
    /// Gibt `Ok(true)` zurück, wenn die Datei geschrieben wurde, `Ok(false)`, wenn sie wegen eines
    /// Fehlers beim Laden bewusst nicht angefasst wurde. Die Datei wird dafür neu gelesen, damit
    /// zwischenzeitliche Änderungen des Benutzers nicht verloren gehen.
    pub fn root_path_speichern(&mut self, root: &Path) -> Result<bool, ConfigFehler> {
        self.config.root_path = Some(root.to_owned());
        self.wert_speichern("root_path", &root.to_string_lossy())
    }

    /// Setzt `design` im Speicher und, wenn möglich, in der Datei. Rückgabe wie bei
    /// [`Geladen::root_path_speichern`].
    pub fn design_speichern(&mut self, design: Design) -> Result<bool, ConfigFehler> {
        self.config.design = design;
        self.wert_speichern("design", design.name())
    }

    fn wert_speichern(&self, schluessel: &str, wert: &str) -> Result<bool, ConfigFehler> {
        if !self.beschreibbar {
            return Ok(false);
        }
        let io_fehler = |e: io::Error| ConfigFehler {
            zeile: None,
            meldung: format!("{}: {e}", self.pfad.display()),
        };
        let text = match fs::read_to_string(&self.pfad) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io_fehler(e)),
        };
        let neu = wert_setzen(&text, schluessel, wert)?;
        if let Some(ordner) = self.pfad.parent() {
            fs::create_dir_all(ordner).map_err(io_fehler)?;
        }
        fs::write(&self.pfad, neu).map_err(io_fehler)?;
        Ok(true)
    }
}

/// Setzt einen Schlüssel der obersten Ebene in einem TOML-Text und lässt alles andere unverändert.
fn wert_setzen(text: &str, schluessel: &str, wert: &str) -> Result<String, ConfigFehler> {
    let (bom, inhalt) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let mut doc: toml_edit::DocumentMut =
        inhalt
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigFehler {
                zeile: e.span().map(|s| zeile_von(inhalt, s.start)),
                meldung: e.message().trim().to_owned(),
            })?;
    match doc.get_mut(schluessel).and_then(|i| i.as_value_mut()) {
        Some(v) => {
            let dekor = v.decor().clone();
            *v = toml_edit::Value::from(wert);
            *v.decor_mut() = dekor;
        }
        None => doc[schluessel] = toml_edit::value(wert),
    }
    Ok(format!("{bom}{doc}"))
}

fn zeile_von(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

// ---------------------------------------------------------------------------------------------
// Rohdaten aus TOML. Alles optional, damit fehlende Schlüssel die Standardwerte bekommen.

#[derive(Debug, Default, Deserialize)]
struct Roh {
    root_path: Option<String>,
    design: Option<String>,
    template: Option<RohTemplate>,
    regel: Option<Vec<RohRegel>>,
}

#[derive(Debug, Default, Deserialize)]
struct RohTemplate {
    lines: Option<Vec<String>>,
    motto: Option<RohWoche>,
    uebung: Option<RohWoche>,
}

#[derive(Debug, Default, Deserialize)]
struct RohWoche {
    montag: Option<String>,
    dienstag: Option<String>,
    mittwoch: Option<String>,
    donnerstag: Option<String>,
    freitag: Option<String>,
    samstag: Option<String>,
    sonntag: Option<String>,
}

impl RohWoche {
    fn tage(&self) -> [&Option<String>; 7] {
        [
            &self.montag,
            &self.dienstag,
            &self.mittwoch,
            &self.donnerstag,
            &self.freitag,
            &self.samstag,
            &self.sonntag,
        ]
    }
}

/// `kuerzel` und `woerter` sind Pflicht, fehlen sie, wird nur diese Regel übersprungen (T11f).
/// Deshalb hier optional statt eines Fehlers für die ganze Datei.
#[derive(Debug, Default, Deserialize)]
struct RohRegel {
    kuerzel: Option<String>,
    woerter: Option<Vec<String>>,
    ausser: Option<Vec<String>>,
}

fn standard_roh() -> &'static Roh {
    static STANDARD: OnceLock<Roh> = OnceLock::new();
    STANDARD.get_or_init(|| toml::from_str(STANDARD_CONFIG).expect("STANDARD_CONFIG ist gültig"))
}

fn aus_roh(roh: &Roh, warnungen: &mut Vec<String>) -> Config {
    let std = standard_roh();
    let std_t = std.template.as_ref().expect("Standard hat [template]");
    let t = roh.template.as_ref();

    let lines = t
        .and_then(|t| t.lines.clone())
        .or_else(|| std_t.lines.clone())
        .unwrap_or_default();
    let woche = |roh_w: Option<&RohWoche>, std_w: Option<&RohWoche>| -> [String; 7] {
        let leer = RohWoche::default();
        let roh_tage = roh_w.unwrap_or(&leer).tage();
        let std_tage = std_w.unwrap_or(&leer).tage();
        std::array::from_fn(|i| {
            roh_tage[i]
                .clone()
                .or_else(|| std_tage[i].clone())
                .unwrap_or_default()
        })
    };
    let template = Template {
        lines,
        motto: woche(t.and_then(|t| t.motto.as_ref()), std_t.motto.as_ref()),
        uebung: woche(t.and_then(|t| t.uebung.as_ref()), std_t.uebung.as_ref()),
    };

    let roh_regeln = roh.regel.as_ref().or(std.regel.as_ref());
    let regeln = roh_regeln
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, r)| regel_pruefen(i + 1, r, warnungen))
        .collect();

    let root_path = roh
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);

    let design = match roh.design.as_deref() {
        None => Design::default(),
        Some(name) => Design::aus_name(name).unwrap_or_else(|| {
            warnungen.push(format!(
                "design = \"{name}\" ist unbekannt, erlaubt sind \"system\", \"hell\" und \"dunkel\"."
            ));
            Design::default()
        }),
    };

    Config {
        root_path,
        design,
        template,
        regeln,
    }
}

fn regel_pruefen(nr: usize, roh: &RohRegel, warnungen: &mut Vec<String>) -> Option<Regel> {
    let nicht_leer = |v: &Option<Vec<String>>| -> Vec<String> {
        v.iter()
            .flatten()
            .filter(|w| !w.is_empty())
            .cloned()
            .collect()
    };
    let kuerzel = roh.kuerzel.as_deref().unwrap_or("").trim();
    let woerter = nicht_leer(&roh.woerter);

    let grund = if kuerzel.is_empty() {
        Some("kuerzel fehlt oder ist leer")
    } else if woerter.is_empty() {
        Some("woerter fehlt oder enthält kein nicht-leeres Wort")
    } else {
        None
    };
    if let Some(grund) = grund {
        warnungen.push(format!("Regel Nr. {nr} wird übersprungen: {grund}."));
        return None;
    }
    Some(Regel {
        kuerzel: kuerzel.to_owned(),
        woerter,
        ausser: nicht_leer(&roh.ausser),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::badge::badge;

    #[test]
    fn standard_ist_vollstaendig() {
        let g = parse(STANDARD_CONFIG).unwrap();
        assert!(g.warnungen.is_empty());
        assert_eq!(g.config, Config::default());
        assert_eq!(g.config.root_path, None);
        assert_eq!(g.config.regeln.len(), 19);
        assert_eq!(
            g.config.regeln[0],
            Regel::new("SQ", &["kniebeuge", "squat"], &[])
        );
        assert_eq!(g.config.template.motto[4], "Rückschau halten");
        assert_eq!(g.config.template.uebung[6], "Klimmzüge");
    }

    #[test]
    fn leere_datei_ergibt_standard() {
        let g = parse("").unwrap();
        assert_eq!(g.config, Config::default());
    }

    #[test]
    fn t11d_ohne_regeln_gilt_standard() {
        let g = parse("root_path = \"\"\n[template]\nlines = []\n").unwrap();
        assert_eq!(g.config.regeln, Config::default().regeln);
        assert!(g.config.template.lines.is_empty());
    }

    #[test]
    fn t11e_regel_leer_bedeutet_keine_kuerzel() {
        let g = parse("regel = []\n").unwrap();
        assert!(g.config.regeln.is_empty());
        assert_eq!(badge("Kniebeugen", &g.config.regeln), "");
    }

    #[test]
    fn t11f_fehlerhafte_regeln_werden_uebersprungen() {
        let text = r#"
[[regel]]
kuerzel = ""
woerter = ["a"]

[[regel]]
kuerzel = "X"
woerter = []

[[regel]]
kuerzel = "Y"

[[regel]]
woerter = ["b"]

[[regel]]
kuerzel = "Z"
woerter = ["", "zett"]
ausser  = [""]
"#;
        let g = parse(text).unwrap();
        assert_eq!(g.config.regeln, vec![Regel::new("Z", &["zett"], &[])]);
        assert_eq!(g.warnungen.len(), 4);
        assert!(g.warnungen[0].contains("Nr. 1"));
        assert!(g.warnungen[2].contains("Nr. 3"));
    }

    #[test]
    fn fehlende_wochentage_bekommen_standard() {
        let g = parse("[template.motto]\nmontag = \"Neu\"\n").unwrap();
        assert_eq!(g.config.template.motto[0], "Neu");
        assert_eq!(g.config.template.motto[1], "Bombige Zeiten");
        assert_eq!(g.config.template.uebung, Config::default().template.uebung);
    }

    #[test]
    fn root_path() {
        let g = parse("root_path = '  '\n").unwrap();
        assert_eq!(g.config.root_path, None);
        let g = parse("root_path = 'D:\\Journal'\n").unwrap();
        assert_eq!(g.config.root_path, Some(PathBuf::from("D:\\Journal")));
    }

    #[test]
    fn gueltiger_root_nur_wenn_ordner_existiert() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Config::default();
        assert!(c.gueltiger_root().is_none());
        c.root_path = Some(dir.path().join("fehlt"));
        assert!(c.gueltiger_root().is_none());
        c.root_path = Some(dir.path().to_owned());
        assert_eq!(c.gueltiger_root(), Some(dir.path()));
    }

    #[test]
    fn bom_wird_ignoriert() {
        let g = parse("\u{feff}root_path = 'x'\n").unwrap();
        assert_eq!(g.config.root_path, Some(PathBuf::from("x")));
    }

    #[test]
    fn syntaxfehler_mit_zeile() {
        let e = parse("root_path = ''\n\n[template\n").unwrap_err();
        assert_eq!(e.zeile, Some(3));
        assert!(e.to_string().starts_with("Fehler in Zeile 3"));
    }

    #[test]
    fn falscher_typ_ist_fehler() {
        let e = parse("[template]\nlines = \"nur ein String\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(2));
    }

    #[test]
    fn design_lesen() {
        assert_eq!(parse("").unwrap().config.design, Design::System);
        assert_eq!(
            parse(
                "design = 'Dunkel'
"
            )
            .unwrap()
            .config
            .design,
            Design::Dunkel
        );
        let g = parse(
            "design = 'lila'
",
        )
        .unwrap();
        assert_eq!(g.config.design, Design::System);
        assert_eq!(g.warnungen.len(), 1);
        assert_eq!(Config::default().design, Design::System);
    }

    #[test]
    fn design_speichern_ergaenzt_alte_datei() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("config.toml");
        // Ältere Datei ohne `design`, mit Tabellen danach.
        fs::write(
            &pfad,
            "root_path = 'x'

[template]
lines = []
",
        )
        .unwrap();
        let mut g = laden(&pfad);
        assert_eq!(g.design_speichern(Design::Hell), Ok(true));
        assert_eq!(laden(&pfad).config.design, Design::Hell);
        assert_eq!(g.design_speichern(Design::Dunkel), Ok(true));
        let text = fs::read_to_string(&pfad).unwrap();
        assert!(text.contains("design = \"dunkel\""), "{text}");
        assert!(text.find("design").unwrap() < text.find("[template]").unwrap());
    }

    #[test]
    fn t19_datei_fehlt_wird_angelegt() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("unter").join("config.toml");
        let g = laden(&pfad);
        assert!(g.fehler.is_none());
        assert!(g.warnungen.is_empty());
        assert_eq!(g.config, Config::default());
        assert_eq!(fs::read_to_string(&pfad).unwrap(), STANDARD_CONFIG);
    }

    #[test]
    fn t22_syntaxfehler_datei_bleibt_unveraendert() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("config.toml");
        let kaputt = "root_path = 'alt'\n[template\nlines = 3\n";
        fs::write(&pfad, kaputt).unwrap();

        let mut g = laden(&pfad);
        assert_eq!(g.fehler.as_ref().unwrap().zeile, Some(2));
        assert_eq!(g.config, Config::default());

        let neu = dir.path().join("journal");
        assert_eq!(g.root_path_speichern(&neu), Ok(false));
        assert_eq!(g.config.root_path, Some(neu));
        assert_eq!(fs::read_to_string(&pfad).unwrap(), kaputt);
    }

    #[test]
    fn t23_kommentare_bleiben_erhalten() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("config.toml");
        let vorher = STANDARD_CONFIG.replace(
            "root_path = \"\"",
            "root_path = \"\"   # mein Kommentar\n# noch einer",
        );
        fs::write(&pfad, &vorher).unwrap();

        let mut g = laden(&pfad);
        let neu = dir.path().join("Journal Ordner");
        assert_eq!(g.root_path_speichern(&neu), Ok(true));

        let nachher = fs::read_to_string(&pfad).unwrap();
        let alt_ohne_root: Vec<_> = vorher
            .lines()
            .filter(|l| !l.starts_with("root_path"))
            .collect();
        let neu_ohne_root: Vec<_> = nachher
            .lines()
            .filter(|l| !l.starts_with("root_path"))
            .collect();
        assert_eq!(alt_ohne_root, neu_ohne_root);
        let root_zeile = nachher
            .lines()
            .find(|l| l.starts_with("root_path"))
            .unwrap();
        assert!(root_zeile.ends_with("   # mein Kommentar"), "{root_zeile}");

        assert_eq!(laden(&pfad).config.root_path, Some(neu));
    }

    #[test]
    fn root_path_fehlt_wird_oben_eingefuegt() {
        let neu = wert_setzen("[template]\nlines = []\n", "root_path", "/j").unwrap();
        let g = parse(&neu).unwrap();
        assert_eq!(g.config.root_path, Some(PathBuf::from("/j")));
        assert!(g.config.template.lines.is_empty());
        assert!(neu.starts_with("root_path"), "{neu}");
    }

    #[test]
    fn root_path_mit_bom_bleibt_bom() {
        let neu = wert_setzen("\u{feff}root_path = ''\n", "root_path", "x").unwrap();
        assert!(neu.starts_with('\u{feff}'));
        assert_eq!(
            parse(&neu).unwrap().config.root_path,
            Some(PathBuf::from("x"))
        );
    }

    #[test]
    fn standard_pfad_endet_auf_config_toml() {
        if let Some(p) = standard_pfad() {
            assert!(p.ends_with("config.toml"));
        }
    }
}
