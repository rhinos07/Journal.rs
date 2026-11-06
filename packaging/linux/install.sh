#!/bin/sh
# Installiert das Tagebuch für den aktuellen Benutzer (ohne root).
# Entfernen: dieselben drei Dateien wieder löschen.
set -e
cd "$(dirname "$0")"
install -Dm755 trainingstagebuch "$HOME/.local/bin/trainingstagebuch"
install -Dm644 trainingstagebuch.desktop "$HOME/.local/share/applications/trainingstagebuch.desktop"
install -Dm644 trainingstagebuch.png "$HOME/.local/share/icons/hicolor/256x256/apps/trainingstagebuch.png"
echo "Installiert. Start über das Anwendungsmenü oder mit: trainingstagebuch"
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "Hinweis: $HOME/.local/bin ist nicht im PATH." ;;
esac
