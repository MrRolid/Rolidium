#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool, Into a face: click a hole, choose the corners and the face, Apply. Then pull the front edge of the plate (the one the hole is near) 20 mm
out with the Pull tool: the holes on that side follow and stay 6 mm from the edge (the original too), the link stays and no question about removing it comes."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 6 6 8; pull-region 0 12 cut
# POINTS: w0=10.00,6.00,7;w1=9.46,8.00,7;w2=8.00,9.46,7;w3=6.00,10.00,7;w4=4.00,9.46,7;w5=2.54,8.00,7;w6=2.00,6.00,7;w7=2.54,4.00,7;w8=4.00,2.54,7;w9=6.00,2.00,7;w10=8.00,2.54,7;w11=9.46,4.00,7;w12=10.00,6.00,5.5;w13=9.46,8.00,5.5;w14=8.00,9.46,5.5;w15=6.00,10.00,5.5;w16=4.00,9.46,5.5;w17=2.54,8.00,5.5;w18=2.00,6.00,5.5;w19=2.54,4.00,5.5;w20=4.00,2.54,5.5;w21=6.00,2.00,5.5;w22=8.00,2.54,5.5;w23=9.46,4.00,5.5;top=50,30,10;s1=50,0,5;s2=30,0,5;s3=70,0,5;s4=50,0,3;s5=40,0,6;s6=60,0,6;s7=80,0,5
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 6, 6), f"a hole at (6, 6): {holes_of(p)}")
key("t")
settle(0.4)
hx, hy = hover_cylinder(p, [f"w{i}" for i in range(24)])
click(hx, hy)
p = settle()
check("curve" in p["hint"].lower() and "face" in p["hint"].lower(), f"the status line says what to click: {p['hint']}")
click(*screen(p, "top"))
p = settle()
check(p["link_armed"] and p["link_mode"] == "Fill" and p["link_rect"], f"a flat face after a hole means the corners: {p['link_mode']}")
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
