#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Layout at 1280x720 (ROLIDIUM_ARGS='--demo --window-size 1280x720')."""
import time
from xdrive import *
time.sleep(2.5)
focus_app_window()
key("e"); time.sleep(0.5)
shot("l0")
