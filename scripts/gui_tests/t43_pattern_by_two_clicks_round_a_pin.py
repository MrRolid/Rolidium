#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern by two clicks, in both orders. (1) T, click a hole, click the round face of a pin: round its centre, 6 pieces over 360 degrees, a preview at once, Enter applies.
(2) The plate is marked first, then T, then the round face of the pin; a click on nothing applies. The text at the cursor says what to click and what a click would do;
there are no tabs and no Pick buttons; Esc cancels."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 60 30 12; pull-region 0 12 cut; sketch xy 10; circle 40 30 12; pull-region 0 6 add
# POINTS: hole_a=63,30,5;hole_b=57,30,5;hole_c=60,33,5;hole_d=60,27,5;pin_a=46,30,13;pin_b=34,30,13;pin_c=40,36,13;pin_d=40,24,13;plate=80,50,10;void=-60,-50,0
from guitest import *
import math

time.sleep(1.0)
focus_app_window()
p = settle()
check(len(p["bodies"]) == 1, "one body (a plate with a hole and a pin)")
key("t")
p = settle(0.5)
check(p["tool"] == "Pattern", "the Pattern tool is on")
move(*screen(p, "plate"))
time.sleep(0.4)
p = probe()
check(p["link_cursor_hint"] == "Click what you want to repeat", f"the text at the cursor asks for an object: {p['link_cursor_hint']}")
hx, hy = hover_cylinder(p, ["hole_a", "hole_b", "hole_c", "hole_d"])
click(hx, hy)
p = settle()
check(p["link_feature"] and not p["link_armed"], "the hole is the source, no kind yet")
check(not any(k in p["widgets"] for k in ("tab_linear", "tab_circular", "tab_fill", "pick_axis", "pick_direction")), "no tabs and no Pick button")
shot("01-hole-clicked")
px, py = hover_cylinder(p, ["pin_a", "pin_b", "pin_c", "pin_d"])
time.sleep(0.4)
p = probe()
check(p["link_cursor_hint"] == "around the centre", f"over the pin the text says what a click does: {p['link_cursor_hint']}")
click(px, py)
p = settle()
check(p["link_armed"] and p["link_mode"] == "Circular" and p["link_count"] == 6 and p["link_full"], f"round its centre, 6 pieces over 360 degrees: {p['link_mode']} {p['link_count']}")
check(close(p["link_axis"][0], 40, 0.2) and close(p["link_axis"][1], 30, 0.2) and close(abs(p["link_axis"][5]), 1.0, 0.01), f"the axis is the axis of the pin: {p['link_axis']}")
check(any(k in p["widgets"] for k in ("kind_round", "kind_line", "kind_grid")), "the panel has the switch of the kind")
shot("02-preview")
key("Return")
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made (Enter applies)")
p = settle(1.0)
body = first_by(p, lambda b: b["visible"])
hs = holes_of(p, p["bodies"].index(body))
for k in range(6):
    a = 2 * math.pi * k / 6
    check(has_hole_at(p, 40 + 20 * math.cos(a), 30 + 20 * math.sin(a), p["bodies"].index(body), 0.1), f"a hole of the circle at {k * 60} degrees")
shot("03-six-holes")
# the other order: the plate is marked first
key("z", ctrl=True)
p = wait_for(lambda q: len(q["links"]) == 0, "Undo takes the link back", 20)
p = settle(0.6)
click(*screen(p, "plate"))
p = settle(0.4)
check(p["selection"], f"the plate is marked: {p['selection']}")
key("t")
p = settle(0.5)
check(p["tool"] == "Pattern" and p["link_sources"] == 1 and not p["link_armed"], "T with the plate marked: the first click is made, no kind yet")
px, py = hover_cylinder(p, ["pin_a", "pin_b", "pin_c", "pin_d"])
time.sleep(0.4)
p = probe()
check(p["link_cursor_hint"] == "around the centre", f"the text at the cursor: {p['link_cursor_hint']}")
click(px, py)
p = settle()
check(p["link_armed"] and p["link_mode"] == "Circular", "the second click chose round the centre")
click(*empty_point(p))     # a click on nothing applies
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made (a click on nothing applies)")
p = settle(1.0)
check(p["links"][0]["count"] == 6, f"six pieces: {p['links']}")
vis = [b for b in p["bodies"] if b["visible"]]
check(len(vis) == 6, f"six bodies (the plate and five copies turned round the pin): {len(vis)}")
shot("04-six-plates")
# Esc cancels
key("z", ctrl=True)
p = wait_for(lambda q: len(q["links"]) == 0, "Undo again", 20)
key("t")
p = settle(0.5)
click(*screen(p, "plate"))
p = settle(0.4)
check(p["link_sources"] == 1, "a source is chosen")
key("Escape")
p = settle(0.5)
check(p["tool"] == "Select" and len(p["links"]) == 0, f"Esc cancels the tool: {p['tool']}")
finish()
