#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Every STL file of the directory ~/rolidium-private/stl-test (or $ROLIDIUM_STL_TEST_DIR) is dropped into the window, imported, its walls and holes are found, the document is
saved and the size of the .rolidium file is compared with the STL. The files are somebody else's: they are not copied, and the screenshots and the table name them by number only.
Without the directory the test says so and passes. A 3MF file is imported the same way (every object a mesh); a file of another kind is refused with a sentence."""
# FIXTURE: box 20 20 20
# ENV: ROLIDIUM_DROP_POLL=1
from guitest import *

d = os.environ.get("ROLIDIUM_STL_TEST_DIR", os.path.expanduser("~/rolidium-private/stl-test"))
# the program runs with another HOME: the real one is in the variable of the harness
if not os.path.isdir(d):
    real = os.environ.get("ROLIDIUM_REAL_HOME")
    if real and os.path.isdir(os.path.join(real, "rolidium-private/stl-test")):
        d = os.path.join(real, "rolidium-private/stl-test")
if not os.path.isdir(d):
    print("PERF the directory ~/rolidium-private/stl-test does not exist: nothing was tried")
    sys.exit(0)
time.sleep(1.0)
focus_app_window()
p = settle()
model = os.environ["ROLIDIUM_MODEL"]
rows = []
files = sorted(os.listdir(d))
for i, name in enumerate(files, 1):
    path = os.path.join(d, name)
    ext = os.path.splitext(name)[1].lower()
    row = {"n": i, "ext": ext, "file_mb": round(os.path.getsize(path) / 1e6, 2)}
    if ext == ".3mf":
        # a 3MF file: every object of it is a mesh (the table says how many and how long it took)
        t0 = time.time()
        drop_files(path)
        p = wait_for(lambda q: q["stl_dialog"], f"the dialog for file {i}", 120)
        items = p["stl_dialog"]["items"]
        row["read_s"] = round(time.time() - t0, 2)
        row["objects"] = len(items)
        row["triangles"] = sum(it["triangles"] for it in items)
        row["size_mm"] = [it["size"] for it in items][:3]
        row["check"] = [it["check"] for it in items][:2]
        shot(f"file{i:02d}-dialog")
        n0 = len(p["meshes"])
        click(*steady("stl_as_mesh"))   # the dialog advises a solid for files of flat walls: this test is about the mesh
        click(*steady("stl_import_ok"))
        p = wait_for(lambda q: len(q["meshes"]) == n0 + len(items) and all(m["shown"] for m in q["meshes"]), f"file {i} is shown", 120)
        p = wait_for(lambda q: all(m["walls"] is not None for m in q["meshes"]) and q["mesh_job"] is None, f"the walls of file {i}", 180)
        row["walls"] = [m["walls"] for m in p["meshes"][n0:]]
        check(True, f"file {i} (3MF): {len(items)} object(s), {row['triangles']} triangles")
        shot(f"file{i:02d}-shown")
        rows.append(row)
        for m in list(p["meshes"][n0:]):
            click(*screen(probe(), f"mesh_row_{m['id']}"))
            settle(0.3)
            key("Delete")
            settle(0.4)
        continue
    if ext != ".stl":
        drop_files(path)
        p = wait_for(lambda q: q["status_error"] and "STL" in q["status"], "a file that is not an STL is refused", 20)
        row["note"] = "refused: " + p["status"]
        check(True, f"file {i} ({ext}): {p['status']}")
        rows.append(row)
        continue
    t0 = time.time()
    drop_files(path)
    p = wait_for(lambda q: q["stl_dialog"], f"the dialog for file {i}", 120)
    row["read_s"] = round(time.time() - t0, 2)
    it = p["stl_dialog"]["items"][0]
    row["triangles"] = it["triangles"]
    row["size_mm"] = it["size"]
    row["check"] = it["check"]
    shot(f"file{i:02d}-dialog")
    click(*steady("stl_as_mesh"))   # the dialog advises a solid for files of flat walls: this test is about the mesh
    click(*steady("stl_import_ok"))
    t1 = time.time()
    n0 = len(p["meshes"])
    p = wait_for(lambda q: len(q["meshes"]) == n0 + 1 and q["meshes"][-1]["shown"], f"file {i} is shown", 120)
    row["shown_s"] = round(time.time() - t1, 2)
    p = wait_for(lambda q: q["meshes"][-1]["walls"] is not None and q["mesh_job"] is None, f"the walls of file {i}", 180)
    row["regions_s"] = round(time.time() - t1, 2)
    m = p["meshes"][-1]
    row["walls"], row["holes"] = m["walls"], len(m["holes"] or [])
    shot(f"file{i:02d}-shown")
    check(m["walls"] is not None, f"file {i}: {m['triangles']} triangles, {m['walls']} walls, {row['holes']} holes")
    # a click on the mesh must pick something (the hierarchy of boxes)
    vx, vy, vw, vh = p["viewport"]
    fast_click(vx + vw / 2, vy + vh / 2)
    q = probe()
    row["pick_ms"] = q["pick"]["ms"]
    rows.append(row)
    # the next file starts alone: the mesh is deleted
    click(*screen(p, f"mesh_row_{m['id']}"))
    settle(0.4)
    key("Delete")
    wait_for(lambda q: len(q["meshes"]) == n0, f"file {i} is deleted", 30)
    settle(0.5)
# all of them together, saved: the size of the file against the STL sizes
stl_paths = [os.path.join(d, n) for n in files if n.lower().endswith(".stl")]
drop_files(*stl_paths)
p = wait_for(lambda q: q["stl_dialog"], "the dialog for all the files", 180)
click(*steady("stl_as_mesh"))   # the dialog advises a solid for files of flat walls: this test is about the mesh
click(*steady("stl_import_ok"))
p = wait_for(lambda q: len(q["meshes"]) == len(stl_paths) and all(m["shown"] for m in q["meshes"]), "all the meshes are shown", 240)
p = wait_for(lambda q: all(m["walls"] is not None for m in q["meshes"]) and q["mesh_job"] is None, "all the walls", 600)
shot("all-together")
key("s", ctrl=True)
p = wait_for(lambda q: q["status"].startswith("Saved") or q["status_error"], "saved", 120)
total_stl = sum(os.path.getsize(x) for x in stl_paths)
size = os.path.getsize(model)
print(f"PERF all {len(stl_paths)} meshes in one document: {size / 1e6:.2f} MB (with the picture) against {total_stl / 1e6:.2f} MB of STL: {100 * size / total_stl:.1f} %")
with open(f"{SHOTS}/perf-stl-test.json", "w") as f:
    json.dump({"rows": rows, "all_rolidium_mb": round(size / 1e6, 2), "all_stl_mb": round(total_stl / 1e6, 2)}, f, indent=1)
for r in rows:
    print("PERF", r)
finish()
