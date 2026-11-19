#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# File handling with real input: save, autosave, crash, recovery. Usage: scripts/gui-files-smoke.sh <output-dir>
set -euo pipefail
OUT=${1:?output directory}
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
mkdir -p "$OUT"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${ROLIDIUM_BIN:-$ROOT/target/release/rolidium}
export ROLIDIUM_NO_SOUND=1 ROLIDIUM_AUTOSAVE_SECS=2
H=$(mktemp -d -p "$ROOT/target/tmp")
export ROLIDIUM_AUTOSAVE_DIR=$H/auto
mkdir -p "$H/work"
xvfb-run -a -s "-screen 0 1280x800x24" bash -c "
  cd '$H/work'
  HOME='$H' XDG_CONFIG_HOME='$H' '$BIN' --demo >'$OUT/app1.log' 2>&1 &
  APP=\$!
  sleep 6
  python3 '$ROOT/scripts/gui_mouse_smoke_files.py' '$OUT'
  kill -9 \$APP
  sleep 1
  ls -la '$H/work' '$H/auto'
  HOME='$H' XDG_CONFIG_HOME='$H' '$BIN' >'$OUT/app2.log' 2>&1 &
  APP=\$!
  sleep 6
  python3 '$ROOT/scripts/gui_mouse_smoke_files2.py' '$OUT'
  kill \$APP 2>/dev/null || true
"
