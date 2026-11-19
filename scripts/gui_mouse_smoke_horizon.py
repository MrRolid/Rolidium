#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Look at the grid at a grazing angle (towards the horizon) and take a screenshot."""
import time
from xdrive import *

time.sleep(1.0)
focus_app_window()
d = drag(600, 400, 600, 345, btn=3)      # lower the camera to a grazing angle
d(); time.sleep(0.8)
shot("h1-horizon")
