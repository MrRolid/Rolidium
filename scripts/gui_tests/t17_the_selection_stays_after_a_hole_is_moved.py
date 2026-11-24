#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A hole is dragged and put at typed distances: it stays selected (the right column shows its distances), so it can be changed in the panel at once."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
release = drag(x, y, x - 24, y + 10)
time.sleep(0.5)
type_text("12")
key("Tab")
type_text("15")
key("Return")
release()
p = wait_for(lambda q: has_hole_at(q, 15, 12), "the hole is at (15, 12)")
p = settle(1.0)
check(len(p["selection"]) == 1, f"the moved hole is still selected: {p['selection']}")
check("edge_dist_0" in p["widgets"], f"the fields of its distances are there at once: {list(p['widgets'])}")
shot("01-selected")
click(*screen(p, "edge_dist_0"))
key("a", ctrl=True)
type_text("20")
key("Return")
p = wait_for(lambda q: has_hole_at(q, 15, 20), "the hole is at (15, 20)")
p = settle(1.0)
check(len(p["selection"]) == 1 and "edge_dist_0" in p["widgets"], "and still selected after the second move")
shot("02-again")
finish()
