#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool, Into a face: click a hole, choose the corners and the face, Apply. Then pull the front edge of the plate (the one the hole is near) 20 mm
out with the Pull tool: the holes on that side follow and stay 6 mm from the edge (the original too), the link stays and no question about removing it comes."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 6 6 8; pull-region 0 12 cut
# POINTS: w1=2,6,5;w2=10,6,5;w3=6,2,5;w4=6,10,5;w5=3.2,8.8,5;w6=8.8,8.8,5;top=50,30,10;s1=50,0,5;s2=30,0,5;s3=70,0,5;s4=50,0,3;s5=40,0,6;s6=60,0,6;s7=80,0,5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 6, 6), f"a hole at (6, 6): {holes_of(p)}")
key("t")
settle(0.4)
hx, hy = hover_cylinder(p, ["w5", "w6", "w2", "w4", "w1", "w3"])
click(hx, hy)
p = settle()
click(*screen(p, "tab_fill"))
p = settle()
click(*screen(p, "pick_rect"))
p = settle()
check("rectangular" in p["hint"].lower() or "face" in p["hint"].lower(), f"the status line says to click the face: {p['hint']}")
click(*screen(p, "top"))
p = settle()
shot("01-face-picked")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
check(p["links"][0]["follows"], "the link follows the rectangle")
for x, y in [(6, 6), (94, 6), (6, 54), (94, 54)]:
    check(has_hole_at(p, x, y), f"a hole at ({x}, {y})")
shot("02-four-corners")
# pull the front side of the plate (y = 0) 20 mm out
key("p")
settle(0.4)
sx, sy = hover_normal(["s1", "s2", "s3", "s4", "s5", "s6", "s7"], (0, -1, 0))
click()
p = settle()
check(len(p["selection"]) == 1, f"the side face is selected: {p['selection']}")
type_text("20")
key("Return")
p = settle(1.5)
check(not p["derived_ask"], "no question about removing the link")
check(len(p["links"]) == 1 and p["links"][0]["error"] is None, f"the link is kept: {p['links']}")
b = first_by(p, lambda b: b["visible"])
print("bbox", b["bbox"])
check(close(b["bbox"][1], -20.0, 0.01) and close(b["bbox"][4], 60.0, 0.01), "the plate is 80 mm wide now (y from -20 to 60)")
for x, y in [(6, -14), (94, -14), (6, 54), (94, 54)]:
    check(has_hole_at(p, x, y, p["bodies"].index(b)), f"a hole at ({x}, {y}), 6 mm from the edges")
shot("03-pulled")
finish()
