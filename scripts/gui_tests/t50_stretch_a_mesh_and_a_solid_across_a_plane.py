#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Stretch across a plane: a mesh block (20 mm) is stretched by 10 mm through its middle, the preview shows the stretched block in its place, Stretch makes it 30 mm long, one Ctrl+Z
takes it back; the same for the solid box of the fixture."""
# FIXTURE: box 10 10 10
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/blk.stl", box_triangles(20, 20, 20, 100, 0, 0, n=1))
p = import_dropped(f"{W}/blk.stl")
mid = p["meshes"][-1]["id"]
click(*steady(f"mesh_row_{mid}"))
p = settle(0.6)
check("stretch_header" in p["widgets"], "the panel Stretch across a plane is there")
click(*screen(p, "stretch_header"))
p = settle(0.5)
click(*screen(p, "stretch_axis_0"))
p = settle(0.4)
check(p["extra"]["stretch"]["plane"]["label"] == "X", f"the plane: {p['extra']['stretch']}")
click(*steady("stretch_distance"))
type_text("10")
settle(0.3)
shot("00-typed")
click(*steady("stretch_preview"))
p = wait_for(lambda q: q["extra"]["preview_mesh"] is not None, "the preview", 30)
pv = p["extra"]["preview_mesh"]
check(abs((pv["bbox"][3] - pv["bbox"][0]) - 30.0) < 1e-3, f"the preview is 30 mm long: {pv['bbox']}")
shot("01-preview")
click(*steady("stretch_apply"))
p = wait_for(lambda q: abs((q["meshes"][-1]["bbox"][3] - q["meshes"][-1]["bbox"][0]) - 30.0) < 1e-3 and not q["busy"], "stretched", 30)
check(p["extra"]["preview_mesh"] is None, "the preview is gone")
check(close(p["meshes"][-1]["volume"], 20 * 20 * 30, 0.001), f"the volume: {p['meshes'][-1]['volume']}")
shot("02-stretched")
key("z", ctrl=True)
wait_for(lambda q: abs((q["meshes"][-1]["bbox"][3] - q["meshes"][-1]["bbox"][0]) - 20.0) < 1e-3, "Undo", 30)
# the solid
bid = probe()["bodies"][0]["id"]
click(*steady(f"body_row_{bid}"))
p = settle(0.6)
check("stretch_header" in p["widgets"], "the panel for the solid")
if "stretch_axis_0" not in p["widgets"]:
    click(*screen(p, "stretch_header"))
    p = settle(0.5)
click(*screen(p, "stretch_axis_0"))
settle(0.3)
click(*steady("stretch_distance"))
key("a", ctrl=True)
type_text("4")
settle(0.3)
click(*steady("stretch_apply"))
p = wait_for(lambda q: close(q["bodies"][0]["volume"], 14 * 100, 0.001) and not q["busy"], "the solid is stretched", 30)
shot("03-solid")
key("z", ctrl=True)
wait_for(lambda q: close(q["bodies"][0]["volume"], 1000, 0.001), "Undo", 30)
finish()
