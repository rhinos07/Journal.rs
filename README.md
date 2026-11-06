# Trainingstagebuch (Rust)

Neuimplementierung der WPF-App aus `D:\Sourcen\Journal.core`, für Windows, macOS und Linux.
Pro Tag eine Textdatei `YYYY-MM-DD.txt` in einem Ordner (z. B. OneDrive, Dropbox, Nextcloud,
Syncthing), dazu ein Monatskalender mit Kürzeln für die Trainingsarten.

- Spezifikation: [docs/SPEZIFIKATION.md](docs/SPEZIFIKATION.md)
- `crates/journal-core`: Logik ohne GUI (Dateien, Index, Kürzel, Konflikte, Überwachung)
- `crates/journal-app`: Oberfläche mit egui/eframe

## Bauen und Starten

Voraussetzung: Rust (stable), unter Windows zusätzlich Visual Studio mit
„Desktopentwicklung mit C++“.

```
cargo run -p journal-app             # Debug-Version starten
cargo build --release -p journal-app # target/release/trainingstagebuch(.exe)
```

Beim ersten Start fragt die App nach dem Journal-Ordner. **Zum Ausprobieren eine Kopie** des
echten Ordners nehmen.

## Tests

```
cargo test --workspace                                  # alle automatischen Tests
cargo test -p journal-app -- --ignored bildschirmfotos  # PNGs nach target/bildschirmfotos/
cargo test -p journal-core -- --ignored papierkorb_echt # legt eine Testdatei in den Papierkorb
```

GitHub Actions führt Formatierung, Clippy und Tests auf Windows, macOS und Linux aus
(`.github/workflows/ci.yml`).

## Pakete

Ein Git-Tag `v*` (z. B. `git tag v1.0.0 && git push --tags`) baut über
`.github/workflows/release.yml` einen Release-Entwurf mit:

| System | Datei | Installation |
|---|---|---|
| Windows | `Tagebuch-windows-x64.zip` | `trainingstagebuch.exe` irgendwo ablegen und starten |
| macOS | `Tagebuch-macos.zip` (Apple Silicon und Intel) | `Tagebuch.app` nach „Programme“ ziehen. Unsigniert: beim ersten Start Rechtsklick → „Öffnen“ |
| Linux | `Tagebuch-linux-x64.tar.gz` | entpacken, `./install.sh` (installiert nach `~/.local`) |

Das App-Icon erzeugt `cargo run -p journal-app --example icon` neu (`crates/journal-app/assets/`).

## Konfiguration

`config.toml` liegt unter

- Windows: `%APPDATA%\Trainingstagebuch\config.toml`
- macOS: `~/Library/Application Support/Trainingstagebuch/config.toml`
- Linux: `~/.config/trainingstagebuch/config.toml`

Sie wird beim ersten Start mit Kommentaren angelegt. Darin stehen der Journal-Ordner, das Design, die Vorlage
für neue Tage (Motto und Übung je Wochentag) und die Kürzel-Regeln. Änderungen wirken nach einem
Neustart (Spezifikation Kap. 4).

## Bedienung

| Taste | Wirkung |
|---|---|
| `Strg+S` (macOS `Cmd+S`) | Speichern |
| `Alt+←` / `Alt+→` | Vortag / Folgetag |
| `Alt+↑` / `Alt+↓` | eine Woche zurück / vor |
| `Alt+Pos1` oder Knopf „Heute“ | Heute |
| Klick auf den Monatstitel | Jahresansicht, noch ein Klick: Jahrzehnt |

Die App folgt dem hellen oder dunklen Design des Betriebssystems. Unten rechts lässt es sich fest
auf hell (☀) oder dunkel (🌙) stellen oder zurück auf „wie das System“ (💻), die Wahl wird in
`config.toml` gespeichert.

Gespeichert wird automatisch beim Tageswechsel, nach 5 Sekunden ohne Tastendruck und beim
Schließen. Ändert ein anderes Gerät den offenen Eintrag, lädt die App ihn neu oder zeigt bei
eigenen Änderungen eine gelbe Leiste. Dann wird nie etwas überschrieben, die andere Version landet
als `YYYY-MM-DD.konflikt-….txt` daneben (⚠ im Kalender).
