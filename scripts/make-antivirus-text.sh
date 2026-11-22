#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Writes dist/release/antivirus/false-positive-report.txt: an English text for reporting a false alarm to an antivirus vendor, with the SHA-256 checksums
# of the Windows zip and of rolidium.exe inside it. Called by package.sh after the packages are made.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
hash=$(git rev-parse --short=7 HEAD)
zip="dist/release/rolidium-$version-windows-x86_64.zip"
out=dist/release/antivirus; mkdir -p "$out"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
unzip -q "$zip" "rolidium-$version/rolidium.exe" -d "$tmp"
zsum=$(sha256sum "$zip" | cut -d' ' -f1); esum=$(sha256sum "$tmp/rolidium-$version/rolidium.exe" | cut -d' ' -f1)
esize=$(stat -c %s "$tmp/rolidium-$version/rolidium.exe")
cat > "$out/false-positive-report.txt" <<TXT
False positive report: Rolidium $version for Windows

Product:      Rolidium $version (build $hash), a 64-bit Windows program
Publisher:    Rolid spol. s r.o.
Contact:      connect@rolid.com
Web:          https://rolidium.com   (downloads: https://rolidium.com/download, privacy notice: https://rolidium.com/privacy)
Source code:  https://github.com/MrRolid/Rolidium   (open source, GNU AGPL-3.0-only; the program can be built from the source, see BUILDING.md)

Files and checksums (SHA-256)
  $(basename "$zip")
      $zsum
  rolidium-$version/rolidium.exe inside the zip ($esize bytes)
      $esum

What the program is
  Rolidium is a direct-modeling CAD program for mechanical parts (3D models for 3D printing and manufacturing), written in Rust with the Open CASCADE geometry kernel
  (C++, linked statically) and an egui/wgpu window. It is one executable with three modes: the window (default), "rolidium.exe cli" (scripts without a window) and
  "rolidium.exe mcp" (a local server for AI assistants over standard input and output). It is cross-compiled on Linux with mingw-w64 (target x86_64-pc-windows-gnu).
  The program is NOT signed with a code-signing certificate (it is new and the publisher has none yet), and it is not packed or obfuscated.

What it does with the disk
  * It reads and writes only the files the user chooses in its own file dialog (.rolidium documents, STEP and STL files) and, for scripts, the folder given on the command line.
  * Settings: %APPDATA%\\rolidium. The log and autosave copies: %LOCALAPPDATA%\\rolidium. Small temporary files in %TEMP% (names start with "rolidium-"; they may stay until the temporary folder is cleaned): a copy of a file of the diagnostics that
    the user opens from the window, and the working folders of the optional graphics test.
  * It does not write to the registry, install a service or a scheduled task, add itself to the start-up programs, change other programs, inject code or ask for administrator rights.
    It reads the processor name and the Windows version from the registry (read only) and with system calls, to put them in the diagnostics the user may send.
  * It starts other programs only on request of the user: Windows Explorer to open a folder or a file (for example the saved diagnostics), the web browser for the links in the About window,
    and a copy of itself ("rolidium.exe --diag-child ...") with a time limit, to try a graphics driver in a separate process when the user presses Save diagnostics.

What it does with the network
  * Nothing, until the user agrees. The commands cli and mcp, and the option ROLIDIUM_OFFLINE=1, never use the network.
  * After the user answers "Yes" to a question at the first start, the window asks https://rolidium.com once a day in the background whether a newer version exists. The request carries the version of the
    program, the name and main number of the system and the architecture, and nothing else (no identifier, no cookies, no file names). The program downloads and starts nothing.
  * Only when the user presses Send in the panel "Report a bug or an idea", the program sends the text of the report, the optional e-mail, the version and the optional files that the user saw listed
    (the diagnostics and a picture of the program's own window) to https://rolidium.com/api/diagnostics.
  * Details: https://rolidium.com/privacy (also docs/PRIVACY.md in the source).

Request
  Please check the files above and remove the detection (or the warning of the reputation service) for rolidium.exe and the zip. If you need anything else (a build from the source, the build
  script, the list of the libraries and their licences, THIRD-PARTY-LICENSES.txt), write to connect@rolid.com.
TXT
echo "$out/false-positive-report.txt"
