# Meshes (STL): a short guide

A mesh is a set of triangles from an STL file (binary or ASCII). It is not converted to a solid: it stays a mesh, and the edits are made on the triangles (see D60 in `DECISIONS.md`).

## In `rolidium cli` and MCP

* `import-stl part.stl [inch | scale k] [weld]`: STL has no units; the numbers are millimetres, or inches, or `scale k` (one unit is k mm). The answer gives the number of triangles, the size (bounding box) and the check.
* `mesh-check`: open edges (holes), edges where the normals disagree, edges of more than two triangles, degenerate triangles, loose pieces, a mesh turned inside out. `mesh-repair` joins duplicate vertices and turns the triangles so that neighbours (and the whole piece) agree about the outside. Holes are not filled.
* `mesh-regions`: the planar walls (`wall #1`, ...) and the round holes and pins (`hole #1`, ...).
* `mesh-pull <wall> <distance>`: the wall moves along its normal and the neighbouring walls stretch; it works when the neighbours are square to the wall (boxes, plates, brackets). Refused with the reason otherwise; the mesh stays as it was. `mesh-hole <hole> <diameter>` changes the diameter of a round hole or pin. `mesh-undo` and `mesh-redo` take back one edit each (only the moved vertices are remembered).
* `mesh-scale sx sy sz` (uneven allowed), `mesh-move`, `mesh-rotate`, `mesh-mirror`, `mesh-pattern`, `export-mesh-stl`, `mesh-save` and `mesh-open` (the compact form: a mesh of a printer is saved in a small part of the size of the STL).
* The commands work on the last mesh, or on `mesh N`.

Examples with their own geometry (CC0) are in `examples/stl/`: `import-stl examples/stl/plate-with-hole.stl`, `mesh-regions`, `mesh-pull 1 5`, `mesh-hole 1 6`.

## Not in the window yet
The window has no import of STL yet (neither the File menu nor dragging a file into the window), and a mesh is not yet a body of the document. See the end of D60.
