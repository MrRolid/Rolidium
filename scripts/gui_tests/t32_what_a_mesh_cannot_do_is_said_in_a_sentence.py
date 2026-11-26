#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Combine of a mesh with a solid that does not touch it (the default operation, merge, finds nothing to join) is refused with one sentence on the status line and changes nothing
(since 0.3.2 a mesh can be cut and joined; the cases that do work are in t51)."""
# FIXTURE: box 20 20 20; import-stl examples/stl/plate-with-hole.stl; mesh-to-doc; move -40 0 0
# POINTS: top=10,10,10;boxtop=-30,10,20
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls are found", 30)
v0 = p["meshes"][0]["volume"]
# Combine: the mesh is the target, the solid the tool
key("i")
settle(0.5)
click(*screen(p, "top"))
click(*screen(p, "boxtop"))
p = settle(0.6)
click(*screen(p, "combine_apply"))
p = settle(1.0)
check(p["status_error"] and ("touch" in p["status"] or "overlap" in p["status"]), f"Combine is refused with a sentence: {p['status']}")
check(close(p["meshes"][0]["volume"], v0, 0.01) and len(p["bodies"]) == 1 and len(p["meshes"]) == 1, "nothing changed")
shot("01-combine-refused")
finish()
