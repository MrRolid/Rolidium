#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A circle drawn on the base plane XY, through the middle of a mesh block (the plane belongs to no body), pulled inwards in Auto (typed -30, down through the block
below the plane): a pocket of that depth is cut out of the mesh (10 mm deep, not a hole through, not "There is nothing to remove"), one Ctrl+Z gives the block back."""
# FIXTURE: box 1 1 1
# POINTS: cc=5,5,0;ce=7,5,0
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/blk.stl", box_triangles(20, 20, 20, -5, -5, -10, n=1))
p = import_dropped(f"{W}/blk.stl")
mid = p["meshes"][-1]["id"]
v0 = p["meshes"][-1]["volume"]
check(close(v0, 8000, 0.001), f"the block: {v0}")
key("Escape")
settle(0.3)
# a sketch on the base plane: K with nothing selected asks for the plane, Enter takes XY
key("k")
p = settle(0.5)
key("Return")
p = settle(1.5)
check(p["sketching"] is not None, f"a sketch is open: {p['sketching']}")
key("c")
settle(0.3)
click(*screen(p, "cc"))
click(*screen(p, "ce"))
p = settle(0.5)
check(len(p["circles"]) == 1, f"the circle is drawn: {p['circles']}")
shot("01-circle")
key("Escape")
key("Escape")
settle(0.4)
key("p")
p = settle(0.4)
click(*screen(p, "cc"))
p = settle(0.4)
check(len(p["extra"]["regions_selected"]) == 1, f"the region is selected: {p['extra']['regions_selected']}")
type_text("-30")
settle(0.4)
key("Return")
p = wait_for(lambda q: not q["busy"] and q["meshes"] and abs(q["meshes"][-1]["volume"] - v0) > 1, "the mesh is cut", 60)
want = 8000 - math.pi * 4 * 10
check(close(p["meshes"][-1]["volume"], want, 1.0), f"a pocket 10 mm deep: {p['meshes'][-1]['volume']} (expected {want:.1f}); {p['status']}")
shot("02-pocket")
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"] and close(q["meshes"][-1]["volume"], 8000, 0.001), "Ctrl+Z gives the block back", 30)
finish()
