#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Import of STL files by dropping them into the window (two files at once): the dialog shows the size in the chosen units (millimetres, inches, a scale of
one's own), the check of each mesh (a good one, and one with flipped triangles and a vertex that is not welded) with the buttons that repair it; Cancel
imports nothing, Import adds the meshes to the list of bodies at the right size."""
# FIXTURE: box 20 20 20
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
p = settle()
check(len(p["meshes"]) == 0, "no mesh yet")
# a good box of 1 x 2 x 3 file units, and a box 10 mm with one triangle turned over and a vertex that lies 3e-6 beside the others
good = box_triangles(1, 2, 3, n=4)
bad = box_triangles(10, 10, 10, 50, 0, 0, n=4)
bad[3] = (bad[3][0], bad[3][2], bad[3][1])
bad[5] = tuple(tuple(c + (3e-6 if (i, k) == (0, 0) else 0) for k, c in enumerate(v)) for i, v in enumerate(bad[5]))
write_stl(f"{W}/good.stl", good)
write_stl(f"{W}/bad.stl", bad, ascii_=True)
drop_files(f"{W}/good.stl", f"{W}/bad.stl")
p = wait_for(lambda q: q["stl_dialog"], "the dialog of the import is open")
d = p["stl_dialog"]
check(len(d["items"]) == 2, f"two files in the dialog: {[i['name'] for i in d['items']]}")
byname = {i["name"]: i for i in d["items"]}
check(byname["good"]["triangles"] == 192 and byname["good"]["clean"], f"the good mesh is closed and consistent: {byname['good']}")
check(byname["good"]["size"] == [1.0, 2.0, 3.0], f"1 x 2 x 3 in millimetres: {byname['good']['size']}")
check(not byname["bad"]["clean"], f"the bad mesh is not clean: {byname['bad']['check']}")
shot("01-dialog-mm")
# inches
click(*screen(p, "stl_units_inch"))
p = settle(0.3)
d = p["stl_dialog"]
good_size = {i["name"]: i for i in d["items"]}["good"]["size"]
check(good_size == [25.4, 50.8, 76.2], f"in inches: 25.4 x 50.8 x 76.2 mm: {good_size}")
shot("02-dialog-inch")
# a scale of one's own: 10 mm per unit
click(*screen(p, "stl_units_custom"))
p = settle(0.3)
click(*screen(p, "stl_custom_field"))
key("a", ctrl=True)
type_text("10")
p = settle(0.4)
good_size = {i["name"]: i for i in p["stl_dialog"]["items"]}["good"]["size"]
check(good_size == [10.0, 20.0, 30.0], f"scale 10: {good_size}")
shot("03-dialog-custom")
# the repair of the bad mesh: merge the vertices, then turn the triangles
click(*screen(p, "stl_merge_1"))
p = wait_for(lambda q: q["stl_dialog"] and "merged" in {i["name"]: i for i in q["stl_dialog"]["items"]}["bad"]["note"], "the vertices are merged")
click(*screen(p, "stl_orient_1")) if "stl_orient_1" in p["widgets"] else None
p = wait_for(lambda q: q["stl_dialog"] and {i["name"]: i for i in q["stl_dialog"]["items"]}["bad"]["clean"], "the bad mesh is clean after the repairs", 20)
b = {i["name"]: i for i in p["stl_dialog"]["items"]}["bad"]
check(b["clean"], f"the repairs made the bad mesh closed and consistent: {b['check']}")
shot("04-dialog-repaired")
# Cancel imports nothing
click(*screen(p, "stl_import_cancel"))
p = settle(0.5)
check(p["stl_dialog"] is None and len(p["meshes"]) == 0, "Cancel: the dialog is gone and nothing was imported")
# again, in inches, and Import
drop_files(f"{W}/good.stl", f"{W}/bad.stl")
p = wait_for(lambda q: q["stl_dialog"], "the dialog is open again")
click(*screen(p, "stl_units_inch"))
p = settle(0.3)
click(*screen(p, "stl_import_ok"))
p = wait_for(lambda q: len(q["meshes"]) == 2, "two meshes were imported")
m = {x["name"]: x for x in p["meshes"]}
bb = m["good"]["bbox"]
check(all(close(a, b, 0.001) for a, b in zip(bb, [0, 0, 0, 25.4, 50.8, 76.2])), f"the good mesh is 25.4 x 50.8 x 76.2 mm: {bb}")
check(m["good"]["triangles"] == 192 and m["bad"]["triangles"] == 192, "192 triangles each")
p = settle(1.0)
check(any(x["walls"] == 6 for x in p["meshes"]), f"the walls were found in the background: {[x['walls'] for x in p['meshes']]}")
shot("05-imported")
finish()
