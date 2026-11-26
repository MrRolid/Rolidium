#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Combine a mesh and a solid: the mesh is the target and the solid the tool. Cut takes the overlap out of the mesh (the solid is used up), Ctrl+Z brings both back; Merge
joins them into one mesh. The mesh block has a missing triangle (a small flat hole), which is patched and said."""
# FIXTURE: box 10 10 10
# POINTS: mtop=15,15,20;boxtop=2,2,10
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
tris = box_triangles(20, 20, 20, 5, 5, 0, n=2)
write_stl(f"{W}/blk.stl", tris)
p = import_dropped(f"{W}/blk.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][-1]["walls"] is not None, "the walls are found", 30)
v0 = p["meshes"][-1]["volume"]
check(close(v0, 8000.0, 0.01), f"the mesh block: {v0}")
mid = p["meshes"][-1]["id"]


def combine(op):
    key("i")
    settle(0.5)
    p = probe()
    for name in ("mtop", "boxtop"):
        x, y = screen(p, name)
        move(x, y)
        time.sleep(0.5)
        click(x, y)
        time.sleep(0.4)
    p = settle(0.6)
    check(p["combine"]["target"] == mid and len(p["combine"]["tools"]) == 1, f"the target is the mesh, the tool the solid: {p['combine']}")
    click(*steady(f"combine_op_{op}"))
    settle(0.3)
    click(*steady("combine_apply"))


combine("cut")
p = wait_for(lambda q: len(q["bodies"]) == 0 and not q["busy"], "the cut is made and the tool is used up", 60)
check(close(p["meshes"][-1]["volume"], 7750.0, 1.0), f"the overlap (5 x 5 x 10) is out of the mesh: {p['meshes'][-1]['volume']}")
shot("01-cut")
key("z", ctrl=True)
p = wait_for(lambda q: len(q["bodies"]) == 1 and close(q["meshes"][-1]["volume"], 8000.0, 0.01), "one step of Undo brings both back", 30)
key("s")
combine("merge")
p = wait_for(lambda q: len(q["bodies"]) == 0 and not q["busy"] and not close(q["meshes"][-1]["volume"], 8000.0, 1.0), "the merge is made", 60)
check(close(p["meshes"][-1]["volume"], 8750.0, 1.0), f"the union: {p['meshes'][-1]['volume']}")
shot("02-merged")
finish()
