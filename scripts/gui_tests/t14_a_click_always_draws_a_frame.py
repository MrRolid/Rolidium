#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The window is drawn again after every press and every release of the button, also when the pointer does not move (the probe does not wake the window here:
ROLIDIUM_PROBE_IDLE), and the first click into a window that never had the keyboard focus is not lost."""
# FIXTURE: box 100 60 10
# POINTS: top=50,30,10;a=20,20,10;b=60,20,10
# ENV: ROLIDIUM_PROBE_IDLE=1
from guitest import *

time.sleep(1.5)
# no focus_app_window() yet: the first click goes into a window that was never focused
x, y = screen(probe(), "top")
move(x, y)
time.sleep(1.0)
f0 = probe()["frame"]
button(1, True)
time.sleep(0.5)
f1 = probe()["frame"]
check(f1 > f0, f"a frame is drawn after the press: {f0} -> {f1}")
button(1, False)
time.sleep(0.5)
q = probe()
check(q["frame"] > f1, f"a frame is drawn after the release: {f1} -> {q['frame']}")
check(len(q["selection"]) == 1, f"the first click into the window was not lost: the top face is selected {q['selection']}")
f2 = q["frame"]
time.sleep(1.5)
check(probe()["frame"] <= f2 + 2, "and the window rests when nothing happens (no frames drawn for ever)")
# the sketch: after each click (no movement in between) the state is already drawn
focus_app_window()
key("k")
time.sleep(0.6)
key("c")
time.sleep(0.6)
x, y = screen(probe(), "a")
move(x, y)
time.sleep(0.8)
f = probe()["frame"]
click()
time.sleep(0.6)
q = probe()
check(q["frame"] >= f + 2, f"after the first click of a circle at least two frames were drawn: {f} -> {q['frame']}")
check(q["drawing"], "the circle is being drawn")
f = q["frame"]
x, y = screen(q, "b")
move(x, y)
time.sleep(0.6)
f = probe()["frame"]
click()
time.sleep(0.6)
q = probe()
check(q["frame"] >= f + 2 and len(q["circles"]) == 1, f"after the second click the circle is there and was drawn: {q['circles']}")
shot("01-circle")
finish()
