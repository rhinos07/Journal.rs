//! Erzeugt das App-Icon in `crates/journal-app/assets/`:
//! `icon-<größe>.png` (16 … 512) und `icon.ico` (16, 24, 32, 48, 64, 256).
//!
//! Aufruf: `cargo run -p journal-app --example icon`
//!
//! Ein Kalenderblatt in den Farben der App: blauer Kopf mit zwei Ringen, darunter ein Raster mit
//! einem grünen Badge („Eintrag vorhanden“) und einem gelben Feld („heute“). Jede Größe wird
//! direkt gerendert (4 × 4 Supersampling), nicht herunterskaliert.

use std::path::Path;

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::{ExtendedColorType, Rgba, RgbaImage};

type Farbe = [u8; 4];

const RAHMEN: Farbe = [0x2F, 0x5E, 0x9E, 255];
const KOPF: Farbe = [0x3A, 0x72, 0xC0, 255];
const PAPIER: Farbe = [255, 255, 255, 255];
const FELD: Farbe = [0xDF, 0xE8, 0xF5, 255];
const BADGE: Farbe = [0xDF, 0xFF, 0xF5, 255];
const BADGE_RAHMEN: Farbe = [0x00, 0x60, 0x00, 255];
const HEUTE: Farbe = [0xF7, 0xD2, 0x75, 255];
const HEUTE_RAHMEN: Farbe = [0xE0, 0x9A, 0x20, 255];
const RING: Farbe = [0xE8, 0xEE, 0xF8, 255];

/// Abgerundetes Rechteck in Einheiten eines 256er-Rasters.
#[derive(Clone, Copy)]
struct Form {
    x: f32,
    y: f32,
    b: f32,
    h: f32,
    r: f32,
}

impl Form {
    const fn new(x: f32, y: f32, b: f32, h: f32, r: f32) -> Self {
        Self { x, y, b, h, r }
    }

    fn enthaelt(&self, px: f32, py: f32) -> bool {
        let (cx, cy) = (self.x + self.b / 2.0, self.y + self.h / 2.0);
        let qx = (px - cx).abs() - (self.b / 2.0 - self.r);
        let qy = (py - cy).abs() - (self.h / 2.0 - self.r);
        let aussen = qx.max(0.0).hypot(qy.max(0.0));
        aussen + qx.max(qy).min(0.0) <= self.r
    }

    fn schrumpfen(&self, d: f32) -> Self {
        Self::new(
            self.x + d,
            self.y + d,
            self.b - 2.0 * d,
            self.h - 2.0 * d,
            (self.r - d).max(0.0),
        )
    }
}

/// Ebenen von unten nach oben: (Form, Farbe).
fn ebenen() -> Vec<(Form, Farbe)> {
    let blatt = Form::new(14.0, 26.0, 228.0, 216.0, 30.0);
    let mut e = vec![
        (blatt, RAHMEN),
        (blatt.schrumpfen(8.0), PAPIER),
        // Kopf: oben abgerundet, unten gerade (zweites Rechteck deckt die untere Rundung ab).
        (Form::new(22.0, 34.0, 212.0, 52.0, 22.0), KOPF),
        (Form::new(22.0, 60.0, 212.0, 26.0, 0.0), KOPF),
        // Ringe
        (Form::new(66.0, 10.0, 22.0, 48.0, 11.0), RAHMEN),
        (Form::new(72.0, 16.0, 10.0, 36.0, 5.0), RING),
        (Form::new(168.0, 10.0, 22.0, 48.0, 11.0), RAHMEN),
        (Form::new(174.0, 16.0, 10.0, 36.0, 5.0), RING),
    ];
    // Raster 3 × 3 Felder
    let (x0, y0, b, h, abstand) = (36.0, 100.0, 54.0, 34.0, 9.0);
    for zeile in 0..3 {
        for spalte in 0..3 {
            let x = x0 + spalte as f32 * (b + abstand);
            let y = y0 + zeile as f32 * (h + abstand);
            let feld = Form::new(x, y, b, h, 7.0);
            match (zeile, spalte) {
                (0, 1) | (2, 0) => {
                    e.push((feld, BADGE_RAHMEN));
                    e.push((feld.schrumpfen(5.0), BADGE));
                }
                (1, 2) => {
                    e.push((feld, HEUTE_RAHMEN));
                    e.push((feld.schrumpfen(5.0), HEUTE));
                }
                _ => e.push((feld, FELD)),
            }
        }
    }
    e
}

fn rendern(groesse: u32) -> RgbaImage {
    const SS: u32 = 4;
    let ebenen = ebenen();
    let massstab = 256.0 / groesse as f32;
    let mut bild = RgbaImage::new(groesse, groesse);
    for (x, y, pixel) in bild.enumerate_pixels_mut() {
        // Vormultipliziert mitteln, damit Kanten nicht dunkel werden.
        let mut summe = [0.0f32; 4];
        for sy in 0..SS {
            for sx in 0..SS {
                let px = (x as f32 + (sx as f32 + 0.5) / SS as f32) * massstab;
                let py = (y as f32 + (sy as f32 + 0.5) / SS as f32) * massstab;
                if let Some((_, f)) = ebenen.iter().rev().find(|(form, _)| form.enthaelt(px, py)) {
                    let a = f[3] as f32 / 255.0;
                    for k in 0..3 {
                        summe[k] += f[k] as f32 * a;
                    }
                    summe[3] += a;
                }
            }
        }
        let n = (SS * SS) as f32;
        let alpha = summe[3] / n;
        *pixel = if alpha > 0.0 {
            Rgba([
                (summe[0] / summe[3]).round() as u8,
                (summe[1] / summe[3]).round() as u8,
                (summe[2] / summe[3]).round() as u8,
                (alpha * 255.0).round() as u8,
            ])
        } else {
            Rgba([0, 0, 0, 0])
        };
    }
    bild
}

fn main() {
    let ziel = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    std::fs::create_dir_all(&ziel).unwrap();

    for groesse in [16, 32, 64, 128, 256, 512] {
        rendern(groesse)
            .save(ziel.join(format!("icon-{groesse}.png")))
            .unwrap();
    }

    let frames: Vec<_> = [16, 24, 32, 48, 64, 256]
        .into_iter()
        .map(|g| {
            let bild = rendern(g);
            IcoFrame::as_png(bild.as_raw(), g, g, ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let datei = std::fs::File::create(ziel.join("icon.ico")).unwrap();
    IcoEncoder::new(datei).encode_images(&frames).unwrap();

    println!("Icons geschrieben nach {}", ziel.display());
}
