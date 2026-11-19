# Windows build (cross-compiled from Linux)

Status: `rolidium.exe` is one program with the window and the `cli` and `mcp` modes. The command-line modes are checked under Wine. The window (`rolidium.exe`) was verified on a real Windows 10 machine with a
Radeon RX 580: grid, axes, sketches and the dark theme draw correctly. Without a graphics-card driver (software rendering, "Microsoft
Basic Render Driver") the grid and sketches may not draw; that is a known limitation.

## Prerequisites (Debian / Ubuntu / Linux Mint)

    rustup target add x86_64-pc-windows-gnu
    sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 wine64     # wine is only for testing the command-line modes

The mingw-w64 compiler must use the **posix** thread model. The Debian default (`win32`) lacks `std::mutex`, so `.cargo/config.toml`
names the `-posix` variants of the compiler and the linker explicitly; no `update-alternatives` change is needed.

## Build

    cargo build --release --target x86_64-pc-windows-gnu -p rolidium-app
    # result: target/x86_64-pc-windows-gnu/release/rolidium.exe

`crates/rolidium-kernel/build.rs` picks the prebuilt Open CASCADE archive for the target
(`occt-8_0_1_rev3-x86_64_pc_windows_gnu.tar.gz`, SHA-256 verified, looked up in `~/rolidium-vendor/` first, then downloaded). The C++ runtime
(`libstdc++`, `winpthread`) is linked statically, so the exe needs only the DLLs every Windows has. `rolidium.exe` gets its icon from
`assets/appicon/rolidium.rc` through `x86_64-w64-mingw32-windres`.

## What differs from Linux

* Settings in `%APPDATA%\rolidium`, autosave and the log in `%LOCALAPPDATA%\rolidium` (`crates/rolidium-app/src/dirs.rs`).
* The error tone is silent (a native player is not written yet).
* The file dialog lists the drive letters (`A:` to `Z:`) and the user's folders; time stamps use the local time zone.
* The window needs a DirectX 12 or Vulkan capable GPU driver. With the setting `auto` the backends are tried one after another, each in
  an instance of its own: DirectX 12 with a real graphics card, then Vulkan with a real card, then OpenGL, and software adapters last. The
  Vulkan driver is not loaded at all while DirectX 12 works. An explicit choice (`--backend dx12|vulkan|gl` or Settings) initialises only
  that backend.

## Drivers that crash at start

A graphics driver can crash the whole program while it is only being loaded, and nothing in the program can catch that. Seen on Windows:
the Vulkan driver of Intel integrated graphics (`igvk64.dll`, version 31.0.101.2137, exception 0xc0000005) crashed `rolidium.exe` 0.2.0 at
every start, while `--backend dx12` worked. Rolidium defends itself like this:

* DirectX 12 goes first, so such a Vulkan driver is normally not touched.
* Before a backend is tried, a marker file (`startup-<pid>.mark`, in `%LOCALAPPDATA%\rolidium`) is written and locked; it is removed after
  the first frames were drawn or when the program ends normally. A marker left behind by a dead process means that start did not get
  through: the next start skips that backend, stores it in `settings.conf` (`backend_blocked`, with the adapter and driver when known) and
  shows a message once. Markers of programs that are still starting, and of other program versions, are ignored.
* Choosing `auto` in Settings (or `--backend auto`) forgets the skipped backends, for example after a driver update.
* If no backend works, a system message box explains what happened, that a driver update usually helps, how to try `--backend` and where
  the log is.
* Every graphics step is written to `%LOCALAPPDATA%\rolidium\rolidium.log` and flushed to the disk before the step runs, and an unhandled
  exception writes one line with its code, address and module, so after a crash the last step is visible.
* `--diagnose` and *About > Save diagnostics* examine each backend in a child process with a time limit; a crash of a driver appears in
  the report as a result.

Verified: the logic by unit tests; the whole sequence (crash, marker, skip, message) on Linux with a simulated crash; the Windows build under
Wine. Not verified: a real crash of a real Intel Vulkan driver.

## Checking the command-line mode under Wine

`scripts/windows-scenario.txt` builds a part (box, pull, round, chamfer, hole, loft), saves it, writes STL and STEP and opens the file again.
Run it with the Linux and the Windows executable and compare: volumes, face counts and the STL, STEP and `.rolidium` files were identical.

    cd some-empty-folder
    rolidium cli run /path/to/scripts/windows-scenario.txt
    wine rolidium.exe cli run /path/to/scripts/windows-scenario.txt
