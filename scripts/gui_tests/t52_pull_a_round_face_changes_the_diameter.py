#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pull tool on the round face of a hole: the face moves along its normal (to the axis), so typing 2 makes the radius 6 -> 4 (the status line says the new diameter), the next
one typed as 4 closes the hole (it is gone, the plate is whole), one Ctrl+Z brings it back. The same with a pin (it grows)."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5;v0=34.00,20.00,8.5;v1=33.46,22.00,8.5;v2=32.00,23.46,8.5;v3=30.00,24.00,8.5;v4=28.00,23.46,8.5;v5=26.54,22.00,8.5;v6=26.00,20.00,8.5;v7=26.54,18.00,8.5;v8=28.00,16.54,8.5;v9=30.00,16.00,8.5;v10=32.00,16.54,8.5;v11=33.46,18.00,8.5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 30, 20), f"a hole at (30, 20): {holes_of(p)}")
key("p")
settle(0.3)
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
click(x, y)
p = settle(0.4)
check(len(p["selection"]) == 1, f"the round face is selected: {p['selection']}")
type_text("2")
p = settle(0.5)
check(p["extra"]["pull_active"], "typing a number starts the pull of the round face")
check("Diameter 12" in p["status"] and "8" in p["status"], f"the status line says the diameter: {p['status']}")
check((p["extra"]["preview_triangles"] or 0) > 0, "the new cylinder is shown")
shot("01-preview")
key("Return")
p = wait_for(lambda q: not q["busy"] and any(close(h[3], 4.0, 0.01) for h in holes_of(q)), "the hole is 8 mm", 30)
check(close(p["bodies"][0]["volume"], 60000 - math.pi * 16 * 10, 0.5), f"the volume: {p['bodies'][0]['volume']}")
shot("02-smaller")
# the same face again, now by the radius: the hole closes
x, y = hover_cylinder(p, [f"v{i}" for i in range(12)])
click(x, y)
settle(0.4)
type_text("4")
settle(0.4)
key("Return")
p = wait_for(lambda q: not q["busy"] and not holes_of(q), "the hole is closed", 30)
check(close(p["bodies"][0]["volume"], 60000, 0.5), f"the plate is whole: {p['bodies'][0]['volume']}")
shot("03-closed")
key("z", ctrl=True)
p = wait_for(lambda q: has_hole_at(q, 30, 20), "Undo brings the hole back", 30)
finish()
