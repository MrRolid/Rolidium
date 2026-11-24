#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sketch tools draw in both ways: click and click, or press, drag and release. A movement under 4 pixels between the press and the release is a click, however
long the button is held. Line, auxiliary axis, rectangle, circle and arc. The live distances to the edges are shown in both ways."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;a=10,10,10;b=40,10,10;c=10,25,10;d=40,25,10;e=55,10,10;f=90,10,10;g=55,30,10;h=90,50,10;k=20,40,10;m=35,40,10;n=50,45,10;o=70,45,10;q=60,52,10;t1=12,52,10;t2=40,52,10;s1=12,57,10;s2=40,57,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
p = settle(0.4)


def shapes(p):
    return p["sketches"][0]["shapes"] if p["sketches"] else 0


key("l")
p = settle(0.4)
check("press and drag" in p["hint"], f"the status line names both ways: {p['hint']}")
# 1. a line by two clicks
click(*screen(p, "a"))
click(*screen(p, "b"))
p = settle(0.4)
check(shapes(p) == 1, f"a line by click and click: {shapes(p)} shape(s)")
key("Escape")
# 2. a line by press, drag and release
release = drag(*screen(p, "c"), *screen(p, "d"))
release()
p = settle(0.4)
check(shapes(p) == 2, f"a line by press, drag and release: {shapes(p)} shape(s)")
key("Escape")
# 3. an auxiliary axis by dragging
key("x")
settle(0.3)
drag(*screen(p, "e"), *screen(p, "f"))()
p = settle(0.4)
check(p["sketches"][0]["axes"] == 1, f"an axis by dragging: {p['sketches']}")
key("Escape")
# 4. a rectangle by dragging
key("r")
settle(0.3)
drag(*screen(p, "g"), *screen(p, "h"))()
p = settle(0.4)
check(any(s["kind"] == "rect" for s in p["shapes"]), "a rectangle by dragging")
key("Escape")
# 5. a circle by dragging, with the live distances to the edges during the drag
key("c")
settle(0.3)
move(*screen(p, "k"))
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["what"] == "circle", f"the live distances before the press: {e}")
release = drag(*screen(p, "k"), *screen(p, "m"))
time.sleep(0.4)
e = probe()["edge_dims"]
check(e and e["what"] == "circle", f"the live distances while the button is down and the pointer drags: {e}")
shot("01-circle-drag")
release()
p = settle(0.4)
check(any(s["kind"] == "circle" for s in p["shapes"]), "a circle by dragging")
check(not p["drawing"], "the drawing ended with the release")
key("Escape")
# 6. an arc: drag from the start to the end, then click a point on the arc
key("a")
settle(0.3)
drag(*screen(p, "n"), *screen(p, "o"))()
time.sleep(0.4)
click(*screen(p, "q"))
p = settle(0.5)
check(any(s["kind"] == "arc" for s in p["shapes"]), f"an arc by dragging and a click: {[s['kind'] for s in p['shapes']]}")
key("Escape")
shot("02-all")
# 7. a movement under 4 pixels is a click: the first point is placed, the next click ends the line
key("l")
settle(0.3)
x, y = screen(p, "t1")
move(x, y)
time.sleep(0.4)
button(1, True)
move(x + 2, y + 1)
button(1, False)
time.sleep(0.4)
p = probe()
check(p["drawing"], "a press and a release 2 pixels apart place the first point (a click)")
n0 = len(p["shapes"])
click(*screen(p, "t2"))
p = settle(0.4)
check(len(p["shapes"]) == n0 + 1, "the next click ends the line")
key("Escape")
# 8. a click that takes long (1.2 s with the button down) is a click too
x, y = screen(p, "s1")
move(x, y)
time.sleep(0.4)
button(1, True)
time.sleep(1.2)
button(1, False)
time.sleep(0.4)
check(probe()["drawing"], "a click held for 1.2 s still places the first point")
click(*screen(p, "s2"))
p = settle(0.4)
check(len(p["shapes"]) == n0 + 2, "and the next click ends the line")
finish()
