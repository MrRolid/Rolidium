#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pull (P) with the mouse: dragging a wall of a mesh block outwards along its normal on the screen makes the block bigger in that direction (never the other way),
for every wall that can be seen, with the camera above, below, behind and at the side."""
# FIXTURE: box 5 5 5
# POINTS: cxp=120,10,10;cxm=100,10,10;cyp=110,20,10;cym=110,0,10;czp=110,10,20;czm=110,10,0;axp=140,10,10;axm=80,10,10;ayp=110,40,10;aym=110,-20,10;azp=110,10,40;azm=110,10,-20
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/blk.stl", box_triangles(20, 20, 20, 100, 0, 0, n=2))
p = import_dropped(f"{W}/blk.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][-1]["walls"] == 6, "the six walls are found", 30)
vx, vy, vw, vh = p["viewport"]
cx, cy = vx + vw / 2, vy + vh / 2
WALLS = [("xp", 0, 1), ("xm", 0, -1), ("yp", 1, 1), ("ym", 1, -1), ("zp", 2, 1), ("zm", 2, -1)]


def size(q, ax):
    b = q["meshes"][-1]["bbox"]
    return b[ax + 3] - b[ax]


def try_walls(label):
    for name, ax, sg in WALLS:
        q = settle(0.4)
        c = q["points"].get("c" + name)
        a = q["points"].get("a" + name)
        if not c or not a:
            continue
        # the wall must be the one under the centre (visible from this side)
        move(*c)
        time.sleep(0.4)
        h = probe()["hover"]
        want = [0.0, 0.0, 0.0]
        want[ax] = float(sg)
        if not (h and h.get("normal") and all(abs(x - y) < 0.02 for x, y in zip(h["normal"], want))):
            continue
        dx, dy = a[0] - c[0], a[1] - c[1]
        ln = (dx * dx + dy * dy) ** 0.5
        if ln < 12:
            print(f"skip {label} {name}: the normal points at the viewer")
            continue
        key("s")
        click(*c)
        settle(0.4)
        key("p")
        settle(0.3)
        s0 = size(probe(), ax)
        release = drag(c[0], c[1], c[0] + dx / ln * 50, c[1] + dy / ln * 50, steps=10)
        time.sleep(0.4)
        release()
        q = wait_for(lambda z: not z["busy"] and abs(size(z, ax) - s0) > 1e-4, f"{label} {name}: the wall was pulled", 20)
        check(size(q, ax) > s0, f"{label} {name}: dragged outwards on the screen, the block grew along {'xyz'[ax]}: {s0:.2f} -> {size(q, ax):.2f}")
        settle(0.8)
        wait_for(lambda z: not z["busy"] and z["mesh_job"] is None, "the program is idle", 30)
        key("z", ctrl=True)
        wait_for(lambda z: abs(size(z, ax) - s0) < 1e-3 and z["meshes"][-1]["walls"] == 6, "Undo", 30)


try_walls("iso")
middle_drag(cx, vy + vh * 0.95, cx, vy + vh * 0.05)
middle_drag(cx, vy + vh * 0.9, cx, vy + vh * 0.3)
try_walls("below")
middle_drag(cx - 300, cy, cx + 300, cy)
try_walls("below-turned")
middle_drag(cx, vy + vh * 0.05, cx, vy + vh * 0.95)
try_walls("above-turned")
middle_drag(cx - 200, cy + 40, cx + 220, cy - 60)
try_walls("side")
finish()
