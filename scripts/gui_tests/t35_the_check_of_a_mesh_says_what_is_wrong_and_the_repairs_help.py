#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The check of a mesh in the panel, in plain words: after "Check the mesh" the result stays, the button stays ("Check again") and the other buttons of the panel
work at once (the scale); a mesh with a loose joint and a turned triangle offers "Join the loose joints", then "Turn them outwards", each result is one sentence that is
no error, and at the end "The mesh is fine for printing"; a mesh whose two parts touch along an edge says what that is and that no button repairs it."""
# FIXTURE: box 5 5 5
from guitest import *

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
bad = box_triangles(10, 10, 10, 50, 0, 0, n=4)
bad[3] = (bad[3][0], bad[3][2], bad[3][1])
bad[5] = tuple(tuple(c + (3e-6 if (i, k) == (0, 0) else 0) for k, c in enumerate(v)) for i, v in enumerate(bad[5]))
write_stl(f"{W}/bad.stl", bad)
# two blocks that touch along one edge
two = box_triangles(10, 10, 10, 100, 0, 0) + box_triangles(10, 10, 10, 110, 10, 0)
write_stl(f"{W}/two.stl", two)
drop_files(f"{W}/bad.stl")
wait_for(lambda q: q["stl_dialog"], "the dialog", 60)
d = probe()["stl_dialog"]["items"][0]
check("loose joints" in d["check"] and "Join the loose joints" in d["check"], f"the dialog says it in plain words and has the button: {d['check']}")
click(*steady("stl_as_mesh"))
click(*steady("stl_import_ok"))
p = wait_for(lambda q: len(q["meshes"]) == 1, "imported", 60)
mid = p["meshes"][0]["id"]
click(*steady(f"mesh_row_{mid}"))
p = settle(0.5)
check("mesh_check" in p["widgets"], "the panel of the mesh is there")
click(*screen(p, "mesh_check"))
p = wait_for(lambda q: q["extra"]["mesh_check"], "the check is done", 30)
check("loose joints" in p["extra"]["mesh_check"], f"loose joints are named: {p['extra']['mesh_check']}")
check("mesh_check" in p["widgets"] and "mesh_merge" in p["widgets"], "the button to check again stays and the repair is offered")
check("mesh_orient" not in p["widgets"], "the turning is not offered while there are joints to join")
shot("01-check")
# the other buttons work at once: a scale of 2 in x
click(*screen(p, "mesh_scale_x"))
key("a", ctrl=True)
type_text("2")
click(*screen(p, "mesh_scale_apply"))
p = wait_for(lambda q: q["status"] != "" and close(q["meshes"][0]["bbox"][3] - q["meshes"][0]["bbox"][0], 20.0, 0.01), "the scale worked without selecting the body again", 30)
# the mesh stays selected after the scale: its panel goes on working (checking again needs no new click on the body)
p = settle(0.6)
check("mesh_check" in p["widgets"], "the panel of the mesh is still there after the scale")
click(*screen(p, "mesh_check"))
p = wait_for(lambda q: q["extra"]["mesh_check"], "the check again", 30)
click(*steady("mesh_merge"))
p = wait_for(lambda q: "Joined" in q["status"], "the joints were joined", 30)
check(not p["status_error"], f"the sentence of the repair is no error: {p['status']}")
check("mesh_check" in p["widgets"], "the panel is still there after the repair")
p = wait_for(lambda q: q["extra"]["mesh_check"] and "face inwards" in q["extra"]["mesh_check"], "the check is made again by itself and offers the turning", 30)
shot("02-after-join")
click(*steady("mesh_orient"))
p = wait_for(lambda q: "Turned" in q["status"], "the triangles were turned", 30)
check(not p["status_error"], f"no error: {p['status']}")
p = wait_for(lambda q: q["extra"]["mesh_check"] and "fine for printing" in q["extra"]["mesh_check"], "fine for printing", 30)
check("mesh_merge" not in p["widgets"] and "mesh_orient" not in p["widgets"], "no repair button is left")
shot("03-clean")
# nothing to repair: no button, and no red text
# two parts that touch along an edge
p = import_dropped(f"{W}/two.stl")
mid2 = p["meshes"][-1]["id"]
click(*steady(f"mesh_row_{mid2}"))
p = settle(0.5)
click(*screen(p, "mesh_check"))
p = wait_for(lambda q: q["extra"]["mesh_check"], "the check of the two parts", 30)
t = p["extra"]["mesh_check"]
check("more than two faces" in t and "do not repair" in t, f"the edges are explained and no button is said to repair them: {t}")
check("mesh_merge" not in p["widgets"] and "mesh_orient" not in p["widgets"], "no repair button for them")
shot("04-edges")
finish()
