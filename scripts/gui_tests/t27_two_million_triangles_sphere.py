#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A sphere of 2 million triangles (a favourable mesh for compression): reading, the display, the search for walls and holes with the progress and Cancel (the
window keeps answering), the frames a second while it is turned, the time of a pick under the pointer. The numbers are in target/gui-shots/perf-sphere.json."""
# FIXTURE: box 20 20 20
# POINTS: top=50,30,10
# ENV: ROLIDIUM_PROBE_IDLE=1 ROLIDIUM_DROP_POLL=1 ROLIDIUM_TEST_REGIONS_DELAY_MS=60
from guitest import *
big_mesh_perf("/tmp/big-sphere.stl", "sphere", cancel=True)
res = json.load(open(f"{SHOTS}/perf-sphere.json"))
check(res["triangles"] > 1_900_000, f"{res['triangles']} triangles")
check(res["progress_samples"] > 3 and res["progress_max"] is not None and res["progress_max"] > 0.1, f"the progress was shown: {res['progress_samples']} samples, up to {res['progress_max']}")
check(res["cancelled"] and res["walls"] is None, "Cancel stopped the search; no walls are shown, the mesh is shown as it was")
check(res["frames_during_search_per_s"] is None or res["frames_during_search_per_s"] > 0.2, f"the window keeps drawing during the search: {res.get('frames_during_search_per_s')} frames a second (software rendering of 2 million triangles, turning: {res['orbit_frames_per_s']})")
finish()
