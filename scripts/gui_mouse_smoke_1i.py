#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Loft with the real mouse (start empty: ROLIDIUM_ARGS=--empty): two circles at different heights, Loft, result."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)      # sketch on XY
key("v"); time.sleep(0.5)
key("c"); time.sleep(0.3)
move(595, 400); time.sleep(0.4); click(); move(715, 400); time.sleep(0.4); click(); time.sleep(0.6)
key("d"); time.sleep(0.5)
key("Escape"); time.sleep(0.3)
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)      # a second sketch on XY...
move(75, 348); click(); key("a", ctrl=True); type_text("40"); key("Return"); time.sleep(0.6)   # ...moved 40 mm up
key("c"); time.sleep(0.3)
move(595, 400); time.sleep(0.4); click(); move(655, 400); time.sleep(0.4); click(); time.sleep(0.6)
key("d"); time.sleep(0.5)
move(70, 544); click(); time.sleep(0.8)
shot("l0-two-sketches-plan")
key("l"); time.sleep(0.4)
move(470, 400); time.sleep(0.5); click(); time.sleep(0.4)       # profile 1: the lower, larger circle
move(600, 400); time.sleep(0.5); click(); time.sleep(0.6)       # profile 2: the upper, smaller circle
move(70, 513); click(); time.sleep(0.8)                          # isometric view
shot("l1-loft-preview")
key("Return"); time.sleep(2.0)
shot("l2-loft-result")
