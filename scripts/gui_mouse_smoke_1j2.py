#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The file dialog with the real mouse and keyboard (ROLIDIUM_ARGS=--empty, FD_DOCS=a folder with part1..3.rolidium)."""
import os, time
from xdrive import *

time.sleep(1.5)
focus_app_window()
key("o", ctrl=True); time.sleep(1.2)
key("l", ctrl=True); time.sleep(0.4)                      # Ctrl+L: type a path
type_text(os.environ["FD_DOCS"]); time.sleep(0.4)
shot("f1-path-typed")
key("Return"); time.sleep(1.0)
shot("f2-docs")
move(822, 218); click(); time.sleep(0.5)                  # header "Size"
move(822, 218); click(); time.sleep(0.5)
shot("f3-sorted-size-desc")
key("Down"); key("Down"); time.sleep(0.4)
shot("f4-arrow-selection")
key("BackSpace"); time.sleep(0.8)                          # one level up
shot("f5-up")
key("Escape"); time.sleep(0.6)
shot("f6-closed")
key("s", ctrl=True, shift=True); time.sleep(1.0)            # Save As: the dialog remembers the last folder
shot("g0-saveas")
type_text("part1"); key("Return"); time.sleep(0.8)
shot("g1-overwrite-question")
key("Escape"); time.sleep(0.5)
type_text("new"); time.sleep(0.2)
shot("g2-new-name")
