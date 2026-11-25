#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Move (M): dragging an arrow outwards on the screen moves the body along the axis in the positive direction, the same way for a solid and for a mesh, in all three
axes, with the camera above, below, behind and at the side, and after the body was turned by 180 degrees."""
# FIXTURE: box 40 30 20; box 10 10 10
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/blk.stl", box_triangles(20, 20, 20, 100, 0, 0, n=2))
p = import_dropped(f"{W}/blk.stl")
mesh_id = p["meshes"][-1]["id"]
box_id = p["bodies"][0]["id"]
vx, vy, vw, vh = p["viewport"]
cx, cy = vx + vw / 2, vy + vh / 2


def centre_of(q, kind, ident):
    bb = (q["meshes"] if kind == "mesh" else q["bodies"])
    b = next(x for x in bb if x["id"] == ident)["bbox"]
    return [(b[i] + b[i + 3]) / 2 for i in range(3)]


def try_axes(label, kind, ident, row):
    for ax, name in enumerate("xyz"):
        key("m")
        settle(0.3)
        click(*steady(row))
        q = settle(0.6)
        arr = q["extra"]["move_arrows"]
        if not arr or not arr["centre"] or not arr[name]:
            check(False, f"{label} {kind} {name}: no arrow")
            continue
        c, t = arr["centre"], arr[name]
        dx, dy = t[0] - c[0], t[1] - c[1]
        ln = (dx * dx + dy * dy) ** 0.5
        if ln < 40:
            print(f"skip {label} {kind} {name}: the axis points at the viewer ({ln:.0f} px)")
            continue
        ux, uy = dx / ln, dy / ln
        x0, y0 = c[0] + dx * 0.9, c[1] + dy * 0.9
        before = centre_of(q, kind, ident)
        release = drag(x0, y0, x0 + ux * 40, y0 + uy * 40, steps=10)
        time.sleep(0.3)
        release()
        q = wait_for(lambda z: not z["busy"] and abs(centre_of(z, kind, ident)[ax] - before[ax]) > 1e-6, f"{label} {kind} {name}: the body moved", 20)
        after = centre_of(q, kind, ident)
        moved = after[ax] - before[ax]
        check(moved > 0, f"{label} {kind} {name}: dragged outwards along the arrow: moved {moved:+.2f} mm along +{name}")
        settle(0.8)
        wait_for(lambda z: not z["busy"] and z["mesh_job"] is None, "the program is idle", 30)
        key("z", ctrl=True)
        wait_for(lambda z: abs(centre_of(z, kind, ident)[ax] - before[ax]) < 1e-3, "Undo", 20)
        key("s")


def look(label):
    q = settle(0.6)
    print("camera", label, q["extra"]["camera"])
    shot(f"cam-{label}")


for label, kind, ident, row in [("box", "body", box_id, f"body_row_{box_id}"), ("mesh", "mesh", mesh_id, f"mesh_row_{mesh_id}")]:
    key("s")
    look("iso")
    try_axes("iso-" + label, kind, ident, row)
    middle_drag(cx, vy + vh * 0.95, cx, vy + vh * 0.05)
    middle_drag(cx, vy + vh * 0.9, cx, vy + vh * 0.3)
    look("below")
    try_axes("below-" + label, kind, ident, row)
    middle_drag(cx - 300, cy, cx + 300, cy)
    look("below-turned")
    try_axes("below-turned-" + label, kind, ident, row)
    middle_drag(cx, vy + vh * 0.05, cx, vy + vh * 0.95)
    look("above-turned")
    try_axes("above-turned-" + label, kind, ident, row)
    middle_drag(cx - 200, cy + 40, cx + 220, cy - 60)
    look("side")
    try_axes("other-" + label, kind, ident, row)
finish()
