# Building Rolidium from source

This page describes how to build Rolidium 0.2.0 yourself. If you only want to use the program, take a release package instead (see
the README).

The packages contain one program, `rolidium`, with three modes: the window (default), `rolidium cli` (the headless tool: runs scripts,
exports STL and STEP, renders pictures) and `rolidium mcp` (the MCP server for AI assistants, see [docs/MCP.md](docs/MCP.md)). The workspace
also builds `rolidium-cli` and `rolidium-mcp` as small separate programs, which developers and the tests use.

## What has been verified

| Way of building | Status |
| --- | --- |
| Linux, Open CASCADE downloaded as a prebuilt archive (the default), on a fresh clone in a new directory following this page | **Verified** (release build, version output, tests of the sketch crate and an example script) |
| Linux, your own Open CASCADE through `ROLIDIUM_OCCT_ROOT` (using the unpacked prebuilt tree as "your own build") | **Verified** that the override and its folder layout work |
| Linux, Open CASCADE built from source with the CMake options below | **Not verified**: the options are taken from the recipe of the prebuilt archive; no build was done |
| Windows cross-build with mingw-w64 (target `x86_64-pc-windows-gnu`) | **Verified on the developer's machine** (the release packages are built this way, and the programs run under Wine and on real Windows 10); **not verified** on a fresh machine from this page alone |
| Rust 1.89 (the minimum in `Cargo.toml`) | **Not verified**; the builds were done with a newer stable Rust |
| Native Windows build with MSVC, macOS | Not set up |

## Linux (x86-64)

### What you need

