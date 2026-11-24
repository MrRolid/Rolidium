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

## In the window

* **Import**: the button **Import STL** in the File block, or drop one or several `.stl` files into the window. A dialog lists the files with the number of triangles, the size in millimetres (it follows the choice of the units: millimetres, inches, or a scale of your own, "1 unit of the file is ... mm"), and the result of the check; a mesh that is not closed or consistent has the buttons **Merge vertices** and **Turn triangles outwards**. **Import** adds the meshes to the document; **Cancel** (or Esc) adds nothing.
* **A mesh in the document** is a body of its own kind (`▦` in the list of bodies with its number of triangles): select it, hide it, rename it (F2), put it into a group, delete it, Ctrl+Z. It is drawn by the same renderer with the flat shading of the triangles, and picked on the processor through a hierarchy of boxes (the same as for solids).
* **Walls and holes** are looked for in the background after the import (a small window shows the progress and a Cancel button; the window keeps drawing and answering). Until they are found the mesh is one piece (a click takes the whole mesh); when they are found, the pointer over a wall or a hole lights it, a click selects it, and the panel next to it shows what it is: for a flat wall the area and the distance to the opposite wall, for a hole or a pin the diameter. A surface that is only flat by chance (a finely divided ball, a scanned or sculpted model) has no walls: a patch is a wall when a crease runs round most of it and it is not smaller than a ten-thousandth of the whole surface.
* **Pull (P)** a flat wall: live preview, a number can be typed (Enter), or the mouse drags; the same as for solids (and also the field "Move by" and the distance to the opposite wall in the panel). The neighbouring walls stretch. A hole or a pin: its diameter in the panel (the field, Enter). A wall that cannot be moved (a neighbour that is not square to it, a mesh that is not closed, a pull through the opposite wall) is refused with the reason on the status line and the mesh does not change.
* **The whole mesh**: Move (M: X, Y, Z, an angle, Copy), Pattern (T) and Mirror (B) make plain copies (no link that follows the original); **Scale** (factors for x, y and z, also uneven) and the check with its repairs are in the Selection panel in the right column. **Sketch (K)** on a flat wall draws in the plane of the wall (with snaps to its outline); what is pulled from the sketch is a new solid.
* **Not possible yet** (the status line says so in one sentence): a round or a chamfer on a mesh, joining a mesh with a solid or cutting it by a solid (a mesh is not a solid; D60 recommends an explicit, cancellable "convert to a solid" for small clean meshes), a mirror image merged with its original, a pattern of a mesh together with a solid.
* **Save and open**: a `.rolidium` file with a mesh is format version 2 (`FILE-FORMAT.md`), the mesh in a compact form; an older program refuses it with a clear sentence. A document with meshes and solids together exports STL as one file (the triangles of the mesh as they are, the solids divided as set in the dialog); with the choice **Only the selected bodies** in the dialog only those.
* In `rolidium cli`: `mesh-to-doc` makes a mesh of the script a body of the document (saved in `.rolidium`, listed by `info`, exported by `export-stl`).

The numbers measured for a mesh of 2 million triangles in the window are in `DECISIONS.md` (D61).
