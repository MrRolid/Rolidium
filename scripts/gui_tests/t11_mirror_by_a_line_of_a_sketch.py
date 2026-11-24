#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Mirror tool: click a hole, then an ordinary line of a sketch (no button): the hole is mirrored by the plane through the line, square to the sketch."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 12 30 8; pull-region 0 12 cut; sketch xy 10; line 50 -5 50 65
# POINTS: w0=16.00,30.00,8.5;w1=15.46,32.00,8.5;w2=14.00,33.46,8.5;w3=12.00,34.00,8.5;w4=10.00,33.46,8.5;w5=8.54,32.00,8.5;w6=8.00,30.00,8.5;w7=8.54,28.00,8.5;w8=10.00,26.54,8.5;w9=12.00,26.00,8.5;w10=14.00,26.54,8.5;w11=15.46,28.00,8.5;axis=50,45,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 12, 30), f"a hole at (12, 30): {holes_of(p)}")
check(p["sketches"][0]["axes"] == 0 and p["sketches"][0]["shapes"] == 1, f"the sketch is one ordinary line: {p['sketches']}")
key("b")
settle(0.4)
hx, hy = hover_cylinder(p, [f"w{i}" for i in range(12)])
click(hx, hy)
p = settle()
check(p["tool"] == "Mirror", "the Mirror tool is on")
click(*screen(p, "axis"))
p = settle()
pl = p["link_plane"]
check(close(pl[0], 50, 0.01) and close(abs(pl[3]), 1.0, 0.01) and close(pl[4], 0, 0.01), f"the plane passes through x = 50, normal along X: {pl}")
shot("01-axis-clicked")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
b = first_by(p, lambda b: b["visible"])
i = p["bodies"].index(b)
check(has_hole_at(p, 12, 30, i), "the hole stays at (12, 30)")
check(has_hole_at(p, 88, 30, i), f"its mirror image at (88, 30): {holes_of(p, i)}")
shot("02-mirrored")
finish()
