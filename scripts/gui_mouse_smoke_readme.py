#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Screenshots for the README. Three runs: ROLIDIUM_ARGS=examples/models/02-flange.rolidium (mode model), --empty (helix), --demo (plane)."""
import os, time
from xdrive import *

mode = os.environ.get("README_SHOT", "model")
time.sleep(2.5)
focus_app_window()
if mode == "model":
    move(108, 487); click(); time.sleep(0.6)      # Isometric
    move(70, 521); click(); time.sleep(0.9)       # Fit all
    move(640, 420); time.sleep(0.4)
    shot("window-model")
elif mode == "helix":
    key("h"); time.sleep(0.5)
    move(560, 400); time.sleep(0.4); click(); time.sleep(0.8)
    shot("window-helix")
else:
    key("k"); time.sleep(0.6)
    key("Return"); time.sleep(1.5)
    move(640, 420); time.sleep(0.4)
    shot("window-sketch-plane")
