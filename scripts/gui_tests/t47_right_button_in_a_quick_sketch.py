#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The right button multiplies a circle also in the quick sketch on the wall of a solid (a drawing tool chosen in the 3D view, one click on the face): the four corners of the
face, the same distances from the two nearest edges, one step of Ctrl+Z."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;c1=13,17,10;c2=18,17,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("c")
p = settle(0.4)
x, y = steady("c1")
move(x, y)
p = wait_for(lambda q: q["extra"]["plane_hover"], "the face under the cursor shows its grid", 10)
click(x, y)
p = wait_for(lambda q: q["sketching"] == "Circle" and q["drawing"], "the sketch is open with the centre placed", 10)
move(*screen(p, "c2"))
time.sleep(0.6)
p = probe()
check(p["multi_hint"] == "Right button: more pieces", f"the diameter box says what the right button does: {p['multi_hint']}")
right_click()
p = settle(0.5)
check(p["multi_copies"] == 3, f"three more circles in the other corners: {p['multi_copies']}")
shot("01-preview")
print("pointer", probe()["viewport"], screen(p, "c2"))
click()   # the pointer is where the diameter was set
p = settle(0.6)
cs = [(round(c["x"], 2), round(c["y"], 2)) for c in p["circles"]]
check(len(cs) == 4, f"four circles: {cs}")
xs = sorted(c[0] for c in cs)
ys = sorted(c[1] for c in cs)
check(close(xs[0] + xs[3], xs[1] + xs[2], 0.1) and close(ys[0] + ys[3], ys[1] + ys[2], 0.1), f"the same distances from the edges in every corner: {cs}")
shot("02-four")
key("z", ctrl=True)
p = settle(0.5)
check(len(p["circles"]) == 0, f"one Ctrl+Z takes them all back: {len(p['circles'])}")
finish()
