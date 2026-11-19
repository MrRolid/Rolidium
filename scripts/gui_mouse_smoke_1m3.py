#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Measure between two vertices; mass and material in the Selection panel (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
key("e"); time.sleep(0.5)
move(280, 350); time.sleep(0.4); shot("v0-hover-vertex")
click(); time.sleep(0.3)
move(770, 470); time.sleep(0.4); click(); time.sleep(0.6)
shot("v1-vertices")
key("s"); time.sleep(0.3)
move(1117, 105); click(); time.sleep(0.6)
shot("v2-body-selected")
move(1230, 330); time.sleep(0.2)
