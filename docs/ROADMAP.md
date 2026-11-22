# Roadmap

What is planned, in rough order and without dates. What is already done is in the [changelog](../CHANGELOG.md); decisions and their
reasons are in [DECISIONS.md](DECISIONS.md).

## Next

The theme of version 0.3.0: **Rolidium for 3D printing.** In rough order:

1. **Repeat along a line and around a circle, and groups of bodies.** A pattern of a body or a feature (holes, screws, carriages) along a line or
   around a circle with a count and a distance or an angle, in the window and in the command-line and MCP modes. Groups of bodies that move, rotate
   about a chosen point, copy and hide together, and that can be listed in the document tree. Today the demos copy every body one by one and rotate only about the
   centre of a body (found while building the demo models, see `examples/demos/README.md`).
   Also **mirroring**:
   * Mirror a body by a base plane, the plane of a sketch or a chosen flat face, with a choice to keep the original and to merge the mirror image with it.
   * Mirror in a sketch by a line.
   * *Live mirror*: the mirror remembers the plane and the pair of bodies, and an edit of one side is carried over to the other. The principle is the same as for the joints with a screw (D47), without a history of the
     model. When the carrying over does not work, a message says why and offers to remove the mirror.
2. **Checks before printing.** Walls that are too thin for the nozzle diameter, overhangs, and a recommended orientation of the part.
3. **Printer profile and fasteners.** Nozzle, clearance and tolerance. A test part for measuring the clearance of the printer. Holes and threads M3 to M12 with clearance for screws and nuts, and pockets for nuts, as fasteners:
   * A library of standard screws and nuts (M3 to M12); a screw is placed into a body along its axis.
   * A screw and its hole make a *joint* that remembers its parameters: the size, the length, the type of the hole and the clearance. The joint is listed among the bodies, and its parameters can be changed after
     it was made; the hole is recalculated. A change of the printer profile recalculates all joints of the document.
   * Types of the hole: a modelled thread with clearance, a hole for a self-tapping screw, a hole for a heat-set insert, a pocket for a nut with a through hole, and a through hole with a countersink.
   * Recalculation: when a joint is made, the removed material is stored; when the joint changes, it is given back and the new hole is cut. If the body was changed by hand at the place of the hole in the meantime and the
     recalculation does not work, a message says why and offers to remove the joint and make it again.
4. **Printing through slicers.** Rolidium does not get a slicer of its own (D43).
   * *Export 3MF*: with units, several bodies at once, their names, and later colours.
   * *Open in a slicer*: the program finds an installed PrusaSlicer, OrcaSlicer, Cura or Bambu Studio and hands the model over as a 3MF placed on the bed, and reads the print time
     and the filament use back from the slicer. An estimate of the mass and of the print time in the status bar.
   * *Print from Rolidium*: the slicer is called from the command line with the user's profile, and the G-code is sent to the printer through OctoPrint, Moonraker or PrusaLink.
     Before printing there is a confirmation with a preview and a warning not to leave the printer unattended. Bambu Lab printers only through the step of opening in a slicer.
   * Slicers run as separate programs; their code is not taken into Rolidium.
5. **Templates.** An electronics box, Gridfinity (boxes and a baseplate for the size of a drawer), tube elbows and couplings, hinges, clips.
6. **STL import and editing.** Select a part of a mesh with a brush, delete it, separate it, fill holes, and turn simple shapes (a cylinder, a box, a cone) into a real solid. The plan for recognising faces is in
   the design notes below (*STL import*).
7. **A check of the STL scale.** STL has no units and is always taken as millimetres; an automatic test exports a cube of 20 mm and checks that it is 20 mm.

## Then

* **Exact helix geometry and a Thread tool.** Bodies swept along a helix (springs, threads) are built today by a general sweep, and
  combining them with a cylinder can be refused by the volume check in some cases (typically when a spring sticks out through the end
  faces of the cylinder, see D30). The plan is to build springs and threads from exact surfaces so that they combine reliably and the
  display mesh is fast, and to keep rounds and chamfers in the history after a fallback combine. Then a Thread tool: metric ISO threads
  (M3 to M24) on a cylindrical face, external and internal, with clearance for 3D printing, modelled and cosmetic. The test matrix is
  `crates/rolidium-kernel/tests/helix_matrix.rs` (ignored by default, run it with `--ignored`).
* **Permanent dimensions.** Today measurements are temporary (the Measure tool). Permanent dimensions are built from the same elements
  (`MElem` in `app/measuretool.rs`), stored in the document and shown in the 3D view. See the design notes below.
