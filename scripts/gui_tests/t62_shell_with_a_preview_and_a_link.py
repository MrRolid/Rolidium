#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The Shell tool (O): the body is chosen by a click, a click on the top face opens it, the preview (the hollow body in the place of the original) comes by itself, Enter makes the
shell as a link. A click on the link in the list opens its panel again; a new thickness is calculated again, Ctrl+Z takes the changes back one by one."""
# FIXTURE: box 40 30 20
# POINTS: top=20,15,20;side=0,15,10;low=20,15,0
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
v0 = p["bodies"][0]["volume"]
key("o")
p = settle(0.5)
check(p["tool"] == "Shell", f"O chooses the Shell tool: {p['tool']}")
click(*screen(p, "top"))
p = settle(0.6)
sh = p["extra"]["shell"]
check(sh["body"] == p["bodies"][0]["id"] and sh["open"] == 0, f"a click on the body chooses it: {sh}")
check("shell_thickness" in p["widgets"] and "shell_apply" in p["widgets"], "the panel has the thickness and Apply")
shot("01-body-chosen")
click(*screen(p, "top"))
p = wait_for(lambda q: q["extra"]["shell"]["open"] == 1 and q["extra"]["shell"]["preview"], "the top face is open and the preview is there", 40)
pv = p["extra"]["preview_mesh"]
check(pv is not None and pv["hides"] == p["bodies"][0]["id"], f"the preview takes the place of the body: {pv and pv['hides']}")
shot("02-preview")
key("Return")
p = wait_for(lambda q: len(q["links"]) == 1 and not q["busy"], "the shell is a link", 40)
want = 24000 - 36 * 26 * 18
check(close(p["bodies"][0]["volume"], want, 2.0), f"the walls are 2 mm thick: {p['bodies'][0]['volume']} (expected {want})")
check(p["links"][0]["kind"] == "Shell", f"the link: {p['links'][0]}")
shot("03-shell")
# the link is opened with a click in the list: the panel shows its numbers
lid = p["links"][0]["id"]
click(*steady(f"link_row_{lid}"))
p = settle(0.8)
sh = p["extra"]["shell"]
check(p["tool"] == "Shell" and sh["editing"] == lid and sh["open"] == 1, f"the panel of the link: {sh}")
# a thicker wall: 3 mm
click(*steady("shell_thickness"))
key("a", ctrl=True)
type_text("3")
key("Return")
p = wait_for(lambda q: q["extra"]["shell"]["preview"] and q["extra"]["shell"]["thickness"] == 3.0, "the preview of 3 mm", 40)
click(*steady("shell_apply"))
p = wait_for(lambda q: not q["busy"] and close(q["bodies"][0]["volume"], 24000 - 34 * 24 * 17, 2.0), "3 mm walls", 40)
check(len(p["links"]) == 1, "still one link")
shot("04-thicker")
key("z", ctrl=True)
p = wait_for(lambda q: not q["busy"] and close(q["bodies"][0]["volume"], want, 2.0), "Ctrl+Z gives the 2 mm walls back", 40)
key("z", ctrl=True)
p = wait_for(lambda q: not q["busy"] and close(q["bodies"][0]["volume"], v0, 1e-3), "and the solid body", 40)
finish()
