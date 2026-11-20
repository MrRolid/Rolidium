#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import math, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from rdm import Script

s = Script("Knurled hand knob: diameter 60, 15 thick, 24 half-round grooves R2.5 around the rim, a hub of diameter 20 standing 10 mm out, a bore of 8 with a keyway 3 x 1.4.")
R, T, HUB, HUB_H = 30, 15, 20, 10

s.comment("Disc and hub (the hub is pulled from the top face and joins the disc).")
s.cyl("xy", 0, 0, 0, 2 * R, T)
s.cyl("xy", T, 0, 0, HUB, HUB_H)

s.comment("24 grooves: circles of diameter 5 centred on the rim, every 15 degrees, cut through the thickness of the disc.")
s.sketch("xy", -1)
for k in range(24):
    a = math.radians(15 * k)
    s.circle(R * math.cos(a), R * math.sin(a), 5)
s.pull(T + 2, "cut", times=24)

s.comment("Bore diameter 8 and a keyway 3 x 1.4 (the slot reaches 1.4 mm beyond the bore).")
s.cyl("xy", -1, 0, 0, 8, T + HUB_H + 2, "cut")
s.sketch("xy", -1)
s.rect(-1.5, 3.5, 1.5, 5.4)
s.pull(T + HUB_H + 2, "cut")

s.comment("A shallow round recess, diameter 44 and 1.5 deep, in the front face (the side without the hub).")
s.cyl("xy", 0, 0, 0, 44, 1.5, "cut")

s.comment("Edges chamfered 1 mm: the top edge of the hub and the mouth of the recess.")
s.chamfer(1, HUB / 2, 0, T + HUB_H)
s.chamfer(1, 22, 0, 0)
s.name(1, "Knurled knob")
s.header(1, 38413.327, connected="yes")
s.raw("info")
s.write(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build.txt"))
