#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Builds the two packages from a clean committed state: dist/rolidium-<version>-linux-x86_64.tar.gz and
# dist/rolidium-<version>-windows-x86_64.zip, plus the copies dist/rolidium-test.tar.gz and dist/rolidium-test-windows.zip.
# Needs the Linux build tools and the Windows cross-build tools (docs/WINDOWS.md). Set ALLOW_DIRTY=1 to package an uncommitted tree.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -n "$(git status --porcelain --untracked-files=no)" ] && [ "${ALLOW_DIRTY:-0}" != 1 ]; then
  echo "the working tree has uncommitted changes: commit first (or ALLOW_DIRTY=1)" >&2; exit 1
fi
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
hash=$(git rev-parse --short=7 HEAD)
echo "packaging $version $hash"
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
python3 - "$version" <<'PY'
import sys, os, tarfile, zipfile, shutil
v = sys.argv[1]
os.chdir("dist")
lin = f"rolidium-{v}-linux-x86_64.tar.gz"
with tarfile.open(lin, "w:gz", compresslevel=9) as t:
    t.add(f"stage/rolidium-{v}", arcname=f"rolidium-{v}")
win = f"rolidium-{v}-windows-x86_64.zip"
with zipfile.ZipFile(win, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    root = f"stage/rolidium-{v}-windows"
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            z.write(p, os.path.join(f"rolidium-{v}", os.path.relpath(p, root)))
shutil.copyfile(lin, "rolidium-test.tar.gz")
shutil.copyfile(win, "rolidium-test-windows.zip")
for f in (lin, win, "rolidium-test.tar.gz", "rolidium-test-windows.zip"):
    print(f, os.path.getsize(f))
PY
(cd dist && sha256sum rolidium-"$version"-linux-x86_64.tar.gz rolidium-"$version"-windows-x86_64.zip | tee SHA256SUMS)
