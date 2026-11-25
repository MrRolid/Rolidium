#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A drawing tool chosen in the 3D view (no sketch open): the cursor asks for a face (a flat face of a solid, a flat wall of a mesh or the base plane), the face under
it shows a grid, one click starts the sketch there and puts the first point (the centre of a circle) where the click was; the view does not turn (V turns it, Esc
leaves the sketch and the view returns); Esc before the click cancels."""
# FIXTURE: box 40 30 20
# POINTS: boxtop=10,8,20;meshtop=115,10,10;ground=60,-25,0
from guitest import *
import math

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
write_stl(f"{W}/block.stl", box_triangles(30, 30, 10, 100, 0, 0))
p = import_dropped(f"{W}/block.stl")
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] == 6, "the walls of the mesh are found", 30)
cam0 = p["extra"]["camera"]


def world_of_circle(p):
    c = p["circles"][-1]
    pl = p["extra"]["sketch_plane"]
    return [pl["origin"][i] + pl["x_dir"][i] * c["x"] + pl["y_dir"][i] * c["y"] for i in range(3)]


def quick_circle(point, z, what):
    key("c")
    p = settle(0.4)
    check(p["extra"]["quick_tool"] == "Circle" and p["extra"]["plane_pick"], f"{what}: the Circle tool asks for a face: {p['extra']['quick_tool']}")
    x, y = steady(point)
    move(x, y)
    p = wait_for(lambda q: q["extra"]["plane_hover"], f"{what}: the face under the cursor shows its grid", 10)
    check(abs(p["extra"]["plane_hover"]["normal"][2]) > 0.99, f"{what}: the face is flat and looks up: {p['extra']['plane_hover']}")
    shot(f"{what}-hover")
    click(x, y)
    p = wait_for(lambda q: q["sketching"] == "Circle" and q["drawing"], f"{what}: the sketch is open with the first point placed", 10)
    check(p["extra"]["sketch_quick"], "it is a quick sketch")
    check(close(p["extra"]["sketch_plane"]["origin"][2], z, 0.01) or abs(p["extra"]["sketch_plane"]["normal"][2]) > 0.99, f"{what}: the sketch lies in the plane z = {z}: {p['extra']['sketch_plane']}")
    check(p["extra"]["camera"] == cam0, f"{what}: the view did not turn: {p['extra']['camera']} was {cam0}")
    # the second click gives the radius
    click(x + 30, y)
    p = wait_for(lambda q: q["circles"], f"{what}: the circle is there", 10)
    w = world_of_circle(p)
    return p, w


p, w = quick_circle("boxtop", 20.0, "01-box")
check(abs(w[0] - 10.0) < 1.6 and abs(w[1] - 8.0) < 1.6 and abs(w[2] - 20.0) < 0.05, f"the centre of the circle is where the click was (10, 8, 20): {w}")
shot("01-box-circle")
key("Escape")
p = settle(0.6)
check(p["sketching"] is None, f"Esc leaves the sketch: {p['sketching']}")
# a wall of a mesh
n0 = len(p["circles"])
key("c")
settle(0.3)
x, y = steady("meshtop")
move(x, y)
p = wait_for(lambda q: q["extra"]["plane_hover"], "the wall of the mesh shows its grid", 10)
click(x, y)
p = wait_for(lambda q: q["sketching"] == "Circle" and q["drawing"], "the sketch on the wall of the mesh", 10)
click(x + 30, y)
p = wait_for(lambda q: len(q["circles"]) == n0 + 1, "the circle on the mesh wall", 10)
w = world_of_circle(p)
check(abs(w[0] - 115.0) < 1.6 and abs(w[1] - 10.0) < 1.6 and abs(w[2] - 10.0) < 0.05, f"the circle is on the top wall of the mesh, at the click: {w}")
shot("02-mesh-circle")
key("Escape")
settle(0.5)
# the base plane
n0 = len(probe()["circles"])
p, w = quick_circle("ground", 0.0, "03-base")
check(abs(w[2]) < 0.05, f"the circle lies in the base plane: {w}")
# V turns the view, Esc returns it
key("v")
p = wait_for(lambda q: q["extra"]["camera"] != cam0, "V turns the view", 10)
shot("04-v")
key("Escape")
p = wait_for(lambda q: q["sketching"] is None, "Esc leaves the sketch", 10)
p = wait_for(lambda q: all(abs(a - b) < 0.02 for a, b in zip(q["extra"]["camera"], cam0)), "the view returned", 10)
# Esc before the click cancels
key("r")
p = settle(0.4)
check(p["extra"]["quick_tool"] == "Rect", "the Rectangle tool asks for a face")
key("Escape")
p = settle(0.4)
check(p["extra"]["quick_tool"] is None and not p["extra"]["plane_pick"] and p["sketching"] is None, "Esc cancels before the click")
finish()
