#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool, a row: a click on the dot of the arrow turns the arrow to the next direction (X, Y, Z, -X, ...)."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("t")
settle(0.4)
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
click(x, y)
p = settle()
check(close(p["link_dir"][0], 1.0, 0.01), f"the row runs along X: {p['link_dir']}")
want = [(0, 1, 0), (0, 0, 1), (-1, 0, 0)]
for w in want:
    click(*screen(p, "link_handle"))
    p = settle()
    d = p["link_dir"]
    check(all(close(a, b, 0.01) for a, b in zip(d, w)), f"the arrow turned to {w}: {d}")
    shot(f"dir-{w[0]}{w[1]}{w[2]}")
finish()
