#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1n: button states, the Sketch button, the view turning to the plane (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
shot("n0-start")
move(30, 22); click(); time.sleep(0.2)       # the Sketch button: choose a plane
move(700, 400); time.sleep(0.4)
shot("n1-plane-choice")
key("Escape"); time.sleep(0.4)
# the top face: select it, then K
move(560, 300); time.sleep(0.3); click(); time.sleep(0.3)
key("k"); time.sleep(0.15)
shot("n2-turning")
time.sleep(1.0)
shot("n3-sketch-top")
key("k"); time.sleep(0.15)
shot("n4-back-turning")
time.sleep(1.0)
shot("n5-back")