* **Units, inches included.** Lengths shown and typed in the chosen unit; the file stays in millimetres.
* **Text and a logo on a body.** Letters engraved into or embossed on a flat face, also a logo from an SVG file.
* **Sketch: corner fillet and trim.** Round the corner between two sketch lines with an arc of a given radius, and trim a line at another
  line or shape. General sketch tools, not only for sweep paths.
* **Pulling cylindrical and conical faces, a draft of a face by an angle, and removing a face with healing.** Pull works on flat faces today; on a curved face it says
  that it works only on flat ones. The next step is to pull a cylindrical or conical face, which changes the diameter of a hole or a shaft
  (the radius grows or shrinks, a cone keeps its angle), to tilt a face by a given angle (draft), so that walls can be made sloping
  for casting and printing, and to remove a face and let the neighbours close the gap. All of it reuses the dimension panel that already changes the diameter of a hole.
* **A generator of gears.**
* **An AI panel in the window**, with the user's own key for the API of an AI service: no server of ours and no credits. The MCP server stays for advanced use.
  A later step lets it drive the document of a running window (a local socket, the same table of commands), so the user watches the model change live and can undo
  what the assistant did. Needs the document to be shared with the window thread and a rule for who may change it at a given moment.
* **More of the kernel on several cores.** Meshing and reading a file use several threads, but the facts about the faces (one native call per face) and the validity check of each body still
  wait for one lock of the kernel, which is about 60 % of opening the router (D40, `docs/PERFORMANCE.md`). The next step is to find out which of those calls are safe to run for different
  bodies at the same time and to take the lock off them, and to run the checks of touching and overlapping bodies on several threads.
* **Distribution.** The Windows program through the Microsoft Store (an MSIX package; the signing and the updates are done by the Store, see D46), later a signed Windows zip, Linux packages from a repository and a
  Flatpak, and a build for macOS. Today there are the zip, the tar.gz, the `.deb` and the `.AppImage`.

## Later

* **A paid part for custom printing**: a price from the print time and the filament, an offer as a PDF, a record of the filament.
* **Assemblies with joints**, a check of collisions and the range of movement.
* **A puzzle made from a model.**
* **2D sections and laser output** (DXF, SVG).
* **Drawings**: 2D views and sections with dimensions, exported to PDF, SVG and DXF.
* **CAM for CNC machines**: 2.5D toolpaths first, then 3D (see the CAM plan below). The program has no CAM today.

## Smaller improvements

* Faster pull and validity check on parts with about 200 faces (about 110 ms now, see D29): check only the faces that changed and unify
  only around the new geometry.
