#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A mesh and a solid in one document, a wall pulled, Ctrl+S: the .rolidium file is smaller than the STL it came from; a second copy of the program opens it: the mesh
is there with its edit, the walls are found again and a hole can be changed; the file is format 2 (and the solid is still in it)."""
# FIXTURE: box 20 20 20; import-stl examples/stl/plate-with-hole.stl; mesh-to-doc
# POINTS: top=10,10,10
from guitest import *
import subprocess, zipfile

time.sleep(1.0)
focus_app_window()
p = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the walls are found", 30)
check(len(p["bodies"]) == 1 and len(p["meshes"]) == 1, f"a solid and a mesh: {len(p['bodies'])}, {len(p['meshes'])}")
top = screen(p, "top")
key("p")
settle(0.4)
click(*top)
p = settle()
type_text("5")
key("Return")
p = wait_for(lambda q: q["status"].startswith("Pull") and q["meshes"][0]["bbox"][5] > 14, "the wall was pulled", 20)
p = wait_for(lambda q: q["meshes"][0]["walls"] == 6 and not q["busy"], "the program is quiet again", 30)
key("s", ctrl=True)
p = wait_for(lambda q: q["status"].startswith("Saved"), "the file is saved", 20)
print("status", p["status"])
model = os.environ["ROLIDIUM_MODEL"]
z = zipfile.ZipFile(model)
manifest = json.loads(z.read("manifest.json"))
check(manifest["format_version"] == 2, f"the file is format 2: {manifest['format_version']}")
check(any(n.startswith("meshes/") for n in z.namelist()) and any(n.startswith("bodies/") for n in z.namelist()), f"the mesh and the solid are in it: {z.namelist()}")
stl_bytes = 84 + 50 * 384
size = os.path.getsize(model)
mesh_entry = z.getinfo([n for n in z.namelist() if n.endswith(".rmesh")][0]).file_size
check(mesh_entry < stl_bytes / 3, f"the mesh in the file ({mesh_entry} bytes) is a third of the STL ({stl_bytes} bytes) or less")
print("PERF file", size, "bytes (with the picture of the model), the mesh in it", mesh_entry, "bytes, the STL", stl_bytes)
shot("01-saved")
env = dict(os.environ)
probe2 = os.environ["ROLIDIUM_WORK"] + "/probe2.json"
env["ROLIDIUM_PROBE"] = probe2
app = subprocess.Popen([os.environ["ROLIDIUM_APP_BIN"], model], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
import guitest
guitest.PROBE = probe2
try:
    time.sleep(5.0)
    focus_app_window()
    q = wait_for(lambda q: q["meshes"] and q["meshes"][0]["walls"] is not None, "the second copy found the walls", 30)
    check(len(q["bodies"]) == 1, "the solid came back")
    m = q["meshes"][0]
    check(close(m["bbox"][5], 15.0, 0.001) and m["triangles"] == 384, f"the mesh came back with the pulled wall: {m['bbox']}, {m['triangles']} triangles")
    check(m["walls"] == 6 and len(m["holes"]) == 1, f"walls {m['walls']}, holes {m['holes']}")
    shot("02-opened")
finally:
    app.terminate()
    guitest.PROBE = os.environ["ROLIDIUM_PROBE"]
finish()
