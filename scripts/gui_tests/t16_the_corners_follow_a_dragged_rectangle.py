#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool, Into a face, the rectangle of a sketch: the holes go into its four corners with the distances the hole has (8 mm). Then the right side of the
rectangle is dragged with the mouse (Select tool): the holes on that side follow, the link stays; one Undo brings everything back."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 28 23 8; pull-region 0 12 cut; sketch xy 10; rect 20 15 80 45
# POINTS: w0=32.00,23.00,7;w1=31.46,25.00,7;w2=30.00,26.46,7;w3=28.00,27.00,7;w4=26.00,26.46,7;w5=24.54,25.00,7;w6=24.00,23.00,7;w7=24.54,21.00,7;w8=26.00,19.54,7;w9=28.00,19.00,7;w10=30.00,19.54,7;w11=31.46,21.00,7;w12=32.00,23.00,5.5;w13=31.46,25.00,5.5;w14=30.00,26.46,5.5;w15=28.00,27.00,5.5;w16=26.00,26.46,5.5;w17=24.54,25.00,5.5;w18=24.00,23.00,5.5;w19=24.54,21.00,5.5;w20=26.00,19.54,5.5;w21=28.00,19.00,5.5;w22=30.00,19.54,5.5;w23=31.46,21.00,5.5;redge=80,30,10;ledge=20,30,10;void=105,75,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(has_hole_at(p, 28, 23), f"a hole at (28, 23): {holes_of(p)}")
key("t")
settle(0.4)
x, y = hover_cylinder(p, [f"w{i}" for i in range(24)])
click(x, y)
p = settle()
click(*screen(p, "ledge"))   # the panel covers the right side of the view
p = settle()
check(p["link_armed"] and p["link_mode"] == "Fill" and p["link_rect"], f"the rectangle of the sketch was taken: {p['link_mode']}")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
check(p["links"][0]["follows"], "the link follows the rectangle")
for hx, hy in [(28, 23), (72, 23), (28, 37), (72, 37)]:
    check(has_hole_at(p, hx, hy), f"a hole at ({hx}, {hy}), 8 mm from the sides of the rectangle")
shot("01-corners")
# the right side of the rectangle is dragged 40 pixels to the right
key("s")
settle(0.4)
x, y = screen(p, "redge")
drag(x, y, x + 40, y)()
p = settle(1.5)
r = next(s for s in p["shapes"] if s["kind"] == "rect")
X = max(r["a"][0], r["b"][0])
check(X > 82, f"the side moved to x = {X}")
check(len(p["links"]) == 1 and p["links"][0]["error"] is None, f"the link stays: {p['links']}")
for hx, hy in [(28, 23), (X - 8, 23), (28, 37), (X - 8, 37)]:
    check(has_hole_at(p, hx, hy, tol=0.1), f"a hole at ({hx}, {hy}) after the drag")
shot("02-dragged")
click(*screen(p, "void"))
key("z", ctrl=True)
p = settle(1.5)
r = next(s for s in p["shapes"] if s["kind"] == "rect")
check(max(r["a"][0], r["b"][0]) == 80, f"one Undo brought the rectangle back: {r}")
for hx, hy in [(28, 23), (72, 23), (28, 37), (72, 37)]:
    check(has_hole_at(p, hx, hy), f"the hole at ({hx}, {hy}) is back")
finish()
