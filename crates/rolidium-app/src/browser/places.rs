// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The "Places" of the file dialog: home, standard folders, mounted disks, network volumes, bookmarks of the
//! file manager and recently used folders. The parsing is separate from reading the system so it can be tested.

#![cfg_attr(windows, allow(dead_code))]

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Root,
    Drive,
    Network,
    Bookmark,
    Recent,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Place {
    pub name: String,
    pub path: PathBuf,
    pub kind: PlaceKind,
}

/// One line of `/proc/self/mounts`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mount {
    pub device: String,
    pub point: String,
    pub fstype: String,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `%20` style escapes of a `file://` URI.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Bookmarks as written by GTK file managers (`file:///path Name` per line). Other URI schemes are skipped.
pub fn parse_bookmarks(text: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("file://") else {
            continue;
        };
        let (uri, label) = match rest.split_once(' ') {
            Some((u, l)) => (u, l.trim()),
            None => (rest, ""),
        };
        let path = PathBuf::from(percent_decode(uri));
        let name = if label.is_empty() {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        } else {
            label.to_string()
        };
        out.push((name, path));
    }
    out
}

/// `~/.config/user-dirs.dirs`: `XDG_DOCUMENTS_DIR="$HOME/Documents"` and so on.
pub fn parse_user_dirs(text: &str, home: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Some(key) = k.trim().strip_prefix("XDG_").and_then(|k| k.strip_suffix("_DIR")) else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        let path = if let Some(rest) = v.strip_prefix("$HOME") {
            home.join(rest.trim_start_matches('/'))
        } else {
            PathBuf::from(v)
        };
        out.push((key.to_string(), path));
    }
    out
}

/// `/proc/self/mounts` escapes spaces and such as `\040`.
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 4 <= b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 4])
                .ok()
                .and_then(|o| u8::from_str_radix(o, 8).ok())
            {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let device = unescape_mount(f.next()?);
            let point = unescape_mount(f.next()?);
            let fstype = f.next()?.to_string();
            Some(Mount {
                device,
                point,
                fstype,
            })
        })
        .collect()
}

const NETWORK_FS: [&str; 10] = [
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "fuse.sshfs",
    "davfs",
    "fuse.rclone",
    "afs",
    "ceph",
];

/// Mount points that belong to the system and are not shown as disks.
const SYSTEM_POINTS: [&str; 13] = [
    "/boot",
    "/snap",
    "/var",
    "/usr",
    "/etc",
    "/opt",
    "/sys",
    "/proc",
    "/dev",
    "/tmp",
    "/srv",
    "/home",
    "/run/user",
];

/// Whether a mount is a network volume, a disk the user attached (USB drive, another partition) or none of them.
pub fn classify(m: &Mount) -> Option<PlaceKind> {
    if NETWORK_FS.contains(&m.fstype.as_str()) {
        return Some(PlaceKind::Network);
    }
    if m.fstype == "fuse.gvfsd-fuse" {
        return Some(PlaceKind::Network);
    }
    if m.point == "/" || !m.device.starts_with("/dev/") || m.device.starts_with("/dev/loop") {
        return None;
    }
    if m.fstype == "squashfs" || m.fstype == "swap" {
        return None;
    }
    let removable =
        m.point.starts_with("/media/") || m.point.starts_with("/run/media/") || m.point.starts_with("/mnt/");
    if !removable
        && SYSTEM_POINTS
            .iter()
            .any(|p| m.point == *p || m.point.starts_with(&format!("{p}/")))
    {
        return None;
    }
    Some(PlaceKind::Drive)
}

fn mount_name(m: &Mount) -> String {
    let base = Path::new(&m.point)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !base.is_empty() {
        return base;
    }
    m.device.clone()
}

