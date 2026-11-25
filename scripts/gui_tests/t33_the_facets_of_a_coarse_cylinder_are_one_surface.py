#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh cylinder made of 16 flat strips (a coarse STL): the strips are one round surface (not dozens of narrow walls), a click on it shows the diameter and the
axis, the diameter is changed in the field of the panel (the surface stays selected, one Ctrl+Z brings the old size back), and the flat end is pulled along the axis."""
# FIXTURE: box 5 5 5
# POINTS: cap=40,0,40;s0=54.71,2.86,20;s1=52.66,10.45,20;s2=47.08,14.0,20;s3=40,14.71,20;s4=32.9,14.0,20;s5=27.3,10.45,20;s6=25.3,2.86,20;s7=25.3,-2.86,20;s8=27.3,-10.45,20;s9=32.9,-14.0,20;s10=40,-14.71,20;s11=47.08,-14.0,20;s12=52.66,-10.45,20;s13=54.71,-2.86,20
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/cyl16.stl", cylinder_triangles(15.0, 40.0, 16, 40.0, 0.0))
p = import_dropped(f"{W}/cyl16.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][-1]["walls"] is not None, "the walls and holes are found", 30)
m = p["meshes"][-1]
cyls = p["extra"]["mesh_cyls"][-1]
check(m["walls"] == 2, f"only the two ends are flat walls, not a wall for every strip: {m['walls']}")
check(len(cyls) == 1 and cyls[0]["facets"] == 16 and close(cyls[0]["d"], 30.0, 0.01) and not cyls[0]["hole"], f"one round surface of 16 strips, 30 mm across: {cyls}")
check(cyls[0]["arc"] > 6.0 and abs(cyls[0]["axis"][2]) > 0.999, f"it goes all the way round, its axis is z: {cyls[0]}")
shot("01-imported")
# the round surface is selected by a click on it
x, y = hover_cylinder(p, [f"s{k}" for k in range(14)])
click(x, y)
p = settle(0.5)
check(p["face"] and close(p["face"]["diameter"], 30.0, 0.01), f"the surface is selected and shows its diameter: {p['face']}")
shot("02-selected")
# the diameter in the field of the panel
click(*screen(p, "panel_field_0"))
key("a", ctrl=True)
type_text("36")
key("Return")
p = wait_for(lambda q: q["status"] == "Done" and q["extra"]["mesh_cyls"][-1] and close(q["extra"]["mesh_cyls"][-1][0]["d"], 36.0, 0.05), "the diameter is 36 mm", 30)
bb = p["meshes"][-1]["bbox"]
check(close(bb[3] - bb[0], 36.0, 0.01) and close(bb[4] - bb[1], 36.0, 0.01), f"the cylinder is 36 mm across: {bb}")
check(len(p["selection"]) == 1, f"the surface stays selected after its diameter was changed: {p['selection']}")
shot("03-diameter-36")
key("Escape")   # the field of the panel has the cursor again (as for a hole of a solid): Esc leaves it, then Ctrl+Z is the program's
settle(0.3)
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"][-1]["walls"] is not None and q["extra"]["mesh_cyls"][-1] and close(q["extra"]["mesh_cyls"][-1][0]["d"], 30.0, 0.05), "Undo: 30 mm again", 30)
# the flat end is pulled along the axis
key("Escape")
settle(0.3)
key("p")
settle(0.4)
click(*steady("cap"))
p = settle(0.5)
check(len(p["selection"]) == 1, f"the top end is selected: {p['selection']}")
type_text("10")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull") and close(q["meshes"][-1]["bbox"][5], 50.0, 0.01), "the end was pulled by 10 mm", 30)
check(p["extra"]["mesh_cyls"][-1] and len(p["extra"]["mesh_cyls"][-1]) == 1, "still one round surface")
shot("04-pulled-end")
finish()
