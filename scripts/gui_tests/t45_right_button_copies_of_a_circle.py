#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Drawing a circle inside a closed shape: the right button multiplies it. (1) Inside a rectangle: a click of the right button shows the same circle in the four corners
with the same distances from the two nearest walls (the text "Right button: more pieces" stands under the diameter box); the left click draws all four as one step of Ctrl+Z.
(2) Inside a circle: every right click adds a piece (2 to 6, evenly round the centre), Shift + right click takes one away; Esc draws nothing. (3) Outside every closed shape
the right button stops the drawing as before."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;ra=20,10,10;rb=80,50,10;rc=29,18,10;rd=34,18,10;cc=50,30,10;cr=75,30,10;pc=62,30,10;pr=65,30,10;out1=95,5,10;out2=97,5,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
p = settle(0.5)
n0 = lambda q: len(q["circles"])
# 1. a rectangle, and a circle inside it
key("r")
settle(0.3)
click(*screen(p, "ra"))
click(*screen(p, "rb"))
p = settle(0.4)
check(any(s["kind"] == "rect" for s in p["shapes"]), "a rectangle is drawn")
key("c")
settle(0.3)
click(*screen(p, "rc"))
p = settle(0.4)
check(p["drawing"], "the centre of the circle is placed")
move(*screen(p, "rd"))
time.sleep(0.6)
p = probe()
check(p["multi_hint"] == "Right button: more pieces", f"the diameter box says what the right button does: {p['multi_hint']}")
check(p["edge_dims"] is not None, "the distances to the walls are shown")
right_click()
p = settle(0.5)
check(p["multi_on"] and p["multi_copies"] == 3, f"the right button shows three more circles: {p['multi_copies']}")
shot("01-four-corners-preview")
print("pointer", probe()["viewport"], screen(p, "rd"))
click()   # the pointer is where the diameter was set
p = settle(0.6)
print("status", p["status"], p["drawing"], p["shapes"])
cs = sorted((round(c["x"], 2), round(c["y"], 2)) for c in p["circles"])
print("circles", cs)
check(len(cs) == 4, f"four circles: {len(cs)}")
x0, y0 = min(cs)[0], min(c[1] for c in cs)
for x, y in [(x0, y0), (100 - x0, y0), (x0, 60 - y0), (100 - x0, 60 - y0)]:
    check(any(close(c[0], x, 0.05) and close(c[1], y, 0.05) for c in cs), f"a circle at ({x}, {y}) (the same distances from the two nearest walls of the rectangle 20..80 x 10..50)")
check(close(x0 - 20, y0 - 10, 3.0) and x0 < 50 and y0 < 30, f"the first one is near the corner: ({x0}, {y0})")
rr = [c["r"] for c in p["circles"]]
check(max(rr) - min(rr) < 0.001, f"all with the same diameter: {rr}")
shot("02-four-circles")
key("z", ctrl=True)
p = settle(0.6)
check(n0(p) == 0, f"one Ctrl+Z takes back all four circles: {n0(p)}")
key("Escape")
# 2. a circle inside a circle
key("c")
settle(0.3)
click(*screen(p, "cc"))
click(*screen(p, "cr"))
p = settle(0.4)
check(n0(p) == 1, f"the big circle: {n0(p)}")
click(*screen(p, "pc"))
p = settle(0.4)
move(*screen(p, "pr"))
time.sleep(0.6)
p = probe()
check(p["multi_hint"] == "Right button: more pieces", f"inside a circle too: {p['multi_hint']}")
for n in range(2, 7):
    right_click()
    p = settle(0.4)
    check(p["multi_copies"] == n - 1, f"{n} pieces: {p['multi_copies']} copies")
shot("03-six-pieces")
right_click(shift=True)
p = settle(0.4)
check(p["multi_copies"] == 4, f"Shift + right click takes one away: {p['multi_copies']}")
key("Escape")
p = settle(0.5)
check(not p["drawing"] and n0(p) == 1, f"Esc draws nothing: {n0(p)} circle(s)")
# five pieces drawn at once, then Ctrl+Z
click(*screen(p, "pc"))
p = settle(0.4)
move(*screen(p, "pr"))
time.sleep(0.4)
for _ in range(4):
    right_click()
p = settle(0.5)
check(p["multi_copies"] == 4, f"five pieces: {p['multi_copies']}")
click(*screen(p, "pr"))
p = settle(0.6)
check(n0(p) == 6, f"the big circle and five small ones: {n0(p)}")
r = [c for c in p["circles"] if close(c["r"], 3, 0.05)]
import math
for c in r:
    d = math.hypot(c["x"] - 50, c["y"] - 30)
    check(close(d, 12, 0.05), f"on the circle of radius 12 round the centre: {d}")
key("z", ctrl=True)
p = settle(0.5)
check(n0(p) == 1, f"one Ctrl+Z takes back the five: {n0(p)}")
# 3. outside every closed shape the right button stops the drawing
click(*screen(p, "out1"))
p = settle(0.4)
check(p["drawing"], "a circle is begun at the corner (outside the circle and the rectangle is not there)")
move(*screen(p, "out2"))
time.sleep(0.4)
right_click()
p = settle(0.4)
print("hint", p["multi_hint"], p["multi_on"], p["drawing"])
finish()
