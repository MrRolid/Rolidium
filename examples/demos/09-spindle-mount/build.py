#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts

s = Script("Router spindle with a clamp mount: housing 65 x 200, nose with a collet nut and a cutter of 6 mm, a cap with a cable gland, and the clamp "
           "mount (bore 65, slit 1.5, two M6 clamp screws, four holes to bolt it down). The tool points down (-Z). Eight bodies.")
s.comment("Every part is built far away (the staging area) and moved into place (parts that touch would be joined into one body by Pull).")
A, SG = "z", -1                       # the spindle axis is Z, the local w runs downward
for builder in (parts.spindle_body, parts.spindle_nut, parts.cutter, parts.spindle_cap, parts.cable_gland, parts.spindle_mount):
    parts.staged(s, A, SG, (0, 0, 0), builder)
mf = Frame(s, A, SG, (0, 0, 0))
first = None
for w in parts.MOUNT_SCREWS:
    pos = mf.world(38, 38, w)                           # on the floor of the pocket (u = 38), at v = 38
    if first is None:
        first = parts.staged(s, "x", 1, pos, parts.screw, 5.8, 70, 10, 6, 5)[0]
        fpos = pos
    else:
        parts.copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
s.name(1, "Spindle housing")
s.name(2, "Collet nut")
s.name(3, "End mill 6 mm")
s.name(4, "Spindle cap")
s.name(5, "Cable gland")
s.name(6, "Clamp mount")
s.name(7, "Screw M6x70")
s.name(8, "Screw M6x70")
s.link(3, 1, "the cutter sits in the bore of the nose (0.1 mm clearance)")
s.link(6, 1, "the housing sits in the clamping bore of the mount (0.05 mm clearance)")
s.header(8, 1006190.014, no_overlap="yes", connected="yes")
s.raw("export-step target/demos/09-spindle-mount/09-spindle-mount.step")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
