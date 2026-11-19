#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Part 1 of the file smoke test (driven by scripts/gui-files-smoke.sh): save with Ctrl+S, change the model,
wait for the autosave. The shell script then kills the program and starts it again to check the recovery."""
import os, subprocess, time
from xdrive import *

def title():
    out = subprocess.run(["xwininfo", "-root", "-tree"], capture_output=True, text=True).stdout
    for l in out.splitlines():
        if "Rolidium" in l and '"' in l:
            return l.split('"')[1]
    return "?"

def main():
    time.sleep(1.0)
    focus_app_window()
    print("title at start:", title())
    key("s", ctrl=True); time.sleep(1.0)
    shot("f1-save-dialog")
    type_text("model"); key("Return"); time.sleep(2.0)
    print("title after save:", title())
    shot("f2-saved")
    move(600, 300); time.sleep(0.4); click(); key("p"); type_text("7"); key("Return"); time.sleep(1.5)
    print("title after change:", title())
    time.sleep(4.0)                      # autosave (ROLIDIUM_AUTOSAVE_SECS=2)
    print("autosave exists:", os.path.exists(os.environ["ROLIDIUM_AUTOSAVE_DIR"] + "/autosave.rolidium"))
    shot("f3-changed")


if __name__ == "__main__":
    main()
