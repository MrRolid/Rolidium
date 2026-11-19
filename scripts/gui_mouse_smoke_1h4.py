#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Rotation ring drag with the real mouse, then the Revolve of a flat face and closing with unsaved changes."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 482); click(); time.sleep(0.4)
move(70, 513); click(); time.sleep(0.6)
key("m"); time.sleep(0.3)
move(450, 450); time.sleep(0.4); click(); time.sleep(0.6)
move(565, 412); time.sleep(0.8)
shot("r0-ring-hover")
d = drag(565, 412, 530, 470, steps=12)
time.sleep(0.5)
shot("r1-ring-dragging")
d(); time.sleep(1.5)
shot("r2-rotated")
