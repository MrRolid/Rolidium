#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1g part C with the real mouse: sketch on the XZ plane, rectangle, Revolve about the Z axis.
Start the program without --demo: ROLIDIUM_ARGS=--empty scripts/gui-mouse-smoke.sh <dir> gui_mouse_smoke_1g4.py"""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
key("k"); time.sleep(0.6)
move(532, 430); click(); time.sleep(0.8)         # XZ
key("v"); time.sleep(0.6)
key("r"); time.sleep(0.3)
shot("r1-xz-sketch")
move(640, 300); time.sleep(0.4); click()
move(720, 420); time.sleep(0.4); click(); time.sleep(0.6)
shot("r2-rect")
key("d"); time.sleep(0.5)
move(680, 360); time.sleep(0.5); click(); time.sleep(0.6)   # select the region
shot("r3-region-selected")
move(796, 462); click(); time.sleep(0.6)
move(571, 150); time.sleep(0.6)
shot("r4-axis-hover")
click(); time.sleep(0.6)
type_text("270"); time.sleep(0.6)
shot("r5-angle-preview")
key("Return"); time.sleep(1.5)
move(70, 451); click(); time.sleep(0.8)
shot("r6-revolved")
