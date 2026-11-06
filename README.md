# Trainingstagebuch (Rust)

Neuimplementierung der WPF-App aus `D:\Sourcen\Journal.core`.
Pro Tag eine Textdatei `YYYY-MM-DD.txt` in einem Ordner (z. B. OneDrive), dazu ein Monatskalender
mit Kürzeln für die Trainingsarten.

- Spezifikation: [docs/SPEZIFIKATION.md](docs/SPEZIFIKATION.md)
- `crates/journal-core`: Logik ohne GUI, getestet mit `cargo test`
- `crates/journal-app`: Oberfläche

## Bauen

```
cargo build
cargo test
cargo run -p journal-app
```
