//! Windows: Fenster beim Wechsel auf einen Monitor mit anderer Skalierung richtig umsetzen.
//!
//! winit 0.30 behält bei `WM_DPICHANGED` die logische Innengröße bei (der Inhalt wird passend
//! skaliert), ignoriert aber das Rechteck, das Windows vorschlägt, und verankert das Fenster
//! links oben. Liegt das größer gewordene Fenster danach überwiegend wieder auf dem alten Monitor,
//! meldet Windows erneut einen DPI-Wechsel, und das Fenster springt zurück.
//!
//! Abhilfe über eine Hülle um die Fensterprozedur (`SetWindowSubclass`):
//! 1. `WM_GETDPISCALEDSIZE`: Windows fragt vorab, wie groß das Fenster mit der neuen Skalierung
//!    werden soll. Die Antwort ist genau die Größe, die auch winit ausrechnet (gleiche logische
//!    Innengröße plus Rahmen bei neuer DPI). Damit platziert Windows das vorgeschlagene Rechteck
//!    passend unter dem Mauszeiger auf dem neuen Monitor.
//! 2. `WM_DPICHANGED`: winit verarbeitet die Nachricht wie bisher (Skalierung, Ereignis an egui),
//!    danach wird das Fenster auf das von Windows vorgeschlagene Rechteck gesetzt.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetWindowLongW, IsZoomed, SWP_NOACTIVATE, SWP_NOZORDER,
    SetWindowPos, WM_DPICHANGED, WM_GETDPISCALEDSIZE, WM_NCDESTROY,
};

/// Kennung dieser Hülle (beliebig, eindeutig pro Fensterprozedur).
const KENNUNG: usize = 0x5441_4742; // "TAGB"

/// Installiert die Korrektur für das Fenster. Ohne Fenster-Handle (z. B. in Tests) passiert nichts.
pub fn installieren(fenster: &impl HasWindowHandle) {
    let Ok(handle) = fenster.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    // SAFETY: `hwnd` ist das gültige Hauptfenster dieses Threads, `huelle` hat die von
    // `SUBCLASSPROC` verlangte Signatur und entfernt sich bei `WM_NCDESTROY` selbst.
    unsafe {
        SetWindowSubclass(hwnd, Some(huelle), KENNUNG, 0);
    }
}

/// Neue Innengröße wie in winit: logische Größe bei alter Skalierung, physisch bei neuer.
fn skalieren(pixel: i32, alt_dpi: u32, neu_dpi: u32) -> i32 {
    let alt = f64::from(alt_dpi) / 96.0;
    let neu = f64::from(neu_dpi) / 96.0;
    (f64::from(pixel) / alt * neu).round() as i32
}

/// Äußere Fenstergröße für die neue DPI, oder `None`, wenn Windows selbst rechnen soll.
///
/// # Safety
/// `hwnd` muss ein gültiges Fenster sein.
unsafe fn groesse_fuer_dpi(hwnd: HWND, neu_dpi: u32) -> Option<SIZE> {
    // SAFETY: laut Aufrufvertrag gültiges Fenster; `innen` ist ein gültiger Ausgabezeiger.
    unsafe {
        if IsZoomed(hwnd) != 0 {
            return None;
        }
        let alt_dpi = GetDpiForWindow(hwnd);
        if alt_dpi == 0 || neu_dpi == 0 {
            return None;
        }
        let mut innen = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(hwnd, &mut innen) == 0 {
            return None;
        }
        let mut aussen = RECT {
            left: 0,
            top: 0,
            right: skalieren(innen.right - innen.left, alt_dpi, neu_dpi),
            bottom: skalieren(innen.bottom - innen.top, alt_dpi, neu_dpi),
        };
        let stil = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let stil_ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if AdjustWindowRectExForDpi(&mut aussen, stil, 0, stil_ex, neu_dpi) == 0 {
            return None;
        }
        Some(SIZE {
            cx: aussen.right - aussen.left,
            cy: aussen.bottom - aussen.top,
        })
    }
}

unsafe extern "system" fn huelle(
    hwnd: HWND,
    nachricht: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _kennung: usize,
    _daten: usize,
) -> LRESULT {
    match nachricht {
        WM_GETDPISCALEDSIZE => {
            // SAFETY: gültiges Fenster; `lparam` zeigt laut Win32-Dokumentation auf ein SIZE,
            // das die Anwendung füllen darf, wenn sie TRUE zurückgibt.
            unsafe {
                match groesse_fuer_dpi(hwnd, wparam as u32) {
                    Some(groesse) => {
                        *(lparam as *mut SIZE) = groesse;
                        1
                    }
                    None => DefSubclassProc(hwnd, nachricht, wparam, lparam),
                }
            }
        }
        WM_DPICHANGED => {
            // SAFETY: Weitergabe an die ursprüngliche Prozedur (winit) mit unveränderten Werten.
            let ergebnis = unsafe { DefSubclassProc(hwnd, nachricht, wparam, lparam) };
            // SAFETY: Bei WM_DPICHANGED zeigt `lparam` laut Win32-Dokumentation auf ein RECT,
            // das bis zum Ende der Nachrichtenverarbeitung gültig ist.
            let r = unsafe { &*(lparam as *const RECT) };
            // SAFETY: einfacher Win32-Aufruf mit gültigem Fenster.
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            ergebnis
        }
        WM_NCDESTROY => {
            // SAFETY: Abmelden derselben Hülle mit derselben Kennung, dann weiterreichen.
            unsafe {
                RemoveWindowSubclass(hwnd, Some(huelle), KENNUNG);
                DefSubclassProc(hwnd, nachricht, wparam, lparam)
            }
        }
        // SAFETY: alle anderen Nachrichten unverändert weiterreichen.
        _ => unsafe { DefSubclassProc(hwnd, nachricht, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::skalieren;

    #[test]
    fn skalieren_wie_winit() {
        assert_eq!(skalieren(500, 96, 144), 750);
        assert_eq!(skalieren(750, 144, 96), 500);
        assert_eq!(skalieren(600, 120, 96), 480);
    }
}
