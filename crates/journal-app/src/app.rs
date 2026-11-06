//! Hauptfenster (Spezifikation Kap. 5–7): Kalender oben, Editor in der Mitte, Buttonleiste unten.
//!
//! Die Logik steckt in `journal_core`. Hier wird nur gezeichnet, Eingaben werden weitergereicht,
//! und die [`Wirkung`] einer Aktion wird umgesetzt.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Days, Local, NaiveDate};
use eframe::egui::{
    self, Color32, Id, Key, KeyboardShortcut, Modifiers, RichText, TextEdit, ViewportCommand,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
};
use journal_core::config::{self, Geladen};
use journal_core::dokument::{Cursor, Dokument, LoeschSchritt, RUECKFRAGE_ENDGUELTIG, Wirkung};
use journal_core::index::{Index, IndexCache};
use journal_core::storage::LoeschFehler;
use journal_core::watcher::{Aenderung, Ueberwachung};

use crate::kalender::Kalender;

/// Nach so langer Zeit ohne Tastendruck wird geänderter Text gespeichert (Kap. 12.4).
const AUTO_SAVE: Duration = Duration::from_secs(5);
/// So lange bleibt ein Hinweis in der Statuszeile stehen.
const HINWEIS_DAUER: Duration = Duration::from_secs(8);
/// Nicht lesbare Dateien (z. B. von OneDrive gesperrt) nach dieser Zeit erneut lesen (F3).
const ERNEUT_LESEN: Duration = Duration::from_secs(5);

const SPEICHERN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const EDITOR: &str = "editor";

/// Alles, was vor dem Öffnen des Fensters feststeht.
pub struct Start {
    pub config: Geladen,
    /// Gültiger Journal-Ordner oder `None` (F1, Schritt 3).
    pub root: Option<PathBuf>,
    /// Meldungen, die nach dem Start nacheinander angezeigt werden.
    pub meldungen: Vec<String>,
    /// Fester Tag für Tests, sonst `None` = heute.
    pub heute: Option<NaiveDate>,
    /// Ordnerauswahl (F2). In Tests ersetzbar.
    pub ordner_waehlen: Box<dyn FnMut() -> Option<PathBuf>>,
}

impl Start {
    /// Programmstart F1, Schritte 1–3: Konfiguration laden, bei Bedarf nach dem Ordner fragen.
    pub fn laden() -> Self {
        let pfad =
            config::standard_pfad().unwrap_or_else(|| PathBuf::from("trainingstagebuch.toml"));
        let mut config = config::laden(&pfad);
        let mut meldungen = Vec::new();
        if let Some(fehler) = &config.fehler {
            meldungen.push(format!(
                "Die Konfigurationsdatei ist fehlerhaft. Es gelten die Standardwerte, \
                 die Datei wird nicht verändert.\n\n{}\n{fehler}",
                pfad.display()
            ));
        }
        for w in &config.warnungen {
            meldungen.push(format!("{}:\n{w}", pfad.display()));
        }

        let mut root = config.config.gueltiger_root().map(Path::to_owned);
        if root.is_none() {
            root = ordner_dialog();
            match &root {
                Some(r) => {
                    if let Err(e) = config.root_path_speichern(r) {
                        meldungen.push(format!("Der Ordner konnte nicht gespeichert werden:\n{e}"));
                    }
                }
                None => meldungen.push(
                    "Kein Journal-Ordner gewählt. Über „Ordner…“ kannst du jederzeit einen wählen."
                        .to_owned(),
                ),
            }
        }

        Self {
            config,
            root,
            meldungen,
            heute: None,
            ordner_waehlen: Box::new(ordner_dialog),
        }
    }
}

/// Nativer Dialog „Ordner auswählen“ (F2).
fn ordner_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Journal-Ordner auswählen")
        .pick_folder()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Dialog {
    Meldung(String),
    /// Rückfrage vor dem Löschen (F12, Schritt 3).
    Loeschen(String),
    /// Kein Papierkorb, endgültig löschen? (F12, Schritt 5)
    Endgueltig,
    /// Speichern beim Beenden fehlgeschlagen (F15, Schritt 4).
    Beenden(String),
}

