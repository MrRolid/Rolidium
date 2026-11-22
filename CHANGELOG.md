# Changelog

All notable changes to Rolidium are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
version numbers follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may break things).

Versions 0.0.0 to 0.1.13 were internal development versions and were not published; they are summarised so that the history of the
features is visible. 0.2.0 is the first public release.

## [Unreleased] - 0.3.0 (Rolidium for 3D printing), in development

### Added
- Patterns (`T`, `rolidium cli linear-pattern` and `circular-pattern`, MCP the same): a body repeated along a line (a direction from an axis, an edge or a face, count and spacing or count and total length, an optional second direction for a grid) or around an axis (an axis, an edge or the axis of a round face; count and angle, or a full circle). The result is new bodies, an addition to a body or a cut out of a body. A translucent preview with the number of pieces, a handle in the view for the spacing, and pieces that overlap or lie outside are shown in red with a message.
- Mirror (`B`, `mirror`): a body by a base plane, the plane of a sketch or a flat face (with an offset), the body itself or with the original kept, merged with the original into one body, or as a live mirror. Mirror and patterns of the shapes of a sketch (`sketch-pattern`, `sketch-mirror`).
- Links: a pattern or a live mirror is an object of the document that remembers its source, parameters and bodies. It is listed in the right column and its numbers can be changed (a live preview, one step of Undo). Editing the source recalculates the pieces (only the pieces that depend on it); a hand edit of a derived body asks whether to remove the link first; "Remove the link" keeps the bodies, "Delete with its bodies" removes them; a failed recalculation keeps the last good result and says why. Links are saved in the `.rolidium` file (`docs/FILE-FORMAT.md`; older files open unchanged).
- Groups of bodies in the right column: made from the marked bodies (Ctrl+G), nested, renamed (F2 or a double click), moved by dragging, shown and hidden together, a click marks all their bodies; saved in the file. In `rolidium cli` and MCP: `group`, `ungroup`, `move-to-group`.
- Builds that are not exactly a tagged release say `-dev` in the version; `scripts/package.sh dev` makes packages for testing into `dist/dev/`.
- `rolidium cli times`: rows for a circular pattern of 12 holes, for the recalculation of a pattern of 50 pieces after the source changed, and for a mirror with merge.

### Not yet
- STEP export of groups as an assembly structure: the prebuilt OpenCascade of the packages has no XCAF libraries, so groups are not written to STEP files (the bodies are).

## [0.2.0] - 2026-11-22

### Added
- Packages: a `.deb` for Debian, Ubuntu and Linux Mint (the program in `/usr/bin`, menu entry, icons, the type `application/x-rolidium`, Debian copyright file, the licences of the components in `/usr/share/doc/rolidium`) and an `.AppImage` (also starts without FUSE with `--appimage-extract-and-run`), next to the tar.gz and the Windows zip. `scripts/package.sh` makes all of them, the demos zip and the checksums with one command into `dist/release/`.
- `ROLIDIUM_PRESENTATION=1` leaves the label of software drawing and the name of the graphics card out of the status bar (for pictures of the window; see `docs/UI-STYLE.md`).
- Demo models (`examples/demos`, CC0): ten models from an electronics box to a CNC router with 338 bodies, built by scripts for `rolidium cli`, with a test that checks their bodies, volumes, that no two bodies overlap and that an assembly is one connected piece.
- Names of bodies: free text that is saved in the file, kept by operations (a copy has the same name, a merged body keeps the name of its target), renamed with F2 or a double click in the list (one step of Undo), given with `name "..."` when a body is made in `rolidium cli` and MCP, changed with the `rename` command and written to and read from STEP files.
- `rolidium --bench file.rolidium`: opens the file, times the first frame, turns the view for 5 seconds, prints the time of a frame (median and worst) with the graphics adapter and interface, writes it to the log and exits.
- Pull from a sketch and Revolve have the mode `new` (a body of its own, even where it touches another body) in `rolidium cli` and MCP; `export-step` and `export-stl` write all bodies unless `body N` is given.
- First steps: a new empty document starts in a sketch on the XY plane with the Circle tool; the isometric view is shown for half a second
  at the start, then it turns square to the plane. A guide of short hints leads to the first body (draw, press P, pull, how to turn and
  move the view, drawing on a face); hints can be closed, the guide switched off and run again from the About window; the progress is remembered.
- Fill (`F`, `rolidium cli fill`, MCP `fill_loop`): a closed flat loop of edges of a body, sketch lines or a 3D line becomes a region with
  the exact geometry, and Pull starts with it (closing a hole, a plug).
- Join ends (`J`, `rolidium cli join-ends`, MCP `join_ends`): free ends of lines, arcs and 3D lines closer than a gap are connected from
  the closest pairs (tiny gaps welded), with a live preview and one step of Undo.
