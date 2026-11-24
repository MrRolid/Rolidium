#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sketch tool Circle: while the centre is placed, two live distances to the nearest straight edges are shown; the snaps to equal distance and to the middle of an
edge catch; typed numbers (Tab switches, Enter confirms) put the centre where they say. The distances of the finished circle are fields in the right column."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;m1=20,15,10;eq=20,20,10;mid=50,5,10;c=7,5,10;circle_edge=10,5,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
settle(0.4)
key("c")
p = settle()
check(p["sketching"] == "Circle", f"the Circle tool is on: {p['sketching']}")
x, y = screen(p, "m1")
move(x, y)
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["what"] == "circle", f"the live distances are shown: {e}")
check(e and close(e["d1"], 15, 1.5) and e["d2"] is not None and close(e["d2"], 20, 1.5), f"15 mm from the front edge and 20 from the left one: {e}")
shot("01-live-distances")
# snaps: equal distance (20, 20) and the middle of the front edge
x, y = screen(p, "eq")
move(x + 2, y - 2)
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["snap"] == "Equal" and close(e["d1"], e["d2"], 0.01), f"the snap to equal distance: {e}")
shot("02-equal-distance")
x, y = screen(p, "mid")
move(x, y)
time.sleep(0.5)
e = probe()["edge_dims"]
check(e and e["snap"] is not None and "Middle" in e["snap"], f"the snap to the middle of an edge: {e}")
shot("03-edge-middle")
# typed distances: 5 mm from the front edge, Tab, 7 mm from the left one
x, y = screen(p, "m1")
move(x, y)
time.sleep(0.4)
type_text("5")
key("Tab")
type_text("7")
time.sleep(0.4)
e = probe()["edge_dims"]
check(e and e["typed"] == ["5", "7"], f"the typed values are shown: {e}")
check(e and close(e["at"][0], 7, 0.05) and close(e["at"][1], 5, 0.05), f"the centre is at (7, 5): {e}")
shot("04-typed")
key("Return")
settle(0.4)
type_text("6")
key("Return")
p = settle()
check(len(p["circles"]) == 1, f"a circle was made: {p['circles']}")
c = p["circles"][0]
check(close(c["x"], 7, 0.05) and close(c["y"], 5, 0.05) and close(c["r"], 3, 0.05), f"its centre is (7, 5) and its diameter 6: {c}")
shot("05-circle-made")
# the distances of the circle are fields in the right column: change one
key("k")
settle(0.5)
key("s")
settle(0.3)
cx, cy = screen(p, "c")
# the outline of the circle: a point of it
p2 = probe()
click(*screen(p2, "circle_edge"))
p = settle()
check(p["entity_sel"] is not None, f"the circle is selected: {p['entity_sel']}")
click(*screen(p, "edge_dist_0"))
key("a", ctrl=True)
type_text("12")
key("Return")
p = settle(0.8)
c = p["circles"][0]
check(close(c["y"], 12, 0.05) or close(c["x"], 12, 0.05), f"the circle moved to 12 mm from an edge: {c}")
shot("06-moved-by-the-field")
finish()
