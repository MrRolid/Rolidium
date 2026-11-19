#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""About window: the buttons 'Feedback and bug reports' and 'Third-party licences' (run with a fake xdg-open in PATH that logs its argument)."""
import time
from xdrive import *
time.sleep(3.0)
focus_app_window()
key("s"); time.sleep(0.3)
move(108, 790); click(); time.sleep(0.8)      # the About button (i) at the bottom of the left column
shot("about0")
move(699, 292); click(); time.sleep(1.0)      # Third-party licences
move(543, 292); click(); time.sleep(1.0)      # Feedback and bug reports
