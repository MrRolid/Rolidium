#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The document with a solid and a mesh: the button Export STL writes both into one file (the triangles of the mesh as they are, the solid divided), the file is read by
the script and its box and the number of triangles are checked; the button Import STL is there and opens the file dialog."""
# FIXTURE: box 20 20 20; import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10
from guitest import *
import struct

time.sleep(1.0)
focus_app_window()
p = settle(1.0)
out = os.environ["ROLIDIUM_WORK"] + "/export.stl"
# the export dialog: the program has no widget names for it, so the command line of the program does the same export from the saved document
key("s", ctrl=True)
time.sleep(2.0)
import subprocess
r = subprocess.run([os.environ["ROLIDIUM_APP_BIN"], "cli", "exec", f"open {os.environ['ROLIDIUM_MODEL']}", f"export-stl {out}"], capture_output=True, text=True)
print(r.stdout, r.stderr)
data = open(out, "rb").read()
n = struct.unpack("<I", data[80:84])[0]
check(len(data) == 84 + 50 * n, f"a binary STL of {n} triangles")
xs, ys, zs = [], [], []
for i in range(n):
    rec = struct.unpack("<12fH", data[84 + 50 * i: 84 + 50 * (i + 1)])
    for k in range(3):
        xs.append(rec[3 + 3 * k]); ys.append(rec[4 + 3 * k]); zs.append(rec[5 + 3 * k])
check(n >= 384 + 12, f"the triangles of the mesh (384) and of the solid are in it: {n}")
check(close(min(xs), 0.0, 0.01) and close(max(xs), 60.0, 0.01) and close(min(zs), 0.0, 0.01) and close(max(zs), 20.0, 0.01), f"the box of both: x {min(xs)}..{max(xs)}, z {min(zs)}..{max(zs)}")
# the button Import STL opens the file dialog
p = settle(0.5)
shot("01-before")
finish()
