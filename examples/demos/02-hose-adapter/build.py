#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from rdm import Script

s = Script("Hose adapter: a turned part, 90 mm long. The half section is drawn once on the XZ plane (x = radius, y = height) and revolved.")

# (radius, height) of the closed half section
pts = []
def p(r, z):
    pts.append((r, z))

# outer side, from the mouth of nipple A (z = 0) up to the mouth of nipple B (z = 90)
p(24, 0); p(25, 1)
for zb in (6, 16, 26):                 # three barbs on nipple A: a ramp that rises toward the collar, then a flat step
    p(25, zb); p(26.6, zb + 3.2)
    p(25, zb + 3.2)
p(25, 41); p(35, 41); p(35, 49); p(19, 49)       # the collar is 70 mm across, 8 wide
for zs in (60, 70, 80):                 # three barbs on nipple B, mirrored: the ramp falls toward the mouth
    p(19, zs); p(20.6, zs); p(19, zs + 3.2)
p(19, 89); p(18, 90)
# bore, from the mouth of B back to A: 30 mm wide on the B side, a cone, 42 mm wide on the A side
p(16, 90); p(15, 89); p(15, 60); p(21, 30); p(21, 1); p(22, 0)

s.comment("Half section (x = radius, y = height), 3 barbs per nipple, bore 42 -> 30 with a smooth cone.")
s.sketch("xz")
s.poly(pts)
s.raw("revolve 0 z 360 add")
s.comment("Transitions rounded R2: the inside corners where the collar meets the nipples and the outer corners of the collar.")
for r, z in [(25, 41), (19, 49), (35, 41), (35, 49)]:
    s.round(2, r, 0, z)
s.name(1, "Hose adapter")
s.header(1, 65994.352, connected="yes")
s.raw("info")
s.write(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build.txt"))