impl Dialog {
    fn text(&self) -> String {
        match self {
            Self::Meldung(t) | Self::Loeschen(t) => t.clone(),
            Self::Endgueltig => RUECKFRAGE_ENDGUELTIG.to_owned(),
            Self::Beenden(e) => format!("Der Eintrag konnte nicht gespeichert werden:\n{e}"),
        }
    }

    /// Buttons, der letzte gilt auch für Escape.
    fn buttons(&self) -> &'static [&'static str] {
        match self {
            Self::Meldung(_) => &["OK"],
            Self::Loeschen(_) | Self::Endgueltig => &["Ja", "Nein"],
            Self::Beenden(_) => &["Erneut versuchen", "Ohne Speichern beenden", "Abbrechen"],
        }
    }
}

/// Baut den Index in einem Hintergrund-Thread (F3, Empfehlung).
struct IndexBauer {
    cache: Arc<Mutex<IndexCache>>,
    tx: Sender<(PathBuf, io::Result<Index>)>,
    rx: Receiver<(PathBuf, io::Result<Index>)>,
    laeuft: bool,
    nochmal: bool,
}

impl IndexBauer {
    fn new(cache: IndexCache) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            cache: Arc::new(Mutex::new(cache)),
            tx,
            rx,
            laeuft: false,
            nochmal: false,
        }
    }

    fn anstossen(&mut self, root: &Path, ctx: &egui::Context) {
        if self.laeuft {
            self.nochmal = true;
            return;
        }
        self.laeuft = true;
        let (cache, tx, root, ctx) = (
            Arc::clone(&self.cache),
            self.tx.clone(),
            root.to_owned(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let ergebnis = cache
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner())
                .aufbauen(&root);
            let _ = tx.send((root, ergebnis));
            ctx.request_repaint();
        });
    }

    fn abholen(&mut self) -> Option<(PathBuf, io::Result<Index>)> {
        let ergebnis = self.rx.try_recv().ok()?;
        self.laeuft = false;
        Some(ergebnis)
    }
}

pub struct TagebuchApp {
    config: Geladen,
    pub(crate) doc: Dokument,
    pub(crate) kalender: Kalender,
    heute_fest: Option<NaiveDate>,
    pub(crate) index: Index,
    bauer: IndexBauer,
    ueberwachung: Option<Ueberwachung>,
    aenderung_tx: Sender<Aenderung>,
    aenderung_rx: Receiver<Aenderung>,
    dialoge: VecDeque<Dialog>,
    status: Option<(String, Instant)>,
    cursor_ausstehend: Option<Cursor>,
    /// Zeile und Spalte des Cursors im letzten Frame, für [`Cursor::Behalten`].
    cursor_zeile_spalte: (usize, usize),
    letzte_eingabe: Option<Instant>,
    erneut_lesen_um: Option<Instant>,
    fenster_fokus: bool,
    darf_schliessen: bool,
    ordner_waehlen: Box<dyn FnMut() -> Option<PathBuf>>,
    ctx: egui::Context,
}

impl TagebuchApp {
    pub fn new(ctx: &egui::Context, start: Start) -> Self {
        // Die Farben von Kalender und Badges sind für einen hellen Hintergrund gemacht.
        ctx.set_theme(egui::Theme::Light);

        let heute = start.heute.unwrap_or_else(|| Local::now().date_naive());
        let mut doc = Dokument::new(start.config.config.template.clone(), None);
        let _ = doc.tag_waehlen(heute);
        let (aenderung_tx, aenderung_rx) = mpsc::channel();
        let mut app = Self {
            bauer: IndexBauer::new(IndexCache::new(start.config.config.regeln.clone())),
            config: start.config,
            doc,
            kalender: Kalender::new(heute),
            heute_fest: start.heute,
            index: Index::default(),
            ueberwachung: None,
            aenderung_tx,
            aenderung_rx,
            dialoge: start.meldungen.into_iter().map(Dialog::Meldung).collect(),
            status: None,
            cursor_ausstehend: None,
            cursor_zeile_spalte: (0, 0),
            letzte_eingabe: None,
            erneut_lesen_um: None,
            fenster_fokus: true,
            darf_schliessen: false,
            ordner_waehlen: start.ordner_waehlen,
            ctx: ctx.clone(),
        };
        if let Some(root) = start.root {
            app.ordner_aktivieren(root, false);
        }
        app
    }

