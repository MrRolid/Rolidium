#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sweep Add/Cut with the profile on a face of the body (ROLIDIUM_ARGS=target/tmp/sw/model2.rolidium)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
move(570, 400); time.sleep(0.3)
for _ in range(2):
    button(5, True); button(5, False); time.sleep(0.2)
time.sleep(0.6)
key("w"); time.sleep(0.5)
shot("z0")
move(702, 437); time.sleep(0.4); click(); time.sleep(0.4)
move(800, 462); time.sleep(0.4); click(); time.sleep(0.6)
shot("z1-picked")
def mode_run(name, mx):
    move(mx, 142); click(); time.sleep(0.3)
    move(756, 188); click(); time.sleep(2.0)
    shot(name)
mode_run("z2-add", 886)
key("z", ctrl=True); time.sleep(0.8)
mode_run("z3-cut", 932)
key("z", ctrl=True); time.sleep(0.8)
mode_run("z4-auto", 754)
