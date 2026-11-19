#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1c with the real mouse: edit a finished round, flat-face panel, snapping while dragging,
Up to, grid controls, settings window. Run through scripts/gui-mouse-smoke.sh <dir> gui_mouse_smoke_1c.py"""
import time
from xdrive import *

time.sleep(1.0)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)      # Isometric
move(70, 451); click(); time.sleep(0.6)      # Fit all
shot("c1-start")

# ---- round the front-right vertical edge by typing 5 ----
move(797, 571); time.sleep(0.8)
shot("c2-edge-hover")
click(); time.sleep(0.5)
key("p"); time.sleep(0.3)
type_text("5"); time.sleep(0.8)
shot("c3-edge-typed-preview")
key("Return"); time.sleep(1.5)
shot("c4-rounded")
# ---- click the finished round: its panel offers the radius ----
key("s"); time.sleep(0.3)
move(797, 571); time.sleep(0.6); click(); time.sleep(0.8)
shot("c5-round-panel")
key("a", ctrl=True); type_text("8"); key("Return"); time.sleep(1.5)
shot("c6-round-r8")
shot("c6b-round-reselected")           # the edited round is selected again, its field is focused
key("a", ctrl=True); type_text("0"); key("Return"); time.sleep(1.5)
shot("c7-round-removed")

# ---- flat face: panel, Move by, Up to ----
key("Escape"); time.sleep(0.3)
move(540, 300); time.sleep(0.5); click(); time.sleep(0.8)
shot("c8-face-panel")
move(712, 337); click(); key("a", ctrl=True); type_text("5"); key("Return"); time.sleep(1.5)
shot("c9-moved-by-5")
key("z", ctrl=True); time.sleep(1.0)
move(540, 300); time.sleep(0.5); click(); time.sleep(0.6)
key("u"); time.sleep(0.4)
move(560, 560); time.sleep(0.6)
shot("c10-up-to-wait")
click(); time.sleep(1.5)                      # a point on the front face: the plane passes through it
shot("c11-up-to-done")
# ---- snapping while dragging ----
key("p"); time.sleep(0.3)
move(540, 300); time.sleep(0.5)
d = drag(540, 300, 540, 270)
shot("c12-drag-snap-grid")
d()
time.sleep(1.2)
key("z", ctrl=True); time.sleep(1.0)
move(540, 300); time.sleep(0.5)
hold("Alt_L", True)
d = drag(540, 300, 540, 273)
shot("c13-drag-alt-free")
d()
hold("Alt_L", False)
time.sleep(1.2)
# ---- settings and grid controls ----
move(70, 780); click(); time.sleep(0.8)
shot("c14-settings")
