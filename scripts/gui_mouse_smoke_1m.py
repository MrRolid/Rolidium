#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A long operation: the indication, turning the view meanwhile, Esc cancels (ROLIDIUM_ARGS=--demo, ROLIDIUM_TEST_DELAY_MS=6000)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
key("p"); time.sleep(0.4)
move(640, 250); time.sleep(0.5); click(); time.sleep(0.5)       # the top face
type_text("5"); time.sleep(0.3); key("Return")                  # pull by 5 mm: a job that takes 6 s here
time.sleep(0.15)
shot("b0-before-indication")                                     # shorter than 0.3 s: nothing shown yet
time.sleep(1.2)
d = drag(640, 600, 720, 600, steps=8); time.sleep(0.3); d()      # turn the view while computing
time.sleep(0.4)
shot("b1-computing")
key("Escape"); time.sleep(1.0)
shot("b2-cancelled")
