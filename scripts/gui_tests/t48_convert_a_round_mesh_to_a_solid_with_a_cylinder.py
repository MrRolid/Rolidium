#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh of a cylinder (24 flat strips) becomes a solid with a real cylindrical face: two discs and one cylinder, the radius is 5, the volume is the mesh's, and one Ctrl+Z
brings the mesh back; the report says that the round face was recognised."""
# FIXTURE: box 5 5 5
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/pin.stl", cylinder_triangles(5.0, 20.0, 24, cx=100.0, cy=0.0))
p = import_dropped(f"{W}/pin.stl")
mid = p["meshes"][-1]["id"]
v_mesh = p["meshes"][-1]["volume"]
click(*steady(f"mesh_row_{mid}"))
p = settle(0.6)
click(*screen(p, "mesh_convert"))
p = wait_for(lambda q: len(q["bodies"]) == 2 and not q["busy"], "the solid is made", 60)
b = p["bodies"][1]
check(b["faces"] == 3, f"two discs and one round face: {b['faces']} faces")
check(close(b["volume"], 3.14159265 * 25 * 20, 0.2), f"the volume is that of the true cylinder, a little more than the mesh's: {b['volume']} (mesh {v_mesh})")
p = settle(0.6)
c = p["extra"]["conversion"]
check(c and c["kinds"][:5] == [2, 1, 0, 0, 0] and c["single"] == 0, f"the report: one cylinder, two planes: {c}")
shot("01-cylinder")
key("z", ctrl=True)
wait_for(lambda q: len(q["bodies"]) == 1 and len(q["meshes"]) == 1, "one step of Undo brings the mesh back", 30)
finish()
