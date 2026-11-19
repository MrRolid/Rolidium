#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Tangent sketch plane on a cylinder wall (ROLIDIUM_ARGS=examples/models/04-turned-part.rolidium)."""
import time
from xdrive import *

time.sleep(3.0)
focus_app_window()
key("k"); time.sleep(0.5)
move(600, 600); time.sleep(0.6)
shot("t0-hover-cylinder")
click(); time.sleep(0.8)
shot("t1-sketch-on-cylinder")
key("c"); time.sleep(0.3)
move(580, 590); time.sleep(0.2); click(); time.sleep(0.2)
move(630, 610); time.sleep(0.3); click(); time.sleep(0.6)
shot("t2-circle")
