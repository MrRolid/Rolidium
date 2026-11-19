#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1g with the real mouse: sketch aids (3D snapping, guides, reference distances, mirrored points)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)
move(70, 451); click(); time.sleep(0.6)
move(540, 300); time.sleep(0.4); click(); time.sleep(0.4)
key("k"); time.sleep(0.8)
key("c"); time.sleep(0.4)
move(305, 526); time.sleep(0.8)
shot("s1-3d-vertex")
move(318, 520); time.sleep(0.6)
shot("s2-near-3d-vertex")
move(540, 300); time.sleep(0.4); click(); time.sleep(0.4)    # circle centre in the middle of the face
key("Escape"); time.sleep(0.3)
move(256, 323); time.sleep(0.6)                              # a face corner: becomes the reference
move(300, 330); time.sleep(0.6)
shot("s3-reference-fields")
type_text("")
key("Tab"); type_text("5"); key("Tab"); type_text("5"); time.sleep(0.4)
shot("s4-typed-distances")
key("Return"); time.sleep(0.4)
type_text("4"); key("Return"); time.sleep(1.0)
shot("s5-hole-5-from-corner")
move(700, 350); time.sleep(0.8)
shot("s6-mirrored-helpers")
