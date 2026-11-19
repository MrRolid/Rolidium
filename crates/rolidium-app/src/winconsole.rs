// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Windows: the program is a window program, which has no console. The command-line modes (`cli`, `mcp-config`, `--version`, ...)
//! attach to the console of the program that started them, so that their output is visible. When the standard handles are already
//! valid (output redirected to a file or a pipe, which is how an MCP client starts the server) nothing is changed.
//! On other systems this does nothing.
//!
//! The same module holds the other small pieces of Win32 code (the only unsafe code of the program): a message box for the case that no
//! graphics backend works, and a handler that writes one line to the log when the program dies of an unhandled exception, so that a
//! crash inside a graphics driver leaves a trace (the exception code, the address and the module).

#[cfg(not(windows))]
pub fn attach() {}

#[cfg(windows)]
#[allow(unsafe_code)]
pub fn attach() {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_INPUT: u32 = -10i32 as u32;
    const STD_OUTPUT: u32 = -11i32 as u32;
    const STD_ERROR: u32 = -12i32 as u32;
    extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, h: Handle) -> i32;
        fn AttachConsole(pid: u32) -> i32;
    }
    let invalid = usize::MAX as Handle;
    let missing = |h: Handle| h.is_null() || h == invalid;
    // SAFETY: plain Win32 calls with valid arguments; the handles come from the opened console files, which stay open for the life of the process.
    unsafe {
        let (i, o, e) = (
            GetStdHandle(STD_INPUT),
            GetStdHandle(STD_OUTPUT),
            GetStdHandle(STD_ERROR),
        );
        if !(missing(i) || missing(o) || missing(e)) {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        use std::os::windows::io::IntoRawHandle;
        let open = |name: &str, write: bool| {
            std::fs::OpenOptions::new()
                .read(!write)
                .write(write)
                .open(name)
                .map(|f| f.into_raw_handle() as Handle)
        };
        if missing(i) {
            if let Ok(h) = open("CONIN$", false) {
                SetStdHandle(STD_INPUT, h);
            }
        }
        if missing(o) {
            if let Ok(h) = open("CONOUT$", true) {
                SetStdHandle(STD_OUTPUT, h);
            }
        }
        if missing(e) {
            if let Ok(h) = open("CONOUT$", true) {
                SetStdHandle(STD_ERROR, h);
            }
        }
    }
}

/// A plain system message box. Elsewhere: the first of `zenity`, `kdialog`, `xmessage` that exists, else standard error.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn message_box(title: &str, text: &str) {
    use std::ffi::c_void;
    extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (t, c) = (wide(text), wide(title));
    const MB_OK_ICONERROR: u32 = 0x10;
    // SAFETY: both strings are NUL-terminated UTF-16 and live until the call returns; no window handle.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_OK_ICONERROR);
    }
}

#[cfg(not(windows))]
pub fn message_box(title: &str, text: &str) {
    use std::process::{Command, Stdio};
    let tries: [(&str, Vec<&str>); 3] = [
        (
            "zenity",
            vec!["--error", "--no-markup", "--title", title, "--text", text],
        ),
        ("kdialog", vec!["--error", text, "--title", title]),
        ("xmessage", vec!["-center", text]),
    ];
    for (prog, args) in tries {
        if let Ok(mut c) = Command::new(prog)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            // a dialog program that could not show its window (no display) exits with an error: try the next way
            if c.wait().is_ok_and(|st| st.success()) {
                return;
            }
        }
    }
    eprintln!("{title}\n{text}");
}

/// Installs the handler that logs an unhandled exception (code, address, module) to `log` before the program dies.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn install_crash_log(log: std::path::PathBuf) {
    use std::ffi::c_void;
    use std::io::Write;
    use std::sync::OnceLock;
    static LOG: OnceLock<std::path::PathBuf> = OnceLock::new();
    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        next: *mut ExceptionRecord,
        address: *mut c_void,
        number_parameters: u32,
        information: [usize; 15],
    }
    #[repr(C)]
    struct ExceptionPointers {
        record: *const ExceptionRecord,
        context: *mut c_void,
    }
    extern "system" {
        fn SetUnhandledExceptionFilter(
            f: Option<unsafe extern "system" fn(*const ExceptionPointers) -> i32>,
        ) -> *mut c_void;
        fn GetModuleHandleExW(flags: u32, address: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, size: u32) -> u32;
    }
    unsafe extern "system" fn handler(p: *const ExceptionPointers) -> i32 {
        const CONTINUE_SEARCH: i32 = 0;
        if p.is_null() || (*p).record.is_null() {
            return CONTINUE_SEARCH;
        }
        let r = &*(*p).record;
        // FROM_ADDRESS | UNCHANGED_REFCOUNT
        let mut module: *mut c_void = std::ptr::null_mut();
        let mut name = String::from("?");
        if GetModuleHandleExW(0x4 | 0x2, r.address, &mut module) != 0 {
            let mut buf = [0u16; 260];
            let n = GetModuleFileNameW(module, buf.as_mut_ptr(), 260) as usize;
            let full = String::from_utf16_lossy(&buf[..n.min(260)]);
            name = full.rsplit(['\\', '/']).next().unwrap_or("?").to_string();
        }
        if let Some(path) = LOG.get() {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(
                    f,
                    "[{secs}] UNHANDLED EXCEPTION {:#010x} at address {:#x} in module {name}",
                    r.code, r.address as usize
                );
                let _ = f.sync_all();
            }
        }
        CONTINUE_SEARCH // the system still shows its own crash report
    }
    let _ = LOG.set(log);
    // SAFETY: registers a handler with the signature the system expects; it only reads the exception record it is given.
    unsafe {
        SetUnhandledExceptionFilter(Some(handler));
    }
}

#[cfg(not(windows))]
pub fn install_crash_log(_log: std::path::PathBuf) {}

/// Internal test hook of the start-up safeguard: a real access violation (Windows only; elsewhere the caller aborts).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn test_access_violation() {
    // SAFETY: deliberately invalid; this is what the test hook is for.
    unsafe {
        std::ptr::write_volatile(8 as *mut u32, 1);
    }
}

#[cfg(not(windows))]
pub fn test_access_violation() {}
