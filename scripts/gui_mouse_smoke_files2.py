#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Part 2: after the crash the recovery question is shown; restore it, then Ctrl+N asks about unsaved changes."""
import time
from xdrive import *
from gui_mouse_smoke_files import title

time.sleep(1.0)
focus_app_window()
shot("f4-recovery-question")
move(510, 448); click(); time.sleep(2.0)
print("title after restore:", title())
shot("f5-restored")
key("n", ctrl=True); time.sleep(0.8)
shot("f6-unsaved-question")
