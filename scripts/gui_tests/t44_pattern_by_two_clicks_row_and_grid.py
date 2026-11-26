#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pattern by two clicks: (1) T, click a small body, click a slanting line of a sketch: a row of 3 along that line (the direction is the line, the text at the cursor says
"along a line"). The number is changed in the panel and the preview follows; Ctrl+click adds a second source. (2) A flat face means a grid on it (3 x 3)."""
# FIXTURE: box 10 10 10; box 80 50 5; sketch xy 25; line 30 -20 70 10
# POINTS: small=5,5,10;linemid=50,-5,25;face=60,30,5;void=-70,-60,0
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
check(len(p["bodies"]) == 2, "two bodies")
key("t")
p = settle(0.5)
click(*screen(p, "small"))
p = settle()
check(p["link_sources"] == 1 and not p["link_armed"], f"the small box is the source: {p['link_sources']}")
move(*screen(p, "linemid"))
time.sleep(0.5)
p = probe()
check(p["link_cursor_hint"] == "along a line", f"over the line: {p['link_cursor_hint']} (hover {p['hover_obj']})")
click(*screen(p, "linemid"))
p = settle()
d = p["link_dir"]
check(p["link_armed"] and p["link_mode"] == "Linear" and not p["link_second"] and p["link_count"] == 3, f"a row of 3: {p['link_mode']} {p['link_count']}")
check(close(d[0], 0.8, 0.01) and close(d[1], 0.6, 0.01) and close(d[2], 0, 0.01), f"the direction is the line: {d}")
shot("01-row-preview")
# the number is changed in the panel (a kind switch is there too)
check("kind_grid" in p["widgets"] and "kind_round" in p["widgets"], "the panel has the switch of the kind")
click(*screen(p, "kind_round"))
p = settle(0.5)
check(p["link_mode"] == "Circular" and p["link_count"] == 6, f"the switch changes the kind: {p['link_mode']}")
click(*screen(p, "kind_line"))
p = settle(0.5)
check(p["link_mode"] == "Linear" and p["link_count"] == 3, f"and back: {p['link_mode']} {p['link_count']}")
key("Return")
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
vis = [b for b in p["bodies"] if b["visible"]]
check(len(vis) == 4, f"the plate and the box and two copies: {len(vis)}")
xs = sorted(b["bbox"][0] for b in vis if b["bbox"][5] - b["bbox"][2] > 9)
sp = p["links"][0]
print("x of the boxes", xs)
check(len(xs) == 3 and close(xs[1] - xs[0], xs[2] - xs[1], 0.15) and xs[2] > xs[0] + 10, f"three boxes at equal steps along the line: {xs}")
shot("02-row")
key("z", ctrl=True)
p = wait_for(lambda q: len(q["links"]) == 0, "Undo", 20)
# a flat face: a grid
key("t")
p = settle(0.5)
click(*screen(p, "small"))
p = settle(0.4)
move(*screen(p, "face"))
time.sleep(0.5)
p = probe()
check(p["link_cursor_hint"] == "into a grid", f"over a flat face: {p['link_cursor_hint']}")
click(*screen(p, "face"))
p = settle()
check(p["link_armed"] and p["link_mode"] == "Linear" and p["link_second"] and p["link_count"] == 3 and p["link_count2"] == 3, f"a grid of 3 x 3: {p['link_mode']} {p['link_count']} x {p['link_count2']}")
shot("03-grid-preview")
key("Return")
p = wait_for(lambda q: len(q["links"]) == 1, "the grid link is made")
p = settle(1.0)
vis = [b for b in p["bodies"] if b["visible"]]
check(len(vis) == 10, f"the plate and nine boxes: {len(vis)}")
shot("04-grid")
finish()
