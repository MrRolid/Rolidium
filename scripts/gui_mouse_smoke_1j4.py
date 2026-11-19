#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The STL export dialog with the folder browser (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(26, 664); click(); time.sleep(1.2)       # Export STL icon (last row of the File section)
shot("e0-stl")
