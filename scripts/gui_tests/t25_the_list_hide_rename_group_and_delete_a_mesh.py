#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh in the list of bodies (marked as a mesh, with its triangles): a click selects it, the eye hides and shows it, F2 renames it, Ctrl+G puts it into a group,
Delete removes it and Ctrl+Z brings it back; each is one step of Undo."""
# FIXTURE: box 20 20 20; import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle(1.0)
check(len(p["meshes"]) == 1 and len(p["bodies"]) == 1, "a mesh and a solid")
mid = p["meshes"][0]["id"]
click(*screen(p, f"mesh_row_{mid}"))
p = settle(0.5)
shot("01-selected-in-the-list")
# hide and show
click(*screen(p, f"mesh_eye_{mid}"))
p = settle(0.8)
check(not p["meshes"][0]["visible"] and p["meshes"][0]["shown"] is None, f"hidden: {p['meshes'][0]['visible']}, shown {p['meshes'][0]['shown']}")
shot("02-hidden")
click(*screen(p, f"mesh_eye_{mid}"))
p = settle(0.8)
check(p["meshes"][0]["visible"] and p["meshes"][0]["shown"], "shown again")
# rename with F2
click(*screen(p, f"mesh_row_{mid}"))
settle(0.4)
key("F2")
settle(0.4)
key("a", ctrl=True)
type_text("plate2")
key("Return")
p = settle(0.6)
check(p["meshes"][0]["name"] == "plate2", f"renamed: {p['meshes'][0]['name']}")
shot("03-renamed")
# a group
click(*screen(p, f"mesh_row_{mid}"))
settle(0.4)
key("g", ctrl=True)
p = settle(0.6)
check(p["groups"] == 1 and p["meshes"][0]["group"], f"in a group: {p['meshes'][0]['group']}")
shot("04-grouped")
key("z", ctrl=True)
p = settle(0.6)
check(p["groups"] == 0 and p["meshes"][0]["group"] is None, "Undo takes it out of the group")
# delete and bring back
click(*screen(p, f"mesh_row_{mid}"))
settle(0.4)
key("Delete")
p = settle(0.8)
check(len(p["meshes"]) == 0, "deleted")
shot("05-deleted")
key("z", ctrl=True)
p = settle(0.8)
check(len(p["meshes"]) == 1 and p["meshes"][0]["shown"], "Ctrl+Z brings it back")
finish()
