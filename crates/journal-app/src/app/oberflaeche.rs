//! Oberflächentests ohne Fenster mit `egui_kittest`.
//!
//! Die Bildschirmfotos (`cargo test -p journal-app -- --ignored bildschirmfotos`) brauchen eine
//! Grafikkarte bzw. einen Software-Renderer und laufen deshalb nicht automatisch.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use journal_core::config;

use super::{Start, TagebuchApp};

fn d(j: i32, m: u32, t: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(j, m, t).unwrap()
}

/// Ein Journal-Ordner und ein getrennter Ordner für `config.toml`.
struct Umgebung {
    journal: tempfile::TempDir,
    konfig: tempfile::TempDir,
}

impl Umgebung {
    fn neu(dateien: &[(&str, &str)]) -> Self {
        let u = Self {
            journal: tempfile::tempdir().unwrap(),
            konfig: tempfile::tempdir().unwrap(),
        };
        for (name, inhalt) in dateien {
            fs::write(u.journal.path().join(name), inhalt).unwrap();
        }
        u
    }

    fn pfad(&self, name: &str) -> std::path::PathBuf {
        self.journal.path().join(name)
    }

    /// App mit „heute“ = Freitag, 6.11.2026, Fenster 500 × 600 wie in der Spezifikation.
    fn app(&self) -> Harness<'static, TagebuchApp> {
        self.app_mit(egui::Theme::Light)
    }

    fn app_mit(&self, theme: egui::Theme) -> Harness<'static, TagebuchApp> {
        let start = Start {
            config: config::laden(&self.konfig.path().join("config.toml")),
            root: Some(self.journal.path().to_owned()),
            meldungen: Vec::new(),
            heute: Some(d(2026, 11, 6)),
            ordner_waehlen: Box::new(|| None),
        };
        let mut h = Harness::builder()
            .with_size([500.0, 600.0])
            .with_theme(theme)
            .build_eframe(|cc| TagebuchApp::new(&cc.egui_ctx, start));
        warte_auf_index(&mut h);
        h
    }
}

fn warte_bis(
    h: &mut Harness<'_, TagebuchApp>,
    was: &str,
    bedingung: impl Fn(&Harness<'_, TagebuchApp>) -> bool,
) {
    let ende = Instant::now() + Duration::from_secs(10);
    loop {
        h.step();
        if bedingung(h) {
            return;
        }
        assert!(Instant::now() < ende, "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn warte_auf_index(h: &mut Harness<'_, TagebuchApp>) {
    warte_bis(h, "Index", |h| !h.state().index_laeuft());
    h.run_steps(2);
}

#[test]
fn t1_t2_t3_start_mit_badges_und_vorlage() {
    let u = Umgebung::neu(&[
        ("2026-11-02.txt", "Kreuzheben 3x5\r\n"),
        ("2026-11-03.txt", "Notiz\r\n"),
    ]);
    let h = u.app();
    let app = h.state();
    assert_eq!(
        app.index.tag(d(2026, 11, 2)).unwrap().badge.as_deref(),
        Some("DL")
    );
    assert_eq!(
        app.index.tag(d(2026, 11, 3)).unwrap().badge.as_deref(),
        Some("")
    );
    assert_eq!(
        app.doc.text(),
        "Freitag, 6. November 2026\nRückschau halten\n5 Werte\nKniebeugen"
    );
    // Die Zellen sind beschriftet, auch für Screenreader.
    h.get_by_label("Montag, 2. November 2026, Eintrag DL");
    h.get_by_label("Dienstag, 3. November 2026, Eintrag ");
    h.get_by_label("Mittwoch, 4. November 2026");
    h.get_by_label("November 2026");
}

#[test]
fn t26_jahres_und_jahrzehntansicht_aendern_den_tag_nicht() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    let text_vorher = h.state().doc.text().to_owned();

    for klick in ["November 2026", "2026", "2027", "März"] {
        h.get_by_label(klick).click();
        h.run_steps(2);
    }
    h.get_by_label("März 2027");
    h.get_by_label("Montag, 1. März 2027");
    let app = h.state();
    assert_eq!(app.doc.datum(), d(2026, 11, 6));
    assert_eq!(app.doc.text(), text_vorher);
}

#[test]
fn t5_tag_klicken_speichert_geaenderten_vortag() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    h.state_mut()
        .doc
        .text_mut()
        .push_str("\nKniebeugen 5x5 mit 80 kg");

    h.get_by_label("Samstag, 7. November 2026").click();
    h.run_steps(2);
    assert_eq!(h.state().doc.datum(), d(2026, 11, 7));
    assert!(
        fs::read_to_string(u.pfad("2026-11-06.txt"))
            .unwrap()
            .contains("80 kg")
    );

    warte_bis(&mut h, "Badge am 6.11.", |h| {
        h.state()
            .index
            .tag(d(2026, 11, 6))
            .and_then(|t| t.badge.as_deref())
            .is_some_and(|b| b.split(' ').any(|k| k == "SQ"))
    });
}

#[test]
fn zelle_im_nachbarmonat_blaettert() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    h.get_by_label("Dienstag, 1. Dezember 2026").click();
    h.run_steps(2);
    assert_eq!(h.state().doc.datum(), d(2026, 12, 1));
    h.get_by_label("Dezember 2026");
}

