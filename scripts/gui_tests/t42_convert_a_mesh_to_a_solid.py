#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh made a solid: from the panel (the faces are merged, the volume is the mesh's, the report is shown, one Ctrl+Z brings the mesh back) and by the choice in the dialog
of the import ("Import as: solid", advised for a mesh of flat pieces); the solid is edited like a drawn one (its floor is pulled)."""
# FIXTURE: box 5 5 5
# POINTS: floor=120,15,1
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/tray.stl", tray_triangles(100, 0))
drop_files(f"{W}/tray.stl")
p = wait_for(lambda q: q["stl_dialog"], "the dialog of the import", 30)
check(p["stl_dialog"]["as_solid"] and "Advised: a solid" in p["stl_dialog"]["advice"], f"a mesh of flat pieces is advised to be a solid: {p['stl_dialog']['advice']}")
shot("01-dialog-advice")
click(*steady("stl_as_mesh"))
settle(0.3)
check(not probe()["stl_dialog"]["as_solid"], "the choice: a mesh")
click(*steady("stl_import_ok"))
p = wait_for(lambda q: len(q["meshes"]) == 1 and q["stl_dialog"] is None, "imported as a mesh", 30)
mid = p["meshes"][0]["id"]
v_mesh = p["meshes"][0]["volume"]
click(*steady(f"mesh_row_{mid}"))
p = settle(0.6)
check("mesh_convert" in p["widgets"], "the button Convert to solid is in the panel")
shot("02-panel")
click(*screen(p, "mesh_convert"))
p = wait_for(lambda q: len(q["bodies"]) == 2 and not q["busy"], "the solid is made", 60)
b = p["bodies"][1]
check(not p["meshes"], "the mesh was replaced by the solid")
check(b["faces"] == 11, f"the flat pieces are merged: {b['faces']} faces")
check(close(b["volume"], v_mesh, 0.001), f"the volume is the mesh's: {b['volume']} against {v_mesh}")
p = settle(0.6)
c = p["extra"]["conversion"]
check(c and c["kinds"][0] == 11 and c["single"] == 0 and c["deviation"] < 0.02, f"the report of the conversion: {c}")
shot("03-converted")
key("z", ctrl=True)
p = wait_for(lambda q: len(q["bodies"]) == 1 and len(q["meshes"]) == 1, "one step of Undo brings the mesh back", 30)
# the solid is edited like a drawn one: the floor is pulled
key("y", ctrl=True)
p = wait_for(lambda q: len(q["bodies"]) == 2, "Redo", 30)
bid = p["bodies"][1]["id"]
v0 = p["bodies"][1]["volume"]
# from the top, the floor of the cavity (z = 1)
vx, vy, vw, vh = p["viewport"]
middle_drag(vx + vw / 2, vy + vh * 0.2, vx + vw / 2, vy + vh * 0.95)
settle(0.8)
key("s")
settle(0.4)
fx, fy = steady("floor")
move(fx, fy)
time.sleep(0.5)
click(fx, fy)
p = settle(0.5)
sel = p["selection"]
check(len(sel) == 1, f"a face of the solid is selected: {sel}")
key("p")
type_text("3")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull") or q["status_error"], "pulled", 30)
check(not p["status_error"], f"the face of the converted solid was pulled: {p['status']}")
shot("04-pulled")
# by the dialog of the import
n_bodies = len(probe()["bodies"])
drop_files(f"{W}/tray.stl")
p = wait_for(lambda q: q["stl_dialog"], "the dialog again", 30)
check(p["stl_dialog"]["as_solid"], "the advice is a solid")
click(*steady("stl_import_ok"))
p = wait_for(lambda q: len(q["bodies"]) == n_bodies + 1 and not q["busy"], "imported and converted at once", 60)
check(len(p["meshes"]) == 0, f"no mesh is left, the file became a solid: {len(p['meshes'])}")
finish()
