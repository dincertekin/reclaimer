#!/bin/sh
# Installs Reclaimer for the current user: binary, icon and menu entry.
set -eu
cd "$(dirname "$0")/../.."

cargo build --release
install -Dm755 target/release/Reclaimer        "$HOME/.local/bin/Reclaimer"
install -Dm644 icons/icon.png                   "$HOME/.local/share/icons/hicolor/512x512/apps/reclaimer.png"
install -Dm644 packaging/linux/reclaimer.desktop "$HOME/.local/share/applications/reclaimer.desktop"
echo "Installed. Reclaimer is in your applications menu; make sure ~/.local/bin is on your PATH."