* A true face offset when pulling a planar face: sloping neighbours are not extended today (D3).
* Highlight the edges of the hovered or selected face.
* Share vertices on face boundaries in the display mesh to remove one-pixel cracks in the ID buffer.
* Zoom to selection; a precision modifier for dragging (the distance resolution is the pixel size at the current zoom).
* Grid: a user-set reach (far plane) as a setting.
* When pulling a face up to the plane of a neighbouring face, a choice between merging the faces (today's behaviour) and keeping the
  outline as an edge for further work.
* Blend features are edited as a whole; per-face editing and variable radii are missing. Rounds and chamfers cannot be limited to the
  selected edge, because OCCT follows edges that continue tangentially (D13).
* Cancelling: file opening, saving and the validity check of a result are not interruptible (D27).
* Mirror the prebuilt OCCT archive under the project's own release, or build it in CI (it is hosted on a third-party release now).
* `egui_kittest` UI tests once its API is checked against egui 0.37 (the UI smoke test drives the real window instead).
* A native audio back end for the error tone if the command-line players turn out to be unreliable.

## Design notes

**Permanent dimensions.**
* A permanent dimension must survive editing. Faces and edges are identified by index plus the kernel history today; dimensions need a
  stable reference (a history link, with the picked point as a fallback) and a visible "reference lost" state when the geometry disappears.
* Add an `annotations` collection to the document and the file format (the preservation of unknown content helps), a precision setting
  and screen-space labels that stay readable. The same dimensions feed the drawings.
* Annotations must carry 3D points and directions, not only screen positions, so that they can be projected into drawings.

**Units.**
* One place that converts (a `Units` setting with a factor and a label, used by every field, tooltip, status bar text, dimension label and
  export dialog), parsing of typed values with an optional unit suffix ("2in", "5 mm"), and rounding that suits the unit. Angles do not change.
* Never format a length with a hard-coded "mm" in new code; keep every stored value in millimetres, so a model looks the same whatever unit
  its author chose. Grid steps need unit-aware presets (1/16 in, 1/8 in, 1 in). The CLI keeps millimetres unless told otherwise.

**Text on a body.**
* Load a font and read the glyph outlines (a TrueType parser crate, MIT/Apache licence), turn the Bezier outlines into closed sketch loops,
  then reuse region detection (it handles holes, so the inside of an "O" works) and Pull in or out.
* The sketch knows only lines, rectangles, circles and arcs. Prepare a curve entity (a Bezier or a polyline with a tolerance) in the sketch
  crate, and keep the text itself (string, font, size, position) as a feature of the body next to rounds and chamfers (`Body::features`), so
  it can be edited later.
* Start with flat faces; wrapping text on a cylinder needs a projection onto the surface.

**Exact helix.** The helix is a 2D line on a cylindrical or conical surface. A circular profile swept along it should be built as exact
torus-like and cylindrical surfaces instead of a general pipe shell. Cutting the path into pieces of at most half a turn (D30) stays as
the fallback.

**STL import.** A mesh is split into regions by normal and curvature (region growing), planes, cylinders, cones and spheres are fitted,
the faces are sewn into a shell and made a solid (`BRepBuilderAPI_Sewing`). Heuristic and the hardest item: expect a tolerance panel and a
report of what could not be recognised.

**2D sections and laser.** Section a body with a plane, take the outline wires, offset them for the kerf and write DXF or SVG.
`rolidium-path2d` already holds 2D paths; `rolidium-cam` is the intended home.

**Drawings.** Projection with hidden-line removal (`HLRBRep_Algo`; check that the toolkit is in the static archive), section views by
cutting the body with a plane first, a sheet object (paper size, frame, title block, scale) that holds views, hidden edges dashed.
Output: SVG is plain text, DXF is a simple writer for lines, arcs and circles, PDF through a small writer crate (MIT/Apache) or an own
minimal writer. The sheet is its own document layer that references bodies; `rolidium-path2d` is the common 2D geometry.

**CAM architecture.** Toolpaths are not part of the model; they belong in a separate document layer that references bodies.

## CAM plan (draft)

Nothing here is done, and it is not decided which parts will be in the open core and which in commercial modules.

**Control systems and formats**
* Supported control systems: only **GRBL**, **LinuxCNC** and **Mach3/Mach4**.
* Other control systems are served through an export of the toolpaths in a neutral format (tool position data in APT notation), which
  other postprocessors can read.
* Toolpaths are computed in a neutral format. Postprocessors are kept separate, as readable text definitions, so that they can be checked
  and adjusted without changing the program.

**Automatic safety checks before export**
* No rapid move inside the material.
* No gouging into the model.
* No collision of the tool holder.
* Every move inside the travel range of the machine.
* Cutting depth checked against the flute length of the tool.
* Every program explicitly sets its units and absolute mode.

**Simulator**
* A simulator that reads the **finished G-code text** (not the internal toolpaths) and shows the material removal.
* The same simulator serves as a test bench for long-running random testing of the toolpath and postprocessor code.
* Its results are cross-checked against the real interpreters of the supported control systems.

**Test program and production program**
* Both are generated from the same toolpaths.
* The test program has limited feed rates and the spindle off; it is meant only for a run without material.
* The program can prove that the two differ only in speeds.

**Program catalogue**
* The G-code, the source model and PDF documents are kept together.
* An automatically generated setup sheet: tools, zero point, stock, workholding.
* A fingerprint of every file and a verification status per concrete machine: generated, passed simulation, dry run done, first part
  measured, released.
* Editing the G-code by hand resets the status to unverified.
* The header of every program carries the Rolidium version and a fingerprint of the settings it was generated with.

## Done

What is finished is listed in the [changelog](../CHANGELOG.md). Items that were on this page before:

* **Notice of a new version** (0.2.0). With the consent of the person (a small card at the first start, a switch in Settings), the program asks rolidium.com at most once a day, in the background, whether a newer
  version exists. The question carries the version of the program, the name and main number of the system and the architecture, and nothing else. The notice offers *Download* (the fixed download page, opened in the
  browser: the program downloads and starts nothing) and *Skip this version*. `rolidium cli`, `rolidium mcp`, `--selftest`, `--bench`, `--diagnose` and `ROLIDIUM_OFFLINE=1` never do it. See [PRIVACY.md](PRIVACY.md).
* **Sending diagnostics** (0.2.0), only when the person asks: they see what is sent (every file with a check box, the picture of the window first), can add a note and an e-mail, and nothing leaves before they confirm.
* **Packages for Linux** (0.2.0): a `.deb`, an `.AppImage` and the tar.gz, made by `scripts/package.sh` together with the Windows zip.
