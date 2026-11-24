#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool: click a hole, "Second piece here", move over the face: two live distances to the nearest edges are shown; type 20, Tab, 50, Enter: the row goes from
the hole to a place 20 mm from the front edge and 50 mm from the left one (the direction and the spacing come from it). Then Apply: three holes in a row."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 30 20 12; pull-region 0 12 cut
# POINTS: w0=36.00,20.00,8.5;w1=35.20,23.00,8.5;w2=33.00,25.20,8.5;w3=30.00,26.00,8.5;w4=27.00,25.20,8.5;w5=24.80,23.00,8.5;w6=24.00,20.00,8.5;w7=24.80,17.00,8.5;w8=27.00,14.80,8.5;w9=30.00,14.00,8.5;w10=33.00,14.80,8.5;w11=35.20,17.00,8.5;t1=50,22,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("t")
settle(0.4)
x, y = hover_cylinder(p, [f"w{i}" for i in range(12)])
click(x, y)
p = settle()
click(*screen(p, "pick_point"))
p = settle()
check(p["link_pick"] == "Some(Point)", "the tool waits for the place of the second piece")
check("second" in p["hint"].lower(), f"the status line says what to click: {p['hint']}")
x, y = screen(p, "t1")
move(x, y)
time.sleep(0.6)
e = probe()["edge_dims"]
check(e and e["what"] == "second", f"the live distances are shown: {e}")
type_text("20")
key("Tab")
type_text("50")
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["typed"] == ["20", "50"] and close(e["at"][0], 50, 0.05) and close(e["at"][1], 20, 0.05), f"the place is (50, 20): {e}")
shot("01-typed")
key("Return")
p = settle()
check(p["link_pick"] == "None", "the place was taken")
d = p["link_dir"]
check(close(d[0], 1, 0.01) and close(d[1], 0, 0.01), f"the row runs along X: {d}")
shot("02-direction")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
for x in (30, 50, 70):
    check(has_hole_at(p, x, 20), f"a hole at ({x}, 20)")
shot("03-row")
finish()
