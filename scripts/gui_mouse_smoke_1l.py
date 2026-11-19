#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""3D line with the real mouse: switch on 3D, click points, lock X, finish (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("k"); time.sleep(0.5)
shot("c0-plane-pick")
key("Return"); time.sleep(0.8)
key("l"); time.sleep(0.4)
shot("c1-line-tool")
move(137, 30); click(); time.sleep(0.5)                 # the 3D switch of the Line tool
move(480, 420); time.sleep(0.4); click(); time.sleep(0.3)
move(720, 420); time.sleep(0.4); click(); time.sleep(0.3)
key("z"); time.sleep(0.3)                                 # lock the Z axis
move(720, 250); time.sleep(0.5)
shot("c2-locked-z")
click(); time.sleep(0.3)
key("Return"); time.sleep(1.0)
shot("c3-curve-made")
