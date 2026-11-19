#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Sweep from a sketch chain with the real mouse (ROLIDIUM_ARGS=target/tmp/sw/model.rolidium)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
move(570, 400); time.sleep(0.3)
for _ in range(4):
    button(5, True); button(5, False); time.sleep(0.2)
time.sleep(0.6)
key("w"); time.sleep(0.5)
move(486, 521); time.sleep(0.5); shot("w0-hover-profile")
click(); time.sleep(0.5)
move(510, 487); time.sleep(0.5); shot("w1-hover-path")
click(); time.sleep(0.8)
shot("w2-both")

def mode_run(name, mx):
    move(mx, 144); click(); time.sleep(0.3)
    move(774, 190); click(); time.sleep(2.0)
    shot(name)

mode_run("w3-new", 838)
key("z", ctrl=True); time.sleep(0.8)
shot("w3b-undone")
mode_run("w4-add", 902)
key("z", ctrl=True); time.sleep(0.8)
mode_run("w5-cut", 948)
key("z", ctrl=True); time.sleep(0.8)
mode_run("w6-auto", 806)
shot("w7-final")