    /// Läuft gerade ein Index-Aufbau im Hintergrund? (für Tests)
    #[cfg(test)]
    pub(crate) fn index_laeuft(&self) -> bool {
        self.bauer.laeuft
    }

    fn heute(&self) -> NaiveDate {
        self.heute_fest.unwrap_or_else(|| Local::now().date_naive())
    }

    // -- Aktionen ------------------------------------------------------------------------------

    /// Wirkung einer Aktion umsetzen.
    fn anwenden(&mut self, wirkung: Wirkung) {
        if let Some(c) = wirkung.editor {
            self.cursor_ausstehend = Some(c);
            self.letzte_eingabe = None;
        }
        if let Some(h) = wirkung.hinweise.last() {
            self.hinweis(h.clone());
        }
        if wirkung.dateien_geaendert {
            self.index_neu();
        }
    }

    fn hinweis(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), Instant::now()));
    }

    fn meldung(&mut self, text: impl Into<String>) {
        self.dialoge.push_back(Dialog::Meldung(text.into()));
    }

    fn index_neu(&mut self) {
        if let Some(root) = self.doc.root().map(Path::to_owned) {
            self.bauer.anstossen(&root, &self.ctx);
        }
    }

    /// Ordner registrieren (F1 Schritt 4, F13): Tag laden, Überwachung, Index.
    fn ordner_aktivieren(&mut self, root: PathBuf, config_speichern: bool) {
        let wirkung = match self.doc.ordner_setzen(root.clone()) {
            Ok(w) => w,
            Err(e) => {
                self.meldung(format!("Der Ordner konnte nicht geöffnet werden:\n{e}"));
                return;
            }
        };
        if config_speichern && let Err(e) = self.config.root_path_speichern(&root) {
            self.meldung(format!(
                "Der Ordner konnte nicht in der Konfiguration gespeichert werden:\n{e}"
            ));
        }

        self.ueberwachung = None;
        let (tx, ctx) = (self.aenderung_tx.clone(), self.ctx.clone());
        match Ueberwachung::starten(&root, move |a| {
            let _ = tx.send(a);
            ctx.request_repaint();
        }) {
            Ok(u) => self.ueberwachung = Some(u),
            Err(e) => self.meldung(format!(
                "Die Ordnerüberwachung konnte nicht gestartet werden. Änderungen von außen \
                 erscheinen erst nach einem Neustart.\n\n{e}"
            )),
        }
        self.index = Index::default();
        self.kalender.zeige(self.doc.datum());
        self.anwenden(wirkung);
    }

    /// F13 bzw. F10 ohne Ordner.
    fn ordner_wechseln(&mut self) {
        if let Some(root) = (self.ordner_waehlen)() {
            self.ordner_aktivieren(root, true);
        }
    }

    /// F7 auslösen.
    pub(crate) fn tag_waehlen(&mut self, datum: NaiveDate) {
        match self.doc.tag_waehlen(datum) {
            Ok(w) => {
                self.kalender.zeige(datum);
                self.anwenden(w);
            }
            Err(e) => self.meldung(format!("Der Tag konnte nicht gewechselt werden:\n{e}")),
        }
    }

    /// F10.
    fn speichern(&mut self) {
        if self.doc.root().is_none() {
            self.ordner_wechseln();
            return;
        }
        match self.doc.speichern() {
            Ok(w) => {
                let konflikt = !w.hinweise.is_empty();
                self.anwenden(w);
                if !konflikt {
                    self.hinweis("Gespeichert.");
                }
            }
            Err(e) => self.meldung(format!("Der Eintrag konnte nicht gespeichert werden:\n{e}")),
        }
    }

    /// F12, Schritt 1.
    fn loeschen(&mut self) {
        match self.doc.loeschen_pruefen() {
            Ok(LoeschSchritt::Nichts) => {}
            Ok(LoeschSchritt::OhneRueckfrage) => {
                let w = self.doc.auf_vorlage_zuruecksetzen();
                self.anwenden(w);
            }
            Ok(LoeschSchritt::Rueckfrage(text)) => self.dialoge.push_back(Dialog::Loeschen(text)),
            Err(e) => self.meldung(format!("Der Ordner ist nicht lesbar:\n{e}")),
        }
    }

    fn loesch_ergebnis(&mut self, ergebnis: Result<Wirkung, LoeschFehler>, papierkorb: bool) {
        match ergebnis {
            Ok(w) => {
                self.anwenden(w);
                self.hinweis(if papierkorb {
                    "In den Papierkorb verschoben."
                } else {
                    "Endgültig gelöscht."
                });
            }
            Err(LoeschFehler::PapierkorbNichtVerfuegbar(_)) => {
                self.dialoge.push_front(Dialog::Endgueltig);
            }
            Err(LoeschFehler::Sonstiger(e)) => {
                self.meldung(format!("Der Eintrag konnte nicht gelöscht werden:\n{e}"));
            }
        }
    }

    /// F15: Speichern und schließen, bei Fehler nachfragen.
    fn beenden_versuchen(&mut self, ctx: &egui::Context) {
        match self.doc.speichern_wenn_geaendert() {
            Ok(_) => {
                self.darf_schliessen = true;
                self.ueberwachung = None;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Err(e) => {
                if !matches!(self.dialoge.front(), Some(Dialog::Beenden(_))) {
                    self.dialoge.push_front(Dialog::Beenden(e.to_string()));
                }
            }
        }
    }

    fn dialog_antwort(&mut self, dialog: Dialog, button: usize, ctx: &egui::Context) {
        match (dialog, button) {
            (Dialog::Loeschen(_), 0) => {
                let r = self.doc.in_papierkorb();
                self.loesch_ergebnis(r, true);
            }
            (Dialog::Endgueltig, 0) => {
                let r = self.doc.endgueltig_loeschen();
                self.loesch_ergebnis(r, false);
            }
            (Dialog::Beenden(_), 0) => self.beenden_versuchen(ctx),
            (Dialog::Beenden(_), 1) => {
                self.darf_schliessen = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            _ => {}
        }
    }

    // -- Ereignisse außerhalb der Oberfläche ---------------------------------------------------

    fn hintergrund(&mut self, ctx: &egui::Context) {
        // Fertige Index-Läufe übernehmen.
        while let Some((root, ergebnis)) = self.bauer.abholen() {
            if self.doc.root() == Some(root.as_path()) {
                match ergebnis {
                    Ok(index) => {
                        if let Some((pfad, fehler)) = index.lesefehler.first() {
                            self.hinweis(format!(
                                "{} ist gerade nicht lesbar ({fehler}), neuer Versuch folgt.",
                                pfad.file_name().unwrap_or_default().to_string_lossy()
                            ));
                            self.erneut_lesen_um = Some(Instant::now() + ERNEUT_LESEN);
                        }
                        self.index = index;
                    }
                    Err(e) => self.hinweis(format!("Der Journal-Ordner ist nicht lesbar: {e}")),
                }
            }
            if self.bauer.nochmal {
                self.bauer.nochmal = false;
                self.index_neu();
            }
        }

        // Ordnerüberwachung (F5, F16 A).
        let mut neu_einlesen = false;
        let mut pruefen = false;
        while let Ok(a) = self.aenderung_rx.try_recv() {
            if self.doc.root() != Some(a.root.as_path()) {
                continue; // verspätet, von einem früheren Ordner
            }
            neu_einlesen = true;
            pruefen |= a.betrifft(self.doc.datum());
        }
        // Fenster bekommt den Fokus zurück (F16 A, verpasste Ereignisse).
        let fokus = ctx.input(|i| i.focused);
        if fokus && !self.fenster_fokus {
            pruefen = true;
            neu_einlesen = true;
        }
        self.fenster_fokus = fokus;
        if let Some(um) = self.erneut_lesen_um {
            if Instant::now() >= um {
                self.erneut_lesen_um = None;
                neu_einlesen = true;
            } else {
                ctx.request_repaint_after(um - Instant::now());
            }
        }
        if neu_einlesen {
            self.index_neu();
        }
        if pruefen {
            let w = self.doc.pruefen();
            self.anwenden(w);
        }

        // Automatisch speichern nach einigen Sekunden ohne Tastendruck (12.4).
        if let Some(t) = self.letzte_eingabe {
            let rest = AUTO_SAVE.saturating_sub(t.elapsed());
            if !self.doc.geaendert() {
                self.letzte_eingabe = None;
            } else if rest.is_zero() {
                self.letzte_eingabe = None;
                match self.doc.speichern_wenn_geaendert() {
                    Ok(w) => self.anwenden(w),
                    Err(e) => self.hinweis(format!("Automatisches Speichern fehlgeschlagen: {e}")),
                }
            } else {
                ctx.request_repaint_after(rest);
            }
        }

        // Hinweis ausblenden.
        if let Some((_, seit)) = &self.status {
            let rest = HINWEIS_DAUER.saturating_sub(seit.elapsed());
            if rest.is_zero() {
                self.status = None;
            } else {
                ctx.request_repaint_after(rest);
            }
        }

        // Beenden (F15).
        if ctx.input(|i| i.viewport().close_requested()) && !self.darf_schliessen {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.beenden_versuchen(ctx);
        }
    }

    // -- Oberfläche ----------------------------------------------------------------------------

    fn tastatur(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SPEICHERN)) {
            self.speichern();
        }
        // Tag wechseln, auch wenn der Editor den Fokus hat (Kap. 7, Empfehlung).
        let datum = self.doc.datum();
        let ziel = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::ALT, Key::ArrowLeft) {
                datum.pred_opt()
            } else if i.consume_key(Modifiers::ALT, Key::ArrowRight) {
                datum.succ_opt()
            } else if i.consume_key(Modifiers::ALT, Key::ArrowUp) {
                datum.checked_sub_days(Days::new(7))
            } else if i.consume_key(Modifiers::ALT, Key::ArrowDown) {
                datum.checked_add_days(Days::new(7))
            } else if i.consume_key(Modifiers::ALT, Key::Home) {
                Some(self.heute())
            } else {
                None
            }
        });
        if let Some(ziel) = ziel {
            self.tag_waehlen(ziel);
        }
    }

    fn leiste(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let aktiv = self.doc.aktiv();
            if ui
                .button("Ordner…")
                .on_hover_text("Journal-Ordner wählen")
                .clicked()
            {
                self.ordner_wechseln();
            }
            let kuerzel = ui.ctx().format_shortcut(&SPEICHERN);
            if ui
                .add_enabled(aktiv, egui::Button::new("Speichern"))
                .on_hover_text(format!("Eintrag speichern ({kuerzel})"))
                .clicked()
            {
                self.speichern();
            }
            if ui
                .add_enabled(aktiv, egui::Button::new("Rückgängig"))
                .on_hover_text("Änderungen seit dem letzten Laden bzw. Speichern verwerfen")
                .clicked()
            {
                let w = self.doc.rueckgaengig();
                self.anwenden(w);
            }
            if ui
                .add_enabled(aktiv, egui::Button::new("Löschen"))
                .on_hover_text("Eintrag in den Papierkorb verschieben")
                .clicked()
            {
                self.loeschen();
            }
            if let Some((text, _)) = &self.status {
                ui.add_space(8.0);
                ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            }
        });
        ui.add_space(4.0);
    }

    fn konflikt_leiste(&mut self, ui: &mut egui::Ui) {
        let Some(konflikt) = self.doc.konflikt() else {
            return;
        };
        let mut extern_laden = false;
        let mut eigene_speichern = false;
        egui::Frame::new()
            .fill(Color32::from_rgb(0xFF, 0xF1, 0xA8))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0xD8, 0xB0, 0x30)))
            .corner_radius(3)
            .inner_margin(6)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(konflikt.meldung()).color(Color32::BLACK));
                    extern_laden = ui.button(konflikt.button_extern()).clicked();
                    eigene_speichern = ui.button(konflikt.button_eigen()).clicked();
                });
            });
        ui.add_space(4.0);
        if extern_laden {
            match self.doc.externe_laden() {
                Ok(w) => self.anwenden(w),
                Err(e) => self.meldung(format!(
                    "Die externe Version konnte nicht geladen werden:\n{e}"
                )),
            }
        }
        if eigene_speichern {
            self.speichern();
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let id = Id::unique(EDITOR);
        let ctx = ui.ctx().clone();
        if let Some(cursor) = self.cursor_ausstehend.take() {
            let mut state = TextEditState::load(&ctx, id).unwrap_or_default();
            // Sonst holt Strg+Z den Text des vorherigen Tages zurück.
            state.clear_undoer();
            let zeichen = match cursor {
                Cursor::EndeMitFokus => self.doc.text().chars().count(),
                Cursor::Behalten => zeichen_index(self.doc.text(), self.cursor_zeile_spalte),
            };
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(zeichen))));
            state.store(&ctx, id);
            if cursor == Cursor::EndeMitFokus {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }

        let aktiv = self.doc.aktiv();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let ausgabe = TextEdit::multiline(self.doc.text_mut())
                    .id(id)
                    .interactive(aktiv)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .hint_text("Kein Journal-Ordner gewählt.")
                    .show(ui);
                if ausgabe.response.response.changed() {
                    self.letzte_eingabe = Some(Instant::now());
                }
                if let Some(bereich) = ausgabe.cursor_range {
                    self.cursor_zeile_spalte =
                        zeile_spalte(self.doc.text(), usize::from(bereich.primary.index));
                }
            });
    }

    fn dialog_zeigen(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialoge.front().cloned() else {
            return;
        };
        let buttons = dialog.buttons();
        let mut wahl = None;
        egui::Modal::new(Id::unique("dialog")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading("Tagebuch");
            ui.add_space(6.0);
            ui.label(dialog.text());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                for (i, b) in buttons.iter().enumerate() {
                    if ui.button(*b).clicked() {
                        wahl = Some(i);
                    }
                }
            });
        });
        if wahl.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            wahl = Some(buttons.len() - 1);
        }
        if let Some(i) = wahl {
            self.dialoge.pop_front();
            self.dialog_antwort(dialog, i, ctx);
        }
    }
}

