#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""In a sketch the wizard's "Along a line or a curve" puts the pieces on the curve itself: on an arc only on the arc, along a line of 198 mm over its whole length, on the
perimeter of a rectangle (the whole chain of four lines is one curve), on a circle. "Round a point" with an arc as the guide spreads the pieces over the arc, not the whole circle."""
# FIXTURE: box 250 120 10; sketch top; circle 10 100 6; line 20 110 218 110; arc 20 70 60 90 100 70; rect 20 10 80 50; circle 190 55 60
# POINTS: top=240,110,10;src=13,100,10;line=119,110,10;arcm=60,90,10;rect=50,10,10;big=220,55,10
from guitest import *
import math

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
p = settle(1.5)
key("s")
settle(0.3)

def pieces(q):
    return [(c["x"], c["y"]) for c in q["circles"] if abs(c["r"] - 3.0) < 1e-6]

def run(curve, what):
    key("s")
    p = settle(0.4)
    click(*screen(p, "src"))
    key("t")
    p = settle(0.5)
    click(*steady("want_curve"))
    p = settle(0.4)
    click(*screen(p, curve))
    p = settle(0.6)
    sp = p["extra"]["sk_pattern"]
    check(sp["open"] == "Path" and sp["guided"], f"{what}: the curve is the path: {sp}")
    check(sp["path"] and sp["path"] >= 2, f"{what}: the polyline of the curve: {sp['path']} points")
    shot(f"{what}-preview")
    click(*steady("skpat_apply"))
    p = settle(0.8)
    return p

# the arc through (20,70) (60,90) (100,70): centre (60,40), radius 50, between the angles 36.9 and 143.1 degrees
p = run("arcm", "arc")
ps = pieces(p)
check(len(ps) >= 3, f"pieces were made: {ps}")
copies = [q for q in ps if abs(q[0] - 10) > 0.1 or abs(q[1] - 100) > 0.1]
for (x, y) in copies:
    d = math.hypot(x - 60, y - 40)
    ang = math.degrees(math.atan2(y - 40, x - 60))
    check(abs(d - 50) < 0.2 and 36.0 <= ang <= 144.0, f"arc: a piece at ({x:.1f}, {y:.1f}) is on the arc (distance {d:.2f}, angle {ang:.1f})")
check(any(abs(q[0] - 20) < 0.2 and abs(q[1] - 70) < 0.2 for q in copies) and any(abs(q[0] - 100) < 0.2 and abs(q[1] - 70) < 0.2 for q in copies), "arc: pieces at both ends of the arc")
shot("arc-pieces")
key("z", ctrl=True)
p = settle(0.8)
check(len(pieces(p)) == 1, f"Ctrl+Z takes the pieces back: {pieces(p)}")

# the line of 198 mm: pieces over the whole length
p = run("line", "line")
copies = [q for q in pieces(p) if abs(q[0] - 10) > 0.1 or abs(q[1] - 100) > 0.1]
xs = sorted(q[0] for q in copies)
check(all(abs(q[1] - 110) < 0.2 for q in copies), f"line: the pieces are on the line: {copies}")
check(xs and abs(xs[0] - 20) < 0.2 and abs(xs[-1] - 218) < 0.2, f"line: from the start to the end of 198 mm: {xs}")
shot("line-pieces")
key("z", ctrl=True)
settle(0.8)

# the rectangle: the whole chain of four lines is the curve; the pieces stand on the perimeter
p = run("rect", "rectangle")
copies = [q for q in pieces(p) if abs(q[0] - 10) > 0.1 or abs(q[1] - 100) > 0.1]
def on_rect(x, y):
    inside = 20 - 0.2 <= x <= 80 + 0.2 and 10 - 0.2 <= y <= 50 + 0.2
    edge = min(abs(x - 20), abs(x - 80), abs(y - 10), abs(y - 50)) < 0.2
    return inside and edge
check(len(copies) >= 3 and all(on_rect(x, y) for x, y in copies), f"rectangle: the pieces are on the perimeter: {copies}")
shot("rect-pieces")
key("z", ctrl=True)
settle(0.8)

# the circle
p = run("big", "circle")
copies = [q for q in pieces(p) if abs(q[0] - 10) > 0.1 or abs(q[1] - 100) > 0.1]
check(len(copies) >= 3 and all(abs(math.hypot(x - 190, y - 55) - 30) < 0.2 for x, y in copies), f"circle: the pieces are on the circle: {copies}")
shot("circle-pieces")
key("z", ctrl=True)
settle(0.8)

# round a point with the arc as the guide: the pieces go over the arc (106 degrees), not the whole circle
key("s")
p = settle(0.4)
click(*screen(p, "src"))
key("t")
p = settle(0.5)
click(*steady("want_point"))
p = settle(0.4)
click(*screen(p, "arcm"))
p = settle(0.6)
sp = p["extra"]["sk_pattern"]
check(sp["open"] == "Circular" and not sp["full"] and sp["fit"], f"the arc is the guide: not the whole circle: {sp}")
check(abs(sp["step"] * (sp["count"] - 1) - 106.26) < 0.5, f"the pieces spread over the span of the arc: step {sp['step']} x {sp['count'] - 1}")
shot("round-arc")
finish()