- Cut mode of a sketch on a plane takes the volume out of the bodies it overlaps (holes and pockets without a Combine).
- The display follows the zoom: the display mesh is made again in the background (with inertia) so that a circle across the window deviates
  less than half a pixel; a cap of triangles protects big models; STL export, measurement and the kernel are not affected.
- Settings: a new document starts with a sketch, the guide, and the largest gap of Join ends (remembered).
- One program with three modes (window, `cli`, `mcp`) instead of three programs: one copy of the geometry kernel in the package, so the
  package is much smaller. `rolidium mcp-config` prints ready-made commands to connect the MCP server to a client.

- Speed in a big document: an operation, Undo or Redo makes the display meshes only of the bodies that changed (a document of 340 bodies took seconds for every small operation), the graphics card keeps the buffers of the others, and the autosave has a thread of its own. `rolidium cli times` has the stages of a small operation in a big document next to the same operation on the body alone, and a test guards the ratio.
- Several cores: the display meshes of many bodies, the reading of the bodies of a file and the booleans of the kernel use more threads (one per core less one by default; Settings: Auto, 1, 2, 4, 8; the variable `ROLIDIUM_THREADS`, also for `cli` and `mcp`). Bodies that share data in the kernel are never worked on at the same time; a test compares the results for 1, 2, 4 and 8 threads.
- The list of bodies on the right marks the bodies picked in the model and scrolls to the last one.
- The Pull panel has the mode New body for a region of a sketch.

- Web and support forum: the About window links to rolidium.com and rolidium.com/forum (also in the README); the links open in the browser of the system.
- Notice of a new version, **opt-in**: a small card at the first start asks (Yes / No; until the answer the program does not use the network); then once a day, in the background, with a limit of 5 seconds, one GET to
  rolidium.com with the version, the system name and main number and the architecture, nothing else. A newer version is offered with *Download* (the fixed download page in the browser; nothing is downloaded or started) and *Skip this version*. Settings has the switch and the exact text of the question. `cli`, `mcp`, `--selftest`, `--bench`, `--diagnose` and `ROLIDIUM_OFFLINE=1` never do it.
- On Windows the summary of the diagnostics (processor model, physical cores, memory, version of the system) and the question about a new version read the facts with system calls and the registry: no other program is started and no console window can flash up. A fact that cannot be found is "not reported".
- Sending diagnostics, only on request: the panel *Report a bug or an idea* at the bottom of the right column (Bug or Idea, a text up to 1000 characters, an optional e-mail, the diagnostics and a picture of the program's own window attached by default to a bug and not to an idea (the picture is taken with eframe's screenshot request when the row is clicked, before the panel opens, kept in memory, at most 1600 pixels, made smaller if the zip would pass 2 MB; *Take again* hides the panel for it; it shows the model and the body names, which the text now says), an idea needs a text, a bug without text goes only with the diagnostics; *Show what is sent* lists every file, each with a check box (a file switched off is neither sent nor saved, `summary.txt` always goes, the choice is remembered, and has the picture of the window (`window.png`, first); the diagnostics of the panel are made in the background only from what is in memory or read without waiting (no child process, no other graphics interface, no disk walk, no network), every step has a limit and is logged with its length, the whole takes about 2 seconds at most and after 5 seconds the panel offers to send without the diagnostics; the test scenes of all graphics interfaces are only in *About > Save diagnostics* (now with a time limit and Cancel) and `--diagnose`; the field for the text is always visible and only the lower part of the panel scrolls; after a crash the panel opens by itself with a note). Nothing is sent before Send. The diagnostics start with a readable summary (graphics card, driver, interface, software drawing, processor, cores, threads, memory, system, window). The diagnostic files hold no models and no model names (the picture of the window does show what is in the window), the home folder is `~`; the report number is shown. When the server does not answer, the zip can be saved and sent by e-mail. See `docs/PRIVACY.md`.
- The network code is one small crate (`rolidium-net`, `ureq` with rustls, no async runtime). New licence in the list of third-party components: webpki-roots (CDLA-Permissive-2.0). Compared with the build before this change the Linux package is 4.5 MB larger (34.9 MB) and the Windows package 4.5 MB larger (31.7 MB): the Linux program grew by 2.5 MB (it includes everything of this change, the TLS library and the list of root certificates among it) and the router file of its author (3.4 MB) is in the examples.
- `rolidium-cli times machine` and `times open` show where the time of a small operation and of opening a big document goes, and which part runs one thread at a time.
- The self test covers: the list of bodies marking and scrolling to a body picked at the end of a long list, the view staying at a sketch drawn far from the origin when Pull is chosen, and a hole of 6 mm in a plate with a hundred holes in a document of 300 bodies (it fails on the program before the fix).
- The demo 10 (the router) is the model of its author, worked on by hand from the script's base (`10-cnc-router-base`); `scripts/refresh-machine.sh` makes its outputs again.

