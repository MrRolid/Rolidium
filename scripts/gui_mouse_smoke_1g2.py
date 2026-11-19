#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1g with the real mouse: Move panel with Copy, plane chooser, sketch aids."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)
move(70, 451); click(); time.sleep(0.6)
key("m"); time.sleep(0.3)
move(560, 420); time.sleep(0.4); click(); time.sleep(0.6)
move(840, 46); click(); type_text("50"); move(803, 160); click(); move(821, 184); click(); time.sleep(1.5)
shot("n1-copy")
key("z", ctrl=True); time.sleep(1.0)
key("s"); time.sleep(0.3)
move(900, 700); click(); time.sleep(0.3)
key("k"); time.sleep(0.8)
shot("n2-plane-chooser")
key("Return"); time.sleep(0.8)
key("v"); time.sleep(0.6)
key("c"); time.sleep(0.4)
move(500, 400); time.sleep(0.6)
shot("n3-sketch-xy")
