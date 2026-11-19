#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Adds Rolidium to the application menu and gives the panel its icon (current user only, no root needed).
# Run it from the folder the package was unpacked to:  ./install-desktop-entry.sh
# Remove again with:  ./install-desktop-entry.sh --remove
set -e
here=$(cd "$(dirname "$0")" && pwd)
data="${XDG_DATA_HOME:-$HOME/.local/share}"
if [ "$1" = "--remove" ]; then
    rm -f "$data/applications/rolidium.desktop"
    for s in 16 24 32 48 64 128 256 512; do rm -f "$data/icons/hicolor/${s}x${s}/apps/rolidium.png"; done
    rm -f "$data/icons/hicolor/scalable/apps/rolidium.svg"
    echo "Rolidium menu entry removed."
    exit 0
fi
for s in 16 24 32 48 64 128 256 512; do
    mkdir -p "$data/icons/hicolor/${s}x${s}/apps"
    cp "$here/icons/rolidium-$s.png" "$data/icons/hicolor/${s}x${s}/apps/rolidium.png"
done
mkdir -p "$data/icons/hicolor/scalable/apps" "$data/applications"
cp "$here/icons/rolidium.svg" "$data/icons/hicolor/scalable/apps/rolidium.svg"
sed "s|@EXEC@|$here/rolidium|" "$here/rolidium.desktop" > "$data/applications/rolidium.desktop"
chmod +x "$data/applications/rolidium.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
echo "Rolidium was added to the application menu (log out and in again if the icon does not show at once)."
