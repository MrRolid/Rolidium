#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The top wall of a block with slanting sides (a mesh): the preview does not promise a pull that will be refused (a red outline and the reason at the cursor, no
prism), the pull is refused with the reason and nothing changes; with "Move anyway, the neighbouring walls slant" the preview is the prism and the pull is made
(the mesh stays closed), Ctrl+Z takes it back; a pull through the opposite wall is shown as not possible too."""
# FIXTURE: box 5 5 5
# POINTS: top=80,20,20
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/frustum.stl", frustum_triangles(40.0, 20.0, 20.0, 60.0, 0.0))
p = import_dropped(f"{W}/frustum.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][-1]["walls"] == 6, "the six walls are found", 30)
v0 = p["meshes"][-1]["volume"]
key("p")
settle(0.4)
click(*steady("top"))
p = settle(0.5)
check(len(p["selection"]) == 1, f"the top wall is selected: {p['selection']}")
type_text("5")
p = settle(0.5)
x = p["extra"]
check(x["pull_blocked"] and "not square" in x["pull_blocked"], f"the reason is at the cursor: {x['pull_blocked']}")
check(x["preview_thick"] and not x["preview_triangles"], f"a red outline, no prism: {x['preview_thick']} {x['preview_triangles']}")
shot("01-blocked-preview")
key("Return")
p = settle(1.0)
check(p["status_error"] and "not square" in p["status"], f"refused with the reason: {p['status']}")
check(close(p["meshes"][-1]["bbox"][5], 20.0, 0.001), f"nothing changed: {p['meshes'][-1]['bbox']}")
# the choice in the panel
click(*screen(p, "mesh_tilt"))
p = settle(0.4)
check(p["extra"]["tilt"], "Move anyway is ticked")
key("p")
type_text("5")
p = settle(0.5)
check(p["extra"]["pull_blocked"] is None and p["extra"]["preview_triangles"], f"the preview is the prism now: {p['extra']['pull_blocked']}")
shot("02-tilt-preview")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull") and close(q["meshes"][-1]["bbox"][5], 25.0, 0.001), "the wall was pulled by 5 mm", 30)
check(p["meshes"][-1]["volume"] > v0 + 1000, f"the volume grew: {v0} -> {p['meshes'][-1]['volume']}")
shot("03-pulled")
key("z", ctrl=True)
p = wait_for(lambda q: close(q["meshes"][-1]["bbox"][5], 20.0, 0.001), "Undo", 30)
check(close(p["meshes"][-1]["volume"], v0, 0.01), "the volume is back")
# far too far: the top would pass the bottom
p = wait_for(lambda q: q["meshes"][-1]["walls"] == 6, "the walls are found again", 30)
click(*steady("top"))
p = settle(0.4)
key("p")
type_text("-60")
p = settle(0.5)
check(p["extra"]["pull_blocked"] and "far" in p["extra"]["pull_blocked"], f"too far is shown as not possible: {p['extra']['pull_blocked']}")
shot("04-too-far")
key("Escape")
finish()
