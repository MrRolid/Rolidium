#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Five circles in a sketch on the top of a plate: Ctrl+click collects their regions (all are highlighted), a drag in the Pull tool pulls them all by the same distance
(typed -3: five pockets of 3 mm), one Ctrl+Z takes back all five."""
# FIXTURE: box 100 60 10; sketch xy 10; circle 20 30 8; circle 35 30 8; circle 50 30 8; circle 65 30 8; circle 80 30 8
# POINTS: c1=20,30,10;c2=35,30,10;c3=50,30,10;c4=65,30,10;c5=80,30,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
key("p")
settle(0.3)
for i, name in enumerate(["c5", "c4", "c3", "c2", "c1"]):  # right to left: the panel of the selection opens to the lower right of the last one
    if i == 0:
        click(*screen(p, name))
    else:
        ctrl_click(*screen(p, name))
    settle(0.3)
p = settle(0.4)
check(len(p["extra"]["regions_selected"]) == 5, f"five regions are selected: {p['extra']['regions_selected']}")
shot("01-selected")
type_text("-3")
p = settle(0.5)
check(p["extra"]["pull_active"] and p["extra"]["pull_group"] == 5, f"the pull takes all five: {p['extra']['pull_group']}")
check("5 things" in p["status"], f"the status line says so: {p['status']}")
shot("02-pulling")
key("Return")
want = 60000 - 5 * math.pi * 16 * 3
p = wait_for(lambda q: not q["busy"] and close(q["bodies"][0]["volume"], want, 1.0), f"five pockets: {want:.1f}", 30)
shot("03-pockets")
key("z", ctrl=True)
p = wait_for(lambda q: not q["busy"] and close(q["bodies"][0]["volume"], 60000, 0.5), "Undo takes back all five at once", 30)
finish()
