#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import math, os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script
import parts

s = Script("Pillow block with a 608 ball bearing (8 x 22 x 7 mm): housing, inner and outer ring with raceways, seven balls of 3.969 mm. "
           "The shaft axis is along Y at a height of 20 mm.")
s.comment("The part is made by parts.bearing_block (the machine, demo 10, uses it too). The rings and balls are revolved about an axis through the origin")
s.comment("(that place has to be empty), the housing is built far away and everything is moved to the place: axis along Y, 20 mm above the floor.")
parts.staged(s, "y", 1, (0, 0, 20), parts.bearing_block)
s.header(10, 30240.269, no_overlap="yes")
s.raw("export-step target/demos/05-bearing-block/05-bearing-block.step")
s.raw("info")
s.write(os.path.join(here, "build.txt"))

# the picture file of the section (outputs.txt, used by build-all.sh): the program has no section view, so a half is cut away
out = ["# a half (x > 0) of every body cut away: the cut face is seen from the right. Balls that lie wholly in the removed half are deleted.",
       "box 80 60 80", "move 0 -30 -10 body 11"]
ball_bodies = {k: (5 + k if k < 6 else 4) for k in range(7)}
cuts, deletes = [1, 2, 3], []
for k, b in ball_bodies.items():
    cx = parts.R_PITCH * math.cos(2 * math.pi * k / 7)
    if cx - parts.R_BALL > 0:
        deletes.append(b)
    elif abs(cx) < parts.R_BALL:
        cuts.append(b)
for b in cuts:
    out.append(f"combine cut {b} 11 keep")
out.append("delete body 11")
for b in sorted(deletes, reverse=True):
    out.append(f"delete body {b}")
out += ["save @OUT@/@NAME@-section.rolidium", "render @OUT@/@NAME@-detail.png 1920 1080 right"]
open(os.path.join(here, "outputs.txt"), "w").write("\n".join(out) + "\n")
