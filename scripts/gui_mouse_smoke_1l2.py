#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Helix and Sweep with the real mouse (ROLIDIUM_ARGS=<file with a curve and a ring>)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
move(70, 493); click(); time.sleep(0.5)
shot("s0-opened")
key("w"); time.sleep(0.6)
shot("s1-sweep-tool")
move(576, 332); time.sleep(0.4); move(576, 330); time.sleep(0.6); click(); time.sleep(0.5)
move(900, 408); time.sleep(0.5); click(); time.sleep(0.6)
shot("s2-profile-and-path")
move(786, 187); time.sleep(0.4); click(); time.sleep(2.5)
move(108, 462); click(); time.sleep(0.8)
shot("s3-result")
