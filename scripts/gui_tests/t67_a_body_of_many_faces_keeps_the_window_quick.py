#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A box with a text of 90 letters (Liberation Sans, 17 mm, engraved 1 mm: some 900 faces; the body of the same text made of short lines had 10 909): the work of the window per frame
stays small while the pointer moves over the faces and edges, a face and an edge are selected and the view is turned (the dearest part of a frame is named when it is not). The window
must never freeze: no frame takes long, and the program answers while it works."""
# FIXTURE: box 1600 60 20; text "Rolidium je program pro modelovani 3D dilu, ktery umi i text. Zkusime delsi vetu 012345678" at 10 20 20 height 17 depth 1 engrave
# POINTS: a=100,20,20;b=400,15,20;c=800,5,10;d=1200,55,20
from guitest import *

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["extra"]["perf"]["faces"] > 300 and not q["busy"], "the model is shown", 400)
p = settle(3.0)
faces = p["extra"]["perf"]["faces"]
print("faces", faces)
check(faces > 300, f"the body has many faces: {faces}")
samples = []

def sample(what):
    q = probe()
    pf = q["extra"]["perf"]
    samples.append((what, pf["window_ms"], pf["pointer_ms"], pf["frame_ms"], pf["parts"] + " | " + pf["worst_kernel"]))

click(*steady("fit_all"))   # Fit all
settle(1.0)
# the pointer over the faces and edges
for i in range(40):
    p = probe()
    x = 260 + (i * 17) % 700
    y = 250 + (i * 11) % 300
    move(x, y)
    time.sleep(0.12)
    sample("hover")
# select an edge and a face
click(*screen(probe(), "a"))
settle(0.5)
sample("select")
# turn the view
f, secs = orbit(4.0)
sample("orbit")
for k in range(10):
    time.sleep(0.2)
    sample("after")
worst = max(samples, key=lambda s: s[1])
med = sorted(s[1] for s in samples)[len(samples) // 2]
pmed = sorted(s[2] for s in samples)[len(samples) // 2]
print("median window", med, "ms; median pointer", pmed, "ms; worst", worst[:3], worst[4][:300])
check(med < 8.0, f"the work of the window per frame stays small (under 8 ms in this debug build on a busy machine; the release build is under 5 ms): median {med} ms")
check(worst[1] < 80.0, f"no frame of the window freezes: the worst is {worst[1]} ms ({worst[4][:200]})")
check(pmed < 5.0, f"the pointer costs under 5 ms: median {pmed} ms")
shot("01-body")
finish()
