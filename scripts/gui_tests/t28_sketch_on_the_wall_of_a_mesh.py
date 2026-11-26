#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A wall of a mesh is selected and K starts a sketch on it (the plane of the wall); a circle is drawn; Pull (mode Auto) joins the new volume to the mesh: the mesh grows a boss that
stands on the wall (since 0.3.2 a mesh can be joined and cut; Ctrl+Z takes the boss away)."""
# FIXTURE: import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10;c=15,10,10;r=20,10,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls are found", 30)
click(*screen(p, "top"))
p = settle(0.5)
key("k")
p = settle(2.0)
check(p["sketching"] is not None, f"the sketch is open: {p['sketching']}")
check("wall of the mesh" in p["status"], f"the status line says it is the wall of a mesh: {p['status']}")
key("c")
p = settle(0.8)
steady("c")
click(*steady("c"))
p = settle(0.5)
type_text("5")
key("Return")
p = settle(0.6)
c = p["circles"][0] if p["circles"] else None
check(c is not None and close(c["x"], 15, 0.6) and close(c["y"], 10, 0.6) and close(c["r"], 2.5, 0.05), f"a circle of 5 mm across at (15, 10) in the plane of the wall: {c}")
shot("01-circle-on-the-wall")
key("Escape")
p = settle(0.4)
key("p")
p = settle(0.6)
click(*steady("c"))
p = settle(0.8)
type_text("3")
key("Return")
p = wait_for(lambda q: q["meshes"] and close(q["meshes"][0]["bbox"][5], 13.0, 0.01) and not q["busy"], "the boss is joined to the mesh", 60)
check(len(p["bodies"]) == 0, f"no solid of its own: {len(p['bodies'])} bodies")
check(close(p["meshes"][0]["bbox"][2], 0.0, 0.01), f"the mesh keeps its floor: {p['meshes'][0]['bbox']}")
shot("02-pulled")
key("z", ctrl=True)
wait_for(lambda q: q["meshes"] and close(q["meshes"][0]["bbox"][5], 10.0, 0.001), "one step of Undo takes the boss away", 30)
finish()