#[test]
fn design_umschalten_wird_gespeichert() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    assert!(!(h.ctx.theme() == egui::Theme::Dark));
    h.get_by_label("🌙").click();
    h.run_steps(2);
    assert!((h.ctx.theme() == egui::Theme::Dark));
    let datei = fs::read_to_string(u.konfig.path().join("config.toml")).unwrap();
    assert!(datei.contains("design = \"dunkel\""), "{datei}");

    h.get_by_label("☀").click();
    h.run_steps(2);
    assert!(!(h.ctx.theme() == egui::Theme::Dark));
    assert_eq!(
        config::laden(&u.konfig.path().join("config.toml"))
            .config
            .design,
        config::Design::Hell
    );
}

#[test]
fn heute_knopf_springt_zurueck() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    h.get_by_label("Dienstag, 1. Dezember 2026").click();
    h.run_steps(2);
    h.get_by_label("Dezember 2026").click(); // Jahresansicht
    h.run_steps(2);
    h.get_by_label("Heute").click();
    h.run_steps(2);
    assert_eq!(h.state().doc.datum(), d(2026, 11, 6));
    h.get_by_label("November 2026");
}

#[test]
fn speichern_button() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    h.get_by_label("Speichern").click();
    h.run_steps(2);
    assert!(u.pfad("2026-11-06.txt").exists());
    h.get_by_label("Gespeichert.");
}

#[test]
fn loeschen_mit_rueckfrage_nein_behaelt_datei() {
    let u = Umgebung::neu(&[("2026-11-06.txt", "Kniebeugen\r\n")]);
    let mut h = u.app();
    h.get_by_label("Löschen").click();
    h.run_steps(2);
    h.get_by_label_contains("Eintrag vom Freitag, 6. November 2026 in den Papierkorb");
    h.get_by_label("Nein").click();
    h.run_steps(2);
    assert!(h.query_by_label("Nein").is_none(), "Dialog geschlossen");
    assert!(u.pfad("2026-11-06.txt").exists());
}

#[test]
fn t24_loeschen_ohne_datei_setzt_vorlage_ohne_rueckfrage() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    h.state_mut().doc.text_mut().push_str("\ngetippt");
    h.get_by_label("Löschen").click();
    h.run_steps(2);
    assert!(h.query_by_label("Ja").is_none());
    assert!(!h.state().doc.text().contains("getippt"));
}

#[test]
fn t27_externe_aenderung_wird_ueber_die_ueberwachung_geladen() {
    let u = Umgebung::neu(&[("2026-11-06.txt", "alt\r\n")]);
    let mut h = u.app();
    fs::write(u.pfad("2026-11-06.txt"), "vom Laptop\r\n").unwrap();
    warte_bis(&mut h, "Neu laden", |h| {
        h.state().doc.text() == "vom Laptop\n"
    });
    h.run_steps(2);
    h.get_by_label("Eintrag wurde von außen aktualisiert.");
}

