#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from rdm import Script

s = Script("Electronics box with a lid: 100 x 60 x 30, wall 2, four screw posts, six vent slots, a cable hole, and a lid with a lip.")
W, D, H, T = 100, 60, 30, 2
corners = [(7, 7), (93, 7), (7, 53), (93, 53)]

s.comment("The box: a block with the four vertical edges rounded R4, then the cavity cut out and its edges rounded R2 (wall 2 everywhere).")
s.box(0, 0, 0, W, D, H)
for x, y in [(0, 0), (W, 0), (0, D), (W, D)]:
    s.round(4, x, y, H / 2)
s.sketch("xy", T)
s.rect(T, T, W - T, D - T)
s.pull(H, "cut")
for x, y in [(T, T), (W - T, T), (T, D - T), (W - T, D - T)]:
    s.round(2, x, y, 16)

s.comment("Four screw posts (diameter 6, hole 2.5) in the corners, 26 mm tall.")
s.sketch("xy", T)
for x, y in corners:
    s.circle(x, y, 6)
s.pull(26, "add", times=4)
s.hole("xy", 13, corners, 2.5, 15)

s.comment("Six vent slots 2 x 15 in the long wall at y = 0 (drawn on a plane inside the box, cut through the wall toward -Y).")
s.sketch("xz", -3)
for i in range(6):
    x = 25 + 10 * i
    s.rect(x - 1, 7.5, x + 1, 22.5)
s.pull(4, "cut", times=6)

s.comment("Cable hole, diameter 8, in the end wall at x = 0.")
s.hole("yz", -1, [(30, 15)], 8, 4)

s.comment("The lid (body 2) is lifted 15 mm above the box: a 2 mm plate, a lip 1.5 mm high that fits the cavity with 0.2 mm clearance on each side.")
s.sketch("xy", H + 15)
s.rect(0, 0, W, D)
s.pull(2, "add")
s.sketch("xy", H + 13.5)
s.rect(T + 0.2, T + 0.2, W - T - 0.2, D - T - 0.2)
s.rect(T + 1.4, T + 1.4, W - T - 1.4, D - T - 1.4)
s.pull(1.5, "add", region=int(os.environ.get("LIP_REGION", "0")))
for x, y in [(0, 0), (W, 0), (0, D), (W, D)]:
    s.round(4, x, y, H + 16, body=2)
s.comment("Four holes 3.2 over the posts, countersunk (the chamfer of the upper edge makes the cone for the screw head).")
s.hole("xy", H + 14, corners, 3.2, 4)
for x, y in corners:
    s.chamfer(1.2, x + 1.6, y, H + 17, body=2)
s.header(2, 43749.262, no_overlap="yes")
s.raw("info")
s.write(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build.txt"))
