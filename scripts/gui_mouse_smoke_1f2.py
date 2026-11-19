#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Round the circular inner edge of a stepped cylinder. ROLIDIUM_ARGS=target/tmp/b/step.rolidium"""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
move(70, 420); click(); time.sleep(0.4)
move(70, 451); click(); time.sleep(0.6)
shot("q1-step")
move(595, 478); time.sleep(0.8)
click(); time.sleep(0.5)
shot("q2-circle-edge-selected")
key("p"); time.sleep(0.3)
type_text("2"); key("Return"); time.sleep(1.5)
shot("q3-circle-rounded")
key("s"); time.sleep(0.3)
move(595, 480); time.sleep(0.5); click(); time.sleep(0.6)
shot("q4-round-panel")
key("a", ctrl=True); type_text("0"); key("Return"); time.sleep(1.5)
shot("q5-removed")
