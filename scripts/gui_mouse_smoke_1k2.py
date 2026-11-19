#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Dragging a panel by its header, the dashed line to the object, double click to put it back (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("m"); time.sleep(0.4)
move(640, 420); time.sleep(0.4); click(); time.sleep(0.8)
shot("p0-panel-default")
d = drag(812, 66, 420, 300, steps=14); time.sleep(0.4); d(); time.sleep(0.6)
shot("p1-panel-moved")
d = drag(520, 306, 900, 520, steps=14); time.sleep(0.4); d(); time.sleep(0.6)
shot("p1b-panel-far-with-line")
move(900, 520); time.sleep(0.3); double_click(); time.sleep(0.6)
shot("p2-panel-back")
