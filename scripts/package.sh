#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Builds all packages from a clean committed state: rolidium-<version>-linux-x86_64.tar.gz, rolidium_<version>_amd64.deb,
# Rolidium-<version>-x86_64.AppImage, rolidium-<version>-windows-x86_64.zip, rolidium-demos-<version>.zip and SHA256SUMS for all of them.
# One command: scripts/package.sh [release|dev]
#   release  into dist/release/, only from a clean checkout of exactly the tag v<version> (the program then reports "<version>"); it refuses otherwise.
#   dev      into dist/dev/, any commit; the files are named <version>-dev.<commit> and the program reports "<version>-dev <commit>".
#   (no argument: release when HEAD is exactly the tag, dev otherwise.) dist/release/ is never touched by a dev build.
# Needs the Linux build tools, the Windows cross-build tools (docs/WINDOWS.md), dpkg-deb + fakeroot and appimagetool (BUILDING.md), and the demos
# built by examples/demos/build-all.sh (target/demos). Set ALLOW_DIRTY=1 to package an uncommitted tree.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -n "$(git status --porcelain --untracked-files=no)" ] && [ "${ALLOW_DIRTY:-0}" != 1 ]; then
  echo "the working tree has uncommitted changes: commit first (or ALLOW_DIRTY=1)" >&2; exit 1
fi
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
hash=$(git rev-parse --short=7 HEAD)
clean=yes; [ -n "$(git status --porcelain --untracked-files=no)" ] && clean=no
exact=no; [ "$(git describe --exact-match --tags HEAD 2>/dev/null || true)" = "v$version" ] && [ $clean = yes ] && exact=yes
mode=${1:-auto}
[ "$mode" = auto ] && { [ $exact = yes ] && mode=release || mode=dev; }
case "$mode" in
  release) [ $exact = yes ] || { echo "release packages are made only from a clean checkout of exactly the tag v$version" >&2; exit 1; }
           OUT=dist/release; fver=$version; debver=$version ;;
  dev)     OUT=dist/dev; fver="$version-dev.$hash"; debver="$version+dev.$hash" ;;
  *)       echo "usage: scripts/package.sh [release|dev]" >&2; exit 1 ;;
