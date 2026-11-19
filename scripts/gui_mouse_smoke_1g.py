#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1g with the real mouse: Move tool, context menu, tree eye."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)
move(70, 451); click(); time.sleep(0.6)
key("m"); time.sleep(0.3)
move(560, 420); time.sleep(0.4); click(); time.sleep(0.6)
shot("m1-gizmo")
d = drag(680, 407, 780, 428, steps=10)
time.sleep(0.5)
shot("m2-dragging-x")
d(); time.sleep(1.5)
shot("m3-moved-x")
key("z", ctrl=True); time.sleep(1.0)
# panel: X 50, Copy
move(840, 46); click(); type_text("50"); move(803, 160); click(); move(821, 184); click(); time.sleep(1.5)
shot("m4-copied")
# right click menu on the body (short click)
key("s"); time.sleep(0.3)
move(420, 520); time.sleep(0.5); button(3, True); button(3, False); time.sleep(0.6)
shot("m5-context-menu")
key("Escape"); time.sleep(0.3)
# tree eye of body 1: hide
move(1075, 57); click(); time.sleep(1.2)
shot("m6-hidden")
