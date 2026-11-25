#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The diameter of a round hole of a mesh in the field of the panel: 5 to 6 mm (the volume is smaller by the ring, the walls are found again, the hole is 6 mm),
and one Ctrl+Z gives the 5 mm back; a hole is also changed with the field of the right column after a click on it."""
# FIXTURE: import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: h0=32.5,20,9;h1=31.77,21.77,9;h2=30,22.5,9;h3=28.23,21.77,9;h4=27.5,20,9;h5=28.23,18.23,9;h6=30,17.5,9;h7=31.77,18.23,9
from guitest import *
import math

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls are found", 30)
v0 = p["meshes"][0]["volume"]
x, y = hover_cylinder(p, [f"h{k}" for k in range(8)])
click(x, y)
p = settle(0.5)
check(p["face"] and close(p["face"]["diameter"], 5.0, 0.01), f"the hole is selected: {p['face']}")
click(*screen(p, "panel_field_0"))
key("a", ctrl=True)
type_text("6")
key("Return")
p = wait_for(lambda q: q["status"] == "Done" and q["meshes"][0]["holes"] and close(q["meshes"][0]["holes"][0]["d"], 6.0, 0.05), "the hole is 6 mm", 30)
m = p["meshes"][0]
ring = (math.pi * 3.0 ** 2 - math.pi * 2.5 ** 2) * 10
check(m["volume"] < v0 - ring * 0.97 and m["volume"] > v0 - ring * 1.03, f"the volume is smaller by the ring of the hole (about {ring:.0f} mm3): {v0} -> {m['volume']}")
check(close(m["holes"][0]["x"], 30.0, 0.01) and close(m["holes"][0]["y"], 20.0, 0.01), f"the hole stayed where it was: {m['holes']}")
check(len(p["selection"]) == 1, f"the hole stays selected after its diameter was changed: {p['selection']}")
shot("01-hole-6")
key("Escape")   # the field of the panel has the cursor again (as for a hole of a solid): Esc leaves it, then Ctrl+Z is the program's
settle(0.3)
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"][0]["walls"] is not None and q["meshes"][0]["holes"] and close(q["meshes"][0]["holes"][0]["d"], 5.0, 0.05), "Undo: the hole is 5 mm again", 30)
check(close(p["meshes"][0]["volume"], v0, 0.01), "the volume is the same as before")
shot("02-undone")
finish()
