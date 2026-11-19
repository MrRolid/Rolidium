#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Main window with the Move panel open, to check that the texts of a language fit (ROLIDIUM_ARGS=--demo)."""
import time
from xdrive import *

time.sleep(2.0)
focus_app_window()
key("m"); time.sleep(0.4)
move(640, 420); time.sleep(0.4); click(); time.sleep(0.8)
move(300, 740); time.sleep(0.4)
shot("lang")
