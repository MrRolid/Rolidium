#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Operations that a mesh cannot take part in yet are refused with one sentence on the status line and change nothing: Combine of a mesh with a solid; a pattern of a mesh
together with a solid; a pull of a wall of a mesh that would turn the mesh inside out (the sentence of the mesh library); a pull of the wall of a hole of a mesh."""
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
check(p["status_error"] and "mesh" in p["status"] and "solid" in p["status"], f"Combine is refused with a sentence: {p['status']}")
check(close(p["meshes"][0]["volume"], v0, 0.01) and len(p["bodies"]) == 1 and len(p["meshes"]) == 1, "nothing changed")
shot("01-combine-refused")
finish()
