#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# The scripted window tests: each test in scripts/gui_tests/*.py starts its own copy of the program in a virtual X server (Xvfb), opens a model made
# for it (the `# FIXTURE:` line of the test: commands of `rolidium cli` separated by `;`), drives the window with synthetic mouse and keyboard events,
# reads what the program says (the probe, `# POINTS:` are the points of the model whose screen place the test needs) and saves screenshots into
# target/gui-shots/. Usage: scripts/gui-tests.sh [test-name ...]   (default: all). Needs xvfb, ImageMagick and libXtst.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${ROLIDIUM_BIN:-$ROOT/target/debug/rolidium}
SHOTS=${GUI_SHOTS:-$ROOT/target/gui-shots}
mkdir -p "$SHOTS" "$ROOT/target/tmp"
export ROLIDIUM_NO_SOUND=1
if [ $# -gt 0 ]; then TESTS=("$@"); else TESTS=($(cd "$ROOT/scripts/gui_tests" && ls *.py | sed 's/\.py$//')); fi
fail=0
for t in "${TESTS[@]}"; do
  py="$ROOT/scripts/gui_tests/$t.py"
  [ -f "$py" ] || { echo "no test $t"; fail=1; continue; }
  W=$(mktemp -d -p "$ROOT/target/tmp" "gui-$t-XXXX")
  FIXTURE=$(sed -n 's/^# FIXTURE: *//p' "$py" | head -1)
  POINTS=$(sed -n 's/^# POINTS: *//p' "$py" | head -1)
  MODEL="$W/model.rolidium"
  if [ -n "$FIXTURE" ]; then
    IFS=';' read -ra CMDS <<< "$FIXTURE"
    ARGS=()
    for c in "${CMDS[@]}"; do ARGS+=("$(echo "$c" | sed 's/^ *//; s/ *$//')"); done
    "$BIN" cli exec "${ARGS[@]}" --file "$MODEL" >"$W/fixture.log" 2>&1 || { echo "FIXTURE FAILED for $t"; cat "$W/fixture.log"; fail=1; continue; }
  fi
  echo "== $t"
  mkdir -p "$W/rolidium"; printf 'check_updates=no\nsound=false\nlanguage=en\n' > "$W/rolidium/settings.conf"
  # the HOME is empty: no settings, no autosave from other runs
  xvfb-run -a -s "-screen 0 1280x800x24" bash -c "
    export HOME='$W' XDG_CONFIG_HOME='$W' XDG_STATE_HOME='$W/state' XDG_DATA_HOME='$W/data'
    export ROLIDIUM_PROBE='$W/probe.json' ROLIDIUM_PROBE_POINTS='$POINTS' GUI_SHOTS='$SHOTS' ROLIDIUM_MODEL='$MODEL' ROLIDIUM_WORK='$W'
    '$BIN' '$MODEL' >'$W/app.log' 2>&1 &
    APP=\$!
    for i in \$(seq 1 80); do [ -s '$W/probe.json' ] && break; sleep 0.25; done
    sleep 3
    PYTHONPATH='$ROOT/scripts' python3 '$py'
    RC=\$?
    kill \$APP 2>/dev/null; wait \$APP 2>/dev/null
    exit \$RC
  " || { echo "TEST FAILED: $t (log: $W/app.log)"; fail=1; }
done
[ $fail -eq 0 ] && echo "ALL WINDOW TESTS PASSED" || echo "SOME WINDOW TESTS FAILED"
exit $fail
