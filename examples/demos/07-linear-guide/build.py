#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts

s = Script("Linear guide of size 15: a rail 300 mm long with counterbored holes every 60 mm and a carriage with two end caps and a grease nipple; the rail runs along X.")
s.comment("Parts are built far away and moved in (parts that touch would be joined into one body by Pull).")
parts.staged(s, "x", 1, (0, 0, 0), parts.linear_rail, 300)
parts.staged(s, "x", 1, (120, 0, 0), parts.carriage, 60)
parts.staged(s, "x", 1, (114, 0, 0), parts.end_cap)            # the caps touch the carriage with a face (a contact is fine)
parts.staged(s, "x", 1, (180, 0, 0), parts.end_cap)
parts.staged(s, "x", 1, (186, 0, 19), parts.grease_nipple)
s.header(5, 117533.047, no_overlap="yes")
s.raw("export-step target/demos/07-linear-guide/07-linear-guide.step")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
