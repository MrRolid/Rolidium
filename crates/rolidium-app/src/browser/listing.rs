// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Reading a folder, sorting and filtering its entries, path helpers. Nothing here touches the UI.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

/// The message shown when a folder cannot be read, in plain words.
pub fn friendly_error(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        PermissionDenied => "You do not have permission to open this folder.".into(),
        NotFound => "This folder does not exist. A disk may have been removed.".into(),
        NotADirectory => "This is not a folder.".into(),
        _ => format!("Cannot read this folder: {e}"),
    }
}

/// All entries of a folder (hidden ones included; `visible` filters). Runs on a worker thread.
pub fn read_entries(dir: &Path) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| friendly_error(&e))?;
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(mut md) = e.metadata() else {
            out.push(Entry {
                name,
                is_dir: false,
                size: 0,
                modified: None,
            });
            continue;
        };
        if md.file_type().is_symlink() {
            if let Ok(target) = std::fs::metadata(e.path()) {
                md = target;
            }
        }
        out.push(Entry {
            name,
            is_dir: md.is_dir(),
            size: if md.is_dir() { 0 } else { md.len() },
            modified: md.modified().ok(),
        });
    }
    Ok(out)
}

/// Sub-folders only, sorted by name, for the folder tree.
pub fn read_subdirs(dir: &Path, hidden: bool) -> Result<Vec<String>, String> {
    let mut v: Vec<String> = read_entries(dir)?
        .into_iter()
        .filter(|e| e.is_dir && (hidden || !e.name.starts_with('.')))
        .map(|e| e.name)
        .collect();
    v.sort_by_key(|n| n.to_lowercase());
    Ok(v)
}

pub fn has_extension(name: &str, exts: &[String]) -> bool {
    Path::new(name)
        .extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| exts.iter().any(|y| x.eq_ignore_ascii_case(y)))
}

/// Indexes of the entries to show: folders always, files only with one of `exts` (`None` shows all files).
pub fn visible(entries: &[Entry], exts: Option<&[String]>, hidden: bool) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            (hidden || !e.name.starts_with('.'))
                && (e.is_dir || exts.is_none_or(|x| has_extension(&e.name, x)))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Folders first, then by the key; a click on the same header again reverses it.
pub fn sort(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by(|a, b| {
        use std::cmp::Ordering;
        let by_name = || {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.name.cmp(&b.name))
        };
        let ord = match key {
            SortKey::Name => by_name(),
            SortKey::Size => {
                if a.is_dir && b.is_dir {
                    by_name()
                } else {
                    a.size.cmp(&b.size).then_with(by_name)
                }
            }
            SortKey::Modified => a.modified.cmp(&b.modified).then_with(by_name),
        };
        let ord: Ordering = if ascending { ord } else { ord.reverse() };
        b.is_dir.cmp(&a.is_dir).then(ord)
    });
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

