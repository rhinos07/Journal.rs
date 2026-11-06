//! Kalender mit Monats-, Jahres- und Jahrzehntansicht (Spezifikation F6, E8).

use chrono::{Datelike, Days, Months, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Rect, RichText, Role, Sense, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};
use journal_core::de;
use journal_core::index::Index;

/// Farben des Kalenders: flach, eine Akzentfarbe (Spezifikation F6: Farben sind KANN, wichtig ist
/// nur die Unterscheidbarkeit von heute, ausgewählt und anderem Monat).
mod farbe {
    use eframe::egui::Color32;

    /// Akzent für „heute“ und „ausgewählt“.
    pub const AKZENT: Color32 = Color32::from_rgb(0x25, 0x63, 0xEB);
    /// Fläche des ausgewählten Tags (Akzent, sehr hell).
    pub const AUSGEWAEHLT: Color32 = Color32::from_rgb(0xE3, 0xEC, 0xFD);
    /// Fläche unter dem Mauszeiger.
    pub const MAUS: Color32 = Color32::from_rgb(0xEE, 0xF2, 0xF7);
    /// Leicht abgetönte Fläche für Samstag und Sonntag.
    pub const WOCHENENDE: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF8);
    pub const TEXT: Color32 = Color32::from_rgb(0x1F, 0x29, 0x37);
    pub const TEXT_LEISE: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
    pub const TEXT_BLASS: Color32 = Color32::from_rgb(0xB4, 0xBB, 0xC6);
    pub const WARNUNG: Color32 = Color32::from_rgb(0xD9, 0x77, 0x06);

    /// Gedämpfte Farbpaare (Fläche, Schrift) für die Kürzel-Pills.
    const PILLS: [(Color32, Color32); 10] = [
        (
            Color32::from_rgb(0xDC, 0xF5, 0xE3),
            Color32::from_rgb(0x16, 0x65, 0x34),
        ), // grün
        (
            Color32::from_rgb(0xDB, 0xEA, 0xFE),
            Color32::from_rgb(0x1E, 0x40, 0xAF),
        ), // blau
        (
            Color32::from_rgb(0xFF, 0xED, 0xD5),
            Color32::from_rgb(0x9A, 0x34, 0x12),
        ), // orange
        (
            Color32::from_rgb(0xF3, 0xE8, 0xFF),
            Color32::from_rgb(0x6B, 0x21, 0xA8),
        ), // violett
        (
            Color32::from_rgb(0xFC, 0xE7, 0xF3),
            Color32::from_rgb(0x9D, 0x17, 0x4D),
        ), // pink
        (
            Color32::from_rgb(0xCF, 0xFA, 0xFE),
            Color32::from_rgb(0x15, 0x5E, 0x75),
        ), // cyan
        (
            Color32::from_rgb(0xFE, 0xF3, 0xC7),
            Color32::from_rgb(0x85, 0x4D, 0x0E),
        ), // gelb
        (
            Color32::from_rgb(0xFE, 0xE2, 0xE2),
            Color32::from_rgb(0x99, 0x1B, 0x1B),
        ), // rot
        (
            Color32::from_rgb(0xE0, 0xE7, 0xFF),
            Color32::from_rgb(0x37, 0x30, 0xA3),
        ), // indigo
        (
            Color32::from_rgb(0xEC, 0xFC, 0xCB),
            Color32::from_rgb(0x3F, 0x62, 0x12),
        ), // limette
    ];

    /// Farbpaar für ein Kürzel. Hängt nur vom Kürzel ab (FNV-1a), ist also auf allen Geräten
    /// und nach jedem Neustart gleich.
    pub fn pill(kuerzel: &str) -> (Color32, Color32) {
        let mut h: u32 = 0x811C_9DC5;
        for b in kuerzel.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        PILLS[h as usize % PILLS.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ansicht {
    Monat,
    Jahr,
    Jahrzehnt,
}

#[derive(Debug, Clone)]
pub struct Kalender {
    pub ansicht: Ansicht,
    /// Angezeigter Monat, immer der 1. des Monats.
    pub anzeige: NaiveDate,
}

fn erster(datum: NaiveDate) -> NaiveDate {
    datum.with_day(1).expect("Tag 1 gibt es immer")
}

impl Kalender {
    pub fn new(datum: NaiveDate) -> Self {
        Self {
            ansicht: Ansicht::Monat,
            anzeige: erster(datum),
        }
    }

    /// Monatsansicht auf den Monat von `datum` stellen (z. B. nach Tastatur-Navigation).
    pub fn zeige(&mut self, datum: NaiveDate) {
        self.ansicht = Ansicht::Monat;
        self.anzeige = erster(datum);
    }

    /// Titel der aktuellen Ansicht, z. B. `November 2026`, `2026` oder `2020-2029`.
    pub fn titel(&self) -> String {
        match self.ansicht {
            Ansicht::Monat => de::monat_jahr(self.anzeige),
            Ansicht::Jahr => self.anzeige.year().to_string(),
            Ansicht::Jahrzehnt => {
                let start = self.anzeige.year().div_euclid(10) * 10;
                format!("{start}-{}", start + 9)
            }
        }
    }

    fn blaettern(&mut self, vor: bool) {
        let monate = match self.ansicht {
            Ansicht::Monat => 1,
            Ansicht::Jahr => 12,
            Ansicht::Jahrzehnt => 120,
        };
        let neu = if vor {
            self.anzeige.checked_add_months(Months::new(monate))
        } else {
            self.anzeige.checked_sub_months(Months::new(monate))
        };
        if let Some(neu) = neu {
            self.anzeige = neu;
        }
    }

    /// Zeichnet den Kalender. Gibt den angeklickten bzw. per Pfeiltaste gewählten Tag zurück.
    ///
    /// `pfeiltasten`: Pfeiltasten wechseln den Tag (nur wenn kein Eingabefeld den Fokus hat).
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ausgewaehlt: NaiveDate,
        heute: NaiveDate,
        index: &Index,
        pfeiltasten: bool,
    ) -> Option<NaiveDate> {
        let mut gewaehlt = None;

        ui.horizontal(|ui| {
            if ui.button("◀").on_hover_text("Zurück").clicked() {
                self.blaettern(false);
            }
            if ui.button("▶").on_hover_text("Weiter").clicked() {
                self.blaettern(true);
            }
            ui.add_space(8.0);
            let titel =
                ui.add(egui::Button::new(RichText::new(self.titel()).size(20.0)).frame(false));
            let titel = match self.ansicht {
                Ansicht::Monat => titel.on_hover_text("Jahresansicht"),
                Ansicht::Jahr => titel.on_hover_text("Jahrzehntansicht"),
                Ansicht::Jahrzehnt => titel,
            };
            if titel.clicked() {
                self.ansicht = match self.ansicht {
                    Ansicht::Monat => Ansicht::Jahr,
                    Ansicht::Jahr | Ansicht::Jahrzehnt => Ansicht::Jahrzehnt,
                };
            }
        });
        ui.add_space(4.0);

        match self.ansicht {
            Ansicht::Monat => {
                if pfeiltasten {
                    gewaehlt = pfeil_navigation(ui, ausgewaehlt);
                }
                if let Some(d) = self.monat_ui(ui, ausgewaehlt, heute, index) {
                    gewaehlt = Some(d);
                }
                if let Some(d) = gewaehlt {
                    self.anzeige = erster(d);
                }
            }
            Ansicht::Jahr => self.jahr_ui(ui, heute),
            Ansicht::Jahrzehnt => self.jahrzehnt_ui(ui, heute),
        }
        gewaehlt
    }

    fn monat_ui(
        &mut self,
        ui: &mut egui::Ui,
        ausgewaehlt: NaiveDate,
        heute: NaiveDate,
        index: &Index,
    ) -> Option<NaiveDate> {
        let breite = ui.available_width();
        let spalte = breite / 7.0;

        // Wochentagsnamen, bei schmalen Spalten abgekürzt.
        let (kopf, _) = ui.allocate_exact_size(vec2(breite, 20.0), Sense::hover());
        for (i, name) in de::WOCHENTAGE.iter().enumerate() {
            let text = if spalte >= 82.0 { name } else { &name[..2] };
            ui.painter().text(
                pos2(kopf.left() + spalte * (i as f32 + 0.5), kopf.center().y),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(12.0),
                farbe::TEXT_LEISE,
            );
        }

        let hoehe = ui.available_height().max(6.0 * 40.0);
        let (raster, _) = ui.allocate_exact_size(vec2(breite, hoehe), Sense::hover());
        let zeile = hoehe / 6.0;
        let start =
            self.anzeige - Days::new(u64::from(self.anzeige.weekday().num_days_from_monday()));

        let mut gewaehlt = None;
        for i in 0..42u32 {
            let datum = start + Days::new(u64::from(i));
            let rect = Rect::from_min_size(
                pos2(
                    raster.left() + spalte * (i % 7) as f32,
                    raster.top() + zeile * (i / 7) as f32,
                ),
                vec2(spalte, zeile),
            )
            .shrink(2.0);
            let antwort = ui.interact(rect, ui.scope_id().with(("tag", i)), Sense::click());
            let tag = index.tag(datum);
            antwort.widget_info(|| {
                let mut text = de::datum_lang(datum);
                if let Some(b) = tag.and_then(|t| t.badge.as_deref()) {
                    text.push_str(&format!(", Eintrag {b}"));
                }
                egui::WidgetInfo::selected(Role::Button, true, datum == ausgewaehlt, text)
            });
            zelle_zeichnen(
                ui,
                rect,
                datum,
                ZellenZustand {
                    anderer_monat: datum.month() != self.anzeige.month(),
                    wochenende: i % 7 >= 5,
                    heute: datum == heute,
                    ausgewaehlt: datum == ausgewaehlt,
                    maus: antwort.hovered(),
                },
                tag.and_then(|t| t.badge.as_deref()),
                tag.map_or(0, |t| t.konflikte.len()),
            );
            let mut tooltip = Vec::new();
            if let Some(b) = tag.and_then(|t| t.badge.as_deref())
                && !b.is_empty()
            {
                tooltip.push(b.to_owned());
            }
            if let Some(t) = tag.filter(|t| !t.konflikte.is_empty()) {
                tooltip.push(format!(
                    "⚠ Konfliktdateien zu diesem Tag ({}):\n{}",
                    t.konflikte.len(),
                    t.konflikte.join("\n")
                ));
            }
            let antwort = if tooltip.is_empty() {
                antwort
            } else {
                antwort.on_hover_text(tooltip.join("\n\n"))
            };
            if antwort.clicked() {
                gewaehlt = Some(datum);
            }
        }
        gewaehlt
    }

    fn jahr_ui(&mut self, ui: &mut egui::Ui, heute: NaiveDate) {
        let jahr = self.anzeige.year();
        let felder: Vec<_> = (1..=12)
            .map(|m| Feld {
                text: de::MONATE[m as usize - 1].to_owned(),
                markiert: m == self.anzeige.month(),
                heute: jahr == heute.year() && m == heute.month(),
                ausgegraut: false,
            })
            .collect();
        if let Some(i) = raster_ui(ui, &felder) {
            self.anzeige = NaiveDate::from_ymd_opt(jahr, i as u32 + 1, 1).expect("gültiger Monat");
            self.ansicht = Ansicht::Monat;
        }
    }

    fn jahrzehnt_ui(&mut self, ui: &mut egui::Ui, heute: NaiveDate) {
        let start = self.anzeige.year().div_euclid(10) * 10;
        let jahre: Vec<i32> = (start - 1..=start + 10).collect();
        let felder: Vec<_> = jahre
            .iter()
            .map(|&j| Feld {
                text: j.to_string(),
                markiert: j == self.anzeige.year(),
                heute: j == heute.year(),
                ausgegraut: j < start || j > start + 9,
            })
            .collect();
        if let Some(i) = raster_ui(ui, &felder)
            && let Some(neu) = NaiveDate::from_ymd_opt(jahre[i], self.anzeige.month(), 1)
        {
            self.anzeige = neu;
            self.ansicht = Ansicht::Jahr;
        }
    }
}

/// Pfeiltasten: links/rechts ein Tag, hoch/runter eine Woche.
fn pfeil_navigation(ui: &egui::Ui, ausgewaehlt: NaiveDate) -> Option<NaiveDate> {
    let (links, rechts, hoch, runter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
        )
    });
    if links {
        ausgewaehlt.pred_opt()
    } else if rechts {
        ausgewaehlt.succ_opt()
    } else if hoch {
        ausgewaehlt.checked_sub_days(Days::new(7))
    } else if runter {
        ausgewaehlt.checked_add_days(Days::new(7))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct ZellenZustand {
    anderer_monat: bool,
    wochenende: bool,
    heute: bool,
    ausgewaehlt: bool,
    maus: bool,
}

const RUNDUNG: u8 = 8;

fn zelle_zeichnen(
    ui: &egui::Ui,
    rect: Rect,
    datum: NaiveDate,
    zustand: ZellenZustand,
    badge: Option<&str>,
    konflikte: usize,
) {
    let painter = ui.painter_at(rect.expand(1.0));
    let rund = CornerRadius::same(RUNDUNG);

    // Fläche: ausgewählt > Maus > Wochenende, sonst frei.
    let flaeche = if zustand.ausgewaehlt {
        Some(farbe::AUSGEWAEHLT)
    } else if zustand.maus {
        Some(farbe::MAUS)
    } else if zustand.wochenende && !zustand.anderer_monat {
        Some(farbe::WOCHENENDE)
    } else {
        None
    };
    if let Some(f) = flaeche {
        painter.rect_filled(rect, rund, f);
    }
    if zustand.ausgewaehlt {
        painter.rect_stroke(
            rect,
            rund,
            Stroke::new(1.5, farbe::AKZENT),
            StrokeKind::Inside,
        );
    }

    // Tageszahl oben links, „heute“ im gefüllten Kreis.
    let mitte = rect.left_top() + vec2(13.0, 12.0);
    let zahl_farbe = if zustand.heute {
        painter.circle_filled(mitte, 9.5, farbe::AKZENT);
        Color32::WHITE
    } else if zustand.anderer_monat {
        farbe::TEXT_BLASS
    } else {
        farbe::TEXT
    };
    painter.text(
        mitte,
        Align2::CENTER_CENTER,
        datum.day(),
        FontId::proportional(12.5),
        zahl_farbe,
    );
    if konflikte > 0 {
        painter.text(
            rect.right_top() + vec2(-6.0, 12.0),
            Align2::RIGHT_CENTER,
            "⚠",
            FontId::proportional(13.0),
            farbe::WARNUNG,
        );
    }

    if let Some(badge) = badge {
        let bereich = Rect::from_min_max(
            pos2(rect.left() + 4.0, rect.top() + 23.0),
            rect.right_bottom() - vec2(4.0, 1.0),
        );
        if badge.trim().is_empty() {
            // Eintrag ohne Kürzel: kleiner Punkt.
            let punkt = if zustand.anderer_monat {
                farbe::TEXT_BLASS.gamma_multiply(0.6)
            } else {
                farbe::TEXT_BLASS
            };
            painter.circle_filled(bereich.left_top() + vec2(10.0, 7.0), 3.5, punkt);
        } else {
            pills_zeichnen(ui, bereich, badge, zustand.anderer_monat);
        }
    }
}

/// Kürzel als abgerundete Kapseln, zeilenweise umbrochen. Was nicht passt, wird als „+n“
/// zusammengefasst (der ganze Text steht im Tooltip der Zelle).
fn pills_zeichnen(ui: &egui::Ui, bereich: Rect, badge: &str, blass: bool) {
    if bereich.width() < 12.0 || bereich.height() < PILL_HOEHE {
        return;
    }
    let painter = ui.painter_at(bereich);
    let schrift = FontId::proportional(10.0);
    let kuerzel: Vec<&str> = badge.split_whitespace().collect();
    let breite = |text: &str| {
        let b = painter
            .layout_no_wrap(text.to_owned(), schrift.clone(), Color32::BLACK)
            .size()
            .x;
        (b + 2.0 * PILL_POLSTER).min(bereich.width())
    };
    let breiten: Vec<f32> = kuerzel.iter().map(|k| breite(k)).collect();
    let zeilen = ((bereich.height() + PILL_ABSTAND) / (PILL_HOEHE + PILL_ABSTAND)).floor() as usize;

    // Wie viele passen vollständig, ggf. zusammen mit einem „+n“ dahinter?
    let mut anzahl = kuerzel.len();
    let plan = loop {
        let mut liste: Vec<f32> = breiten[..anzahl].to_vec();
        if anzahl < kuerzel.len() {
            liste.push(breite(&format!("+{}", kuerzel.len() - anzahl)));
        }
        match anordnen(&liste, bereich.width(), zeilen) {
            Some(plan) => break plan,
            // Mindestens ein Kürzel zeigen, notfalls ohne „+n“.
            None if anzahl > 1 => anzahl -= 1,
            None => break vec![(0.0, 0, breiten[0])],
        }
    };

    let anzahl = anzahl.min(plan.len());
    for (i, (x, zeile, b)) in plan.into_iter().enumerate() {
        let pos = bereich.left_top() + vec2(x, zeile as f32 * (PILL_HOEHE + PILL_ABSTAND));
        if i < anzahl {
            let mut farben = farbe::pill(kuerzel[i]);
            if blass {
                farben = (farben.0.gamma_multiply(0.5), farben.1.gamma_multiply(0.5));
            }
            pill(&painter, pos, b, kuerzel[i], farben, &schrift);
        } else {
            let text = format!("+{}", kuerzel.len() - anzahl);
            pill(
                &painter,
                pos,
                b,
                &text,
                (farbe::MAUS, farbe::TEXT_LEISE),
                &schrift,
            );
        }
    }
}

const PILL_HOEHE: f32 = 13.0;
const PILL_ABSTAND: f32 = 2.0;
const PILL_POLSTER: f32 = 4.0;

/// Ordnet Kapseln der Breite nach zeilenweise an: (x, Zeile, Breite), `None` wenn sie nicht in
/// `zeilen` Zeilen passen.
fn anordnen(breiten: &[f32], platz: f32, zeilen: usize) -> Option<Vec<(f32, usize, f32)>> {
    let mut plan = Vec::with_capacity(breiten.len());
    let (mut x, mut zeile) = (0.0, 0);
    for &b in breiten {
        if x > 0.0 && x + b > platz {
            zeile += 1;
            x = 0.0;
        }
        if zeile >= zeilen {
            return None;
        }
        plan.push((x, zeile, b));
        x += b + PILL_ABSTAND;
    }
    Some(plan)
}

fn pill(
    painter: &egui::Painter,
    links_oben: egui::Pos2,
    breite: f32,
    text: &str,
    (flaeche, schrift_farbe): (Color32, Color32),
    schrift: &FontId,
) {
    let rect = Rect::from_min_size(links_oben, vec2(breite, PILL_HOEHE));
    painter.rect_filled(rect, CornerRadius::same(6), flaeche);
    // Zu lange Kürzel linksbündig abschneiden, damit der Anfang lesbar bleibt.
    let innen = rect.shrink2(vec2(PILL_POLSTER, 0.0));
    let galley = painter.layout_no_wrap(text.to_owned(), schrift.clone(), schrift_farbe);
    let x = if galley.size().x <= innen.width() {
        innen.center().x - galley.size().x / 2.0
    } else {
        innen.left()
    };
    painter
        .with_clip_rect(innen.intersect(painter.clip_rect()))
        .galley(
            pos2(x, innen.center().y - galley.size().y / 2.0),
            galley,
            schrift_farbe,
        );
}

struct Feld {
    text: String,
    markiert: bool,
    heute: bool,
    ausgegraut: bool,
}

/// 4 × 3 Felder für Jahres- und Jahrzehntansicht. Gibt den Index des angeklickten Felds zurück.
fn raster_ui(ui: &mut egui::Ui, felder: &[Feld]) -> Option<usize> {
    let groesse = vec2(ui.available_width(), ui.available_height().max(3.0 * 40.0));
    let (raster, _) = ui.allocate_exact_size(groesse, Sense::hover());
    let feld = Vec2::new(raster.width() / 4.0, raster.height() / 3.0);
    let mut geklickt = None;
    for (i, f) in felder.iter().enumerate() {
        let rect = Rect::from_min_size(
            raster.min + vec2(feld.x * (i % 4) as f32, feld.y * (i / 4) as f32),
            feld,
        )
        .shrink(3.0);
        let antwort = ui.interact(rect, ui.scope_id().with(("feld", i)), Sense::click());
        antwort.widget_info(|| egui::WidgetInfo::selected(Role::Button, true, f.markiert, &f.text));
        let painter = ui.painter();
        let rund = CornerRadius::same(RUNDUNG);
        if f.markiert {
            painter.rect_filled(rect, rund, farbe::AUSGEWAEHLT);
            painter.rect_stroke(
                rect,
                rund,
                Stroke::new(1.5, farbe::AKZENT),
                StrokeKind::Inside,
            );
        } else if antwort.hovered() {
            painter.rect_filled(rect, rund, farbe::MAUS);
        }
        let (schrift, text_farbe) = if f.heute {
            (FontId::proportional(16.0), farbe::AKZENT)
        } else if f.ausgegraut {
            (FontId::proportional(15.0), farbe::TEXT_BLASS)
        } else {
            (FontId::proportional(15.0), farbe::TEXT)
        };
        let text_rect = painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &f.text,
            schrift,
            text_farbe,
        );
        if f.heute {
            // Kleiner Punkt unter dem Text markiert den aktuellen Monat bzw. das aktuelle Jahr.
            painter.circle_filled(
                pos2(rect.center().x, text_rect.bottom() + 5.0),
                2.5,
                farbe::AKZENT,
            );
        }
        if antwort.clicked() {
            geklickt = Some(i);
        }
    }
    geklickt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(j: i32, m: u32, t: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(j, m, t).unwrap()
    }

    #[test]
    fn titel_und_blaettern() {
        let mut k = Kalender::new(d(2026, 11, 6));
        assert_eq!(k.titel(), "November 2026");
        k.blaettern(true);
        assert_eq!(k.titel(), "Dezember 2026");
        k.ansicht = Ansicht::Jahr;
        k.blaettern(true);
        assert_eq!(k.titel(), "2027");
        k.ansicht = Ansicht::Jahrzehnt;
        assert_eq!(k.titel(), "2020-2029");
        k.blaettern(false);
        assert_eq!(k.titel(), "2010-2019");
    }

    #[test]
    fn pills_umbrechen() {
        // 3 Kapseln à 20 px in 50 px Breite: zwei in Zeile 0, eine in Zeile 1.
        let plan = anordnen(&[20.0, 20.0, 20.0], 50.0, 2).unwrap();
        assert_eq!(plan, vec![(0.0, 0, 20.0), (22.0, 0, 20.0), (0.0, 1, 20.0)]);
        assert!(anordnen(&[20.0, 20.0, 20.0], 50.0, 1).is_none());
    }

    #[test]
    fn pill_farbe_haengt_nur_vom_kuerzel_ab() {
        assert_eq!(farbe::pill("SQ"), farbe::pill("SQ"));
        let verschieden: std::collections::HashSet<_> = ["SQ", "DL", "BP", "OHP", "Kz", "Jo"]
            .iter()
            .map(|k| farbe::pill(k))
            .collect();
        assert!(
            verschieden.len() >= 4,
            "Kürzel sollten sich farblich unterscheiden"
        );
    }
}