### Fixed
- The list of bodies on the right could not be scrolled: with hundreds of bodies the rest of the list was cut off.
- A hole in a plate that has many holes stayed a polygon when zoomed in, because the estimate of the triangles made the whole plate look too expensive (it counted every face as growing with 1 / deflection; planes and cylinders grow with its square root).
- The names of bodies were lost in a STEP file for bodies that were wrapped in a compound (found with the author's router: 3 of 338).
- The number of threads can no longer be changed while the program runs (changing it could stop the program with an error of the thread pool of Open CASCADE); Settings says that a new number takes effect at the next start.
- Small holes and screw heads in a big document were drawn as polygons when zoomed in: a body is now refined for the pixel at its own distance and the triangle budget is shared by the size of the bodies on the screen.
- After leaving a sketch the view jumped back to where it was before the sketch (often the origin); it now keeps looking at the sketch.
- `rolidium --bench` no longer prints the time of a pick (it measured something else and showed 0.0 ms).
- The Settings and About buttons in the left column were cut off at the bottom (the status bar covered the column); the whole column now fits
  a window 720 px high (checked by the self test at 1280 x 720 and 1920 x 1080).
- Unavailable buttons now have the same white text as the available ones; the unavailability shows in the background, the frame, the dimmed icon and the indicator.
- The messages of rounds, chamfers and other operations were partly shown in English in other languages; a test now checks every message that
  an operation can produce.
- A round or chamfer that does not fit names the largest size that still works and why (for the chamfer with an angle, both distances, and
  what Flip would allow).
- A new volume drawn on a plane that touched or overlapped a body became a separate body, so a later chamfer at the joint only removed material; it now joins the body.
- Windows: the program crashed at every start on a machine whose Intel integrated-graphics Vulkan driver (`igvk64.dll` 31.0.101.2137)
  fails when loaded. `auto` now tries DirectX 12 first with a real graphics card, then Vulkan, then OpenGL, software last, each backend
  in an instance of its own, so Vulkan is not loaded while DirectX 12 works. A marker file guards the start: when a start did not get
  through, the next start skips that backend, remembers it (settings, with adapter and driver) and says so once; `auto` in the settings
  forgets it. If nothing works, a message box says what to do. Every graphics step is logged and flushed to the disk, an unhandled
  exception is logged, and the diagnostics and the render part of the self test examine every backend in a child process with a time limit.

### Changed
- The File block of the left column has a frame, large icons (three sizes by the room available) and the letters STL and STEP are drawn below
  the sheet so that they are readable; the left column is 196 px wide.
- The minimum Rust version for building is 1.89 (file locks of the start-up safeguard).
- The Linux package shrank from 58 to 28 MB and the Windows package from 70 to 25 MB: one copy of the geometry kernel instead of three, and
  symbols are stripped from release builds. On Windows the program is a window program without a console box.
- Public release preparation: licensed under AGPL-3.0-only (LICENSE, SPDX headers in all sources), third-party licences shipped as
  `THIRD-PARTY-LICENSES.txt`, About window with licence, source and issue links, published by Rolid spol. s r.o.
- Documentation for the public: README, BUILDING, CONTRIBUTING, SECURITY, TRADEMARKS, issue forms, this changelog.

## [0.1.13] - 2026-11-19

### Added
- `rolidium-mcp`: an MCP server (JSON-RPC over stdio) so that AI assistants can build, inspect, export and render models without a window;
  files only under `--root`, no network, a time limit per operation, its own software renderer.
- Whole-body highlight in Move and Combine, double click takes a body, smooth (tangent) edges drawn faintly, cycling through
  candidates when picking a sweep path.

### Changed
- The traffic-light indication of button availability is the only style (the greyed-out variant is removed); the view cannot zoom
  closer than 20 mm.

### Fixed
- Combine could lose a swept body (a spring merged with the cylinder it winds on). Every boolean result is now verified against an
  independent volume calculation, other ways are tried, and a result that does not add up is refused with the model unchanged.
  Helix paths are built in pieces of at most half a turn.

## [0.1.12] - 2026-11-19

### Added
- The view turns square to the sketch plane (animated, can be switched off) and back after the sketch; the same animation for the view
  buttons, Fit all and Plan view.
- Traffic-light indication of whether a button can be used now (the greyed-out style stays as a setting).
- Sketch is the main button and becomes "Finish sketch"; a hint in an empty document.
- Measure: Ctrl+click takes a point (middle of an edge, centre of a circle).
- `rolidium-cli times` and stage timings in the log.

### Changed
- Faster volume, display mesh and validity check.

## [0.1.11] - 2026-11-19

### Added
- Measure tool (E): distances with X, Y, Z parts, angles, parallel faces, cylinder axes, with dimension lines in the view.
- Mass and material of a body in the Selection panel; CLI `measure`, `material`, `mass`.
- Long calculations can be cancelled (Cancel button or Esc); the model stays exactly as it was.

### Fixed
- The tangent sketch plane on a curved face when Sketch was started without a selection.
- A click near the outline of a region picks the region in region tools.

## [0.1.10] - 2026-11-19

### Added
- 3D line with snapping to 3D points, axis locks, typed lengths and corner radius.
- Helix tool (axis from a cylindrical face, an edge, a line or a point; pitch, height or turns, handedness, start and cone angle).
- Sweep tool (paths: curves, sketch chains, edge chains; Add / Cut / New body; sharp or rounded corners; checked results).
- CLI `curve`, `helix`, `curves`, `sweep`; examples: bent tube, spring, M10 thread.
- Shortcut letters on the toolbar buttons; plane names and a hover preview of the sketch plane; tangent plane on a curved face.
- Properties in the Selection panel (click copies a value).

### Fixed
- The normal of faces with a left-handed placement (a sketch on a slanted chamfer face pulled the wrong way).

## [0.1.9] - 2026-11-19

### Added
- Dark theme (Dark / Light / Follow the system), toolbar above the view, draggable panels with a leader line.
- German, Spanish, French and Japanese interface (machine translated), Noto Sans JP fallback font.
- Graphics diagnostics: backend choice, `--diagnose` and About > Save diagnostics, off-screen scene check in `--selftest`.
- Rounds and chamfers on whole smooth edge chains, with every result checked.

### Changed
- The grid reaches the horizon with coarser levels; shader changes against missing grid, axes and sketches on some Windows drivers.
- New application icon.

## [0.1.8] - 2026-11-18

### Added
- Move sketch plane (drag, typed distance, up to a vertex or face); loft from profiles selected beforehand.
- New file dialog: places, mounted disks and network volumes, folder tree, sortable columns, typed path with completion, filter.
- Application icon, desktop entry, Windows icon; example scripts with volume checks in the tests.
- Windows build (cross-compiled with mingw-w64).

## [0.1.7] - 2026-11-18

### Added
- Loft tool: sketch regions or flat faces, ruled or smooth, New body / Add / Cut, translucent preview; CLI `loft`.

### Fixed
- Angled chamfer preview, sketch plane offset for planes that already hold a sketch.

## [0.1.6] - 2026-11-18

### Added
- Combine tool: merge, cut, intersect, keep tools, split into pieces.
- Arc tool (three points, typed radius, tangent continuation), regions with arcs.
- Sketch plane offset; angled chamfer (distance and angle, flip); About window with the Support development section.

## [0.1.5] - 2026-11-18

### Added
- Sketch snapping to 3D features with dotted guides and reference distances; context menu; hide and show; rename.
- Move tool with arrows and rings, copy; base planes XY / XZ / YZ; Revolve.
- Rounds next to existing rounds.

## [0.1.4] - 2026-11-18

### Added
- `.rolidium` file format, New / Open / Save / Save As, recent files, autosave and recovery after a crash, atomic saving.
- STEP import and export.
- Rounds and chamfers of inner (concave) edges.

## [0.1.3] - 2026-11-18

### Added
- A region pulled beside a body becomes a new body; multiple selection with Ctrl and Shift; tangent chains highlighted.

### Changed
- Grid lines fade out when they get too dense; Shift (not Alt) switches snapping off while dragging.

## [0.1.2] - 2026-11-18

### Added
- Editing finished rounds and chamfers (new size, or back to the sharp edge).
- Face panel with Move by, distance to the opposite face, mode and Up to; snapping while dragging; grid control.
- Translations as text files (Czech first), Settings window, error tone, icons.

## [0.1.1] - 2026-11-18

### Added
- Version and build hash in the status bar and `--version`; STL export dialog with fineness presets and a size estimate.
- Edge hover and selection, Round and Chamfer with a background preview; size panel for sketch shapes and holes; icons and help cards.

## [0.0.0] - 2026-11-18

### Added
- Workspace on Rust, Open CASCADE 8.0.1 (own C++ bridge, statically linked), egui and wgpu.
- Viewport with orbit, pan and zoom, grid and axes, ID-buffer picking; Pull tool with preview and typed distance; undo and redo.
- Headless CLI with STL export and off-screen rendering; benchmarks and tests.
- Sketch mode: lines, rectangles, circles, typed dimensions, snapping, automatic region detection, Pull from sketch regions.

[0.2.0]: https://github.com/MrRolid/Rolidium/releases/tag/v0.2.0
