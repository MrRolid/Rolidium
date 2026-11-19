#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Thread M10 in the window: zoom to the profile, sweep it along the helix as Cut (ROLIDIUM_ARGS=target/tmp/sw/thread.rolidium)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
move(670, 632); time.sleep(0.3)
for _ in range(3):
    button(4, True); button(4, False); time.sleep(0.25)
time.sleep(0.5)
key("w"); time.sleep(0.5)
move(670, 632); time.sleep(0.5)
shot("t1-zoomed")
click(); time.sleep(0.5)
move(440, 510); time.sleep(0.5); click(); time.sleep(0.6)
move(949, 142); click(); time.sleep(0.3)
shot("t2-ready")
move(774, 188); click(); time.sleep(0.6)
shot("t3-computing")
time.sleep(6.0)
shot("t4-done")
