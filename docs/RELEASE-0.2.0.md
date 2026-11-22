# Rolidium 0.2.0: the first public release

Rolidium is a free, open-source **direct-modeling CAD program** for mechanical parts. Instead of editing a feature tree you draw a sketch, pull it into a
solid, push and pull faces, round edges and combine bodies, in the spirit of DesignSpark Mechanical and SpaceClaim. Today it models parts and exports STL
and STEP (3D printing, input for manufacturing); toolpaths for milling and output for laser cutting are planned, not part of this version. It is written in
Rust on the Open CASCADE geometry kernel and runs on Linux and Windows.

> **This is an alpha version.** It is useful for experiments and for simple parts, but it is not ready for production work. Features are missing, there are
> rough edges, and the file format may still change. Keep copies of important models; STEP export is a good archive format.

## What it can do

* **Direct modelling**: pull and push faces and sketch regions with the mouse or a typed distance, also *up to* a vertex, edge or face; move, rotate and copy
  bodies; merge, cut and intersect them. Every result is checked (valid solid, volume, self-intersection), and a result that fails is refused and the model stays as it was.
* **Sketching**: lines, rectangles, circles and arcs with snapping and visible guides, automatic regions with holes, sketches on the base planes, on flat
  faces, on tangent planes of curved faces and on offset planes.
* **Revolve, loft and sweep**, including sweeps along a helix for springs and threads. **Rounds and chamfers** on whole chains of edges, editable later.
* **Measure** distances and angles with dimension lines in the view; **mass** of a body for a chosen material.
* **Files**: native `.rolidium` documents with autosave and crash recovery, STEP import and export, STL export. Names of bodies are saved and written to STEP.
* **Headless tool and MCP server**: `rolidium cli` runs scripts without a window; `rolidium mcp` lets an AI assistant build, inspect and export models and get
  pictures of them; `rolidium mcp-config` prints ready-made commands that connect it to a client (see `MCP.md`).
* **Big documents**: an operation costs what the changed bodies cost, not what the whole document costs; meshes, reading files and booleans use several threads.
  The router demo has 338 bodies.
* **Ten demo models** (public domain, CC0) in `examples/demos`, from an electronics box to a gantry CNC router.
* Interface in English, Czech, German, Spanish, French and Japanese; dark and light appearance. The German, Spanish, French and Japanese texts are machine translations.

## What is new in this release

This is the first public release, so everything is new. The changelog (`CHANGELOG.md`) lists it in full. The last steps before the release were:

* A panel **Report a bug or an idea** at the bottom of the right column, only on request: Bug or Idea, a text, an optional e-mail, the diagnostics and a picture of
  the program's own window (both on for a bug, off for an idea). You see every file first and can switch each one off; nothing leaves before you press Send, and
  the text can be saved to a file and sent by e-mail instead. The picture shows what is in the window, including the model.
* An optional **question about a new version**, once a day and only after you agreed; it sends the version, the system name and number and the architecture.
* Windows: the facts about the computer in the diagnostics are read with system calls and the registry; no other program is started.
* Packages: a `.deb`, an `.AppImage`, a tar.gz for Linux and a zip for Windows.

## Download and requirements

| File | For |
| --- | --- |
| `rolidium_0.2.0_amd64.deb` | Debian, Ubuntu, Linux Mint: `sudo apt install ./rolidium_0.2.0_amd64.deb` |
| `Rolidium-0.2.0-x86_64.AppImage` | any Linux x86-64: `chmod +x` and run (`--appimage-extract-and-run` without FUSE) |
| `rolidium-0.2.0-linux-x86_64.tar.gz` | any Linux x86-64: unpack and run `./rolidium` |
| `rolidium-0.2.0-windows-x86_64.zip` | Windows 10 or 11, 64-bit: unpack and run `rolidium.exe` |
| `rolidium-demos-0.2.0.zip` | the ten demo models with their STEP files, pictures and STL where it fits |
| `SHA256SUMS` | the SHA-256 checksums of the files above: `sha256sum -c SHA256SUMS` |

* **Windows**: 10 or 11, 64-bit.
* **Linux**: x86-64, glibc 2.35 or newer and libstdc++ 12 or newer (Ubuntu 22.04 and later, Debian 12 and later, Linux Mint 21 and later, Fedora 36 and later).
* **Graphics**: DirectX 12 (Windows), Vulkan or OpenGL 3.3, with the driver of the graphics card. Without one Rolidium draws by software, which works but is slow.
* The Windows program is **not signed** with a certificate. Windows SmartScreen may warn about it (*More info*, *Run anyway*) and some antivirus programs may
  block it as unknown; it is a false alarm. Check the download with the checksums, and add an exception for the folder if needed.

The packages and the checksums are also at [rolidium.com/download](https://rolidium.com/download). The privacy notice is at [rolidium.com/privacy](https://rolidium.com/privacy).
Web: [rolidium.com](https://rolidium.com), support forum: [rolidium.com/forum](https://rolidium.com/forum).

## Known limitations

* Without a graphics-card driver (only the Windows software adapter "Microsoft Basic Render Driver") the grid and sketches may not be drawn. Install the driver of
  your graphics card. The window was verified on real Windows 10 with an AMD Radeon RX 580, with an earlier build; the build in this release was not tried on real Windows, only cross-compiled and started under Wine (it does not draw there).
* On Windows, DirectX 12 is tried first. Older Vulkan drivers of Intel integrated graphics (a crash in `igvk64.dll`, version 31.0.101.2137) crashed the program at start;
  now Vulkan is not loaded while DirectX 12 works, and a backend that did not get through the start is skipped at the next start.
* The preparation of the diagnostics in the report panel has time limits and gives up after 5 seconds; this was tested on Linux, not on a Windows machine with software drawing.
* The glibc requirement of 2.35 was read from the version tables of the program; the program was not run on an older Linux than the build system.
* Rolidium refuses operations whose result fails its checks; some combinations of swept bodies (a spring or a thread) with other bodies can therefore be refused. The model stays unchanged.
* The command-line modes of `rolidium.exe` in an interactive Windows console are not verified yet (the MCP mode over pipes was checked under Wine only).
* In a big document some work is still done one call at a time, so opening the router does not get faster beyond about 4 threads. A document of hundreds of bodies needs hundreds of MB of memory.
  The speeds were measured with software drawing and under load; the numbers on a real graphics card were not measured.
* The server behind the new-version notice and the report panel may not answer; then nothing is shown for the notice, and the panel offers to save the report to a file.
* The router demo was edited by hand: some of its bodies overlap or touch nothing by face (see its README).
* No CAM (toolpaths, G-code), no permanent dimensions, constraints, drawings or STL import; millimetres only.

## Reporting bugs and ideas

Questions and help: the support forum at [rolidium.com/forum](https://rolidium.com/forum). Bugs and ideas: the forum, [GitHub Issues](https://github.com/MrRolid/Rolidium/issues), or the
panel *Report a bug or an idea* in the program. For crashes and drawing problems please attach the zip from *About > Save diagnostics* (no personal paths, no models).

## Licence

Rolidium is licensed under the GNU Affero General Public License, version 3 only (AGPL-3.0-only). A commercial licence is available for those whom the AGPL does not
suit: connect@rolid.com. The files you create with Rolidium belong to you; the program licence does not apply to them. The licences of the third-party components are in
`THIRD-PARTY-LICENSES.txt` in the packages (in `/usr/share/doc/rolidium/` in the .deb). Rolidium makes use of facilities provided by the Open CASCADE Technology software.

Published by Rolid spol. s r.o., connect@rolid.com.
