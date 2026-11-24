#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh (STL) in the window: the pointer over a wall lights the wall (its kind is known), a click selects it and the panel gives its area and the distance to the
opposite wall; over the wall of a hole the diameter; a click beside the mesh takes nothing. The pointer picks with the hierarchy of boxes on the processor."""
# FIXTURE: import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10;front=30,0,5;h0=32.5,20,9;h1=31.77,21.77,9;h2=30,22.5,9;h3=28.23,21.77,9;h4=27.5,20,9;h5=28.23,18.23,9;h6=30,17.5,9;h7=31.77,18.23,9
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls and holes are found", 30)
m = p["meshes"][0]
check(m["walls"] == 6, f"six walls: {m['walls']}")
check(len(m["holes"]) == 1 and close(m["holes"][0]["d"], 5.0, 0.01) and m["holes"][0]["hole"], f"one hole of 5 mm: {m['holes']}")
p = settle(0.5)
# the pointer over the top wall: it is a flat wall facing up
move(*screen(p, "top"))
time.sleep(0.8)
h = probe()["hover"]
check(h and h["kind"] == "Plane" and close(h["normal"][2], 1.0, 0.01), f"the top wall is under the pointer: {h}")
shot("01-hover-wall")
click(*screen(p, "top"))
p = settle(0.5)
f = p["face"]
check(f and f["mesh"] and f["kind"] == "Plane" and close(f["area"], 2380.43, 0.5), f"the top wall is selected, its area is 2380.43 mm2 (the hole is taken off): {f}")
rows = dict(p["panel"])
check("Distance to opposite face" in rows and close(float(rows["Distance to opposite face"]), 10.0, 0.01), f"the panel gives the thickness, 10 mm: {p['panel']}")
shot("02-selected-wall")
# the front wall
click(*screen(p, "front"))
p = settle(0.5)
f = p["face"]
check(f and close(f["normal"][1], -1.0, 0.01) and close(f["area"], 600.0, 0.5), f"the front wall is selected: {f}")
rows = dict(p["panel"])
check("Distance to opposite face" in rows and close(float(rows["Distance to opposite face"]), 40.0, 0.01), f"the distance to the back wall is 40 mm: {p['panel']}")
shot("03-front-wall")
# the wall of the hole
x, y = hover_cylinder(p, [f"h{k}" for k in range(8)])
shot("04-hover-hole")
click(x, y)
p = settle(0.5)
f = p["face"]
check(f and f["kind"] == "Cylinder" and close(f["diameter"], 5.0, 0.01), f"the hole is selected, 5 mm: {f}")
rows = dict(p["panel"])
check("Diameter" in rows and close(float(rows["Diameter"]), 5.0, 0.01), f"the panel has the diameter: {p['panel']}")
shot("05-selected-hole")
# a click on the background takes nothing
vx, vy, vw, vh = p["viewport"]
click(vx + 40, vy + vh - 40)
p = settle(0.5)
check(p["selection"] == [], f"nothing is selected after a click beside the mesh: {p['selection']}")
check(p["pick"]["bvh"], f"the pick used the hierarchy of boxes: {p['pick']}")
finish()
