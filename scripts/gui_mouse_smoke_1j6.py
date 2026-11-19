#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Keyboard shortcuts of the file commands (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(640, 400); time.sleep(0.3)
key("s", ctrl=True, shift=True); time.sleep(1.0)
shot("k0-saveas-shortcut")
