#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Select tool: the shapes of a sketch are dragged by a handle: a circle by its centre (it moves) or its rim (the radius), a line by its middle (it moves) or an end,
a rectangle by a corner or a side. One Undo takes back each drag. While a point moves the live distances to the nearest edges are shown and can be typed over."""
# FIXTURE: box 100 60 10; sketch xy 10; circle 30 30 12; line 10 50 40 50; rect 60 10 90 40
# POINTS: centre=30,30,10;rim=36,30,10;lend=40,50,10;lmid=25,50,10;corner=90,40,10;redge=90,25,10;void=105,75,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()


def shape(p, kind):
    return next(s for s in p["shapes"] if s["kind"] == kind)


def undo_by_key():
    click(*screen(probe(), "void"))   # leaves the field of the panel next to the shape, where Ctrl+Z would belong to the text
    key("z", ctrl=True)
    return settle(0.6)


c0, l0, r0 = shape(p, "circle"), shape(p, "line"), shape(p, "rect")
n0 = len(p["shapes"])
# 1. the circle by its centre
x, y = screen(p, "centre")
release = drag(x, y, x + 30, y + 14)
time.sleep(0.4)
q = probe()
check(q["shape_drag"] and q["edge_dims"] and q["edge_dims"]["what"] == "shape", f"the live distances are shown while the centre is dragged: {q['edge_dims']}")
shot("01-centre")
release()
p = settle(0.8)
c = shape(p, "circle")
check((abs(c["x"] - 30) > 1 or abs(c["y"] - 30) > 1) and close(c["r"], 6, 0.001), f"the circle moved, the radius stays: {c}")
p = undo_by_key()
c = shape(p, "circle")
check(close(c["x"], 30, 0.001) and close(c["y"], 30, 0.001), f"one Undo took the move back: {c}")
check(len(p["shapes"]) == n0, "no shape was added or lost")
# 2. the circle by its rim
x, y = screen(p, "rim")
drag(x, y, x + 30, y)()
p = settle(0.8)
c = shape(p, "circle")
check(c["r"] > 7, f"the radius grew: {c}")
shot("02-rim")
p = undo_by_key()
check(close(shape(p, "circle")["r"], 6, 0.001), "one Undo took the radius back")
# 3. the line by an end and by its middle
x, y = screen(p, "lend")
drag(x, y, x + 20, y - 25)()
p = settle(0.8)
l = shape(p, "line")
check(l["b"] != l0["b"] and l["a"] == l0["a"], f"the end moved, the other end stays: {l}")
p = undo_by_key()
check(shape(p, "line")["b"] == l0["b"], "one Undo took the end back")
x, y = screen(p, "lmid")
drag(x, y, x + 12, y + 22)()
p = settle(0.8)
l = shape(p, "line")
da = (l["a"][0] - l0["a"][0], l["a"][1] - l0["a"][1])
db = (l["b"][0] - l0["b"][0], l["b"][1] - l0["b"][1])
check(close(da[0], db[0], 0.01) and close(da[1], db[1], 0.01) and (abs(da[0]) > 0.5 or abs(da[1]) > 0.5), f"the line moved as a whole: {da} {db}")
p = undo_by_key()
check(shape(p, "line")["a"] == l0["a"], "one Undo took the move back")
# 4. the rectangle by a corner and by a side
x, y = screen(p, "corner")
drag(x, y, x + 25, y + 10)()
p = settle(0.8)
r = shape(p, "rect")
check(r["b"] != r0["b"] and r["a"] == r0["a"], f"the corner moved, the opposite one stays: {r}")
shot("03-corner")
p = undo_by_key()
check(shape(p, "rect")["b"] == r0["b"], "one Undo took the corner back")
x, y = screen(p, "redge")
drag(x, y, x + 30, y)()
p = settle(0.8)
r = shape(p, "rect")
check(r["b"][0] > r0["b"][0] + 1 and close(r["b"][1], r0["b"][1], 0.001) and r["a"] == r0["a"], f"the side moved: {r}")
p = undo_by_key()
check(shape(p, "rect")["b"] == r0["b"], "one Undo took the side back")
# 5. typed numbers over the live distances: 15 mm from the line, 25 mm from the side of the rectangle
x, y = screen(p, "centre")
release = drag(x, y, x + 5, y + 2)
time.sleep(0.4)
type_text("15")
key("Tab")
type_text("25")
time.sleep(0.4)
e = probe()["edge_dims"]
check(e and e["typed"] == ["15", "25"], f"the typed numbers are shown: {e}")
shot("04-typed")
key("Return")
release()
p = settle(1.0)
c = shape(p, "circle")
check(close(c["x"], 35, 0.05) and close(c["y"], 35, 0.05), f"the centre is 15 mm from the line and 25 mm from the side: {c}")
finish()
