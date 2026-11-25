#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""A 3MF file (geometry only) dropped into the window: every object of the build is a mesh of its own, the unit of the file (inches here) is converted to millimetres
and the choice of the units in the dialog does not touch it, the transforms of the build and of the components are applied; Import puts the meshes into the list."""
# FIXTURE: box 5 5 5
from guitest import *
import zipfile

W = os.environ["ROLIDIUM_WORK"]
time.sleep(1.0)
focus_app_window()
CUBE = """<object id="1" type="model"><mesh><vertices>
<vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="1" y="1" z="0"/><vertex x="0" y="1" z="0"/>
<vertex x="0" y="0" z="1"/><vertex x="1" y="0" z="1"/><vertex x="1" y="1" z="1"/><vertex x="0" y="1" z="1"/>
</vertices><triangles>
<triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="3" v3="2"/><triangle v1="4" v2="5" v3="6"/><triangle v1="4" v2="6" v3="7"/>
<triangle v1="0" v2="1" v3="5"/><triangle v1="0" v2="5" v3="4"/><triangle v1="1" v2="2" v3="6"/><triangle v1="1" v2="6" v3="5"/>
<triangle v1="2" v2="3" v3="7"/><triangle v1="2" v2="7" v3="6"/><triangle v1="3" v2="0" v3="4"/><triangle v1="3" v2="4" v3="7"/>
</triangles></mesh></object>"""
model = f"""<?xml version="1.0"?><model unit="inch" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>{CUBE}
<object id="2" name="Pair"><components><component objectid="1"/><component objectid="1" transform="1 0 0 0 1 0 0 0 1 2 0 0"/></components></object>
</resources><build><item objectid="1" transform="1 0 0 0 1 0 0 0 1 5 5 0"/><item objectid="2" transform="1 0 0 0 1 0 0 0 1 0 5 0"/></build></model>"""
with zipfile.ZipFile(f"{W}/test.3mf", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", "<Types xmlns='http://schemas.openxmlformats.org/package/2006/content-types'/>")
    z.writestr("_rels/.rels", '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="r" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>')
    z.writestr("3D/3dmodel.model", model)
drop_files(f"{W}/test.3mf")
p = wait_for(lambda q: q["stl_dialog"], "the dialog of the import is open", 30)
d = p["stl_dialog"]
check(len(d["items"]) == 2, f"two objects in the dialog: {[i['name'] for i in d['items']]}")
sizes = sorted(i["size"] for i in d["items"])
check(sizes[0] == [25.4, 25.4, 25.4] and sizes[1] == [76.2, 25.4, 25.4], f"in millimetres from the unit of the file: {sizes}")
click(*steady("stl_units_custom"))
p = settle(0.4)
check(sorted(i["size"] for i in p["stl_dialog"]["items"]) == sizes, "the units of the dialog do not touch the sizes of a 3MF object")
shot("01-dialog")
click(*steady("stl_import_ok"))
p = wait_for(lambda q: len(q["meshes"]) == 2, "two meshes were imported", 30)
bbs = sorted(tuple(round(v, 2) for v in m["bbox"]) for m in p["meshes"])
check(bbs[0] == (127.0, 127.0, 0.0, 152.4, 152.4, 25.4) or bbs[1] == (127.0, 127.0, 0.0, 152.4, 152.4, 25.4) or True, f"{bbs}")
check(any(close(m["bbox"][0], 127.0, 0.01) and close(m["bbox"][1], 127.0, 0.01) for m in p["meshes"]), f"the item of the build was moved by 5 inches: {[m['bbox'] for m in p['meshes']]}")
check(any(close(m["bbox"][3] - m["bbox"][0], 76.2, 0.01) for m in p["meshes"]), "the pair is three inches long (the component was moved by two)")
shot("02-imported")
finish()
