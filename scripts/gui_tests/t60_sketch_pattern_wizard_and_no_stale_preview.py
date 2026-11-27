#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""In a sketch S selects the shapes of the sketch (a click, Ctrl, a rectangle) and stays in the sketch; T is the same wizard as in 3D: "Click what you want to repeat", the three
big buttons, the click on a guide (a line of the sketch), then the numbers, with "all the shapes of the sketch" only a choice in the panel. The blue preview of the copies goes with
Ctrl+Z, with Esc and with a new document (it used to stay on the screen until the next zoom)."""
# FIXTURE: box 100 60 10
# POINTS: top=90,50,10;cc=30,30,10;cr=45,30,10;sm=75,48,10;sc=82,48,10;l0=10,8,10;l1=90,8,10
from guitest import *
import subprocess

def blue_pixels(name):
    """The pixels of the colour of the preview of the copies (a light blue line) in the picture of the view."""
    path = f"{SHOTS}/{os.path.splitext(os.path.basename(sys.argv[0]))[0]}-{name}.png"
    out = subprocess.run(["convert", path, "-crop", "780x700+196+46", "-fuzz", "4%", "-fill", "white", "-opaque", "rgb(89,166,255)",
                          "-fill", "black", "+opaque", "white", "-format", "%[fx:mean*w*h]", "info:"], capture_output=True, text=True).stdout
    return float(out or 0)

time.sleep(1.0)
focus_app_window()
p = settle()
click(*screen(p, "top"))
key("k")
p = settle(1.5)
key("c")
settle(0.3)
click(*screen(p, "cc"))
click(*screen(p, "cr"))
settle(0.6)
key("Escape")
key("c")
p = settle(0.4)
click(*screen(p, "sm"))
time.sleep(0.5)
click(*screen(p, "sc"))
settle(0.4)
key("Escape")
p = settle(0.4)
check(len(p["circles"]) == 2, f"two circles: {len(p['circles'])}")
key("l")
click(*screen(p, "l0"))
click(*screen(p, "l1"))
key("Escape")
p = settle(0.4)
# C: S stays in the sketch
key("s")
p = settle(0.4)
check(p["sketching"] == "None", f"S stays in the sketch with no drawing tool: {p['sketching']}")
# select the small circle, Ctrl + the big one
click(*screen(p, "sc"))
ctrl_click(*screen(p, "cr"))
p = settle(0.4)
check(p["extra"]["entities_selected"] == 2, f"two shapes chosen with Ctrl: {p['extra']['entities_selected']}")
# A: T asks for the object, then offers the three ways
key("t")
p = settle(0.5)
check(p["extra"]["sk_pattern"]["open"] == "Linear", f"the panel is open: {p['extra']['sk_pattern']}")
for w in ("want_curve", "want_point", "want_face"):
    check(w in p["widgets"], f"the big button {w} is in the panel")
check(not p["extra"]["sk_pattern"]["all"], "the pattern does not repeat all the shapes unless it is chosen")
shot("01-three-buttons")
click(*steady("want_curve"))
p = settle(0.4)
check(p["extra"]["sk_pattern"]["want"] == "Curve", f"Along a line is chosen: {p['extra']['sk_pattern']}")
click(*screen(p, "l0"))
move(*screen(p, "l1"))
p = settle(0.2)
click(*screen(p, "l1"))
p = settle(0.6)
check(p["extra"]["sk_pattern"]["guided"], f"the line is the guide: {p['extra']['sk_pattern']}")
check(close(p["extra"]["sk_pattern"]["angle"], 0.0, 0.5), f"the direction of the row is the direction of the line: {p['extra']['sk_pattern']['angle']}")
check(p["extra"]["sk_pattern"]["preview"] > 0 and p["extra"]["sk_pattern"]["drawn"] > 0, f"the preview of the copies is drawn: {p['extra']['sk_pattern']}")
shot("02-preview")
b2 = blue_pixels("02-preview")
print("blue pixels with the preview:", b2)
check(b2 > 30, f"the preview is on the picture: {b2}")
# B: Ctrl+Z with the preview open, then a new document
key("z", ctrl=True)
p = settle(0.6)
check(p["extra"]["sk_pattern"]["drawn"] == 0, f"after Ctrl+Z no blue circles are drawn: {p['extra']['sk_pattern']}")
shot("03-after-undo")
key("Escape")
p = settle(0.4)
check(p["extra"]["sk_pattern"]["open"] is None and p["extra"]["sk_pattern"]["drawn"] == 0, f"Esc closes the panel: {p['extra']['sk_pattern']}")
# the same with a new document
key("s")
key("t")
settle(0.3)
key("n", ctrl=True)
p = settle(0.5)
if "confirm_discard" in p["widgets"]:
    click(*steady("confirm_discard"))
    p = settle(0.6)
check(p["extra"]["sk_pattern"]["drawn"] == 0 and p["extra"]["sk_pattern"]["open"] is None, f"a new document has no preview: {p['extra']['sk_pattern']}")
shot("04-new-document")
b4 = blue_pixels("04-new-document")
check(b4 < 5, f"the picture of the new document has no blue copies: {b4}")
finish()
