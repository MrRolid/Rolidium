#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Snapping in a sketch: the crossing of two lines is a snap point with the label "intersection", the quadrant of a circle is "quadrant", a point of a line away from every
special point is "on curve", an end point wins over everything and the Shift key turns the snapping off for the moment."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;a1=10,10,10;a2=50,50,10;b1=20,55,10;b2=20,5,10;mid=18,18,10;cc=75,30,10;cr=85,30,10;q=85,30,10;qn=85.4,30.3,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
p = settle(0.6)
key("l")
settle(0.3)
click(*screen(p, "a1"))
click(*screen(p, "a2"))
key("Escape")
settle(0.3)
key("l")
click(*screen(p, "b1"))
click(*screen(p, "b2"))
key("Escape")
p = settle(0.5)
check(len([s for s in p["shapes"] if s["kind"] == "line"]) == 2, f"two lines: {p['shapes']}")
# where the two lines cross (from the lines as they were drawn), and the pixel of a point of the sketch (the view is the plan: an affine map)
ls = [s for s in p["shapes"] if s["kind"] == "line"]
(ax, ay), (bx, by) = ls[0]["a"], ls[0]["b"]
(cx, cy), (dx, dy) = ls[1]["a"], ls[1]["b"]
den = (bx - ax) * (dy - cy) - (by - ay) * (dx - cx)
t = ((cx - ax) * (dy - cy) - (cy - ay) * (dx - cx)) / den
cross = (ax + t * (bx - ax), ay + t * (by - ay))
a1s, a2s = screen(p, "a1"), screen(p, "a2")
kx, ky = (a2s[0] - a1s[0]) / 40.0, (a2s[1] - a1s[1]) / 40.0
pix = lambda x, y: (a1s[0] + (x - 10) * kx, a1s[1] + (y - 10) * ky)
print("crossing", cross, pix(*cross))
key("c")
settle(0.3)
move(*pix(cross[0] + 0.5, cross[1] - 0.4))
time.sleep(0.6)
p = probe()
check(p["extra"]["snap"]["label"] == "intersection", f"near the crossing: {p['extra']['snap']}")
check(abs(p["extra"]["snap"]["x"] - cross[0]) < 0.01 and abs(p["extra"]["snap"]["y"] - cross[1]) < 0.01, f"at the crossing: {p['extra']['snap']}")
shot("01-intersection")
# Shift turns it off
c = keycode("Shift_L")
xtst.XTestFakeKeyEvent(D, c, 1, 0)
sync(0.1)
move(*pix(cross[0] + 0.5, cross[1] - 0.4))
time.sleep(0.5)
q = probe()
xtst.XTestFakeKeyEvent(D, c, 0, 0)
sync(0.1)
check(q["extra"]["snap"]["label"] in ("", "grid") and abs(q["extra"]["snap"]["x"] - p["extra"]["snap"]["x"]) > 0.05, f"Shift turns the snapping off: {q['extra']['snap']}")
# a point of a line away from the special points
move(*screen(p, "mid"))
time.sleep(0.6)
q = probe()
check(q["extra"]["snap"]["label"] in ("on curve", "intersection", "endpoint", "midpoint"), f"on the line: {q['extra']['snap']}")
shot("02-on-curve")
# a circle and its quadrant
click(*screen(p, "cc"))
click(*screen(p, "cr"))
key("Escape")
p = settle(0.5)
key("l")
settle(0.3)
move(*screen(p, "qn"))
time.sleep(0.6)
q = probe()
check(q["extra"]["snap"]["label"] == "quadrant", f"near the quadrant of the circle: {q['extra']['snap']}")
shot("03-quadrant")
finish()
