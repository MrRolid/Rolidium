#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Save As: remembered folder, name, overwrite question (ROLIDIUM_ARGS=--demo, FD_DOCS=folder with part1.rolidium)."""
import os, time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(26, 632); click(); time.sleep(1.0)
key("l", ctrl=True); time.sleep(0.4)
type_text(os.environ["FD_DOCS"]); key("Return"); time.sleep(0.8)
move(600, 618); click(); time.sleep(0.2)
type_text("part1"); time.sleep(0.3)
shot("g0-name-typed")
key("Return"); time.sleep(0.6)
shot("g1-overwrite-question")
key("Escape"); time.sleep(0.4)
shot("g2-after-escape")
key("Return"); time.sleep(0.5)
move(640, 400)
shot("g3-question-again")
