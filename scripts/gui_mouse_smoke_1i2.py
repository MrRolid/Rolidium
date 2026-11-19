#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Tangent arc offer with the real mouse (ROLIDIUM_ARGS=--empty)."""
import time
from xdrive import *

time.sleep(1.5)
focus_app_window()
key("k"); time.sleep(0.5); key("Return"); time.sleep(0.8)
key("v"); time.sleep(0.5)
key("l"); time.sleep(0.3)
move(500, 400); time.sleep(0.4); click(); move(600, 400); time.sleep(0.4); click(); time.sleep(0.4)
key("Escape"); time.sleep(0.3)
key("a"); time.sleep(0.3)
move(584, 406); time.sleep(0.5); click()
move(700, 300); time.sleep(0.5); click(); time.sleep(0.4)
move(663, 376); time.sleep(0.8)
shot("t1-tangent-offer")
move(640, 330); time.sleep(0.8)
shot("t2-not-tangent")
move(663, 376); click(); time.sleep(0.6)
shot("t3-tangent-arc-placed")