/// `2026-11-18 21:45` from seconds since 1970 and a zone offset in seconds.
#[cfg(test)]
pub fn format_time(unix: i64, offset: i32) -> String {
    let Ok(ts) = jiff::Timestamp::from_second(unix) else {
        return String::new();
    };
    let Ok(zone) = jiff::tz::Offset::from_seconds(offset) else {
        return String::new();
    };
    ts.to_zoned(jiff::tz::TimeZone::fixed(zone))
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

/// The modification time in the local time zone.
pub fn format_modified(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let Ok(ts) = jiff::Timestamp::try_from(t) else {
        return String::new();
    };
    ts.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

/// Clickable pieces of a path: label and the folder it leads to.
pub fn breadcrumbs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut acc = PathBuf::new();
    for c in dir.components() {
        acc.push(c.as_os_str());
        match c {
            std::path::Component::RootDir if matches!(out.last(), Some((_, p)) if p.parent().is_none() && !p.has_root()) =>
            {
                // Windows: "C:" and the root belong together, the crumb leads to "C:\"
                if let Some(last) = out.last_mut() {
                    last.1 = acc.clone();
                }
            }
            std::path::Component::RootDir => out.push(("/".to_string(), acc.clone())),
            other => out.push((other.as_os_str().to_string_lossy().into_owned(), acc.clone())),
        }
    }
    out
}

/// Folders (with a trailing separator) that complete what was typed; `input` may be a partial last name.
pub fn complete(input: &str, hidden: bool) -> Vec<String> {
    let (parent, prefix) = match input.rfind(['/', '\\']) {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => return Vec::new(),
    };
    let Ok(rd) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let low = prefix.to_lowercase();
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.to_lowercase().starts_with(&low) && (hidden || prefix.starts_with('.') || !n.starts_with('.'))
        })
        .collect();
    v.sort_by_key(|n| n.to_lowercase());
    v.truncate(12);
    let sep = if input.contains('\\') && !input.contains('/') {
        '\\'
    } else {
        '/'
    };
    v.into_iter().map(|n| format!("{parent}{n}{sep}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn e(name: &str, dir: bool, size: u64, t: u64) -> Entry {
        Entry {
            name: name.into(),
            is_dir: dir,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(t)),
        }
    }

    fn sample() -> Vec<Entry> {
        vec![
            e("b.rolidium", false, 300, 10),
            e("Zeta", true, 0, 5),
            e("a.step", false, 100, 30),
            e("alpha", true, 0, 50),
            e(".hidden", false, 1, 1),
            e("c.txt", false, 200, 20),
        ]
    }

    fn names(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|x| x.name.as_str()).collect()
    }

    #[test]
    fn sorting_keeps_folders_first_and_reverses() {
        let mut v = sample();
        sort(&mut v, SortKey::Name, true);
        assert_eq!(
            names(&v),
            ["alpha", "Zeta", ".hidden", "a.step", "b.rolidium", "c.txt"]
        );
        sort(&mut v, SortKey::Name, false);
        assert_eq!(
            names(&v),
            ["Zeta", "alpha", "c.txt", "b.rolidium", "a.step", ".hidden"]
        );
        sort(&mut v, SortKey::Size, true);
        assert_eq!(
            names(&v),
            ["alpha", "Zeta", ".hidden", "a.step", "c.txt", "b.rolidium"]
        );
        sort(&mut v, SortKey::Modified, false);
        assert_eq!(
            names(&v),
            ["alpha", "Zeta", "a.step", "c.txt", "b.rolidium", ".hidden"]
        );
    }

    #[test]
    fn filter_by_extension_and_hidden() {
        let v = sample();
        let exts = vec!["rolidium".to_string(), "step".to_string()];
        let shown: Vec<&str> = visible(&v, Some(&exts), false)
            .into_iter()
            .map(|i| v[i].name.as_str())
            .collect();
        assert_eq!(shown, ["b.rolidium", "Zeta", "a.step", "alpha"]);
        assert_eq!(visible(&v, None, false).len(), 5);
        assert_eq!(visible(&v, None, true).len(), 6);
        assert!(
            has_extension("X.STEP", &exts)
                && !has_extension("x.stl", &exts)
                && !has_extension("noext", &exts)
        );
    }

    #[test]
    fn time_and_size_formatting() {
        assert_eq!(format_time(0, 0), "1970-01-01 00:00");
        assert_eq!(format_time(1_700_000_000, 0), "2023-11-14 22:13");
        assert_eq!(format_time(1_700_000_000, 3600), "2023-11-14 23:13");
        assert_eq!(format_time(951_782_400, 0), "2000-02-29 00:00");
        assert_eq!(human_size(10), "10 B");
        assert_eq!(human_size(1536), "1.5 KB");
    }

    #[test]
    fn breadcrumbs_and_unreadable_folders() {
        let b = breadcrumbs(Path::new("/home/u/x"));
        assert_eq!(
            b.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            ["/", "home", "u", "x"]
        );
        assert_eq!(b[2].1, Path::new("/home/u"));
        let err = read_entries(Path::new("/definitely/not/here")).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn permission_denied_is_explained() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("rolidium-noaccess-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = read_entries(&dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        // root can read everything: only check the message when access was really denied
        if let Err(e) = r {
            assert!(e.contains("permission"), "{e}");
        }
    }

    #[test]
    fn completion_lists_matching_folders() {
        let base = std::env::temp_dir().join(format!("rolidium-compl-{}", std::process::id()));
        for d in ["alpha", "Alps", "beta"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        std::fs::write(base.join("alone.txt"), "x").unwrap();
        let input = format!("{}/al", base.display());
        let c = complete(&input, false);
        assert_eq!(
            c,
            vec![
                format!("{}/alpha/", base.display()),
                format!("{}/Alps/", base.display())
            ]
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
