#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""In 3D the wizard's "Along a line or a curve" puts the pieces (copies of a body) on the curve itself: along a line of 198 mm over its whole length, on an arc only on the arc, on the
perimeter of a rectangle of a sketch (the chain of four lines), on a circle. The link keeps the curve."""
# FIXTURE: box 250 120 1; box 10 10 10; sketch xy 5; line 20 110 218 110; arc 20 70 60 90 100 70; rect 20 10 80 50; circle 190 55 60
# POINTS: src=5,5,10;line=119,110,5;arcm=60,90,5;rect=50,10,5;big=220,55,5
from guitest import *
import math

time.sleep(1.0)
focus_app_window()
p = settle()
click(*steady("fit_all"))   # Fit all
p = settle(1.2)

def centres(q):
    out = []
    for b in q["bodies"]:
        lo, hi = b["bbox"][:3], b["bbox"][3:]
        if abs(hi[2] - lo[2] - 10) > 0.01:
            continue   # the plate that makes Fit all show the whole sketch
        out.append(tuple((lo[i] + hi[i]) / 2 for i in range(3)))
    return out

def run(curve, what, count=None):
    key("s")
    p = settle(0.4)
    click(*screen(p, "src"))
    key("t")
    p = settle(0.5)
    click(*steady("want_curve"))
    p = settle(0.4)
    move(*screen(p, curve))
    time.sleep(0.5)
    click(*screen(p, curve))
    p = settle(0.7)
    check(p["link_armed"] and p["link_mode"] == "Path", f"{what}: the curve is the path: {p['link_mode']} armed {p['link_armed']}")
    shot(f"{what}-preview")
    key("Return")
    wait_for(lambda q: len(q["bodies"]) > 2 and not q["busy"], f"{what}: the pieces are made", 40)
    return settle(1.0)

p = run("line", "line")
cs = [c for c in centres(p) if abs(c[0] - 5) > 0.1 or abs(c[1] - 5) > 0.1]
xs = sorted(c[0] for c in cs)
check(len(cs) >= 4 and all(abs(c[1] - 110) < 0.3 for c in cs), f"line: the pieces are on the line: {cs}")
check(xs and abs(xs[0] - 20) < 0.3 and abs(xs[-1] - 218) < 0.3, f"line: over the whole length of 198 mm: {xs}")
shot("line-pieces")
key("z", ctrl=True)
wait_for(lambda q: len(q["bodies"]) == 2 and not q["busy"], "Ctrl+Z", 30)

p = run("arcm", "arc")
cs = [c for c in centres(p) if abs(c[0] - 5) > 0.1 or abs(c[1] - 5) > 0.1]
bad = [c for c in cs if abs(math.hypot(c[0] - 60, c[1] - 40) - 50) > 0.3 or not (36 <= math.degrees(math.atan2(c[1] - 40, c[0] - 60)) <= 144)]
check(len(cs) >= 4 and not bad, f"arc: the pieces are on the arc only: {cs}")
shot("arc-pieces")
key("z", ctrl=True)
wait_for(lambda q: len(q["bodies"]) == 2 and not q["busy"], "Ctrl+Z", 30)

p = run("rect", "rectangle")
cs = [c for c in centres(p) if abs(c[0] - 5) > 0.1 or abs(c[1] - 5) > 0.1]
on = lambda x, y: 19.8 <= x <= 80.2 and 9.8 <= y <= 50.2 and min(abs(x - 20), abs(x - 80), abs(y - 10), abs(y - 50)) < 0.3
check(len(cs) >= 4 and all(on(c[0], c[1]) for c in cs), f"rectangle: the pieces are on the perimeter: {cs}")
shot("rect-pieces")
key("z", ctrl=True)
wait_for(lambda q: len(q["bodies"]) == 2 and not q["busy"], "Ctrl+Z", 30)

p = run("big", "circle")
cs = [c for c in centres(p) if abs(c[0] - 5) > 0.1 or abs(c[1] - 5) > 0.1]
check(len(cs) >= 4 and all(abs(math.hypot(c[0] - 190, c[1] - 55) - 30) < 0.3 for c in cs), f"circle: the pieces are on the circle: {cs}")
shot("circle-pieces")
finish()
