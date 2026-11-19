#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Measure tool with the real mouse (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
key("e"); time.sleep(0.5)
shot("m0-tool")
move(640, 250); time.sleep(0.5)
shot("m1-hover-face")
click(); time.sleep(0.4)                      # top face
shot("m2-first")
move(560, 470); time.sleep(0.4); click(); time.sleep(0.6)     # front face
shot("m3-two-faces")
move(715, 500); time.sleep(0.3); click(); time.sleep(0.3)      # right face
move(560, 470); time.sleep(0.3); click(); time.sleep(0.6)      # front face again
shot("m4-two-results")
key("Escape"); time.sleep(0.5)
shot("m5-cleared")
