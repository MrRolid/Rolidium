#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Screenshot of a loaded example model (ROLIDIUM_ARGS=<file.rolidium>)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
move(108, 462); click(); time.sleep(0.5)     # Isometric
move(70, 493); click(); time.sleep(0.8)      # Fit all
move(640, 420); time.sleep(0.4)
shot("window-model")
