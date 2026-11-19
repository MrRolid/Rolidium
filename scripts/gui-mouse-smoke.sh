#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Starts Rolidium in a virtual X server and drives it with real mouse/keyboard events.
# Usage: scripts/gui-mouse-smoke.sh <output-dir> [script]   (default script: gui_mouse_smoke.py;
#        needs xvfb, ImageMagick, libXtst). Set ROLIDIUM_BIN to test another binary, LANG for a language, ROLIDIUM_SETTINGS for a settings file.
set -euo pipefail
OUT=${1:?output directory}
SCRIPT=${2:-gui_mouse_smoke.py}
mkdir -p "$OUT"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${ROLIDIUM_BIN:-$ROOT/target/debug/rolidium}
[ -n "${ROLIDIUM_SOUND_TEST:-}" ] || export ROLIDIUM_NO_SOUND=1
H=$(mktemp -d)
# ROLIDIUM_SETTINGS=<file>: settings the program starts with (theme, language, ...)
if [ -n "${ROLIDIUM_SETTINGS:-}" ]; then mkdir -p "$H/rolidium"; cp "$ROLIDIUM_SETTINGS" "$H/rolidium/settings.conf"; fi
xvfb-run -a -s "-screen 0 1280x800x24" bash -c "
  HOME='$H' XDG_CONFIG_HOME='$H' '$BIN' ${ROLIDIUM_ARGS:---demo} >'$OUT/app.log' 2>&1 &
  APP=\$!
  sleep 6
  python3 '$ROOT/scripts/$SCRIPT' '$OUT'
  kill \$APP 2>/dev/null || true
  cp '$H'/.local/state/rolidium/rolidium.log '$OUT/rolidium.log' 2>/dev/null || true
"
