# Rolidium 0.2.0: the first public release

Rolidium is a free, open-source **direct-modeling CAD program** for mechanical parts. Today it models parts and exports STL and STEP for 3D printing and manufacturing; toolpaths for milling and output for laser cutting are planned. Instead of
editing a feature tree you push and pull faces, draw a sketch and pull it into a solid, round edges and combine bodies, in the spirit of
DesignSpark Mechanical and SpaceClaim. It is written in Rust on the Open CASCADE geometry kernel and runs on Linux and Windows.

> **This is an alpha version.** It is useful for experimenting and for simple parts, but it is not ready for production work. Features
> are missing, there are rough edges, and the file format may still change. Keep copies of important models; STEP export is a good
> archive format.

## Highlights

* **Direct modelling**: pull and push faces and sketch regions with the mouse or a typed distance, *up to* a vertex, edge or face;
  move, rotate and copy bodies; merge, cut and intersect them.
* **Sketching** with lines, rectangles, circles and arcs, snapping with visible guides, automatic regions with holes, sketches on base
  planes, flat faces, tangent planes of curved faces and offset planes.
* **Revolve, loft and sweep**, including sweeps along a helix for springs and threads.
* **Rounds and chamfers** on whole smooth chains, editable later; every result is checked and a damaged one is refused.
* **Measure** distances and angles with dimension lines in the view; **mass** of a body for a chosen material.
* **Safe calculations**: long operations run in the background and can be cancelled; every result is verified, and the model is never
  left half-changed.
* **Files**: native `.rolidium` documents with autosave and crash recovery, STEP import and export, STL export with a live size estimate.
* **Headless tool and MCP server**: `rolidium cli` runs scripts without a window; `rolidium mcp` lets an AI assistant build, inspect and
  export models (see [MCP.md](MCP.md)); `rolidium mcp-config` prints ready-made commands that connect it to a client.
* Interface in English, Czech, German, Spanish, French and Japanese, dark and light appearance.

## Requirements

* **Linux**: x86-64, glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Linux Mint 21, Fedora 36 or newer), a graphics driver with Vulkan.
* **Windows**: 64-bit Windows 10 or 11 with the driver of your graphics card (Vulkan or DirectX 12).

## Download

Packages for Linux and Windows and their SHA-256 checksums are on the [releases page](https://github.com/MrRolid/Rolidium/releases).

## Known limitations

* Without a graphics-card driver (only the Windows software adapter "Microsoft Basic Render Driver"), the grid and sketches may not be
  drawn. Install the driver of your graphics card. The window was verified on real Windows with an AMD Radeon RX 580.
* On Windows, DirectX 12 is tried first. Older Vulkan drivers of Intel integrated graphics (a crash in `igvk64.dll`, version 31.0.101.2137) crashed the program at start; now Vulkan is not loaded while DirectX 12 works, and a backend whose start did not get through is skipped on the next start (see [WINDOWS.md](WINDOWS.md)). Tested with a simulated crash and under Wine only.
* The German, Spanish, French and Japanese translations are machine translations and wait for a review by native speakers.
* Rolidium refuses operations whose result fails its checks (for example the volume check); some combinations of swept bodies with
  other bodies can therefore be refused. The model stays unchanged.
* The command-line modes of `rolidium.exe` in an interactive Windows console are not verified yet (the MCP mode over pipes was checked under Wine only).
* No CAM (toolpaths, G-code) yet, no permanent dimensions, constraints, drawings or STL import; millimetres only.

## Reporting bugs and ideas

Please use [GitHub Issues](https://github.com/MrRolid/Rolidium/issues). For crashes and drawing problems, attach the zip from
*About > Save diagnostics* (it contains no personal paths). Security problems: see [SECURITY.md](../SECURITY.md).

## Licence

Rolidium is licensed under the GNU Affero General Public License, version 3 only (AGPL-3.0-only). A commercial licence is available
for those whom the AGPL does not suit: connect@rolid.com. The files you create with Rolidium belong to you; the program licence does not
apply to them. The licences of the third-party components are in `THIRD-PARTY-LICENSES.txt` in the package.
Rolidium makes use of facilities provided by the Open CASCADE Technology software.

Published by Rolid spol. s r.o., connect@rolid.com.
