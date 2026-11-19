#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Ctrl+Shift+S, Ctrl+S and Ctrl+O must open their dialogs every time (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
for n in range(3):
    move(640, 400); time.sleep(0.3)
    key("s", ctrl=True, shift=True); time.sleep(0.9)
    shot(f"s{n}-saveas")
    key("Escape"); time.sleep(0.6)
