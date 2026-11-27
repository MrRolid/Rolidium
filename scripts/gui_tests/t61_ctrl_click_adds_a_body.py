#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Select tool, whole bodies: the body chosen in the list (or by a double click), Ctrl + a click on another body adds it to the selection, Ctrl + a click on a chosen body takes it out again."""
# FIXTURE: box 20 20 20; box 20 20 20; move 100 0 0 body 2
# POINTS: a=10,10,20;b=110,10,20
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("s")
settle(0.3)
sel = lambda q: sorted(set(q["extra"]["body_dims_bodies"]))
# the first body is chosen in the list (the first row of Bodies)
click(1110, 103)
p = settle(0.5)
check(sel(p) == [1], f"a click in the list takes the first body: {sel(p)}")
ctrl_click(*screen(p, "b"))
p = settle(0.5)
check(sel(p) == [1, 2], f"Ctrl + a click adds the second body: {sel(p)}")
shot("01-two-bodies")
ctrl_click(*screen(p, "b"))
p = settle(0.5)
check(sel(p) == [1], f"Ctrl + a click on a chosen body takes it out: {sel(p)}")
finish()
