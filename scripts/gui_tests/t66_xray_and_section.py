#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""X-ray (Q) and the section (Y): the picture shows the inside of a hollow box. The section is cut by a plane that is chosen in a panel (X, Y, Z, a flat face), the cut is filled with
colour, only the picture changes (the volume stays), and both work together with the Shell tool; the Shell tool shows its walls through a section by itself."""
# FIXTURE: box 60 40 30; shell 3 open z+
# POINTS: top=30,20,30
from guitest import *
import subprocess

def count_color(name, rgb, fuzz="6%"):
    path = f"{SHOTS}/t66_xray_and_section-{name}.png"
    out = subprocess.run(["convert", path, "-crop", "780x700+196+46", "-fuzz", fuzz, "-fill", "white", "-opaque", f"rgb({rgb[0]},{rgb[1]},{rgb[2]})",
                          "-fill", "black", "+opaque", "white", "-format", "%[fx:mean*w*h]", "info:"], capture_output=True, text=True).stdout
    return float(out or 0)

CAP = (242, 158, 64)
time.sleep(1.0)
focus_app_window()
p = settle()
v0 = p["bodies"][0]["volume"]
shot("00-solid")
check(count_color("00-solid", CAP) < 20, "no cap colour in the plain view")
key("q")
p = settle(0.6)
check(p["extra"]["view"]["xray"], f"Q turns the X-ray view on: {p['extra']['view']}")
shot("01-xray")
key("q")
p = settle(0.5)
check(not p["extra"]["view"]["xray"], "Q again turns it off")
key("y")
p = settle(0.7)
sec = p["extra"]["view"]["section"]
check(sec["on"] and "section_x" in p["widgets"] and "section_pos" in p["widgets"], f"Y turns the section on and shows its panel: {sec}")
click(*steady("section_y"))
p = settle(0.6)
shot("02-section-y")
cap = count_color("02-section-y", CAP)
check(cap > 150, f"the cut is filled with colour: {cap} pixels")
check(abs(p["bodies"][0]["volume"] - v0) < 1e-6, "the model is not changed")
click(*steady("section_flip"))
p = settle(0.6)
shot("03-section-flipped")
check(p["extra"]["view"]["section"]["flip"], "the other side")
# a position typed: the cut moves
click(*steady("section_pos"))
key("a", ctrl=True)
type_text("5")
key("Return")
p = settle(0.6)
check(abs(p["extra"]["view"]["section"]["pos"] - 5.0) < 0.01, f"a typed position: {p['extra']['view']['section']['pos']}")
shot("04-section-moved")
click(*steady("section_z"))
p = settle(0.6)
check(p["extra"]["view"]["section"]["axis"] == 2, "the Z axis")
key("y")
p = settle(0.5)
check(not p["extra"]["view"]["section"]["on"], "Y turns the section off")
shot("05-off")
# with the Shell tool the section comes by itself and goes away with the tool
key("o")
p = settle(0.5)
click(*screen(p, "top"))
p = wait_for(lambda q: q["extra"]["shell"]["preview"], "the preview of the shell", 40)
p = settle(0.8)
check(p["extra"]["view"]["section"]["on"] and p["extra"]["view"]["section"]["by_tool"], f"the Shell shows the walls through a section: {p['extra']['view']['section']}")
shot("06-shell-section")
key("Escape")
p = settle(0.6)
check(not p["extra"]["view"]["section"]["on"], "the section goes away with the tool")
finish()
