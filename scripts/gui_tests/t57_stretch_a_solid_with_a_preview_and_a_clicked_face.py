#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Stretch across a plane for a solid: the plane is taken by a click on a flat face (the side wall x = 20, so the plane is square to X), the distance 5 is typed, the preview
shows the stretched solid in the place of the original (the box 25 mm long), Stretch makes it, one Ctrl+Z takes it back."""
# FIXTURE: box 20 20 10
# POINTS: side=20,10,5;top=10,10,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
bid = p["bodies"][0]["id"]
click(*steady(f"body_row_{bid}"))
p = settle(0.6)
click(*screen(p, "stretch_header"))
p = settle(0.5)
click(*steady("stretch_pick"))
p = settle(0.4)
check(p["extra"]["stretch"]["pick"], "the click on a face is waited for")
x, y = hover_normal(["side", "top"], [1.0, 0.0, 0.0])
click(x, y)
p = settle(0.6)
st = p["extra"]["stretch"]
check(st["plane"] is not None and abs(st["plane"]["normal"][0]) > 0.99, f"the plane is the clicked wall: {st['plane']}")
# the clicked wall is the end of the body: the plane is moved to the middle
click(*steady("stretch_position"))
key("a", ctrl=True)
type_text("10")
settle(0.3)
click(*steady("stretch_distance"))
type_text("5")
settle(0.3)
click(*steady("stretch_preview"))
p = wait_for(lambda q: q["extra"]["preview_mesh"] is not None, "the preview of the solid", 30)
print("status", p["status"])
pv = p["extra"]["preview_mesh"]
check(abs((pv["bbox"][3] - pv["bbox"][0]) - 25.0) < 0.05, f"the preview is 25 mm long: {pv['bbox']}")
shot("01-preview")
click(*steady("stretch_apply"))
p = wait_for(lambda q: close(q["bodies"][0]["volume"], 25 * 20 * 10, 0.5) and not q["busy"], "the solid is stretched", 30)
check(p["extra"]["preview_mesh"] is None, "the preview is gone")
shot("02-stretched")
key("z", ctrl=True)
wait_for(lambda q: close(q["bodies"][0]["volume"], 4000, 0.5), "Undo", 30)
finish()
