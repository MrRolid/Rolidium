#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1h with the real mouse: Combine, About/Donate, angled chamfer."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 482); click(); time.sleep(0.4)      # Isometric
move(70, 513); click(); time.sleep(0.6)      # Fit all
key("m"); time.sleep(0.3)
move(560, 440); time.sleep(0.4); click(); time.sleep(0.6)
move(840, 46); click(); type_text("15"); move(803, 160); click(); move(821, 184); click(); time.sleep(1.5)
move(70, 513); click(); time.sleep(0.6)
shot("h1-two-bodies")
key("i"); time.sleep(0.4)
move(400, 500); time.sleep(0.5)
shot("h2-hover-body")
click(); time.sleep(0.3)                      # target: the left body
move(800, 500); time.sleep(0.4); click(); time.sleep(0.5)   # tool: the right body
shot("h3-combine-selected")
move(880, 94); click(); time.sleep(0.4)       # Cut
shot("h3b-combine-cut-chosen")
key("Return"); time.sleep(1.5)
shot("h4-combined")
# ---- About / Donate ----
key("s"); time.sleep(0.3)
move(555, 780); click(); time.sleep(0.8)      # the Donate button in the status bar
shot("h5-donate")
