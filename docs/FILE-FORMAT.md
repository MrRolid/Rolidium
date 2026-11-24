# The `.rolidium` file format (format version 1, and 2 for a file with a mesh)

A `.rolidium` file is a **zip archive** (deflate). All text is UTF-8 JSON, units are millimetres, angles radians.

```
manifest.json               required
bodies/<id>.brep            required per body: OpenCASCADE binary BREP of the solid (without display triangulation; older files may carry it, it is ignored)
bodies/<id>.json            required per body: metadata
sketches/<id>.json          one per sketch
meshes/<id>.rmesh           (format 2) one per triangle mesh (STL): the compact form of the triangles, see "Meshes"
meshes/<id>.json            (format 2) one per mesh: its properties
preview.png                 optional, 320 x 240 isometric picture (only when a GPU adapter was available)
<anything else>             kept as it is (see "Unknown content")
```

## manifest.json

```json
{
  "format_version": 1,          // integer, starts at 1
  "program_version": "0.2.0",   // the program that wrote the file
  "units": "mm",
  "next_id": 7,                 // next free id for bodies and sketches
  "body_counter": 2,            // numbering of new bodies and sketches ("Body 3", "Sketch 2")
  "sketch_counter": 1,
  "bodies":   [ { "id": 1, "brep": "bodies/1.brep", "meta": "bodies/1.json" } ],
  "sketches": [ { "id": 2, "file": "sketches/2.json" } ]
}
```

## bodies/<id>.json

```json
{
  "name": "Plate", "color": [0.72, 0.76, 0.82], "visible": true,   // hidden bodies stay hidden after opening
  "face_count": 8,                       // check: the BREP must have this many faces
  "features": [                          // rounds and chamfers (a hint only: they are recognised from the geometry when the file is opened)
    { "kind": "round", "size": 5.0, "faces": [3] }   // chamfers also have "angle" (degrees, 45 = symmetric) and "flip"   // kind: "round" | "chamfer"; faces: face indices in the BREP
  ]
}
```

`name` is the name of the body: free text chosen by the user (1 to 100 characters), not translated, and it may repeat. The program keeps it through
operations (pulls, rounds, cuts; a merged body keeps the name of its target, a copy gets the same name) and writes it into STEP files as the name
of the solid (and reads it back). A file without a `name` (written before the names could be chosen) is opened with the default names "Body N".

Face indices are positions in the face order of the BREP as OpenCASCADE reads it (the writer/reader keep that order; the check
`face_count` guards it). If the check fails, or an index is out of range, the features are dropped and the user is told; the
geometry itself is never affected.

## sketches/<id>.json

```json
{
  "name": "Sketch 1",
  "plane": { "origin": [0,0,20], "normal": [0,0,1], "x_dir": [1,0,0] },
  "target_body": 1,                     // body the sketch was drawn on, or null (base plane)
  "fill": true,                         // optional: made by Fill; pulling its region adds material in either direction
  "visible": true,                      // optional, default true
  "entities": [
    { "type": "line",   "a": [x,y], "b": [x,y] },
    { "type": "rect",   "a": [x,y], "b": [x,y] },
    { "type": "circle", "center": [x,y], "radius": r },
    { "type": "axis",   "a": [x,y], "b": [x,y] },      // an auxiliary axis (added after 0.2.1): the line through a and b, no region, no profile
    { "type": "arc", "center": [x,y], "radius": r, "start": a0, "sweep": da }   // radians, sweep > 0 counter-clockwise
  ]
}
```

## Unknown content

* **Unknown zip entries** (for example `cam/...`, `laser/...`) are kept in memory and written back unchanged.
* **Unknown fields** in `manifest.json`, in body JSON and in sketch JSON are kept and written back.
* A sketch shape whose `type` is not known is kept (as `entities_unknown`) and written back into `entities`.

A program without a module therefore never destroys that module's data. Future modules store their data under their own
directory (`cam/`, ...) and may add fields.

## Versions

* The undo history is **not** saved.
* A file with a **newer** `format_version` than the program knows is refused with a clear message and is not touched. (A program of the versions up to 0.2.2 says: "This file was saved by a newer Rolidium (file format 2; this program understands up to 1). It was not opened and not changed. Please update Rolidium." This was tried with 0.2.1: the file is not opened and not changed.)
* **Older** versions are read by `migrate` (in `crates/rolidium-doc/src/file.rs`): one step per version, applied before the file is
  interpreted. Version 1 is the first, so there is nothing to migrate yet.
