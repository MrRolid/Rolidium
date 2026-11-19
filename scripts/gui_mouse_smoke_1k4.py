#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Loft with the real mouse in the new layout: two circles at different heights, then Loft (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)
key("v"); time.sleep(0.5)
key("c"); time.sleep(0.3)
move(595, 370); time.sleep(0.4); click(); move(715, 370); time.sleep(0.4); click(); time.sleep(0.5)
key("d"); time.sleep(0.5)
key("Escape"); time.sleep(0.3)
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)
move(935, 20); click(); key("a", ctrl=True); type_text("40"); key("Return"); time.sleep(0.5)
key("c"); time.sleep(0.3)
move(595, 370); time.sleep(0.4); click(); move(690, 370); time.sleep(0.4); click(); time.sleep(0.5)
key("d"); time.sleep(0.5)
move(108, 462); click(); time.sleep(0.6)   # isometric
move(70, 493); click(); time.sleep(0.8)    # fit all
move(570, 380); time.sleep(0.3)
for _ in range(2):                          # mouse wheel: zoom out
    button(5, True); button(5, False); time.sleep(0.15)
time.sleep(0.6)
shot("k0-two-sketches")
key("s"); time.sleep(0.4)                                  # Select tool, pick the profiles with Ctrl+click
move(580, 386); time.sleep(0.5); click(); time.sleep(0.4)
hold("Control_L", True); time.sleep(0.4)
move(590, 176); time.sleep(0.3); move(594, 174); time.sleep(0.5); click(); time.sleep(0.5)
hold("Control_L", False); time.sleep(0.3)
shot("k1-preselected")
key("l"); time.sleep(0.8)                                  # Loft takes the selection
shot("k2-loft-preview")
key("Return"); time.sleep(2.0)
move(300, 600); time.sleep(0.5)
shot("k3-loft-result")
