#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh tray, a box with a cavity open at the top (a floor of 1 mm): the outer bottom (seen from below) and the floor of the cavity (seen from above) show the normals that look out of the material;
the floor is made thicker by 3 mm (Add), pushing it 1.5 mm down through the bottom is refused with how far it can go ("at most 1 mm down"), and the outer bottom pulled
outwards (5) goes down by 5 mm while -5 (inwards) is refused the same way."""
# FIXTURE: box 5 5 5
# POINTS: floor=120,15,1;bottom=120,15,0
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/hollow.stl", tray_triangles(100, 0))
p = import_dropped(f"{W}/hollow.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][-1]["walls"] == 11, "the walls are found", 30)
vx, vy, vw, vh = p["viewport"]
v0 = p["meshes"][-1]["volume"]
bb0 = p["meshes"][-1]["bbox"]
# the floor of the cavity from above
middle_drag(vx + vw / 2, vy + vh * 0.2, vx + vw / 2, vy + vh * 0.95)
settle(0.8)
click(*steady("floor"))
p = settle(0.6)
check(p["face"] and p["face"]["normal"] == [0.0, 0.0, 1.0], f"the floor of the cavity looks up (out of the material): {p['face']}")
key("p")
settle(0.3)
type_text("3")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull +3") or q["status_error"], "the floor was thickened", 30)
check(not p["status_error"], f"the floor of 1 mm can be made thicker: {p['status']}")
check(p["meshes"][-1]["volume"] > v0 + 3 * 28 * 18, f"the volume grew: {v0} -> {p['meshes'][-1]['volume']}")
key("z", ctrl=True)
p = wait_for(lambda q: close(q["meshes"][-1]["volume"], v0, 0.01), "Undo", 30)
# pushed down through the bottom
p = wait_for(lambda q: q["meshes"][-1]["walls"] == 11 and not q["busy"], "the walls are known again", 30)
click(*steady("floor"))
settle(0.5)
key("p")
type_text("-1.5")
p = settle(0.6)
check(p["extra"]["pull_blocked"], f"the preview says no: {p['extra']['pull_blocked']}")
key("Return")
p = settle(1.2)
check(p["status_error"] and "at most" in p["status"] and "down" in p["status"], f"the refusal says how far: {p['status']}")
check(close(p["meshes"][-1]["volume"], v0, 0.01), "the mesh did not change")
shot("floor-refused")
# the outer bottom, from below
middle_drag(vx + vw / 2, vy + vh * 0.95, vx + vw / 2, vy + vh * 0.05)
middle_drag(vx + vw / 2, vy + vh * 0.9, vx + vw / 2, vy + vh * 0.3)
settle(0.8)
click(*steady("bottom"))
p = settle(0.6)
check(p["face"] and p["face"]["normal"] == [0.0, 0.0, -1.0], f"the outer bottom looks down (out of the material): {p['face']}")
shot("bottom-selected")
key("p")
type_text("5")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull +5") or q["status_error"], "the bottom was pulled outwards", 30)
check(not p["status_error"] and close(p["meshes"][-1]["bbox"][2], -5.0, 0.01), f"the bottom went down by 5 mm: {p['meshes'][-1]['bbox']}")
finish()
