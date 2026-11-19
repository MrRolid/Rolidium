#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A failing pull plays the error tone, cancelling with Esc does not (a fake `paplay` in PATH records the calls)."""
import time
from xdrive import *

time.sleep(1.0)
focus_app_window()
move(540, 300); time.sleep(0.5); click(); time.sleep(0.5)    # select the top face
key("p"); time.sleep(0.3)
type_text("5"); key("Escape"); time.sleep(1.0)               # cancel: no tone
shot("a1-cancelled")
type_text("-100"); key("Return"); time.sleep(2.0)            # impossible pull: error and tone
shot("a2-error")
