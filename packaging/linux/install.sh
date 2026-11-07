#!/bin/sh
# Installs Reclaimer for the current user: program, icon and menu entry.
#
# From a release download, run it from the unpacked folder: it installs the
# program that came with it. From a copy of the source code, it builds first.
set -eu
here=$(cd "$(dirname "$0")" && pwd)

if [ -x "$here/Reclaimer" ]; then
    program="$here/Reclaimer"
    icon="$here/reclaimer.png"
    entry="$here/reclaimer.desktop"
else
    cd "$here/../.."
    cargo build --release
    program=target/release/Reclaimer
    icon=icons/icon.png
    entry=packaging/linux/reclaimer.desktop
fi

install -Dm755 "$program" "$HOME/.local/bin/Reclaimer"
install -Dm644 "$icon"    "$HOME/.local/share/icons/hicolor/512x512/apps/reclaimer.png"
install -Dm644 "$entry"   "$HOME/.local/share/applications/reclaimer.desktop"
echo "Installed. Reclaimer is in your applications menu; make sure ~/.local/bin is on your PATH."
