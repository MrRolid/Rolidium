#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Preview of an angled chamfer while dragging an edge (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 513); click(); time.sleep(0.4)      # Isometric (layout with the Loft button)
move(70, 544); click(); time.sleep(0.6)      # Fit all
move(797, 571); time.sleep(0.6); click(); time.sleep(0.6)
move(912, 513); click(); time.sleep(0.5)     # Chamfer in the panel
move(878, 561); click(); key("a", ctrl=True); type_text("30"); time.sleep(0.3)
key("Escape"); time.sleep(0.2)
key("p"); time.sleep(0.3)
d = drag(797, 571, 850, 560, steps=12)
time.sleep(2.0)
shot("p1-angled-preview")
key("Escape"); d(); time.sleep(0.5)
