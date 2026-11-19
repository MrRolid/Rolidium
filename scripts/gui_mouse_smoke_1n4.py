#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Measure with Ctrl+click: the middle of an edge and the centre of a circle (ROLIDIUM_ARGS=examples/models/04-turned-part.rolidium)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
key("e"); time.sleep(0.4)
hold("Control_L", True); time.sleep(0.2)
move(600, 640); time.sleep(0.5); shot("c0-hover-near-centre")   # the lower circle edge, near its front middle
click(); time.sleep(0.3)
move(600, 175); time.sleep(0.5); click(); time.sleep(0.5)       # upper circle
hold("Control_L", False)
shot("c1-two-points")