esac
echo "packaging $version $hash into $OUT (mode $mode)"
export PACKAGE_OUT=$OUT PACKAGE_FVER=$fver
cargo build --release -p rolidium-app
cargo build --release --target x86_64-pc-windows-gnu -p rolidium-app
rm -rf dist/stage && mkdir -p dist/stage
L=dist/stage/rolidium-$version
W=dist/stage/rolidium-$version-windows
mkdir -p "$L/docs" "$W/docs"
# ---- Linux
cp target/release/rolidium "$L/"
ln -s rolidium "$L/rolidium-cli"
ln -s rolidium "$L/rolidium-mcp"
cp -r assets/lang "$L/lang"; mkdir -p "$L/icons"
cp assets/appicon/png/* assets/appicon/rolidium.svg "$L/icons/"
cp assets/icons-preview.html "$L/"
cp assets/desktop/install-desktop-entry.sh assets/desktop/rolidium.desktop "$L/"
cp LICENSE assets/legal/THIRD-PARTY-LICENSES.txt CHANGELOG.md "$L/"
cp docs/MCP.md "$L/docs/"; cp -r docs/images "$L/docs/images"; cp -r examples "$L/examples"
cat > "$L/README.txt" <<EOF
Rolidium $version ($hash), Alpha. Developed by Martin Vorel, published by Rolid spol. s r.o. (connect@rolid.com). Licence: AGPL-3.0-only (LICENSE).
Third-party licences (Open CASCADE, libraries, fonts): THIRD-PARTY-LICENSES.txt, also reachable from About in the window.
Start:  ./rolidium            (empty document; --demo = a box; or give a .rolidium file).   Version: ./rolidium --version
Menu entry and icon in the panel: ./install-desktop-entry.sh   (undo with --remove).
One program, three modes: ./rolidium (window), ./rolidium cli ... (scripts; link ./rolidium-cli), ./rolidium mcp ... (MCP server for AI assistants; link ./rolidium-mcp).
Connect the MCP server to an AI client: ./rolidium mcp-config   prints ready-made commands with the real path of this folder (docs/MCP.md).
Examples: ./rolidium cli run examples/01-bracket.txt      Times of the main operations: ./rolidium cli times
Needs glibc >= 2.35, libstdc++ with GLIBCXX_3.4.30 (Linux Mint 21 or newer) and a Vulkan driver (the cli and mcp modes need none).
Bug reports and ideas: https://github.com/MrRolid/Rolidium/issues  (About > Save diagnostics for graphics problems).
EOF
# ---- Windows
cp target/x86_64-pc-windows-gnu/release/rolidium.exe "$W/"
cp LICENSE assets/legal/THIRD-PARTY-LICENSES.txt CHANGELOG.md "$W/"
cp docs/MCP.md "$W/docs/"; cp scripts/windows-scenario.txt "$W/"; cp -r examples "$W/examples"
cat > "$W/README.txt" <<EOF
Rolidium $version ($hash) for 64-bit Windows, Alpha. Developed by Martin Vorel, published by Rolid spol. s r.o. (connect@rolid.com).
Licence: AGPL-3.0-only (LICENSE). Third-party licences: THIRD-PARTY-LICENSES.txt (also reachable from About in the window).
rolidium.exe is one program with three modes: the window (default), "rolidium.exe cli ..." (scripts, no window) and "rolidium.exe mcp --root C:\\models"
(MCP server for AI assistants, no window and no graphics card needed, see docs\\MCP.md). "rolidium.exe mcp-config" prints ready-made commands that connect the
server to an AI client, with the real path of this folder.
No extra DLLs are needed. The window needs a GPU driver with Vulkan or DirectX 12.
If something is not drawn: About > Save diagnostics (or "rolidium.exe --diagnose") and send the zip.
Another graphics system: Settings > Graphics backend, or "rolidium.exe --backend vulkan" / "gl" / "dx12".
Settings: %APPDATA%\\rolidium, autosave and log: %LOCALAPPDATA%\\rolidium. The error tone is silent.
Bug reports and ideas: https://github.com/MrRolid/Rolidium/issues
EOF
# ---- archives (python: symlinks in the tar, plain files in the zip)
rm -rf "$OUT" && mkdir -p "$OUT"
python3 - "$version" "$fver" "${OUT#dist/}" <<'PY'
import sys, os, tarfile, zipfile
v, fv, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir("dist")
lin = f"{out}/rolidium-{fv}-linux-x86_64.tar.gz"
with tarfile.open(lin, "w:gz", compresslevel=9) as t:
    t.add(f"stage/rolidium-{v}", arcname=f"rolidium-{v}")
win = f"{out}/rolidium-{fv}-windows-x86_64.zip"
with zipfile.ZipFile(win, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    root = f"stage/rolidium-{v}-windows"
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            z.write(p, os.path.join(f"rolidium-{v}", os.path.relpath(p, root)))
for f in (lin, win):
    print(f, os.path.getsize(f))
PY
# ---- the .deb and the AppImage
scripts/package-linux.sh "$L" "$debver" "$OUT" "$fver"
# ---- the demos
scripts/package-demos.sh
# ---- the text for reporting a false alarm to antivirus vendors
scripts/make-antivirus-text.sh
# ---- checksums of everything
(cd "$OUT" && rm -f SHA256SUMS && sha256sum rolidium-"$fver"-linux-x86_64.tar.gz rolidium_"$debver"_amd64.deb Rolidium-"$fver"-x86_64.AppImage \
    rolidium-"$fver"-windows-x86_64.zip rolidium-demos-"$fver".zip | tee SHA256SUMS)
