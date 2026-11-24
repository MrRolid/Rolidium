#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A wall of a mesh is selected and K starts a sketch on it (the plane of the wall); a circle is drawn; Pull makes a new solid of it that stands on the wall of the mesh
(the mesh itself is not changed: a mesh is not a solid and cannot be joined with one yet, which the status line says when it is tried)."""
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
p = wait_for(lambda q: len(q["bodies"]) == 1, "a solid was made", 20)
b = p["bodies"][0]
check(close(b["bbox"][2], 10.0, 0.01) and close(b["bbox"][5], 13.0, 0.01), f"the new solid stands on the wall, z 10 to 13: {b['bbox']}")
check(close(p["meshes"][0]["bbox"][5], 10.0, 0.001), "the mesh was not changed")
shot("02-pulled")
finish()
