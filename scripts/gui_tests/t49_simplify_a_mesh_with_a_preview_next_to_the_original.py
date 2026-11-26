#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Simplify the mesh: a block of 6144 triangles goes down to 5 per cent with the sharp edges kept; the result is shown next to the original (the same view), the panel gives the
numbers; Apply makes it the mesh, one Ctrl+Z brings the original back."""
# FIXTURE: box 5 5 5
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/blk.stl", box_triangles(20, 20, 20, 100, 0, 0, n=16))
p = import_dropped(f"{W}/blk.stl")
mid = p["meshes"][-1]["id"]
n0 = p["meshes"][-1]["triangles"]
check(n0 == 6 * 16 * 16 * 2, f"the block has {n0} triangles")
click(*steady(f"mesh_row_{mid}"))
p = settle(0.6)
check("simplify_header" in p["widgets"], "the panel Simplify the mesh is there")
click(*screen(p, "simplify_header"))
p = settle(0.5)
fx, fy = screen(p, "simplify_value")
click(fx, fy)
key("a", ctrl=True)
type_text("5")
click(*screen(p, "simplify_preview"))
p = wait_for(lambda q: q["extra"]["simplify"]["result"] is not None, "the result of the simplification", 60)
r = p["extra"]["simplify"]["result"]
check(r["before"] == n0 and r["after"] <= n0 * 0.05 + 2 and r["after"] >= 12, f"the numbers: {r}")
check(r["max_dev"] < 1e-3, f"a flat block loses no shape: {r['max_dev']}")
p = settle(0.5)
pv = p["extra"]["preview_mesh"]
check(pv and pv["triangles"] == r["after"], f"the result is shown: {pv}")
orig = p["meshes"][-1]["bbox"]
check(pv["bbox"][0] > orig[3], f"it stands next to the original: {pv['bbox']} / {orig}")
shot("01-preview")
click(*screen(p, "simplify_apply"))
p = wait_for(lambda q: q["meshes"][-1]["triangles"] <= n0 * 0.05 + 2 and not q["busy"], "applied", 60)
check(p["extra"]["preview_mesh"] is None, "the preview is gone")
check(close(p["meshes"][-1]["volume"], 8000.0, 0.001), f"the volume is the same: {p['meshes'][-1]['volume']}")
shot("02-applied")
key("z", ctrl=True)
wait_for(lambda q: q["meshes"][-1]["triangles"] == n0, "one step of Undo brings the original back", 30)
finish()
