#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Button states for the screenshots: an empty document (most tools unavailable), then a model (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
move(640, 600); time.sleep(0.4)
shot("b-empty")
