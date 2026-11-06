//! Kalender mit Monats-, Jahres- und Jahrzehntansicht (Spezifikation F6, E8).

use chrono::{Datelike, Days, Months, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Rect, RichText, Role, Sense, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};
use journal_core::de;
use journal_core::index::Index;

/// Farben aus der C#-Version (F6). Sie sind KANN, wichtig ist nur die Unterscheidbarkeit.
mod farbe {
    use eframe::egui::Color32;

    pub const KOPF: Color32 = Color32::from_rgb(0xDF, 0xE8, 0xF5);
    pub const RAHMEN: Color32 = Color32::from_rgb(0x5D, 0x8C, 0xC9);
    pub const HEUTE_KOPF: Color32 = Color32::from_rgb(0xF7, 0xD2, 0x75);
    pub const HEUTE_RAHMEN: Color32 = Color32::from_rgb(0xF3, 0xB8, 0x4B);
    pub const AUSGEWAEHLT: Color32 = Color32::from_rgb(0xBA, 0xDD, 0xE9);
    pub const ANDERER_MONAT: Color32 = Color32::from_rgb(0xE9, 0xED, 0xF5);
    pub const ANDERER_TEXT: Color32 = Color32::from_rgb(0x77, 0x77, 0x77);
    pub const BADGE_RAHMEN: Color32 = Color32::from_rgb(0x00, 0x60, 0x00);
    pub const BADGE: Color32 = Color32::from_rgb(0xDF, 0xFF, 0xF5);
    pub const WARNUNG: Color32 = Color32::from_rgb(0xC0, 0x60, 0x00);
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
                FontId::proportional(13.0),
                ui.visuals().text_color(),
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
            .shrink(1.0);
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
    heute: bool,
    ausgewaehlt: bool,
    maus: bool,
}

fn zelle_zeichnen(
    ui: &egui::Ui,
    rect: Rect,
    datum: NaiveDate,
    zustand: ZellenZustand,
    badge: Option<&str>,
    konflikte: usize,
) {
    let painter = ui.painter_at(rect);
    let hintergrund = if zustand.ausgewaehlt {
        farbe::AUSGEWAEHLT
    } else if zustand.anderer_monat {
        farbe::ANDERER_MONAT
    } else {
        Color32::WHITE
    };
    painter.rect_filled(rect, CornerRadius::ZERO, hintergrund);
    if zustand.maus && !zustand.ausgewaehlt {
        painter.rect_filled(
            rect,
            CornerRadius::ZERO,
            farbe::AUSGEWAEHLT.gamma_multiply(0.5),
        );
    }

    let kopf = Rect::from_min_size(rect.min, vec2(rect.width(), 18.0_f32.min(rect.height())));
    let kopf_farbe = if zustand.heute {
        farbe::HEUTE_KOPF
    } else {
        farbe::KOPF
    };
    painter.rect_filled(kopf, CornerRadius::ZERO, kopf_farbe);
    let text_farbe = if zustand.anderer_monat {
        farbe::ANDERER_TEXT
    } else {
        Color32::BLACK
    };
    painter.text(
        kopf.left_center() + vec2(4.0, 0.0),
        Align2::LEFT_CENTER,
        datum.day(),
        FontId::proportional(13.0),
        text_farbe,
    );
    if konflikte > 0 {
        painter.text(
            kopf.right_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            "⚠",
            FontId::proportional(13.0),
            farbe::WARNUNG,
        );
    }

    if let Some(badge) = badge {
        let oben = kopf.bottom() + 4.0;
        let hoehe = 20.0_f32.min(rect.bottom() - oben - 2.0);
        if hoehe > 6.0 {
            let kasten = Rect::from_min_size(
                pos2(rect.left() + 4.0, oben),
                vec2(rect.width() - 8.0, hoehe),
            );
            painter.rect(
                kasten,
                CornerRadius::same(4),
                farbe::BADGE,
                Stroke::new(1.0, farbe::BADGE_RAHMEN),
                StrokeKind::Inside,
            );
            // Passt der Text nicht, linksbündig, damit der Anfang lesbar bleibt (ganzer Text im
            // Tooltip der Zelle).
            let innen = kasten.shrink2(vec2(4.0, 2.0));
            let text_painter = ui.painter_at(innen);
            let galley = text_painter.layout_no_wrap(
                badge.to_owned(),
                FontId::proportional(13.0),
                Color32::BLACK,
            );
            let x = if galley.size().x <= innen.width() {
                innen.center().x - galley.size().x / 2.0
            } else {
                innen.left()
            };
            let y = innen.center().y - galley.size().y / 2.0;
            text_painter.galley(pos2(x, y), galley, Color32::BLACK);
        }
    }

    let rahmen = if zustand.heute {
        Stroke::new(2.0, farbe::HEUTE_RAHMEN)
    } else {
        Stroke::new(0.5, farbe::RAHMEN)
    };
    painter.rect_stroke(rect, CornerRadius::ZERO, rahmen, StrokeKind::Inside);
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
        let hintergrund = if f.markiert {
            farbe::AUSGEWAEHLT
        } else if f.ausgegraut {
            farbe::ANDERER_MONAT
        } else {
            Color32::WHITE
        };
        painter.rect_filled(rect, CornerRadius::same(4), hintergrund);
        if antwort.hovered() && !f.markiert {
            painter.rect_filled(
                rect,
                CornerRadius::same(4),
                farbe::AUSGEWAEHLT.gamma_multiply(0.5),
            );
        }
        let rahmen = if f.heute {
            Stroke::new(2.0, farbe::HEUTE_RAHMEN)
        } else {
            Stroke::new(0.5, farbe::RAHMEN)
        };
        painter.rect_stroke(rect, CornerRadius::same(4), rahmen, StrokeKind::Inside);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &f.text,
            FontId::proportional(15.0),
            if f.ausgegraut {
                farbe::ANDERER_TEXT
            } else {
                Color32::BLACK
            },
        );
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
}
