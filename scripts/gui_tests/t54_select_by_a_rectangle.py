#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Select tool, a drag from an empty place: from left to right a full rectangle selects the bodies that lie inside it entirely (one that is cut by the edge is not), from right to
left a dashed rectangle of another colour selects also the bodies it only crosses; Ctrl adds to the selection. The camera is not touched (it turns and moves with the
middle and the right button)."""
# FIXTURE: box 20 20 20; box 20 20 20; move 100 0 0 body 2
# POINTS: a0=0,0,0;a1=0,0,20;a2=0,20,0;a3=0,20,20;a4=20,0,0;a5=20,0,20;a6=20,20,0;a7=20,20,20;b0=100,0,0;b1=100,0,20;b2=100,20,0;b3=100,20,20;b4=120,0,0;b5=120,0,20;b6=120,20,0;b7=120,20,20
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("s")
settle(0.3)
cam0 = p["extra"]["camera"]

def box_of(p, prefix):
    xs = [p["points"][f"{prefix}{i}"][0] for i in range(8)]
    ys = [p["points"][f"{prefix}{i}"][1] for i in range(8)]
    return min(xs), min(ys), max(xs), max(ys)

ax0, ay0, ax1, ay1 = box_of(p, "a")
bx0, by0, bx1, by1 = box_of(p, "b")
print("boxes", (ax0, ay0, ax1, ay1), (bx0, by0, bx1, by1))
sel = lambda q: sorted(set(q["extra"]["body_dims_bodies"]))

def rect(x0, y0, x1, y1, ctrl=False):
    c = keycode("Control_L")
    if ctrl:
        xtst.XTestFakeKeyEvent(D, c, 1, 0)
        sync(0.1)
    release = drag(x0, y0, x1, y1)
    time.sleep(0.3)
    release()
    if ctrl:
        xtst.XTestFakeKeyEvent(D, c, 0, 0)
        sync(0.1)

# 1. a full rectangle round the first body only
rect(ax0 - 25, ay0 - 25, ax1 + 25, ay1 + 25)
p = settle(0.5)
check(sel(p) == [1], f"the full rectangle takes the first body: {sel(p)}")
check("1 bodies selected" in p["status"] or "selected" in p["status"], f"the status says so: {p['status']}")
shot("01-window-first")
# 2. a full rectangle that cuts the second body in the middle selects nothing of it
cx = (bx0 + bx1) / 2
rect(ax0 - 25, ay0 - 25, cx, max(ay1, by1) + 25)
p = settle(0.5)
check(sel(p) == [1], f"a body that the edge of the full rectangle cuts is not taken: {sel(p)}")
# 3. a dashed rectangle from right to left takes the one it crosses
rect(bx1 + 25, by1 + 25, cx, by0 - 25)
p = settle(0.5)
check(sel(p) == [2], f"the dashed rectangle takes the second body: {sel(p)}")
shot("02-crossing-second")
# 4. Ctrl adds the first body to the second
rect(ax0 - 25, ay0 - 25, ax1 + 25, ay1 + 25, ctrl=True)
p = settle(0.5)
check(sel(p) == [1, 2], f"Ctrl adds: {sel(p)}")
shot("03-both")
settle(0.3)
cam1 = probe()["extra"]["camera"]
check(cam0 == cam1, f"the camera did not move: {cam0} {cam1}")
finish()
