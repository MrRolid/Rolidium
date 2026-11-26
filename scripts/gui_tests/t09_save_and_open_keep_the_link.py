#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Corners from a hole, then Ctrl+S, then a second copy of the program opens the saved file: the link is there and still follows the plate (a pull of the
edge there moves the holes)."""
# FIXTURE: box 100 60 10; sketch xy -1; circle 6 6 8; pull-region 0 12 cut
# POINTS: w0=10.00,6.00,7;w1=9.46,8.00,7;w2=8.00,9.46,7;w3=6.00,10.00,7;w4=4.00,9.46,7;w5=2.54,8.00,7;w6=2.00,6.00,7;w7=2.54,4.00,7;w8=4.00,2.54,7;w9=6.00,2.00,7;w10=8.00,2.54,7;w11=9.46,4.00,7;w12=10.00,6.00,5.5;w13=9.46,8.00,5.5;w14=8.00,9.46,5.5;w15=6.00,10.00,5.5;w16=4.00,9.46,5.5;w17=2.54,8.00,5.5;w18=2.00,6.00,5.5;w19=2.54,4.00,5.5;w20=4.00,2.54,5.5;w21=6.00,2.00,5.5;w22=8.00,2.54,5.5;w23=9.46,4.00,5.5;top=50,30,10;s1=50,0,5;s2=30,0,5;s3=70,0,5;s4=50,0,3;s5=40,0,6;s6=60,0,6;s7=80,0,5
from guitest import *
import subprocess

time.sleep(1.0)
focus_app_window()
p = settle()
key("t")
settle(0.4)
x, y = hover_cylinder(p, [f"w{i}" for i in range(24)])
click(x, y)
p = settle()
click(*screen(p, "top"))
p = settle()
check(p["link_armed"] and p["link_mode"] == "Fill", "the flat face chose the corners")
click(*screen(p, "link_apply"))
p = wait_for(lambda q: len(q["links"]) == 1, "the link is made")
p = settle(1.0)
check(has_hole_at(p, 94, 54), "four corner holes")
key("s", ctrl=True)
time.sleep(2.0)
p = settle()
print("status", p["status"])
shot("01-saved")
# a second copy of the program opens the saved file
env = dict(os.environ)
probe2 = os.environ["ROLIDIUM_WORK"] + "/probe2.json"
env["ROLIDIUM_PROBE"] = probe2
app = subprocess.Popen([os.environ["ROLIDIUM_APP_BIN"], os.environ["ROLIDIUM_MODEL"]], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
import guitest
guitest.PROBE = probe2
try:
    time.sleep(5.0)
    focus_app_window()
    q = settle(1.0)
    check(len(q["links"]) == 1 and q["links"][0]["follows"], f"the link was saved and opened: {q['links']}")
    for x, y in [(6, 6), (94, 6), (6, 54), (94, 54)]:
        check(has_hole_at(q, x, y), f"a hole at ({x}, {y}) after the file was opened")
    shot("02-opened")
    key("p")
    settle(0.4)
    guitest.PROBE = probe2
    x, y = hover_normal(["s1", "s2", "s3", "s4", "s5", "s6", "s7"], (0, -1, 0))
    click()
    q = settle()
    type_text("15")
    key("Return")
    q = settle(1.5)
    check(not q["derived_ask"] and q["links"][0]["error"] is None, f"the opened link follows a pull: {q['links']}")
    for x, y in [(6, -9), (94, -9), (6, 54), (94, 54)]:
        check(has_hole_at(q, x, y), f"a hole at ({x}, {y}) after the pull")
    shot("03-pulled-after-open")
finally:
    app.kill()
finish()
