#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Move sketch plane with the real mouse (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)
move(330, 20); click(); time.sleep(0.6)
shot("m1-arrow")
d = drag(595, 330, 595, 250, steps=12); time.sleep(0.5)
shot("m2-dragged")
d()
time.sleep(0.5)
shot("m3-after")
