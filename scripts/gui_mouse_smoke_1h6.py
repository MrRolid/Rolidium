#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sketch plane with an offset (K, XY, Offset 15)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 482); click(); time.sleep(0.4)
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)
move(75, 316); click(); key("a", ctrl=True); type_text("15"); key("Return"); time.sleep(0.8)
shot("o1-offset-plane")
