#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Helix tool with the real mouse (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("h"); time.sleep(0.5)
move(560, 400); time.sleep(0.4); click(); time.sleep(0.8)
shot("h0-helix-preview")
move(1000, 280)                      # the panel: Pitch field, type a new value
time.sleep(0.3)
