#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Round: the size can be typed before an edge is clicked (the field in the Edges block of the left column) and changed after the click (the field next to the edge); the field with the
radius is always visible; Enter makes the round of the size that stands there."""
# FIXTURE: box 40 30 20
# POINTS: e1=0,0,10;e2=40,0,10;top=20,15,20
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check("round_size" in p["widgets"], f"the field of the size is in the left column before any edge is chosen: {list(p['widgets'])}")
click(*steady("round_size"))
key("a", ctrl=True)
type_text("3")
key("Return")
p = settle(0.4)
check(p["extra"]["round_size"] == "3", f"the size typed in advance: {p["extra"].get("round_size")}")
click(*screen(p, "e1"))
p = settle(0.6)
check("panel_field_0" in p["widgets"], "the field with the radius is next to the chosen edge")
check(p["panel"] and p["panel"][0][1] == "3", f"it holds the size typed in advance: {p['panel']}")
shot("01-edge-chosen")
# changed after the click
click(*steady("panel_field_0"))
key("a", ctrl=True)
type_text("5")
key("Return")
p = wait_for(lambda q: not q["busy"] and any(abs(b["volume"] - 24000.0) > 1 for b in q["bodies"]), "the round is made", 30)
v = p["bodies"][0]["volume"]
want = 24000.0 - (1 - 3.141592653589793 / 4) * 25 * 20
check(abs(v - want) < 0.5, f"the radius is 5: volume {v} (expected {want:.1f})")
finish()
