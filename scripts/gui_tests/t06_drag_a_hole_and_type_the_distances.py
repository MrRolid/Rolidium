#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Select tool: drag the round face of a hole: the hole follows the pointer along its face with two live distances to the nearest edges; type 12, Tab, 15 and
Enter: the hole is 12 mm from the front edge and 15 from the left one. Esc cancels a drag."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 30, 20), f"a hole at (30, 20): {holes_of(p)}")
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
release = drag(x, y, x - 24, y + 10)
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["what"] == "hole", f"the live distances are shown while the hole is dragged: {e}")
shot("01-dragging")
type_text("12")
key("Tab")
type_text("15")
time.sleep(0.4)
e = probe()["edge_dims"]
check(e and e["typed"] == ["12", "15"], f"the typed values: {e}")
shot("02-typed")
key("Return")
release()
p = wait_for(lambda q: has_hole_at(q, 15, 12), "the hole is at (15, 12)")
check(has_hole_at(p, 15, 12), f"the hole is 12 mm from the front edge and 15 from the left one: {holes_of(p)}")
check(not has_hole_at(p, 30, 20), "the old place is material again")
v = p["bodies"][0]["volume"]
check(close(v, 100 * 60 * 10 - math.pi * 36 * 10, 0.5), f"the volume is that of the plate with one hole: {v}")
shot("03-moved")
finish()
