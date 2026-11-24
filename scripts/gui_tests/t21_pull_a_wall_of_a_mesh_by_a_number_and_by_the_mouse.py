#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Pull (P) of a flat wall of a mesh: a number typed gives +5 mm (the box gets taller, the volume grows by the area times 5, the mesh stays closed), Ctrl+Z takes it
back in one step and Ctrl+Y does it again; the mouse drags the wall; a pull that would turn the mesh inside out is refused with the reason and nothing changes."""
# FIXTURE: import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls are found", 30)
v0 = p["meshes"][0]["volume"]
key("p")
settle(0.4)
click(*screen(p, "top"))
p = settle()
check(len(p["selection"]) == 1, f"the top wall is selected: {p['selection']}")
type_text("5")
shot("01-live-preview")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull") and q["meshes"][0]["bbox"][5] > 14, "the wall was pulled", 20)
m = p["meshes"][0]
check(close(m["bbox"][5], 15.0, 0.001), f"the plate is 15 mm high: {m['bbox']}")
check(close(m["volume"], v0 + 2380.432 * 5, 0.5), f"the volume grew by the area times 5: {m['volume']} from {v0}")
p = wait_for(lambda q: q["meshes"][0]["walls"] == 6, "the walls are found again", 30)
shot("02-pulled")
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"][0]["bbox"][5] < 11, "Undo", 20)
check(close(p["meshes"][0]["bbox"][5], 10.0, 0.001) and close(p["meshes"][0]["volume"], v0, 0.01), "one step of Undo brings the mesh back exactly")
key("y", ctrl=True)
p = wait_for(lambda q: q["meshes"][0]["bbox"][5] > 14, "Redo", 20)
check(close(p["meshes"][0]["bbox"][5], 15.0, 0.001), "Redo")
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"][0]["bbox"][5] < 11, "Undo again", 20)
p = wait_for(lambda q: q["meshes"][0]["walls"] == 6, "the walls are found", 30)
# the mouse: press on the wall, drag up the screen, release
x, y = screen(p, "top")
key("p")
settle(0.4)
move(x, y)
time.sleep(0.5)
release = drag(x, y, x, y - 70, steps=10)
time.sleep(0.4)
shot("03-dragging")
release()
p = wait_for(lambda q: q["meshes"][0]["bbox"][5] > 11, "the wall was dragged out", 20)
check(p["meshes"][0]["bbox"][5] > 11, f"a drag with the mouse pulled the wall: {p['meshes'][0]['bbox']}")
key("z", ctrl=True)
p = wait_for(lambda q: q["meshes"][0]["bbox"][5] < 10.5, "Undo of the drag", 20)
p = wait_for(lambda q: q["meshes"][0]["walls"] == 6, "the walls are found", 30)
# a pull of -20 would carry the wall through the bottom: refused, and the mesh stays
key("p")
settle(0.4)
click(*screen(p, "top"))
p = settle()
type_text("-20")
key("Return")
p = settle(1.5)
check(p["status_error"], f"refused: {p['status']}")
check(close(p["meshes"][0]["bbox"][5], 10.0, 0.001), f"the mesh did not change: {p['meshes'][0]['bbox']}")
shot("04-refused")
finish()
