# Architecture

Cargo workspace; dependency direction is strictly downwards.

| Crate | State | Role |
|---|---|---|
| `rolidium-kernel` | working | Own geometry types over a static Open CASCADE build, through a `cxx` bridge (`src/bridge.cpp`). The only place that touches OCCT. |
| `rolidium-path2d` | started | Contours of line segments and arcs (area, containment). No CAD-kernel dependency. |
| `rolidium-sketch` | working | Sketch entities (line, rectangle, circle, arc), closed-region detection by planar arrangement (nesting, overlaps, touching), sketch planes. |
| `rolidium-mesh` | placeholder | Empty; meant for STL import and B-rep recovery (see the roadmap). |
| `rolidium-doc` | working | Bodies, sketches, curves, commands (pull, revolve, loft, sweep, combine, rounds and more), undo/redo, the file format. Commands are computed off-thread from an immutable snapshot and committed atomically. |
| `rolidium-cam` | placeholder | Empty; meant for 2D sections and toolpaths (see the roadmap). |
| `rolidium-render` | working | (depends on `rolidium-sketch` for drawing sketches) wgpu renderer (offscreen MSAA colour + ID buffer pass), orbit camera, Pull preview geometry, offscreen PNG. |
| `rolidium-app` | working | eframe/egui window, tools, panels, worker thread. Binary `rolidium`, which also hosts the command-line and MCP modes (`rolidium cli`, `rolidium mcp`, `rolidium mcp-config`). |
| `rolidium-cli` | working | Library with the headless tool (scripts, offscreen render, benchmarks) and the MCP server, both driven by one table of commands. Also builds the small `rolidium-cli` and `rolidium-mcp` programs used by the tests and developers (D32). |
| `rolidium-svg2png` | working | Small build-time tool that renders the icon and help SVG files to PNG. |

## Reliability mechanisms

* Every native call goes through `guarded()` in the bridge: global lock, all exceptions turned into `Result` errors.
* Shapes are immutable. A failed operation cannot damage its inputs.
* Every modelling result is checked (`BRepCheck_Analyzer`, exactly one solid) before it is returned.
* The document only changes through `Document::commit`, which rejects results computed from an older revision.
* The UI never calls the kernel for modelling: a worker thread computes the operation and the display meshes;
  panics in the worker are caught and reported, the document stays at its last valid state. The autosave has a thread of its own, so it never keeps an operation waiting.
* The display meshes and the facts about the faces of a body are made by `rolidium_doc::display::MeshCache`, once per kernel shape: an operation, Undo or Redo costs what the changed bodies cost, not what
  the document costs (D39). `rolidium_kernel::par` runs the same work for independent bodies on several threads (D40).
* Errors are logged to `$XDG_STATE_HOME/rolidium/rolidium.log` (default `~/.local/state/rolidium/`).

## Identity of faces

A face is an index into the face list of one solid version (`FaceId`). Operations return a `History`
(`operand, source face, result face`) merged from the boolean and the same-domain unification, so selections can
be carried across operations. The app currently re-selects the pulled face geometrically.

## Rendering

Scene pass: 4x MSAA colour + depth, faces (Lambert + headlight, hover/selection tint by face ID),
edges as lines, infinite grid in the shader, Z axis line, translucent preview. ID pass: same geometry
into an `R32Uint` target; one pixel under the cursor is read back asynchronously (one frame of latency).
The result is resolved to a texture and blitted into the egui render pass by a paint callback.

## Sketches

A sketch lives on a plane (`PlaneDef`: origin, outward normal, X direction) and optionally belongs to a body
(`target`). Region detection splits all curves at their mutual intersections, drops bridges and dangling chains,
walks the faces of the remaining planar graph and turns components inside other faces into holes. A region knows
which entities border it and which border only it (`exclusive_entities`): those disappear when the region is pulled.
Pulling a region extrudes its profile (with holes) along the plane normal; with a target body the prism is fused
(positive distance) or cut (negative), without one it becomes a new body. Sketch edits are synchronous (no kernel);
pulls run on the worker thread like every other kernel operation.

## Edges and the dimension panel

* Kernel: `cylinder_info`, `disc_partner`, `set_cylinder_diameter` (a hole is filled and recut, a pin fused or cut as a tube), ASCII STL.
* Renderer: thick-line layer (edge/shape highlights), face-ID pass only when `Frame::render_ids` is set.
* App: `app/pick.rs` (edge and shape picking in screen space with an occlusion test), `app/dim.rs` (size panel),
  `app/edgepull.rs` (background preview jobs: only the newest value is computed, the real result replaces the body while dragging),
  `exportdialog.rs`, `settings.rs` (`~/.config/rolidium/settings.conf`), `icons.rs` and `assets/` (generated by `scripts/gen_assets.py`).

## Editing rounds and chamfers

* **Features.** `rolidium-doc::Body::features` lists the rounds and chamfers of a body. Every command that returns a kernel
  `History` moves the faces of the features to their successors (`remap_features`); a feature whose faces vanish is dropped. New
  blend faces are the result faces that descend from no face of the operand (`History::new_faces`).
* **Editing a blend** (`Command::EditBlend`, `Solid::edit_blend`): the blend faces are removed with OCCT defeaturing so the
  neighbours close the gap, the new sharp edge is found as the edge shared by two neighbours that were separated by the blend, and
  that edge is rounded or chamfered with the new size (size 0 stops after defeaturing). The two histories are composed.
* **Pull modes** (`PullMode`): `Auto` keeps the sign, `Add` forces its absolute value, `Cut` the negative of it.
* **Snapping and up-to** are pure functions in `rolidium-app/src/app/snap.rs` (candidate planes and vertices, snap radius 9 px,
  distance to a target plane/point/edge, opposite face distance), with unit tests.
* **Translations**: `rolidium-app/src/i18n.rs`. The English text is the key; a catalogue is a text file with `English => Translation`
  lines; entries with `{}` act as patterns, so messages with numbers are translated as a whole. Files are read from the
  executable's `lang/` directory and `~/.config/rolidium/lang/` on top of the built-in copy. Set `ROLIDIUM_LANG_DIR` to add a directory.
* **Error tone** (`sound.rs`): a two-note soft tone is synthesised as a WAV and piped to the first available player; the call
  returns at once (own thread) and does nothing without a player. `ROLIDIUM_NO_SOUND=1` silences it.
* **Grid**: `rolidium-render::grid_levels(step, px_per_mm)` returns the minor and major spacing shown; the same minor spacing is
  used for sketch snapping.

## Files and exchange

* `rolidium-doc::file`: save/load (zip, JSON, BREP), `Extras` for content this version does not know, atomic write. `Document` is now
  `Clone` (cheap `Arc` snapshots) so saving runs on the worker thread; `is_dirty` compares snapshots.
* Worker jobs: `SaveFile` (also autosave and the preview PNG, rendered with a second renderer on the shared GPU), `OpenFile`, `ReadStep`,
  `ExportStep`. The UI never waits for the disk.
* `rolidium-app/src/app/files.rs`: New/Open/Save flow, confirmation, recovery, autosave timer; `filedialog.rs`: the built-in dialog.
