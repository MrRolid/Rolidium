#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1h: the Arc tool with the real mouse (start empty: ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)   # XY plane
key("v"); time.sleep(0.5)
key("l"); time.sleep(0.3)
move(500, 400); time.sleep(0.4); click(); move(600, 400); time.sleep(0.4); click(); time.sleep(0.4)  # a line
key("Escape"); time.sleep(0.3)
key("a"); time.sleep(0.3)
move(600, 400); time.sleep(0.5); click()                     # start at the end of the line (tangent possible)
move(700, 300); time.sleep(0.5); click(); time.sleep(0.4)    # end point
move(680, 380); time.sleep(0.6)
shot("a1-arc-tangent-offer")
move(650, 300); time.sleep(0.5)
shot("a2-arc-free")
type_text("40"); time.sleep(0.4)
shot("a3-arc-radius-typed")
key("Return"); time.sleep(0.8)
key("Escape"); time.sleep(0.3)
shot("a4-arc-done")