impl eframe::App for TagebuchApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.hintergrund(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.dialoge.is_empty() {
            self.tastatur(&ctx);
        }

        egui::Panel::bottom("leiste").show(ui, |ui| self.leiste(ui));

        let hoehe = (ui.available_height() * 0.6).max(400.0);
        egui::Panel::top("kalender")
            .exact_size(hoehe)
            .show(ui, |ui| {
                let pfeiltasten = self.dialoge.is_empty() && ui.memory(|m| m.focused().is_none());
                let (datum, heute) = (self.doc.datum(), self.heute());
                if let Some(neu) = self.kalender.ui(ui, datum, heute, &self.index, pfeiltasten)
                    && neu != datum
                {
                    self.tag_waehlen(neu);
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.konflikt_leiste(ui);
            self.editor(ui);
        });

        self.dialog_zeigen(&ctx);
    }
}

/// Zeile und Spalte (beide ab 0, in Zeichen) eines Zeichenindex.
fn zeile_spalte(text: &str, zeichen: usize) -> (usize, usize) {
    let (mut zeile, mut spalte) = (0, 0);
    for c in text.chars().take(zeichen) {
        if c == '\n' {
            zeile += 1;
            spalte = 0;
        } else {
            spalte += 1;
        }
    }
    (zeile, spalte)
}

/// Zeichenindex zu Zeile und Spalte, gekürzt auf Zeilen- bzw. Textende (F16 B3).
fn zeichen_index(text: &str, (zeile, spalte): (usize, usize)) -> usize {
    let mut index = 0;
    for (nr, inhalt) in text.split('\n').enumerate() {
        let laenge = inhalt.chars().count();
        if nr == zeile {
            return index + spalte.min(laenge);
        }
        index += laenge + 1;
    }
    text.chars().count()
}

#[cfg(test)]
mod oberflaeche;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_umrechnen() {
        let text = "ab\ncde\n\nf";
        assert_eq!(zeile_spalte(text, 0), (0, 0));
        assert_eq!(zeile_spalte(text, 4), (1, 1));
        assert_eq!(zeile_spalte(text, 7), (2, 0));
        assert_eq!(zeichen_index(text, (1, 1)), 4);
        assert_eq!(zeichen_index(text, (1, 99)), 6);
        assert_eq!(zeichen_index(text, (99, 0)), 9);
        assert_eq!(zeichen_index("", (3, 3)), 0);
        for i in 0..=text.chars().count() {
            assert_eq!(zeichen_index(text, zeile_spalte(text, i)), i);
        }
    }
}