/// The places for mounted volumes. `gvfs` mount points are expanded by `collect` (each network share is a folder in them).
pub fn mount_places(mounts: &[Mount]) -> Vec<Place> {
    mounts
        .iter()
        .filter_map(|m| {
            let kind = classify(m)?;
            if m.fstype == "fuse.gvfsd-fuse" {
                return None;
            }
            Some(Place {
                name: mount_name(m),
                path: PathBuf::from(&m.point),
                kind,
            })
        })
        .collect()
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Drive letters of Windows: `exists("C:\\")` tells which are present.
pub fn windows_drives(exists: impl Fn(&str) -> bool) -> Vec<Place> {
    ('A'..='Z')
        .filter_map(|c| {
            let p = format!("{c}:\\");
            exists(&p).then(|| Place {
                name: format!("{c}:"),
                path: PathBuf::from(p),
                kind: PlaceKind::Drive,
            })
        })
        .collect()
}

/// A gvfs folder name like `smb-share:server=nas,share=photos` as something readable.
pub fn gvfs_name(folder: &str) -> String {
    let mut server = "";
    let mut share = "";
    for part in folder.split_once(':').map_or("", |x| x.1).split(',') {
        if let Some(v) = part.strip_prefix("server=") {
            server = v;
        } else if let Some(v) = part.strip_prefix("share=") {
            share = v;
        } else if let Some(v) = part.strip_prefix("host=") {
            server = v;
        }
    }
    match (share.is_empty(), server.is_empty()) {
        (false, false) => format!("{share} ({server})"),
        (true, false) => server.to_string(),
        _ => folder.to_string(),
    }
}

/// Everything the dialog lists on the left, in this order: home and standard folders, root, disks,
/// network volumes, bookmarks, recent folders. Entries that do not exist are left out (recent ones are kept).
pub fn collect(recent: &[String]) -> Vec<Place> {
    fn add_to(out: &mut Vec<Place>, p: Place) {
        if !out.iter().any(|x| x.path == p.path && x.kind == p.kind) {
            out.push(p);
        }
    }
    let mut out: Vec<Place> = Vec::new();
    let home = home_dir();
    if let Some(h) = &home {
        add_to(
            &mut out,
            Place {
                name: "Home".into(),
                path: h.clone(),
                kind: PlaceKind::Home,
            },
        );
        let dirs = std::fs::read_to_string(h.join(".config/user-dirs.dirs")).unwrap_or_default();
        let xdg = parse_user_dirs(&dirs, h);
        for (key, kind, name, fallback) in [
            ("DESKTOP", PlaceKind::Desktop, "Desktop", "Desktop"),
            ("DOCUMENTS", PlaceKind::Documents, "Documents", "Documents"),
            ("DOWNLOAD", PlaceKind::Downloads, "Downloads", "Downloads"),
        ] {
            let p = xdg
                .iter()
                .find(|(k, _)| k == key)
                .map_or_else(|| h.join(fallback), |x| x.1.clone());
            if p.is_dir() && p != *h {
                add_to(
                    &mut out,
                    Place {
                        name: name.into(),
                        path: p,
                        kind,
                    },
                );
            }
        }
    }
    #[cfg(windows)]
    {
        for d in windows_drives(|p| Path::new(p).exists()) {
            add_to(&mut out, d);
        }
    }
    #[cfg(not(windows))]
    {
        add_to(
            &mut out,
            Place {
                name: "File system".into(),
                path: PathBuf::from("/"),
                kind: PlaceKind::Root,
            },
        );
        let mounts = std::fs::read_to_string("/proc/self/mounts")
            .map(|t| parse_mounts(&t))
            .unwrap_or_default();
        for p in mount_places(&mounts) {
            add_to(&mut out, p);
        }
        for m in mounts.iter().filter(|m| m.fstype == "fuse.gvfsd-fuse") {
            if let Ok(rd) = std::fs::read_dir(&m.point) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().into_owned();
                    add_to(
                        &mut out,
                        Place {
                            name: gvfs_name(&n),
                            path: e.path(),
                            kind: PlaceKind::Network,
                        },
                    );
                }
            }
        }
        // the standard location for media mounted by the file manager, in case /proc/self/mounts is unreadable
        if let Some(user) = std::env::var_os("USER") {
            let dir = Path::new("/run/media").join(user);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    add_to(
                        &mut out,
                        Place {
                            name: e.file_name().to_string_lossy().into_owned(),
                            path: e.path(),
                            kind: PlaceKind::Drive,
                        },
                    );
                }
            }
        }
    }
    if let Some(h) = &home {
        for f in [".config/gtk-3.0/bookmarks", ".gtk-bookmarks"] {
            if let Ok(t) = std::fs::read_to_string(h.join(f)) {
                for (name, path) in parse_bookmarks(&t) {
                    if path.is_dir()
                        && !out
                            .iter()
                            .any(|x| x.path == path && x.kind != PlaceKind::Bookmark)
                    {
                        add_to(
                            &mut out,
                            Place {
                                name,
                                path,
                                kind: PlaceKind::Bookmark,
                            },
                        );
                    }
                }
                break;
            }
        }
    }
    for r in recent {
        let path = PathBuf::from(r);
        let name = path
            .file_name()
            .map_or_else(|| r.clone(), |n| n.to_string_lossy().into_owned());
        add_to(
            &mut out,
            Place {
                name,
                path,
                kind: PlaceKind::Recent,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
sysfs /sys sysfs rw 0 0
/dev/nvme0n1p2 / ext4 rw 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
/dev/loop3 /snap/core/123 squashfs ro 0 0
tmpfs /run tmpfs rw 0 0
/dev/sdb1 /media/user/USB\\040STICK vfat rw 0 0
/dev/sda3 /mnt/data ext4 rw 0 0
/dev/sdc1 /run/media/user/BACKUP exfat rw 0 0
//nas/photos /mnt/photos cifs rw 0 0
server:/export /home/user/nfs nfs4 rw 0 0
gvfsd-fuse /run/user/1000/gvfs fuse.gvfsd-fuse rw 0 0
";

    #[test]
    fn mounted_volumes_are_recognised() {
        let places = mount_places(&parse_mounts(MOUNTS));
        let names: Vec<(&str, PlaceKind)> = places.iter().map(|p| (p.name.as_str(), p.kind)).collect();
        assert_eq!(
            names,
            vec![
                ("USB STICK", PlaceKind::Drive),
                ("data", PlaceKind::Drive),
                ("BACKUP", PlaceKind::Drive),
                ("photos", PlaceKind::Network),
                ("nfs", PlaceKind::Network),
            ]
        );
        assert_eq!(places[0].path, Path::new("/media/user/USB STICK"));
    }

    #[test]
    fn bookmarks_and_user_dirs() {
        let b = parse_bookmarks(
            "file:///home/u/Projects\nfile:///mnt/My%20Disk Disk\nsftp://host/x Remote\n# comment\n",
        );
        assert_eq!(
            b,
            vec![
                ("Projects".into(), PathBuf::from("/home/u/Projects")),
                ("Disk".into(), PathBuf::from("/mnt/My Disk"))
            ]
        );
        let d = parse_user_dirs(
            "# c\nXDG_DESKTOP_DIR=\"$HOME/Plocha\"\nXDG_DOWNLOAD_DIR=\"/data/dl\"\nFOO=1\n",
            Path::new("/home/u"),
        );
        assert_eq!(
            d,
            vec![
                ("DESKTOP".into(), PathBuf::from("/home/u/Plocha")),
                ("DOWNLOAD".into(), PathBuf::from("/data/dl"))
            ]
        );
    }

    #[test]
    fn windows_letters_and_gvfs_names() {
        let d = windows_drives(|p| p == "C:\\" || p == "E:\\");
        assert_eq!(
            d.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["C:", "E:"]
        );
        assert_eq!(gvfs_name("smb-share:server=nas,share=photos"), "photos (nas)");
        assert_eq!(gvfs_name("sftp:host=example.org"), "example.org");
    }
}