* **Rust 1.89 or newer** (the `rust-version` in `Cargo.toml`), installed with [rustup](https://rustup.rs). Distribution packages of Rust
  are often too old.
* **A C++17 compiler**: GCC 9 or newer, or Clang 10 or newer. It compiles the small C++ bridge to Open CASCADE.
* **curl**, used once to download the prebuilt Open CASCADE (not needed if you supply Open CASCADE yourself, see below).
* **git**, to get the sources.

On Debian, Ubuntu or Linux Mint:

    sudo apt install build-essential curl git
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # then open a new terminal

No `-dev` packages for X11, Wayland or Vulkan are needed to build: the window libraries and the Vulkan loader are opened when the
program starts. To **run** the window you need a graphics driver with Vulkan (on Debian/Ubuntu: `mesa-vulkan-drivers` for AMD and Intel,
or the NVIDIA driver), the Vulkan loader (`libvulkan1`), `libxkbcommon0` and the X11 libraries (`libx11-6`, `libxcursor1`, `libxrandr2`,
`libxi6`) or Wayland (`libwayland-client0`). A normal desktop installation has all of them. Released packages need glibc 2.35 or newer.

### Build

    git clone https://github.com/MrRolid/Rolidium.git
    cd Rolidium
    cargo build --release

The first build takes a few minutes and about 2 GB of disk space in `target/` (Open CASCADE itself is not compiled, only linked). The
programs are then in `target/release/`:

Development builds keep only line tables as debug information (`[profile.dev]` in `Cargo.toml`); with full debug info every test binary was
about 600 MB. `scripts/tidy-target.sh` removes what can be rebuilt (incremental caches, old test binaries, old temporary files and window-test
screenshots) and, if `target/` is still over 25 GB, the Windows and debug builds; `--deep` does that always. Run it at the end of a work step.

    ./target/release/rolidium --version
    ./target/release/rolidium                 # the application
    ./target/release/rolidium cli help        # the headless tool

### Tests

    cargo test -p rolidium-sketch             # quick check, a few seconds
    cargo test --workspace                    # everything, several minutes

`cargo test --workspace` includes tests that render pictures off screen and need a working Vulkan driver (software rendering with Mesa
lavapipe is enough), and one that runs the real window in a virtual X server: it needs `xvfb-run` (`sudo apt install xvfb`) and fails
on purpose when it is missing.

## Where Open CASCADE comes from

Rolidium uses [Open CASCADE Technology](https://dev.opencascade.org/) (OCCT) 8.0.1 as its geometry kernel, linked **statically**. You do
not have to install it. The build script `crates/rolidium-kernel/build.rs` looks for it in this order:

1. **`ROLIDIUM_OCCT_ROOT`**: if this environment variable is set, it must point at an OCCT installation with the headers in
   `include/opencascade/` and the static libraries in `lib/`. Nothing is downloaded. Use this for your own OCCT build (see below).
2. **A local copy of the prebuilt archive** in the directory `$ROLIDIUM_VENDOR_DIR`, or `~/rolidium-vendor/` if that variable is not
   set. Useful for building offline.
3. **Download** of the same prebuilt archive with `curl` from the releases of the [cadrum](https://github.com/lzpel/cadrum) project,
   release `occt-8_0_1_rev3` (about 44 MB, one time).

In cases 2 and 3 the archive is checked against a fixed SHA-256 checksum before it is used, and unpacked into
`target/rolidium-occt/`. A wrong or damaged archive stops the build.

| Target | Archive | SHA-256 |
| --- | --- | --- |
| Linux x86-64 | `occt-8_0_1_rev3-x86_64_unknown_linux_gnu.tar.gz` | `f76c341123fc008391d6230b5e9157c5531166ed4cf301f6484f69a44aee59c8` |
| Windows x86-64 (mingw) | `occt-8_0_1_rev3-x86_64_pc_windows_gnu.tar.gz` | `dc2cd5a2b4df2f58e6590f027fd04eef0b65d21a5003933d01e74e275c18301c` |

The archives are at `https://github.com/lzpel/cadrum/releases/download/occt-8_0_1_rev3/<archive>`. To build offline, download the
archive once and put it into `~/rolidium-vendor/` (or into the directory named by `ROLIDIUM_VENDOR_DIR`).

The prebuilt archive is OCCT 8.0.1 from upstream with two small fixes by the cadrum project in the sweep code (`BRepFill_PipeShell`,
`GeomFill_GuideTrihedronPlan`). The archive contains the changed source files, and the fixes are in the `patches/` folder of the cadrum
repository. OCCT is licensed under the LGPL 2.1 with the Open CASCADE exception; see `assets/legal/`.

### Building Open CASCADE yourself

If you prefer not to use a prebuilt binary, build OCCT 8.0.1 from the official sources. You need CMake (3.16 or newer) and the C++
compiler from above.

    git clone --depth 1 --branch V8_0_1 https://github.com/Open-Cascade-SAS/OCCT.git occt-src
    cmake -S occt-src -B occt-build \
        -DCMAKE_BUILD_TYPE=Release \
        -DBUILD_LIBRARY_TYPE=Static \
        -DCMAKE_INSTALL_PREFIX="$HOME/occt-8.0.1" \
        -DBUILD_MODULE_FoundationClasses=ON \
        -DBUILD_MODULE_ModelingData=ON \
        -DBUILD_MODULE_ModelingAlgorithms=ON \
        -DBUILD_MODULE_DataExchange=ON \
        -DBUILD_MODULE_Visualization=OFF \
        -DBUILD_MODULE_ApplicationFramework=OFF \
        -DBUILD_MODULE_Draw=OFF \
        -DUSE_FREETYPE=OFF -DUSE_TK=OFF -DUSE_TCL=OFF -DUSE_XLIB=OFF -DUSE_OPENGL=OFF \
        -DUSE_FREEIMAGE=OFF -DUSE_FFMPEG=OFF -DUSE_TBB=OFF -DUSE_VTK=OFF -DUSE_RAPIDJSON=OFF -DUSE_DRACO=OFF \
        -DBUILD_DOC_Overview=OFF -DBUILD_ENABLE_FPE_SIGNAL_HANDLER=OFF
    cmake --build occt-build --parallel
    cmake --install occt-build

Rolidium links these static libraries, so they must be in `$HOME/occt-8.0.1/lib/` after the installation (as `libTK*.a`):

    TKDESTEP TKDECascade TKDE TKXSBase TKFillet TKOffset TKShHealing TKBool TKBO TKMesh
    TKTopAlgo TKPrim TKGeomAlgo TKGeomBase TKBRep TKG3d TKG2d TKMath TKernel

(the list is `OCCT_LIBS` in `crates/rolidium-kernel/build.rs`). Then build Rolidium against it:

    export ROLIDIUM_OCCT_ROOT="$HOME/occt-8.0.1"
    cargo build --release

If you change `ROLIDIUM_OCCT_ROOT`, Cargo rebuilds the kernel crate automatically. Plain upstream OCCT lacks the two cadrum fixes
mentioned above; they concern sweeps with a guide curve, which Rolidium does not use today.

## Windows (64-bit), cross-built on Linux

The Windows programs are built on Linux with the mingw-w64 toolchain (target `x86_64-pc-windows-gnu`). Building natively on Windows with
MSVC is not set up. Details and the test procedure are in [docs/WINDOWS.md](docs/WINDOWS.md); in short:

    rustup target add x86_64-pc-windows-gnu
    sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
    cargo build --release --target x86_64-pc-windows-gnu -p rolidium-app
    # result: target/x86_64-pc-windows-gnu/release/rolidium.exe (add -p rolidium-cli for the small developer programs)

* mingw-w64 must use the **posix** thread model (the `win32` model lacks `std::mutex`). `.cargo/config.toml` names the `-posix` compiler
  and linker explicitly, so the Debian/Ubuntu packages above work without `update-alternatives`.
* Open CASCADE is found the same way as on Linux, but the Windows archive from the table above is used (chosen by the target, not by the
  machine you build on). For your own OCCT build for Windows, configure it with a mingw-w64 toolchain file, add
  `-DINSTALL_DIR_LAYOUT=Unix` so that the headers land in `include/opencascade/` and the libraries in `lib/`, and set `ROLIDIUM_OCCT_ROOT`.
* The C++ runtime is linked statically; the `.exe` files need only the DLLs every Windows has. `rolidium.exe` gets its icon through
  `x86_64-w64-mingw32-windres` (part of the mingw-w64 packages).

## Making the release packages

One command makes everything that a release has, from a clean committed state, into `dist/release/`: the Linux tar.gz, the `.deb`, the
`.AppImage`, the Windows zip, the zip of the demos and `SHA256SUMS` for all of them:

    examples/demos/build-all.sh target/release/rolidium     # once: the demos that go into the demos zip (target/demos)
    scripts/package.sh

What it needs, besides the Linux and Windows build tools above:

* `dpkg-deb` and `fakeroot` (`sudo apt install dpkg-dev fakeroot`) for the `.deb`. `lintian` is run on it when it is installed.
* `appimagetool` for the `.AppImage`. It is itself an AppImage; put a copy into `~/.local/bin/appimagetool` (no root needed) and the runtime file
  it embeds into `~/.local/share/appimage-runtime-x86_64` (the file names can be changed with `APPIMAGETOOL` and `APPIMAGE_RUNTIME`). Both come from
  the releases of [AppImage/appimagetool](https://github.com/AppImage/appimagetool) and [AppImage/type2-runtime](https://github.com/AppImage/type2-runtime).
  The script starts the tool with `--appimage-extract-and-run`, so FUSE is not needed.
* Python 3 and zip.

The `.deb` has `/usr/bin/rolidium` and links `rolidium-cli` and `rolidium-mcp`, the menu entry, the icons of the sizes 16 to 512 and the scalable one, the
type `application/x-rolidium` (`*.rolidium`, with the same icons), a Debian `copyright` file and the licences of the third-party components in
`/usr/share/doc/rolidium/`. It depends on `libc6 (>= 2.35)`, `libstdc++6 (>= 12)`, `libgcc-s1`, `libxkbcommon0` and `libx11-6` or `libwayland-client0`;
the graphics driver (`libvulkan1`, Mesa) and the other window libraries that the program opens when it starts are recommended. The highest
glibc symbol the program needs is `GLIBC_2.35`; `pidfd_spawnp` and `pidfd_getpid` of glibc 2.39 are optional (a weak version dependency of the Rust
standard library), so the programs of a build on a newer system still start on glibc 2.35. This was checked by reading the version tables
(`objdump -T`, `readelf -V`), and not by running it on an older system.

## Troubleshooting

* **`curl` is required to download Open CASCADE**: install curl, or put the archive into `~/rolidium-vendor/`.
* **`Open CASCADE archive checksum mismatch`**: the download was damaged or replaced. Delete `target/rolidium-occt/` and try again;
  if it keeps failing, download the archive by hand and compare it with the SHA-256 in the table.
* **`OCCT headers not found in ...`**: `ROLIDIUM_OCCT_ROOT` points to a directory without `include/opencascade/`.
* **Undefined references to `TK...` symbols when linking**: one of the libraries listed above is missing in your own OCCT build.
* **The window does not open or shows nothing**: run `rolidium --diagnose` (or *About > Save diagnostics*) and attach the zip to a bug
  report. `rolidium --backend vulkan` (or `gl`, `dx12`) picks a graphics backend by hand.
