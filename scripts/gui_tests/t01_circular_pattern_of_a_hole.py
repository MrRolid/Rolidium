#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern tool: click a hole, then the edge of a pin: a circle of 6 holes round the axis of the pin (the axis is not at the origin)."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 60 30 12; pull-region 0 12 cut; sketch xy 10; circle 40 30 12; pull-region 0 6 add
# POINTS: hole_a=63,30,5;hole_b=57,30,5;hole_c=60,33,5;hole_d=60,27,5;pin_edge=40,36,16;plate=80,50,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
shot("00-open")
check(len(p["bodies"]) == 1, "one body (a plate with a hole and a pin)")
hole = [h for h in holes_of(p)]
check(any(close(h[0], 60) and close(h[1], 30) for h in hole), f"the hole is at (60, 30): {hole}")
key("t")
settle(0.5)
hx, hy = hover_cylinder(p, ["hole_a", "hole_b", "hole_c", "hole_d"])
click(hx, hy)
p = settle()
check(p["tool"] == "Pattern", "the Pattern tool is on")
check("click" in p["hint"].lower(), f"the status line says what to click next: {p['hint']}")
click(*screen(p, "tab_circular"))
p = settle()
click(*screen(p, "pick_axis"))
p = settle()
check(p["link_pick"] == "Some(Axis)", "the tool waits for the axis")
# the round edge of the pin: the pointer goes over its top edge
ex, ey = screen(p, "pin_edge")
move(ex, ey)
time.sleep(0.4)
click()
p = settle()
check(close(p["link_axis"][0], 40, 0.2) and close(p["link_axis"][1], 30, 0.2), f"the axis passes through the centre of the pin: {p['link_axis']}")
check(close(abs(p["link_axis"][5]), 1.0, 0.01), "the axis is vertical")
shot("01-axis-picked")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
v0 = 0
check(len(p["links"]) == 1 and p["links"][0]["count"] == 6, f"one link with 6 pieces: {p['links']}")
body = first_by(p, lambda b: b["visible"])
hs = holes_of(p, p["bodies"].index(body))
print("holes", hs)
import math
for k in range(6):
    a = 2 * math.pi * k / 6
    x, y = 40 + 20 * math.cos(a), 30 + 20 * math.sin(a)
    check(any(close(h[0], x, 0.1) and close(h[1], y, 0.1) for h in hs), f"a hole at ({x:.2f}, {y:.2f})")
shot("02-six-holes")
finish()
