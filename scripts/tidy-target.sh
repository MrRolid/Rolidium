#!/usr/bin/env bash
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Keeps target/ small (aim: under ~25 GB). Run at the end of every work step; `--deep` also removes what is slow to rebuild.
# Only files that can be rebuilt are removed. Never touches dist/, target/tmp/prechodka.rolidium or anything outside the repo.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
T="$ROOT/target"
LIMIT_GB=${TARGET_LIMIT_GB:-25}
size_gb() { du -sk "$T" 2>/dev/null | awk '{printf "%d", $1/1048576}'; }
before=$(size_gb)
# 1. leftovers of tests and packaging (everything in target/tmp older than an hour, except the user's file)
find "$T/tmp" -mindepth 1 -maxdepth 1 ! -name prechodka.rolidium -mmin +60 -exec rm -rf {} + 2>/dev/null
# 2. incremental caches and old test binaries (cargo keeps one copy per hash; only the newest of each name is useful)
rm -rf "$T/debug/incremental" "$T/release/incremental" "$T/x86_64-pc-windows-gnu/debug/incremental" "$T/x86_64-pc-windows-gnu/release/incremental"
for d in "$T/debug/deps" "$T/release/deps"; do
  [ -d "$d" ] || continue
  ls -t "$d" | grep -E '^[a-z0-9_]+-[0-9a-f]{16}$' | sed -E 's/-[0-9a-f]{16}$//' | sort -u | while read -r n; do
    ls -t "$d"/"$n"-???????????????? 2>/dev/null | tail -n +2 | while read -r f; do [ -x "$f" ] && [ ! -d "$f" ] && rm -f "$f" "$f.d"; done
  done
done
# 3. old window-test screenshots: keep only the last run (files newer than the newest minus 3 hours)
if [ -d "$T/gui-shots" ]; then
  newest=$(find "$T/gui-shots" -type f -printf '%T@\n' | sort -n | tail -1)
  [ -n "${newest:-}" ] && find "$T/gui-shots" -type f ! -name 'old-0.2.1*' ! -name 'perf-*' ! -newermt "@$(awk "BEGIN{printf \"%d\", $newest-10800}")" -delete
fi
# 4. still above the limit (or --deep): the cross build for Windows and the debug build go; they are rebuilt when needed
if [ "${1:-}" = "--deep" ] || [ "$(size_gb)" -gt "$LIMIT_GB" ]; then
  rm -rf "$T/x86_64-pc-windows-gnu" "$T/debug"
fi
echo "target/: ${before} GB -> $(size_gb) GB (limit ${LIMIT_GB} GB); disk free: $(df -h --output=avail "$ROOT" | tail -1 | tr -d ' ')"
