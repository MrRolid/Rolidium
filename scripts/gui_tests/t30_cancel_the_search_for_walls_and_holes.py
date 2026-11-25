#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The search for walls and holes runs in the background and is made slow here (ROLIDIUM_TEST_REGIONS_DELAY_MS): a little window shows the progress and Cancel; while it
runs the window answers (a click on the mesh selects it, the whole mesh); Cancel stops it: no walls, the status line says so, the mesh can still be moved; a move of the mesh
afterwards starts a new search for the moved mesh."""
# FIXTURE: box 20 20 20
# POINTS: top=10,10,10
# ENV: ROLIDIUM_TEST_REGIONS_DELAY_MS=2500
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
p = settle()
write_stl(f"{W}/plate.stl", box_triangles(60, 40, 10, 30, 0, 0, n=4))
drop_files(f"{W}/plate.stl")
p = wait_for(lambda q: q["stl_dialog"], "the dialog is open", 30)
click(*steady("stl_as_mesh"))
click(*screen(p, "stl_import_ok"))
p = wait_for(lambda q: q["mesh_job"] is not None and "mesh_job_cancel" in q["widgets"], "the progress window is there", 30)
shot("01-progress")
check(p["mesh_job"] is not None and 0.0 <= p["mesh_job"] <= 1.0, f"the progress is a number: {p['mesh_job']}")
check(p["meshes"][0]["walls"] is None, "the walls are not known yet")
# the window answers: a click on the mesh selects the whole mesh (no walls yet)
click(*screen(p, "mesh_row_%d" % p["meshes"][0]["id"]))
p = settle(0.3)
check(p["mesh_job"] is not None, "the search goes on while the window is used")
click(*screen(p, "mesh_job_cancel"))
p = wait_for(lambda q: q["mesh_job"] is None, "the search stopped", 30)
p = settle(0.6)
check(p["meshes"][0]["walls"] is None and p["status"].startswith("Stopped"), f"Cancel: no walls, the status line says so: {p['status']}")
shot("02-cancelled")
# the mesh can still be moved
key("m")
settle(0.4)
click(*screen(p, "mesh_row_%d" % p["meshes"][0]["id"]))
p = settle(0.8)
click(*screen(p, "move_field_0"))
type_text("25")
click(*screen(p, "move_apply"))
p = wait_for(lambda q: q["meshes"][0]["bbox"][0] > 50, "moved", 20)
p = settle(1.5)
check(p["mesh_job"] is not None or p["meshes"][0]["walls"] is not None, "the moved mesh is another mesh: its walls are looked for again")
finish()
