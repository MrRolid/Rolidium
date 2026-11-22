#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Makes rolidium_<version>_amd64.deb and Rolidium-<version>-x86_64.AppImage from the staged Linux tree of package.sh.
# Usage: scripts/package-linux.sh <stage dir of the Linux package> <version> <out dir>
# Needs dpkg-deb and fakeroot (the .deb) and appimagetool (the AppImage; a copy in ~/.local/bin, see BUILDING.md). No root is needed.
set -euo pipefail
cd "$(dirname "$0")/.."
STAGE=${1:?stage dir}; VERSION=${2:?version}; OUT=${3:?out dir}
mkdir -p "$OUT"
SIZES="16 24 32 48 64 128 256 512"
WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT

# ---- the tree of files that goes under /usr (shared by the .deb and the AppDir)
usr_tree() {  # $1 = the directory that becomes /usr
    local u=$1
    mkdir -p "$u/bin" "$u/share/applications" "$u/share/mime/packages" "$u/share/doc/rolidium" "$u/share/icons/hicolor/scalable/apps" "$u/share/icons/hicolor/scalable/mimetypes"
    install -m 0755 "$STAGE/rolidium" "$u/bin/rolidium"
    ln -s rolidium "$u/bin/rolidium-cli"; ln -s rolidium "$u/bin/rolidium-mcp"
    sed 's|^Exec=.*|Exec=rolidium %f|' assets/desktop/rolidium.desktop > "$u/share/applications/rolidium.desktop"
    chmod 0644 "$u/share/applications/rolidium.desktop"
    install -m 0644 assets/linux/rolidium.xml "$u/share/mime/packages/rolidium.xml"
    for s in $SIZES; do
        mkdir -p "$u/share/icons/hicolor/${s}x${s}/apps" "$u/share/icons/hicolor/${s}x${s}/mimetypes"
        install -m 0644 "assets/appicon/png/rolidium-$s.png" "$u/share/icons/hicolor/${s}x${s}/apps/rolidium.png"
        install -m 0644 "assets/appicon/png/rolidium-$s.png" "$u/share/icons/hicolor/${s}x${s}/mimetypes/application-x-rolidium.png"
    done
    install -m 0644 assets/appicon/rolidium.svg "$u/share/icons/hicolor/scalable/apps/rolidium.svg"
    install -m 0644 assets/appicon/rolidium.svg "$u/share/icons/hicolor/scalable/mimetypes/application-x-rolidium.svg"
    install -m 0644 assets/legal/THIRD-PARTY-LICENSES.txt "$u/share/doc/rolidium/THIRD-PARTY-LICENSES.txt"
    install -m 0644 assets/legal/OCCT_LGPL_EXCEPTION.txt assets/legal/LICENSE_LGPL_21.txt "$u/share/doc/rolidium/"
    install -m 0644 docs/MCP.md "$u/share/doc/rolidium/MCP.md"
    cp -r examples "$u/share/doc/rolidium/examples"
    find "$u/share/doc/rolidium/examples" -type d -exec chmod 0755 {} + ; find "$u/share/doc/rolidium/examples" -type f -exec chmod 0644 {} +
    chmod 0755 "$u/share/doc/rolidium/examples/build-all.sh" 2>/dev/null || true
    find "$u/share/doc/rolidium/examples" -name __pycache__ -prune -exec rm -rf {} +
}

# ---- .deb
DEB=$WORK/deb
usr_tree "$DEB/usr"
install -m 0644 assets/linux/copyright "$DEB/usr/share/doc/rolidium/copyright"
# a native package: changelog.gz is in the format of Debian (the date of the commit being packaged); the changelog of the project is CHANGELOG.md.gz
{
    printf 'rolidium (%s) unstable; urgency=low\n\n  * Release %s. See CHANGELOG.md.gz and https://rolidium.com for what is in it.\n\n -- Rolid spol. s r.o. <connect@rolid.com>  %s\n' \
        "$VERSION" "$VERSION" "$(LC_ALL=C date -R -d "@$(git log -1 --format=%ct)")"
} | gzip -9n > "$DEB/usr/share/doc/rolidium/changelog.gz"
gzip -9n -c CHANGELOG.md > "$DEB/usr/share/doc/rolidium/CHANGELOG.md.gz"
chmod 0644 "$DEB/usr/share/doc/rolidium/changelog.gz" "$DEB/usr/share/doc/rolidium/CHANGELOG.md.gz"
find "$DEB" -type d -exec chmod 0755 {} +
mkdir -p "$DEB/DEBIAN"
SIZE_KB=$(du -sk --exclude=DEBIAN "$DEB" | cut -f1)
cat > "$DEB/DEBIAN/control" <<CTRL
Package: rolidium
Version: $VERSION
Architecture: amd64
Maintainer: Rolid spol. s r.o. <connect@rolid.com>
Installed-Size: $SIZE_KB
Depends: libc6 (>= 2.35), libstdc++6 (>= 12), libgcc-s1, libxkbcommon0, libx11-6 | libwayland-client0
Recommends: libvulkan1, mesa-vulkan-drivers | libegl1, libxcursor1, libxi6, libx11-xcb1, libxcb1, libxkbcommon-x11-0, libwayland-egl1, shared-mime-info
Section: graphics
Priority: optional
Homepage: https://rolidium.com
Description: Direct modeling CAD (Alpha)
 Rolidium is a free, open source direct-modeling CAD program: sketch on a plane, then push and pull faces. There is no feature tree. It reads
 and writes STEP and STL, has a command-line mode (rolidium-cli) and an MCP server (rolidium-mcp) for AI assistants.
 .
 The program is one file. The window needs a graphics driver with Vulkan or OpenGL 3.3 (without one it draws by software, slowly).
CTRL
install -m 0755 assets/linux/postinst assets/linux/postrm "$DEB/DEBIAN/"
DEBFILE="$OUT/rolidium_${VERSION}_amd64.deb"
fakeroot dpkg-deb --root-owner-group -Zxz -z9 --build "$DEB" "$DEBFILE" >/dev/null
echo "$DEBFILE"
if command -v lintian >/dev/null 2>&1; then lintian --no-tag-display-limit "$DEBFILE" || true; else echo "(lintian is not installed: the .deb was not checked by it)"; fi

# ---- AppImage
APP=$WORK/Rolidium.AppDir
usr_tree "$APP/usr"
find "$APP" -type d -exec chmod 0755 {} +
install -m 0755 assets/linux/AppRun "$APP/AppRun"
cp "$APP/usr/share/applications/rolidium.desktop" "$APP/rolidium.desktop"
echo "X-AppImage-Version=$VERSION" >> "$APP/rolidium.desktop"
cp assets/appicon/png/rolidium-256.png "$APP/rolidium.png"; cp "$APP/rolidium.png" "$APP/.DirIcon"
TOOL=${APPIMAGETOOL:-$(command -v appimagetool || echo "$HOME/.local/bin/appimagetool")}
RUNTIME=${APPIMAGE_RUNTIME:-$HOME/.local/share/appimage-runtime-x86_64}
if [ -x "$TOOL" ] && [ -f "$RUNTIME" ]; then
    IMG="$OUT/Rolidium-${VERSION}-x86_64.AppImage"
    # --appimage-extract-and-run: the tool itself is an AppImage and needs no FUSE this way
    ARCH=x86_64 "$TOOL" --appimage-extract-and-run --runtime-file "$RUNTIME" "$APP" "$IMG" >/dev/null 2>&1
    echo "$IMG"
else
    echo "appimagetool or its runtime file not found: no AppImage (see BUILDING.md)" >&2; exit 1
fi
