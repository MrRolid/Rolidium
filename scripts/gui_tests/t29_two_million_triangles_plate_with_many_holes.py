#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A plate of 2 million triangles with 1024 holes (the numbers do not pack as well as those of a ball): reading, display, the search for walls and holes (6 walls, 1024
holes), the frames a second while it is turned, the picks, a pull of the top wall by 1 mm (typed) with Undo. The numbers are in target/gui-shots/perf-holes.json."""
# FIXTURE: box 20 20 20
# POINTS: top=160,160,4
# ENV: ROLIDIUM_PROBE_IDLE=1 ROLIDIUM_DROP_POLL=1
from guitest import *
big_mesh_perf("/tmp/big-holes.stl", "holes", edit=True)
res = json.load(open(f"{SHOTS}/perf-holes.json"))
check(res["triangles"] > 1_900_000, f"{res['triangles']} triangles")
check(res["walls"] == 6 and res["holes"] == 1024, f"6 walls and 1024 holes were found: {res['walls']}, {res['holes']}")
check(res.get("pull_status", "").startswith("Pull"), f"the wall was pulled: {res.get('pull_status')}")
finish()
