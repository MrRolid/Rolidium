#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Revolve of a flat face of a body about one of its edges, then closing the window with unsaved changes."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 482); click(); time.sleep(0.4)
move(70, 513); click(); time.sleep(0.6)
move(540, 300); time.sleep(0.5); click(); time.sleep(0.8)
move(736, 404); click(); time.sleep(0.6)               # the Revolve button of the face panel
move(536, 392); time.sleep(0.6)
shot("v1-axis-hover")
click(); time.sleep(0.5)
type_text("90"); time.sleep(0.6)
shot("v2-angle-preview")
key("Return"); time.sleep(1.5)
shot("v3-revolved")
close_window(); time.sleep(1.0)
shot("v4-close-asks")