* Changing the meaning of existing fields needs a new `format_version`; adding fields does not.

## Writing

The file is written to a temporary file next to the target, flushed, the previous file is renamed to `<name>.bak`, and the temporary
file is renamed to the target name. If the program dies before the last step, the old file is intact. Autosave uses the same format
(in the user's data directory, `autosave.rolidium` and `autosave.json`) and is removed on a clean exit.

## Material of a body (added in program version 0.1.11, still format version 1)

`bodies/<id>.json` may contain `"material": {"name": "PLA", "density": 1.24}` (density in g/cm³; `name` is one of `steel`, `aluminium`, `brass`,
copper, `PLA`, `PETG`, `ABS` or `custom`). The mass shown in the Selection panel is the volume times this density. A body without the key has the
default material (steel, 7.85 g/cm³); the program writes the key only when the material was changed, so a file from 0.1.10 opens and saves unchanged.

## Curves (added in program version 0.1.10, still format version 1)

Curves (3D lines and helices) are stored next to the sketches. The addition is purely additive, so the format version stays 1:

* `manifest.json` gets `"curve_counter": <n>` and, when the document has curves, `"curves": [{"id": <id>, "file": "curves/<id>.json"}, ...]`.
  A document without curves has neither key, exactly as a file written by 0.1.9 or earlier.
* `curves/<id>.json`: `name`, `visible`, and `type`:
  * `"polyline"`: `points` (a list of `[x, y, z]`, mm) and `radius` (mm; above 0 every corner is rounded by a tangent arc of that radius).
  * `"helix"`: `origin`, `axis`, `x_dir` (3 numbers each), `radius`, `pitch`, `turns`, `right` (true: right-handed), `start_deg`, `cone_deg`
    (half angle of the cone the helix lies on, 0 for a cylinder).
* A file from 0.1.9 opens unchanged. A program that does not know curves keeps the `curves` key and the `curves/` entries as unknown content
  (see "unknown content" above) and writes them back; a curve of a type this program does not know is left in the file as it is.
* Ids of curves come from the same counter as the ids of bodies and sketches (`next_id`).


## STL export

STL has no units. Rolidium always writes it in **millimetres** (the units of the document), binary or ASCII; slicers read it as millimetres. A test
(`crates/rolidium-cli/tests/stl_scale.rs`) builds a cube of 20 mm, exports it, reads the file back and checks that its bounding box is 20 x 20 x 20 mm to 0.001 mm.

## Links and groups (added in program version 0.2.1, still format version 1)

A *link* is a pattern or a live mirror: an object of the document that remembers its source bodies, its parameters and the bodies it made. A *group* is a
list of bodies in the right column. Both are additive: a file without them opens in every version, and a program that does not know them keeps the entries as they are (see "Unknown content").

Manifest keys (all optional): `link_counter`, `group_counter` (numbering of new "Linear pattern 3", "Group 2"), and

```json
"links":  [ { "id": 9, "file": "links/9.json", "base": "links/9.base.brep" } ],
"groups": [ { "id": 3, "name": "Frame", "parent": null, "visible": true, "bodies": [1, 2, 4] } ]
```

`links/<id>.json`:

```json
{
  "type": "linear",            // "linear", "circular", "mirror", "loft" or "sweep"; a link of another type is kept without being used
  "name": "Linear pattern 1",
  "sources": [1],              // ids of the bodies the pieces are made of
  "results": [5, 6, 7],        // ids of the bodies the link made (the copies, or the one target body of "add" and "cut")
  "error": null,               // why the last calculation failed (the bodies then keep the last good result)
  "mode": "new",               // linear and circular: "new", "add" or "cut"
  "target": null,              // linear and circular: the body of "add" and "cut"
  "axis": { "origin": [0,0,0], "dir": [1,0,0] },   // linear: the direction; circular: the axis
  "count": 4, "spacing": 20.0, "by_total": false, // linear: the spacing is the distance of two pieces, or the whole length
  "second": null,              // linear: { "axis", "count", "spacing", "by_total" } of the second direction of a grid
  "angle_deg": 60.0, "full": true,                // circular: angle between pieces, or the whole circle divided evenly
  "origin": [0,0,0], "normal": [1,0,0],           // mirror: the plane
  "keep_orientation": false,   // circular: the copies are carried round the axis without turning
  "fill": { ... }              // optional (added after 0.2.1): the pattern follows a rectangle, see below
}
```

`fill` (a pattern of a hole or a pin into the corners or a grid of a rectangle): `"rect"` is `{"face": {"origin", "normal", "near"}}` (a flat rectangular face of the target body, found again by its plane and the point `near`) or `{"sketch": {"id", "entity"}}` (a rectangle of a sketch); `"how"` is `{"corners": true}` or `{"columns", "rows", "margin"}`; `sign_a`, `near_a`, `sign_b`, `near_b` are the sides and the distances of the original to the nearest edges along the two directions of the rectangle (the directions are the two sides, the one with the smaller dominant axis first), `at` the point of the source on its plane. When the target body or the rectangle changes, the source is placed again at those distances and the pieces are calculated again from `links/<id>.base.brep`. A program that does not know `fill` opens the link as a plain row or grid.

A loft and a sweep (`"type": "loft"` and `"sweep"`) have `"mode"` and `"target"` as above (a new body, or added to / cut from the target) and

```json
{ "type": "loft",  "profiles": [ {"kind": "region", "sketch": 3, "region": 0}, {"kind": "face", "body": 2, "face": 5},
                                 {"kind": "edge_loop", "body": 2, "edge": 7} ], "ruled": false }
{ "type": "sweep", "profile": {"kind": "region", "sketch": 4, "region": 0}, "rounded": false,
  "path": {"kind": "curve", "curve": 8}            // or {"kind": "sketch", "sketch": 5, "entities": [1,2]} or {"kind": "edges", "body": 2, "edges": [4,5]}
}
```

`sources` lists the bodies they name (not the target); the sketches and curves are the ones of the file with those ids. A link whose sketch, curve or body is not in the file is dropped (its body stays as an ordinary body). The sketches the body was made from stay in the file with `visible: false`.

`links/<id>.base.brep` (only for "add" and "cut"): the solid of the target body before the pattern; every recalculation starts from it. Units are millimetres, angles degrees.
Opening a file recalculates nothing: the bodies are stored as they were, and the link recalculates when its source changes.

## Sketches on the end of a curve (added in program version 0.2.1, still format version 1)

A sketch may have `"anchor": { "curve": 8, "end": "start" | "end", "plane": "axis_plane" | "normal_to_tangent" | "normal_to_axis" | "parallel_xy" | "parallel_xz" | "parallel_yz" | "free", "normal": [..], "x_dir": [..] }` in `sketches/<id>.json` (`normal` and `x_dir` only for `free`). The plane of the sketch is calculated from the end of the curve whenever the curve changes, so the `plane` stored in the file is the last result. An anchor whose curve is not in the file is dropped when the file is opened (the sketch stays where it was). A program that does not know the key keeps it unchanged.


## Meshes (format version 2, added in program version 0.3.0)

A document that has a triangle mesh (an imported STL file) is written as `"format_version": 2`; a document without a mesh is still written as version 1, so that a program of the earlier versions opens it. The manifest has

```json
"meshes": [ { "id": 5, "mesh": "meshes/5.rmesh", "meta": "meshes/5.json" } ]
```

The ids are of the same series as the ids of the bodies (a group lists a mesh in its `"bodies"` like a body). `meshes/<id>.json`:

```json
{ "name": "bracket", "color": [0.8, 0.75, 0.66], "visible": true, "verified": true, "triangles": 2000000, "vertices": 1000000 }
```

`verified` says that the mesh was checked and is closed and consistent (the edits of a wall or a hole then need no new check). Unknown fields are kept like those of a body. `meshes/<id>.rmesh` is the welded indexed mesh in a compact form (stored in the zip as it is, it is already deflated): the 5 bytes `RMSH1`, then a zlib stream of: the number of vertices (u32 LE), the number of triangles (u32 LE), the coordinates (3 f32 per vertex, taken as u32 bit patterns) as four **byte planes** (all the first bytes, then all the second bytes, and so on), and the vertex indices of the triangles as differences to the previous index (zigzag coded) in four byte planes. A mesh from a printer is mostly neighbouring numbers, so the planes pack well: a sphere of 2 million triangles is 3 MB (the binary STL is 99 MB); a mesh of scattered numbers is bigger. The regions (walls and holes) are not stored: they are found again, in the background, when the file is opened.
