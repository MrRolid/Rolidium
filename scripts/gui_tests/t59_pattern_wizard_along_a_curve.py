#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The Pattern tool as a wizard. T with nothing chosen says "Click what you want to repeat" (at the cursor and in the panel). After a click on the body the panel offers three big
buttons (Along a line or a curve / Round a point / On a face). "Along a line or a curve" asks for a click on a curve: a circle of a sketch becomes the path (a closed curve: the
pieces spread evenly), Enter makes the link: five bodies, one Ctrl+Z takes the four copies away."""
# FIXTURE: box 10 10 10; sketch xy 10; circle 5 5 30
# POINTS: b=5,5,10;k0=20.000,5.000,10;k1=17.990,12.500,10;k2=12.500,17.990,10;k3=5.000,20.000,10;k4=-2.500,17.990,10;k5=-7.990,12.500,10;k6=-10.000,5.000,10;k7=-7.990,-2.500,10;k8=-2.500,-7.990,10;k9=5.000,-10.000,10;k10=12.500,-7.990,10;k11=17.990,-2.500,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("t")
move(*screen(p, "b"))
p = settle(0.5)
check(p["link_cursor_hint"] == "Click what you want to repeat", f"at the cursor: {p['link_cursor_hint']!r}")
shot("01-nothing-chosen")
click(*screen(p, "b"))
p = settle(0.5)
for w in ("want_curve", "want_point", "want_face"):
    check(w in p["widgets"], f"the big button {w} is in the panel")
shot("02-three-buttons")
click(*steady("want_curve"))
click(*steady("fit_all"))   # Fit all: the circle of the sketch is bigger than the box
p = settle(1.2)
check(p["link_want"] == "Some(Curve)", f"Along a line or a curve is chosen: {p['link_want']}")
vx, vy, vw, vh = p["viewport"]
def visible(name):
    x, y = screen(p, name)
    return vx + 5 < x < vx + vw - 5 and vy + 5 < y < vy + vh - 5 and not (700 < x < 990 and 55 < y < 360)
cands = [n for n in (f"k{i}" for i in range(12)) if visible(n)]
check(cands, f"a point of the circle is in the view: {[screen(p, f'k{i}') for i in range(12)]}")
move(*screen(p, cands[0]))
time.sleep(0.5)
click(*screen(p, cands[0]))
p = settle(0.6)
check(p["link_armed"] and p["link_mode"] == "Path", f"the circle is the path: {p['link_mode']} armed {p['link_armed']}")
check(p["link_path"] > 20 and p["link_path_closed"], f"the polyline of a closed curve: {p['link_path']} points, closed {p['link_path_closed']}")
shot("03-path")
key("Return")
p = wait_for(lambda q: len(q["bodies"]) == 6 and not q["busy"], "the object and five pieces on the circle", 30)
check(p["links"] and p["links"][0]["kind"] == "Pattern along a curve", f"the link: {p['links']}")
shot("04-pieces")
key("z", ctrl=True)
p = wait_for(lambda q: len(q["bodies"]) == 1 and not q["busy"], "one Ctrl+Z takes the copies away", 30)
finish()
