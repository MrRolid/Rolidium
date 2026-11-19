#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Scenario 1o: a whole body is highlighted by Move and Combine, smooth edges are faint, a double click takes the body
(ROLIDIUM_ARGS=target/tmp/spring-cyl.rolidium)."""
import time
from xdrive import *

time.sleep(2.5)
focus_app_window()
shot("o0-open")
# Move tool (M): hover over the spring, the whole spring is tinted
key("m"); time.sleep(0.3)
move(640, 330); time.sleep(0.6)
shot("o1-move-hover")
click(); time.sleep(0.4)
move(900, 650); time.sleep(0.4)
shot("o2-move-selected")
key("Escape"); time.sleep(0.2)
# Select tool: double click takes the whole body, the panel shows the body
key("s"); time.sleep(0.2)
move(640, 330); time.sleep(0.3)
double_click(); time.sleep(0.5)
move(900, 650); time.sleep(0.4)
shot("o3-double-click")
