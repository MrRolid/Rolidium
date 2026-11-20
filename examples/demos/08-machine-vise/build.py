#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts
from parts import STAGE, staged

s = Script("Machine vise: a base 200 x 80 x 25 with a guide and two clamping slots, a fixed and a moving jaw with replaceable inserts (80 x 25 x 8 on two "
           "screws each), a lead screw of 16 x 180 with a real thread (pitch 4), a nut in the moving jaw and a crank. Jaws open 60 mm. 14 bodies.")
AX, AY, AZ = 0, 40, 50              # the screw axis: y = 40, z = 50 above the base (the base is 25 high), along X

s.comment("Base (the body made by the box command): a guide groove 20 x 8 along X for the tongue of the moving jaw, two clamping slots 12 x 40 through it.")
s.box(0, 0, 0, 200, 80, 25)
s.sketch("yz", -1)
s.rect(30, 17, 50, 26)
s.pull(202, "cut")
for x0 in (8, 180):
    s.sketch("xy", -1)
    s.rect(x0, 20, x0 + 12, 60)
    s.pull(27, "cut")

s.comment("Everything else is built far away (the staging area) and moved, because bodies that touch are joined into one by Pull.")
def fixed_jaw(s, f):
    start = s.n + 1
    f.ext("w", 20, 50, ("rect", 0, 25, 80, 75), "add", joins=False)
    f.ext("w", 19, 51, ("circle", AY, AZ, 16.4), "cut")                       # the screw passes with 0.2 mm clearance
    for u in (15, 65):
        f.ext("w", 42, 51, ("circle", u, AZ, 5.9), "cut")                     # blind holes for the screws of the insert
    return [start]

def moving_jaw(s, f):
    start = s.n + 1
    f.ext("w", 126, 156, ("rect", 0, 25, 80, 75), "add", joins=False)
    f.ext("w", 126, 156, ("rect", 30.1, 17.1, 49.9, 25), "add")               # the tongue in the guide (0.1 mm clear on every side)
    f.ext("w", 125, 157, ("circle", AY, AZ, 16.4), "cut")
    f.ext("w", 130, 150, ("circle", AY, AZ, 26.2), "cut")                      # the pocket of the nut
    for u in (15, 65):
        f.ext("w", 125, 134, ("circle", u, AZ, 5.9), "cut")
    return [start]

def insert(s, f, x0, x1, exposed_high):
    """An insert 80 x 25 x 8: x0..x1, bore of 16.4, two counterbored holes (6.4, counterbore 10.4 x 6.2) from the exposed face."""
    start = s.n + 1
    f.ext("w", x0, x1, ("rect", 0, 37.5, 80, 62.5), "add", joins=False)
    f.ext("w", x0 - 1, x1 + 1, ("circle", AY, AZ, 16.4), "cut")
    for u in (15, 65):
        f.ext("w", x0 - 1, x1 + 1, ("circle", u, AZ, 6.4), "cut")
        if exposed_high:
            f.ext("w", x1 - 6.2, x1 + 1, ("circle", u, AZ, 10.4), "cut")
        else:
            f.ext("w", x0 - 1, x0 + 6.2, ("circle", u, AZ, 10.4), "cut")
    return [start]

def nut(s, f):
    start = s.n + 1
    f.ext("w", 130, 150, ("circle", AY, AZ, 26), "add", joins=False)
    f.ext("w", 129, 151, ("circle", AY, AZ, 16.1), "cut")                      # the thread of the nut is not drawn: a smooth bore
    return [start]

staged(s, "x", 1, (0, 0, 0), fixed_jaw)
staged(s, "x", 1, (0, 0, 0), moving_jaw)
staged(s, "x", 1, (0, 0, 0), insert, 50, 58, True)
staged(s, "x", 1, (0, 0, 0), insert, 118, 126, False)
staged(s, "x", 1, (0, 0, 0), nut)

s.comment("Four screws of the inserts: head 10 x 6 in the counterbore, shank 5.8, 9.8 long (M6, the thread is not drawn).")
for (head_x, sign, shank_end) in ((51.8, 1, 42.0), (124.2, -1, 134.0)):
    first = None
    for u in (15, 65):
        pos = (head_x, u, AZ)
        if first is None:
            first = staged(s, "x", sign, pos, parts.screw, 5.8, 9.8, 10, 6, 5)[0]
            fpos = pos
        else:
            parts.copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))

s.comment("The lead screw: a cylinder of 16 x 180 built standing up, with the cross hole for the crank and a thread of pitch 4 (a trapezoid groove swept along a helix, 27 turns), then laid along X.")
SX, SY, SZ = STAGE
s.sketch("xy", SZ)
s.circle(SX, SY, 16)
s.pull(180, "add"); s.n += 1
screw_body = s.n
s.sketch("xz", -(SY + 10))
s.circle(SX, SZ + 10, 8.2)                      # the cross hole at 10 mm from the head end (it becomes x = 0), along Y
s.pull(20, "cut")
z0 = SZ + 70                                      # the thread starts at x = 60 and runs 108 mm
s.raw(f"helix radius 7.2 pitch 4 height 108 at {SX} {SY} {z0}")
s.sketch("xz", -SY)
s.line(SX + 6.0, z0 - 0.3, SX + 8.4, z0 - 1.5)
s.line(SX + 8.4, z0 - 1.5, SX + 8.4, z0 + 1.5)
s.line(SX + 8.4, z0 + 1.5, SX + 6.0, z0 + 0.3)
s.line(SX + 6.0, z0 + 0.3, SX + 6.0, z0 - 0.3)
s.raw("sweep 0 curve 1 cut")
s.rotate("y", 90, screw_body)
s.move(80 - SX, AY - SY, AZ - (SZ + 90), screw_body)

s.comment("The crank: a bar of 8 x 100 through the cross hole and two end pieces of 14 x 12.")
staged(s, "y", 1, (0, -10, AZ), lambda s, f: f.ext("w", 0, 100, ("circle", 0, 0, 8), "add", joins=False) or [s.n])
staged(s, "y", 1, (0, -22, AZ), lambda s, f: f.ext("w", 0, 12, ("circle", 0, 0, 14), "add", joins=False) or [s.n])
staged(s, "y", 1, (0, 90, AZ), lambda s, f: f.ext("w", 0, 12, ("circle", 0, 0, 14), "add", joins=False) or [s.n])
s.name(1, "Vise base")
s.name(2, "Fixed jaw")
s.name(3, "Moving jaw")
s.name(4, "Fixed jaw insert")
s.name(5, "Moving jaw insert")
s.name(6, "Lead screw nut")
s.name(7, "Screw M6x10")
s.name(8, "Screw M6x10")
s.name(9, "Screw M6x10")
s.name(10, "Screw M6x10")
s.name(11, "Lead screw")
s.name(12, "Crank bar")
s.name(13, "Crank end left")
s.name(14, "Crank end right")
s.link(11, 6, "the thread of the nut is not modelled (smooth bore of the nut)")
s.link(12, 11, "the crank bar sits in the cross hole of the screw (0.2 mm clearance)")
s.header(14, 648097.941, no_overlap="yes", slow="yes", connected="yes")
s.raw("export-step target/demos/08-machine-vise/08-machine-vise.step")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
