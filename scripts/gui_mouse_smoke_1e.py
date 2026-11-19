#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1e with the real mouse and keyboard: Ctrl+click and Shift+click multi-selection, rounding of all
selected edges, tangent chain highlight, grid step 0.1. Run: scripts/gui-mouse-smoke.sh <dir> gui_mouse_smoke_1e.py"""
import subprocess, time
from xdrive import *

time.sleep(1.0)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)      # Isometric
move(70, 451); click(); time.sleep(0.6)      # Fit all

def click_with(mod, x, y):
    move(x, y); time.sleep(0.5)
    hold(mod, True); click(); hold(mod, False); time.sleep(0.5)

move(536, 392); time.sleep(0.5); click(); time.sleep(0.4)   # top-front edge
click_with("Control_L", 797, 571)                             # + right vertical edge
shot("e1-ctrl-two-edges")
click_with("Shift_L", 360, 248)                               # + top-left edge by Shift
shot("e2-shift-third-edge")
click_with("Control_L", 797, 571)                             # Ctrl again removes it
shot("e3-ctrl-removes")
key("p"); time.sleep(0.3)
type_text("3"); key("Return"); time.sleep(1.5)
shot("e4-two-edges-rounded")

# ---- tangent chain: two opposite top edges rounded, then the edge between them ----
key("Escape"); key("s"); time.sleep(0.3)
key("z", ctrl=True); time.sleep(0.8)   # undo the rounding of the two edges
move(536, 392); time.sleep(0.5); click(); time.sleep(0.4)
click_with("Control_L", 675, 209)                             # top-back edge
key("p"); time.sleep(0.3)
type_text("4"); key("Return"); time.sleep(1.5)
key("s"); time.sleep(0.3)
move(360, 248); time.sleep(0.8)
shot("e5-chain-hover")
click(); time.sleep(0.8)
shot("e6-chain-selected")
