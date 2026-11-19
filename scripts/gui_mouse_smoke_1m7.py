#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sweep along an edge (ROLIDIUM_ARGS=target/tmp/sw/model3.rolidium)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
key("w"); time.sleep(0.5)
move(244, 408); time.sleep(0.5); shot("e0-hover-profile")
click(); time.sleep(0.4)
move(492, 482); time.sleep(0.5); shot("e1-hover-edge")
click(); time.sleep(0.6)
shot("e2-picked")
def mode_run(name, mx):
    move(mx, 142); click(); time.sleep(0.3)
    move(756, 188); click(); time.sleep(2.0)
    shot(name)
mode_run("e3-new", 821)
key("z", ctrl=True); time.sleep(0.8)
mode_run("e4-add", 886)
key("z", ctrl=True); time.sleep(0.8)
mode_run("e5-cut", 932)
