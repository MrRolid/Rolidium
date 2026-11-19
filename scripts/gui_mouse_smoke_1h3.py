#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1h with the real mouse: angled chamfer panel, rotation ring drag, Revolve from a face, closing with unsaved changes."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 482); click(); time.sleep(0.4)
move(70, 513); click(); time.sleep(0.6)
move(797, 571); time.sleep(0.6); click(); time.sleep(0.6)
shot("c1-edge-panel")
move(912, 513); click(); time.sleep(0.5)
move(878, 537); click(); key("a", ctrl=True); type_text("5")
shot("c2-chamfer-panel")
move(878, 561); click(); key("a", ctrl=True); type_text("30"); key("Return"); time.sleep(1.5)
shot("c3-chamfered")
move(797, 571); time.sleep(0.6); click(); time.sleep(0.8)   # click the new chamfer face
shot("c4-chamfer-face-panel")
move(770, 530); time.sleep(0.6); click(); time.sleep(0.8)
shot("c5-chamfer-face-selected")
key("Escape"); key("m"); time.sleep(0.3)
move(450, 450); time.sleep(0.4); click(); time.sleep(0.6)
shot("c6-gizmo")
move(670, 395); time.sleep(0.6)
d = drag(670, 395, 760, 415, steps=10)
time.sleep(0.5)
shot("d0-arrow-dragging")
d(); time.sleep(1.2)
key("z", ctrl=True); time.sleep(1.0)
move(620, 438); time.sleep(0.8)
d = drag(620, 438, 660, 470, steps=10)
time.sleep(0.5)
shot("d1-ring-dragging")
d(); time.sleep(1.5)
shot("d2-rotated")
key("z", ctrl=True); time.sleep(1.0)
d = drag(680, 407, 760, 425, steps=10)
time.sleep(0.5)
shot("d3-arrow-dragging")
d(); time.sleep(1.0)
# ---- Revolve from a flat face of a body ----
key("Escape"); key("s"); time.sleep(0.3)
move(900, 700); click(); time.sleep(0.3)
move(540, 300); time.sleep(0.5); click(); time.sleep(0.8)
shot("e1-top-face-panel")
