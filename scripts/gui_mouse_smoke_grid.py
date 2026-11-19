#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Drag a face with grid step 0.1 at a high zoom and read the distance from the status bar."""
import subprocess, time
from xdrive import *

time.sleep(1.0)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)
move(70, 451); click(); time.sleep(0.6)
move(665, 788); click(); key("a", ctrl=True); type_text("0.1"); key("Return"); time.sleep(0.5)
move(600, 300); time.sleep(0.4); click(); time.sleep(0.3)
key("p")
for _ in range(int(__import__("os").environ.get("ZOOM", "7"))):
    xtst.XTestFakeButtonEvent(ctypes.c_void_p(dpy), 4, 1, 0)
    xtst.XTestFakeButtonEvent(ctypes.c_void_p(dpy), 4, 0, 0)
    sync()
time.sleep(0.5)
shot("g1-zoomed")
for dy in (8, 9, 10, 11, 12):
    move(600, 450); d = drag(600, 450, 600, 450 - dy, steps=dy)
    time.sleep(0.5)
    subprocess.run(["import", "-window", "root", "-crop", "320x20+330+780", f"{out}/g2-status-{dy}px.png"], check=True)
    key("Escape"); d(); time.sleep(0.6)