#[test]
fn t28_t30_konfliktleiste_und_externe_version_laden() {
    let u = Umgebung::neu(&[("2026-11-06.txt", "alt\r\n")]);
    let mut h = u.app();
    h.state_mut().doc.text_mut().push_str("lokal");
    fs::write(u.pfad("2026-11-06.txt"), "extern\r\n").unwrap();

    warte_bis(&mut h, "Hinweisleiste", |h| {
        h.query_by_label("Externe Version laden").is_some()
    });
    h.get_by_label("Dieser Eintrag wurde außerhalb der App geändert.");
    h.get_by_label("Externe Version laden").click();
    h.run_steps(2);

    assert_eq!(h.state().doc.text(), "extern\n");
    let kopien: Vec<_> = fs::read_dir(u.journal.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains(".konflikt-"))
        .collect();
    assert_eq!(kopien.len(), 1, "{kopien:?}");
    assert_eq!(
        fs::read_to_string(u.pfad(&kopien[0])).unwrap(),
        "alt\r\nlokal\r\n"
    );
    warte_bis(&mut h, "⚠ im Index", |h| {
        h.state()
            .index
            .tag(d(2026, 11, 6))
            .is_some_and(|t| !t.konflikte.is_empty())
    });
}

/// Rendert einige Zustände als PNG nach `target/bildschirmfotos/`, zum Ansehen.
#[test]
#[ignore]
fn bildschirmfotos() {
    let ziel = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bildschirmfotos");
    fs::create_dir_all(&ziel).unwrap();
    let u = Umgebung::neu(&[
        ("2026-11-02.txt", "Kreuzheben 3x5, Kniebeugen\r\n"),
        ("2026-11-03.txt", "Notiz\r\n"),
        ("2026-11-04.txt", "Handstand, Yoga, Joggen, Schwimmen\r\n"),
        ("2026-11-05.txt", "Klimmzüge 3x8\r\n"),
        ("2026-11-05.konflikt-20261105-101500.txt", "x"),
        ("2026-10-30.txt", "Overhead Press, Bench Press\r\n"),
        (
            "2026-11-06.txt",
            "Freitag, 6. November 2026\r\nRückschau halten\r\n5 Werte\r\nKniebeugen 5x5\r\n",
        ),
    ]);
    let mut h = u.app();
    let speichern = |h: &mut Harness<'_, TagebuchApp>, name: &str| {
        h.run_steps(2);
        h.render()
            .expect("Rendern")
            .save(ziel.join(format!("{name}.png")))
            .unwrap();
    };
    speichern(&mut h, "1_monat");

    h.state_mut().doc.text_mut().push_str("lokal");
    fs::write(u.pfad("2026-11-06.txt"), "extern\r\n").unwrap();
    warte_bis(&mut h, "Hinweisleiste", |h| {
        h.query_by_label("Externe Version laden").is_some()
    });
    speichern(&mut h, "2_konflikt");

    h.get_by_label("November 2026").click();
    speichern(&mut h, "3_jahr");
    h.get_by_label("2026").click();
    speichern(&mut h, "4_jahrzehnt");
    h.get_by_label("2026").click();
    h.run_steps(2);
    h.get_by_label("November").click();
    h.run_steps(2);

    h.get_by_label("Löschen").click();
    speichern(&mut h, "5_rueckfrage");

    let mut h = u.app_mit(egui::Theme::Dark);
    speichern(&mut h, "6_dunkel");
    h.get_by_label("November 2026").click();
    speichern(&mut h, "7_dunkel_jahr");
}

#[test]
fn zwischenstand_des_index_erscheint_sofort_mit_fortschritt() {
    let u = Umgebung::neu(&[]);
    let mut h = u.app();
    let mut index = journal_core::index::Index::default();
    index.tage.insert(
        d(2026, 11, 2),
        journal_core::index::Tag {
            badge: Some(String::new()),
            konflikte: Vec::new(),
        },
    );
    h.state()
        .bauer
        .tx
        .send(super::IndexNachricht::Zwischen {
            root: u.journal.path().to_owned(),
            index,
            gelesen: 120,
            gesamt: 691,
        })
        .unwrap();
    h.run_steps(2);
    h.get_by_label("Lese Einträge … 120 / 691");
    h.get_by_label("Montag, 2. November 2026, Eintrag ");
}
