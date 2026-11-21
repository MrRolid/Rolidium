// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Facts about the Windows computer for the summary of the diagnostics, read with system calls and the registry: no other program is started
//! (so no console window can flash up) and nothing takes longer than a fraction of a second. Every function returns `None` when the
//! fact cannot be found. This is the only place of Win32 code besides `winconsole.rs`.
#![allow(unsafe_code)]

use std::ffi::c_void;

#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page: u64,
    avail_page: u64,
    total_virt: u64,
    avail_virt: u64,
    avail_ext: u64,
}

/// SYSTEM_LOGICAL_PROCESSOR_INFORMATION: the mask, the kind of the record (0 = a processor core) and a union of 16 bytes.
#[repr(C)]
struct ProcessorInfo {
    mask: usize,
    relationship: i32,
    union: [u64; 2],
}

#[repr(C)]
struct OsVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    csd: [u16; 128],
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    fn GetLogicalProcessorInformation(buffer: *mut ProcessorInfo, length: *mut u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: isize,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn RtlGetVersion(info: *mut OsVersionInfo) -> i32;
}

/// The physical memory in megabytes.
pub fn memory_mb() -> Option<u64> {
    let mut s = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page: 0,
        avail_page: 0,
        total_virt: 0,
        avail_virt: 0,
        avail_ext: 0,
    };
    // SAFETY: `s` is a valid structure with its size set, as the call requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut s) };
    (ok != 0 && s.total_phys > 0).then_some(s.total_phys / 1024 / 1024)
}

/// The number of physical cores (the records of the kind "processor core").
pub fn physical_cores() -> Option<usize> {
    let mut len: u32 = 0;
    // SAFETY: the first call only asks for the size of the buffer.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len) };
    let size = std::mem::size_of::<ProcessorInfo>() as u32;
    if len == 0 || !len.is_multiple_of(size) || len > 1 << 20 {
        return None;
    }
    let mut buf: Vec<ProcessorInfo> = (0..len / size)
        .map(|_| ProcessorInfo {
            mask: 0,
            relationship: -1,
            union: [0; 2],
        })
        .collect();
    // SAFETY: the buffer has room for `len` bytes, as the call requires.
    let ok = unsafe { GetLogicalProcessorInformation(buf.as_mut_ptr(), &mut len) };
    if ok == 0 {
        return None;
    }
    let n = buf.iter().filter(|r| r.relationship == 0).count();
    (n > 0).then_some(n)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A text value of `HKEY_LOCAL_MACHINE`.
pub fn registry_text(sub_key: &str, value: &str) -> Option<String> {
    const HKLM: isize = 0x8000_0002_u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x2;
    let (k, v) = (wide(sub_key), wide(value));
    let mut size: u32 = 0;
    // SAFETY: the strings end with a zero; the first call only asks for the size.
    let r = unsafe {
        RegGetValueW(
            HKLM,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if r != 0 || !(2..=4096).contains(&size) {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: the buffer has room for `size` bytes.
    let r = unsafe {
        RegGetValueW(
            HKLM,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if r != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The model of the processor, from the registry.
pub fn cpu_model() -> Option<String> {
    registry_text(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
}

/// The version of Windows: major, minor and build number.
pub fn version() -> Option<(u32, u32, u32)> {
    let mut i = OsVersionInfo {
        size: std::mem::size_of::<OsVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd: [0; 128],
    };
    // SAFETY: `i` is a valid structure with its size set.
    let status = unsafe { RtlGetVersion(&mut i) };
    (status >= 0 && i.major > 0).then_some((i.major, i.minor, i.build))
}
