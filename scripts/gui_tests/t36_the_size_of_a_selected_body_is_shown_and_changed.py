#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Three sizes round a selected body (a box with a rounded edge, a cylinder, a mesh): a click on a size opens it for typing. With one flat wall at the end the wall is
moved (the round of the box stays, the mesh stretches its neighbours); the middle can stay (symmetric); without such a wall (a cylinder) the body is scaled, and the
window warns that circles become ellipses; "Keep the proportions" scales all three directions."""
# FIXTURE: import-stl examples/stl/plate-with-hole.stl; mesh-to-doc; box 40 30 20; round 3 near 0 0 10; sketch xy 0; circle 100 100 15; pull-region 0 40 new
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None and len(q["bodies"]) == 2, "the document is there", 30)
box, cyl = p["bodies"][0], p["bodies"][1]
mesh = p["meshes"][0]
faces0 = box["faces"]
# the box with a rounded edge
click(*steady(f"body_row_{box['id']}"))
p = settle(0.6)
check(p["extra"]["body_dims"] == [40.0, 30.0, 20.0], f"the three sizes of the box: {p['extra']['body_dims']}")
shot("01-box-sizes")
click(*steady("body_dim_x"))
p = settle(0.4)
check(p["extra"]["body_dims_editing"] == 0, "the size is open for typing")
click(*screen(p, "body_dim_field"))
key("a", ctrl=True)
type_text("50")
key("Return")
p = wait_for(lambda q: q["bodies"][0]["bbox"] and close(q["bodies"][0]["bbox"][3] - q["bodies"][0]["bbox"][0], 50.0, 0.01), "the box is 50 mm long", 30)
b = p["bodies"][0]
check(b["faces"] == faces0, f"the wall was moved: the round of the box stays ({faces0} faces): {b['faces']}")
check(close(b["bbox"][0], 0.0, 0.001), f"the end at the negative side stayed: {b['bbox']}")
shot("02-box-50")
# symmetric: Z to 30 about the middle
click(*steady(f"body_row_{box['id']}"))
settle(0.5)
click(*steady("body_dim_z"))
p = settle(0.4)
click(*screen(p, "body_dim_symmetric"))
click(*screen(p, "body_dim_field"))
key("a", ctrl=True)
type_text("30")
key("Return")
p = wait_for(lambda q: q["bodies"][0]["bbox"] and close(q["bodies"][0]["bbox"][5] - q["bodies"][0]["bbox"][2], 30.0, 0.01) and close(q["bodies"][0]["bbox"][2], -5.0, 0.01), "Z is 30 mm and the middle stayed", 40)
shot("03-box-z-symmetric")
# the cylinder: no flat wall at the end in X: it is scaled, with the warning
click(*steady(f"body_row_{cyl['id']}"))
p = settle(0.6)
click(*steady("body_dim_x"))
p = settle(0.4)
check(p["extra"]["body_dims_editing"] == 0, "the size of the cylinder is open")
shot("04-cylinder-warning")
click(*screen(p, "body_dim_field"))
key("a", ctrl=True)
type_text("45")
key("Return")
p = wait_for(lambda q: close(q["bodies"][1]["bbox"][3] - q["bodies"][1]["bbox"][0], 45.0, 0.05), "the cylinder was scaled in X (an ellipse now)", 40)
check(close(p["bodies"][1]["bbox"][4] - p["bodies"][1]["bbox"][1], 15.0, 0.05), f"Y stayed 15 mm: {p['bodies'][1]['bbox']}")
# keep the proportions: X to 90 (the scale 2 in all directions)
click(*steady(f"body_row_{cyl['id']}"))
settle(0.5)
click(*steady("body_dim_x"))
p = settle(0.4)
click(*screen(p, "body_dim_proportional"))
click(*screen(p, "body_dim_field"))
key("a", ctrl=True)
type_text("90")
key("Return")
p = wait_for(lambda q: close(q["bodies"][1]["bbox"][4] - q["bodies"][1]["bbox"][1], 30.0, 0.1), "all three directions were scaled by 2", 40)
check(close(p["bodies"][1]["bbox"][5] - p["bodies"][1]["bbox"][2], 80.0, 0.1), f"Z too (40 to 80): {p['bodies'][1]['bbox']}")
shot("05-cylinder-proportional")
# the mesh: Z to 15 pulls its top wall
click(*steady(f"mesh_row_{mesh['id']}"))
p = settle(0.6)
check(p["extra"]["body_dims"] and close(p["extra"]["body_dims"][2], 10.0, 0.01), f"the sizes of the mesh: {p['extra']['body_dims']}")
click(*steady("body_dim_z"))
p = settle(0.4)
click(*screen(p, "body_dim_field"))
key("a", ctrl=True)
type_text("15")
key("Return")
p = wait_for(lambda q: close(q["meshes"][0]["bbox"][5] - q["meshes"][0]["bbox"][2], 15.0, 0.01), "the wall of the mesh was pulled", 40)
shot("06-mesh-15")
finish()
