#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts

s = Script("NEMA 23 stepper motor with a mounting bracket, a clamp coupling and screws; the shaft points along +X. Nine bodies.")
s.comment("Every part is built far away (the staging area) and moved into place, because parts that touch would be joined into one body.")
AXIS = "x"
bodies = []
motor = parts.staged(s, AXIS, 1, (0, 0, 0), parts.nema23_motor)
bracket = parts.staged(s, AXIS, 1, (0, 0, 0), parts.motor_bracket)
# the coupling starts 6 mm in front of the bracket plate (the plate ends at 5)
cf = Frame(s, AXIS, 1, (6, 0, 0))
coupling = parts.staged(s, AXIS, 1, (6, 0, 0), parts.shaft_coupling)
# four M5 screws (head 8.5 x 5, socket 4, shank 4.9 x 9.5) from the front of the plate into the flange
sf = Frame(s, AXIS, 1, (0, 0, 0))
first = None
for i, (sx, sy) in enumerate([(-1, -1), (1, -1), (-1, 1), (1, 1)]):
    pos = sf.world(sx * 23.57, sy * 23.57, 5)
    if first is None:
        first = parts.staged(s, AXIS, 1, pos, parts.screw, 4.9, 9.5, 8.5, 5, 4)
        first = first[0]
    else:
        parts.copy_to(s, first, tuple(pos[k] - firstpos[k] for k in range(3)))
    if i == 0:
        firstpos = pos
# two M3 screws (head 5.5 x 3, socket 2.5, shank 2.9 x 17) in the clamp, from the +Z side (local +v) down
first = None
for i, w in enumerate(parts.COUPLING_SCREWS):
    pos = cf.world(8.5, 8, w)
    if first is None:
        first = parts.staged(s, "z", 1, pos, parts.screw, 2.9, 17, 5.5, 3, 2.5)[0]
        firstpos = pos
    else:
        parts.copy_to(s, first, tuple(pos[k] - firstpos[k] for k in range(3)))
s.header(9, 225089.134, no_overlap="yes")
s.raw("export-step target/demos/06-stepper-motor/06-stepper-motor.step")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
