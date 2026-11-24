#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Select tool: click the round face of a hole: the right column shows its distances to the two nearest edges as fields; typing a number and Enter moves the hole."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5;m0=36.00,12.00,8.5;m1=35.20,15.00,8.5;m2=33.00,17.20,8.5;m3=30.00,18.00,8.5;m4=27.00,17.20,8.5;m5=24.80,15.00,8.5;m6=24.00,12.00,8.5;m7=24.80,9.00,8.5;m8=27.00,6.80,8.5;m9=30.00,6.00,8.5;m10=33.00,6.80,8.5;m11=35.20,9.00,8.5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
click()
p = settle(0.8)
check("edge_dist_0" in p["widgets"] and "edge_dist_1" in p["widgets"], f"the two fields are there: {list(p['widgets'])}")
shot("01-fields")
click(*screen(p, "edge_dist_0"))
key("a", ctrl=True)
type_text("12")
key("Return")
p = wait_for(lambda q: has_hole_at(q, 30, 12), "the hole is 12 mm from the front edge")
check(has_hole_at(p, 30, 12), f"the hole moved to (30, 12): {holes_of(p)}")
p = settle(0.8)
# the selection is gone with the old face: click the hole again, now at its new place
hover_cylinder(p, [f"m{i}" for i in range(12)])
click()
p = settle(0.8)
click(*screen(p, "edge_dist_1"))
key("a", ctrl=True)
type_text("25")
key("Return")
p = wait_for(lambda q: has_hole_at(q, 25, 12), "the hole is 25 mm from the left edge")
check(has_hole_at(p, 25, 12), f"the hole moved to (25, 12): {holes_of(p)}")
shot("02-moved")
finish()
