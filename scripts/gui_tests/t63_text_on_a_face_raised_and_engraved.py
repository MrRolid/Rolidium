#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""The Text tool (G): a click on the top face puts the text there (its outlines are drawn on the face), the words are typed in the panel, Apply raises the letters 1 mm (a link),
a click on the link in the list opens the panel again, the words are changed and the letters are made again; Engraved cuts them into the face instead."""
# FIXTURE: box 60 30 10
# POINTS: spot=8,10,10
from guitest import *

time.sleep(1.0)
focus_app_window()
p = settle()
v0 = p["bodies"][0]["volume"]
key("g")
p = settle(0.5)
check(p["tool"] == "Text", f"G chooses the Text tool: {p['tool']}")
click(*screen(p, "spot"))
p = settle(0.8)
tx = p["extra"]["text"]
check(tx["target"] is not None and tx["outline"] > 20, f"the text is put there and its outlines are drawn: {tx}")
for w in ("text_field", "text_height", "text_apply", "text_raised", "text_engraved", "text_depth"):
    check(w in p["widgets"], f"the panel has {w}")
shot("01-placed")
# the words
click(*steady("text_field"))
key("a", ctrl=True)
type_text("rolidium")
p = settle(0.6)
check(p["extra"]["text"]["text"] == "rolidium", f"the words: {p['extra']['text']['text']!r}")
n8 = p["extra"]["text"]["outline"]
shot("02-typed")
click(*steady("text_apply"))
p = wait_for(lambda q: len(q["links"]) == 1 and not q["busy"], "the text is a link", 60)
check(p["links"][0]["kind"] == "Text", f"the link: {p['links'][0]}")
v1 = p["bodies"][0]["volume"]
check(v1 > v0 + 10, f"the letters add material: {v0} -> {v1}")
check(close(p["bodies"][0]["bbox"][5], 11.0, 0.01), f"the letters stand 1 mm high: {p['bodies'][0]['bbox']}")
shot("03-raised")
# the words are changed through the link
lid = p["links"][0]["id"]
click(*steady(f"link_row_{lid}"))
p = settle(0.8)
check(p["tool"] == "Text" and p["extra"]["text"]["editing"] == lid and p["extra"]["text"]["text"] == "rolidium", f"the panel of the link: {p['extra']['text']}")
click(*steady("text_field"))
key("a", ctrl=True)
type_text("Roli")
click(*steady("text_engraved"))
click(*steady("text_depth"))
key("a", ctrl=True)
type_text("0.6")
key("Return")
p = settle(0.5)
click(*steady("text_apply"))
p = wait_for(lambda q: not q["busy"] and q["bodies"][0]["bbox"][5] < 10.01, "engraved: the top is flat again", 60)
check(p["bodies"][0]["volume"] < v0 - 1, f"engraved letters take material away: {p['bodies'][0]['volume']} (the plain box {v0})")
check(len(p["links"]) == 1, "still one link")
shot("04-engraved")
key("z", ctrl=True)
p = wait_for(lambda q: not q["busy"] and q["bodies"][0]["bbox"][5] > 10.9, "Ctrl+Z gives the raised letters back", 60)
finish()
