#!/bin/sh
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
# Scale check of the STL export: a cube of 20 mm is made with `rolidium cli` and exported (binary and ASCII), the files are read back
# by a parser of their own and the bounding box must be 20 x 20 x 20 mm to 0.001 mm. The expected and the measured box are always
# printed. The same check runs in `rolidium --selftest` and in `cargo test -p rolidium-cli --test stl_scale`.
#   scripts/stl-scale-check.sh                 # builds and runs the test
#   scripts/stl-scale-check.sh path/to/rolidium-cli   # checks an installed or unpacked program with python3
set -e
cd "$(dirname "$0")/.."
if [ -z "$1" ]; then
    exec cargo test -q -p rolidium-cli --test stl_scale -- --nocapture --test-threads=1
fi
cli="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
script="$PWD/examples/scale-check/cube-20.txt"
(cd "$dir" && "$cli" run "$script" >/dev/null)
python3 - "$dir" <<'PY'
import struct, sys
d = sys.argv[1]
exp_min, exp_max = (0.0, 0.0, 0.0), (20.0, 20.0, 20.0)
bad = False
def box(path):
    b = open(path, 'rb').read()
    pts = []
    n = struct.unpack('<I', b[80:84])[0] if len(b) >= 84 else -1
    if len(b) == 84 + 50 * n:
        for t in range(n):
            o = 84 + 50 * t + 12
            for v in range(3):
                pts.append(struct.unpack('<3f', b[o + 12 * v:o + 12 * v + 12]))
    else:
        for line in b.decode().splitlines():
            w = line.split()
            if len(w) == 4 and w[0] == 'vertex':
                pts.append(tuple(float(x) for x in w[1:]))
    return [min(p[i] for p in pts) for i in range(3)], [max(p[i] for p in pts) for i in range(3)], len(pts) // 3
fmt = lambda v: '(%.4f, %.4f, %.4f)' % tuple(v)
for name, kind in (('cube-20.stl', 'binary'), ('cube-20-ascii.stl', 'ASCII')):
    lo, hi, n = box(d + '/' + name)
    size = [hi[i] - lo[i] for i in range(3)]
    diff = [abs(s - 20.0) for s in size]
    ok = n >= 12 and max(diff) <= 0.001
    bad |= not ok
    print('STL scale check, %s (%s, %d triangles)' % (name, kind, n))
    print('  expected  min %s  max %s  size (20.0000, 20.0000, 20.0000)' % (fmt(exp_min), fmt(exp_max)))
    print('  measured  min %s  max %s  size %s' % (fmt(lo), fmt(hi), fmt(size)))
    print('  difference of the size %s  (tolerance 0.001 mm)  %s' % (fmt(diff), 'OK' if ok else 'WRONG SCALE'))
sys.exit(1 if bad else 0)
PY
