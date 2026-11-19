#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1f part B with the real mouse: round and chamfer inner (concave) edges of a pocket.
Run: ROLIDIUM_ARGS=target/tmp/b/pocket.rolidium scripts/gui-mouse-smoke.sh <dir> gui_mouse_smoke_1f.py"""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)      # Isometric
move(70, 451); click(); time.sleep(0.6)      # Fit all
shot("p1-pocket")
move(506, 262); time.sleep(0.8)
shot("p2-hover-inner-edge")
click(); time.sleep(0.5)
shot("p3-inner-edge-selected")
key("p"); time.sleep(0.3)
type_text("2"); time.sleep(0.8)
shot("p4-inner-preview")
key("Return"); time.sleep(1.5)
shot("p5-inner-rounded")
key("s"); time.sleep(0.3)
move(506, 262); time.sleep(0.5); click(); time.sleep(0.6)
shot("p6-round-panel")
key("a", ctrl=True); type_text("3"); key("Return"); time.sleep(1.5)
shot("p7-r3")
key("a", ctrl=True); type_text("0"); key("Return"); time.sleep(1.5)
shot("p8-removed")
