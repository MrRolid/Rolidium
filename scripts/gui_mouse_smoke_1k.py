#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A plain screenshot of the window after start (ROLIDIUM_ARGS=--demo); used for the theme and toolbar checks."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
move(640, 400); time.sleep(0.4)
shot("start")
