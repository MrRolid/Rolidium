// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Translations. English is built in: the English text itself is the key, so a missing
//! translation simply shows the English text. Other languages are plain text files (one per
//! language) that can be edited without compiling:
//!
//! ```text
//! # comment
//! @name = Language name shown in Settings
//! English text => Translated text
//! Text with a value: {} mm => Translation with the value: {} mm
//! ```
//!
//! `\n` stands for a line break. A `{}` in the English text matches any value, so messages that
//! contain numbers can be translated as a whole (the values are copied over in order).
//! Files are looked for in `~/.config/rolidium/lang/`, then in `lang/` next to the program, and
//! the file shipped with the program is built in as a fallback; later sources override earlier.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

#[derive(Default, Clone, Debug)]
pub struct Catalog {
    pub name: String,
    exact: HashMap<String, String>,
    patterns: Vec<(Vec<String>, String)>,
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
}

impl Catalog {
    pub fn parse(text: &str) -> Catalog {
        let mut c = Catalog::default();
        c.merge(text);
        c
    }

    /// Adds the entries of a language file; existing entries are replaced.
    pub fn merge(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('@') {
                if let Some((k, v)) = rest.split_once('=') {
                    if k.trim() == "name" {
                        self.name = v.trim().to_string();
                    }
                }
                continue;
            }
            let Some((k, v)) = line.split_once(" => ") else {
                continue;
            };
            let (k, v) = (unescape(k.trim()), unescape(v.trim()));
            if v.is_empty() {
                continue;
            }
            if k.contains("{}") {
                let parts: Vec<String> = k.split("{}").map(str::to_string).collect();
                self.patterns.retain(|(p, _)| *p != parts);
                self.patterns.push((parts, v));
            } else {
                self.exact.insert(k, v);
            }
        }
    }

    #[cfg(test)]
    pub fn has(&self, key: &str) -> bool {
        self.exact.contains_key(key) || self.patterns.iter().any(|(p, _)| p.join("{}") == key)
    }

    pub fn translate(&self, text: &str) -> String {
        if let Some(t) = self.exact.get(text) {
            return t.clone();
        }
        for (parts, out) in &self.patterns {
            if let Some(vals) = match_pattern(parts, text) {
                let mut it = vals.into_iter();
                let mut res = String::new();
                for (i, piece) in out.split("{}").enumerate() {
                    if i > 0 {
                        res.push_str(&it.next().unwrap_or_default());
                    }
                    res.push_str(piece);
                }
                return res;
            }
        }
        text.to_string()
    }
}

/// Splits `text` along the literal `parts` of a pattern; the pieces between them are the values.
fn match_pattern(parts: &[String], text: &str) -> Option<Vec<String>> {
    let first = parts.first()?;
    let mut rest = text.strip_prefix(first.as_str())?;
    let mut vals = Vec::new();
    for (i, lit) in parts.iter().enumerate().skip(1) {
        let last = i + 1 == parts.len();
        if last {
            let v = rest.strip_suffix(lit.as_str())?;
            vals.push(v.to_string());
            rest = "";
        } else {
            let at = if lit.is_empty() {
                rest.len()
            } else {
                rest.find(lit.as_str())?
            };
            vals.push(rest[..at].to_string());
            rest = &rest[at + lit.len()..];
        }
    }
    rest.is_empty().then_some(vals)
}

struct State {
    code: String,
    catalog: Catalog,
}

fn state() -> &'static RwLock<State> {
    static S: OnceLock<RwLock<State>> = OnceLock::new();
    S.get_or_init(|| {
        RwLock::new(State {
            code: "en".into(),
            catalog: Catalog::default(),
        })
    })
}

/// Translates a built-in English text (or a message that matches a `{}` pattern).
pub fn tr(text: &str) -> String {
    let s = state().read().unwrap_or_else(|e| e.into_inner());
    if s.code == "en" {
        return text.to_string();
    }
    s.catalog.translate(text)
}

/// Translates a message that may be several sentences joined by "; " (a notice of an operation with several things to say): the whole text when it is known,
/// else each sentence by itself.
pub fn tr_joined(text: &str) -> String {
    let whole = tr(text);
    if whole != text || !text.contains("; ") {
        return whole;
    }
    text.split("; ").map(tr).collect::<Vec<_>>().join("; ")
}

/// Translates `text` and fills its `{}` placeholders with `args`, in order.
pub fn trf(text: &str, args: &[String]) -> String {
    let t = tr(text);
    let mut out = String::new();
    let mut it = args.iter();
    for (i, piece) in t.split("{}").enumerate() {
        if i > 0 {
            out.push_str(it.next().map_or("", |s| s.as_str()));
        }
        out.push_str(piece);
    }
    out
}

#[cfg(test)]
pub fn current_language() -> String {
    state().read().unwrap_or_else(|e| e.into_inner()).code.clone()
}

const EMBEDDED: &[(&str, &str)] = &[
    ("cs", include_str!("../../../assets/lang/cs.txt")),
    ("de", include_str!("../../../assets/lang/de.txt")),
    ("es", include_str!("../../../assets/lang/es.txt")),
    ("fr", include_str!("../../../assets/lang/fr.txt")),
    ("ja", include_str!("../../../assets/lang/ja.txt")),
];

fn lang_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lang")))
    {
        v.push(exe);
    }
    if let Some(b) = crate::dirs::config_base() {
        v.push(b.join("rolidium").join("lang"));
    }
    if let Some(d) = std::env::var_os("ROLIDIUM_LANG_DIR") {
        v.push(PathBuf::from(d));
    }
    v
}

/// Loads a language: built-in file first, then files next to the program, then the user's own.
pub fn load(code: &str) -> Option<Catalog> {
    let mut cat: Option<Catalog> = None;
    if let Some((_, text)) = EMBEDDED.iter().find(|(c, _)| *c == code) {
        cat = Some(Catalog::parse(text));
    }
    for d in lang_dirs() {
        if let Ok(text) = std::fs::read_to_string(d.join(format!("{code}.txt"))) {
            cat.get_or_insert_with(Catalog::default).merge(&text);
        }
    }
    cat
}

/// All languages that can be chosen: `(code, name)`, English first.
pub fn available() -> Vec<(String, String)> {
    let mut codes: Vec<String> = EMBEDDED.iter().map(|(c, _)| c.to_string()).collect();
    for d in lang_dirs() {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "txt") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        if !codes.iter().any(|c| c == stem) && stem != "en" {
                            codes.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }
    codes.sort();
    let mut out = vec![("en".to_string(), "English".to_string())];
    for c in codes {
        if let Some(cat) = load(&c) {
            let name = if cat.name.is_empty() {
                c.clone()
            } else {
                cat.name.clone()
            };
            out.push((c, name));
        }
    }
    out
}

/// The two-letter language code of a locale name such as `cs_CZ.UTF-8`, `cs-CZ` or `ja`.
pub fn language_of(locale: &str) -> Option<String> {
    let code: String = locale
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase();
    (!code.is_empty() && code != "c" && code != "posix").then_some(code)
}

/// Language of the system: the environment (`LC_ALL`, `LC_MESSAGES`, `LANG`), else the system locale (Windows, macOS),
/// else English.
pub fn system_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let code: String = v
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_lowercase();
            if !code.is_empty() && code != "c" && code != "posix" {
                return code;
            }
        }
    }
    sys_locale::get_locale()
        .and_then(|l| language_of(&l))
        .unwrap_or_else(|| "en".into())
}

/// Switches the language at once. An empty `setting` follows the system; an unknown language
/// falls back to English.
pub fn set_language(setting: &str) {
    let code = if setting.is_empty() {
        system_language()
    } else {
        setting.to_string()
    };
    let (code, catalog) = match load(&code) {
        Some(c) if code != "en" => (code, c),
        _ => ("en".to_string(), Catalog::default()),
    };
    *state().write().unwrap_or_else(|e| e.into_inner()) = State { code, catalog };
}

/// Parses a number typed by the user; both `.` and `,` are accepted as the decimal mark.
pub fn parse_num(s: &str) -> Option<f64> {
    let v: f64 = s.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_pattern_translation() {
        let c = Catalog::parse(
            "@name = Test\nPull => Tah\nPull {} mm done => Tah {} mm hotov\nLine\\nbreak => A\\nB\n",
        );
        assert_eq!(c.name, "Test");
        assert_eq!(c.translate("Pull"), "Tah");
        assert_eq!(c.translate("Pull +5.00 mm done"), "Tah +5.00 mm hotov");
        assert_eq!(
            c.translate("Unknown text"),
            "Unknown text",
            "missing text stays English"
        );
        assert_eq!(c.translate("Line\nbreak"), "A\nB");
    }

    #[test]
    fn patterns_with_several_values() {
        let c = Catalog::parse("The {} of {} mm does not fit => {} {} mm nesedi\n");
        assert_eq!(
            c.translate("The radius of 5 mm does not fit"),
            "radius 5 mm nesedi"
        );
        assert_eq!(
            c.translate("The radius of 5 does not fit"),
            "The radius of 5 does not fit"
        );
    }

    #[test]
    fn numbers_accept_point_and_comma() {
        assert_eq!(parse_num("12.5"), Some(12.5));
        assert_eq!(parse_num(" 12,5 "), Some(12.5));
        assert_eq!(parse_num("-3,25"), Some(-3.25));
        assert_eq!(parse_num("abc"), None);
        assert_eq!(parse_num("NaN"), None);
        assert_eq!(parse_num("inf"), None);
    }

    #[test]
    fn language_switch_without_restart() {
        // Uses a private directory so the test does not depend on installed files.
        let dir = std::env::temp_dir().join(format!("rolidium-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("xx.txt"), "@name = Xx\nPull => Zieh\n").unwrap();
        std::env::set_var("ROLIDIUM_LANG_DIR", &dir);
        set_language("xx");
        assert_eq!(tr("Pull"), "Zieh");
        assert_eq!(tr("Select"), "Select");
        assert!(available().iter().any(|(c, n)| c == "xx" && n == "Xx"));
        set_language("en");
        assert_eq!(tr("Pull"), "Pull");
        set_language("nonexistent");
        assert_eq!(current_language(), "en");
    }
}

#[cfg(test)]
mod coverage {
    use super::*;
    use std::path::{Path, PathBuf};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// `{size}` / `{distance:.1}` (a value named inside the text) becomes `{}`: that is how the message is translated.
    fn inline_values(s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            match after.find('}') {
                Some(j)
                    if after[..j].chars().all(|c| {
                        c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '+'
                    }) && !after[..j].is_empty() =>
                {
                    out.push_str("{}");
                    rest = &after[j + 1..];
                }
                _ => {
                    out.push('{');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Texts handed to `tr`/`trf`/`set_status`/`Invalid` as string literals, found in the sources.
    fn scan(text: &str, out: &mut Vec<String>) {
        let text = text.split("#[cfg(test)]").next().unwrap_or(text);
        let bytes = text.as_bytes();
        for name in ["tr", "trf", "set_status", "set_status_quiet", "Invalid", "fail"] {
            let pat = format!("{name}(");
            let mut from = 0;
            while let Some(i) = text[from..].find(&pat) {
                let at = from + i;
                from = at + pat.len();
                if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                    continue;
                }
                let rest = text[from..].trim_start();
                // `Invalid(format!("... {size} ..."))`: the message with its values in the place of the names
                let rest = rest.strip_prefix("format!(").map_or(rest, str::trim_start);
                if let Some(lit) = rest.strip_prefix('"') {
                    let mut s = String::new();
                    let mut chars = lit.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => match chars.next() {
                                Some('n') => s.push('\n'),
                                Some(o) => s.push(o),
                                None => break,
                            },
                            '"' => break,
                            c => s.push(c),
                        }
                    }
                    out.push(inline_values(&s));
                }
            }
        }
        // `#[error("...")]` messages without values
        let mut from = 0;
        while let Some(i) = text[from..].find("#[error(\"") {
            let start = from + i + 9;
            from = start;
            if let Some(end) = text[start..].find("\")]") {
                let m = &text[start..start + end];
                if !m.contains('{') {
                    out.push(m.to_string());
                }
            }
        }
    }

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// Texts that reach the screen through tables or `format!`, so the scan cannot see them.
    const EXTRA: &[&str] = &[
        "Pieces", "Round size {} → {} mm", "Diameter {} → {} mm", "the round is gone", "the hole is closed", "the pin is gone",
        "Organic model, it stays a mesh", "The round next to the wall could not be kept at the new place, so the wall was moved without it",
        "The pull went away from the body, so it was cut the other way", "{} edges where two parts of the mesh touched were separated",
        "the mesh had {} separate pieces: a solid was made of each", "Checking the mesh", "Converting a piece of the mesh", "Looking for flat and round faces",
        "Looking for flat faces and cylinders only", "Making the solid from the faces found", "Making the solid from flat faces and cylinders", "Making flat faces",
        "Making flat faces at a finer accuracy", "Making a face of every triangle",
        "Click the flat rectangular face the copies go into (Pick the face)",
        "Into a face needs a hole or a pin: click its round face first",
        "The hole would lie outside the body", "The pin would not touch the body", "The shift must be numbers",
        "Plane through the axis of the helix and the point", "Plane square to the curve", "Plane square to the axis of the helix",
        "Plane square to the edge", "Plane of the round edge", "Plane parallel to XY", "Plane parallel to XZ", "Plane parallel to YZ", "Free plane",
        "Mirror the body itself", "The body is mirrored where it is.", "Keep the original", "The mirror image is a new body.",
        "Merge into one body", "The image is fused with the original into one symmetric body (the body must touch the plane).",
        "Live mirror", "The mirror image follows when you change the original. A merged body cannot be live: an edit of one half could not be carried to the other reliably.",
        "Click the body to repeat or mirror", "Click the body the pieces are added to or cut out of",
        "Select the bodies to group first", "Group made", "Group dissolved", "Moved", "The link was removed",
        "There is nothing to cut: the bodies do not overlap",
        "The result of the operation does not add up (its volume is impossible for these bodies), so nothing was changed. Try moving one body by a hundredth of a millimetre, or combine the bodies in another order",
        "Select", "Pull", "Sketch", "Line", "Rectangle", "Circle", "Plan view", "3D mode", "Edges", "Round",
        "Chamfer", "Edit", "Undo", "Redo", "View", "Top", "Front", "Right", "Isometric", "Fit all", "File",
        "Export STL", "Settings", "Draft", "Standard", "Fine", "Custom", "Auto", "Add", "Cut", "Radius",
        "Distance", "Size", "Diameter", "Width", "Height", "Length", "Plane", "Cylinder", "Cone", "Sphere",
        "Torus", "Other", "quadrant", "on curve", "endpoint", "midpoint", "center", "face vertex", "on face edge", "grid",
        "Pull {} mm done", "Exported {} ({} triangles)", "Top view", "Front view", "Right view",
        "Measuring needs a body, a curve or a sketch", "Cylindrical face of {}", "Face of {}", "Distance", "Δ X", "Δ Y", "Δ Z", "Distance of the parallel faces", "Distance of the axes", "Angle", "Custom", "Mass", "Material changed", "The body no longer exists", "steel", "aluminium", "brass", "copper", "PLA", "PETG", "ABS", "Cancel", "Isometric view", "Edges ({})", "Pin", "Hole", "Hidden", "Shown", "Show", "Merge", "Intersect", "New body", "Ruled", "Smooth", "Name", "Modified", "Face", "Volume", "Surface area", "Size X", "Size Y", "Size Z", "Total area", "Area", "Diameter", "Radius", "Total length", "Length", "Choose the Line tool first", "Sweep needs a profile (a sketch region or a flat face) and a path", "The profile does not lie at the start of the path; the sweep was made anyway", "The profile is not perpendicular to the path where it starts; the sweep was made anyway", "Helix", "3D line", "Sweep", "Start a sketch first", "Nothing to pull yet: draw a sketch or open a model", "There is no body to move", "Combine needs at least two bodies", "Loft needs at least two profiles: sketch regions or flat faces", "Select an edge first", "There is nothing to undo", "There is nothing to redo", "There is nothing to export", "Wait until the running operation finishes", "More", "Light", "Dark", "Follow the system", "New", "The round could not be made cleanly, so the model was not changed. Try a smaller size or select the whole edge loop.", "The chamfer could not be made cleanly, so the model was not changed. Try a smaller size or select the whole edge loop.", "Distance of the sketch plane from where it started; positive is along its normal", "Not available while a shape is being drawn: finish it first", "Rolidium documents (*.rolidium)", "STEP files (*.step, *.stp)", "STL files (*.stl)", "You do not have permission to open this folder.", "This folder does not exist. A disk may have been removed.", "This is not a folder.", "Cannot read this folder: {}", "Home", "File system", "Desktop", "Documents", "Downloads", "The axis must lie in the plane of the profile",
        "The axis passes through the profile; it must be beside it or along its edge", "The axis has no direction",
        "There is nothing to remove here: the shape does not overlap the body", "Renamed", "Deleted (Ctrl+Z brings it back)", "vertex (3D)",
        "edge midpoint (3D)", "circle centre (3D)", "on edge (3D)", "intersection", "aligned", "extension", "parallel",
        "perpendicular", "symmetric", "Move", "Open", "Save As", "Import STEP", "Export STEP", "{} seconds ago", "{} minutes ago",
        "{} hours ago",
        "This is not a valid Rolidium file or it is damaged: {}. Nothing was changed.",
        "This file was saved by a newer Rolidium (file format {}; this program understands up to {}). It was not opened and not changed. Please update Rolidium.", "on a face", "on XY",
        "The radius of {} mm does not fit these edges (it is too large or the edges meet in a way that cannot be rounded)",
        "The chamfer of {} mm does not fit these edges (it is too large or the edges meet in a way that cannot be chamfered)",
        "The radius of {} mm does not fit here (it is too large for the neighbouring faces)",
        "The chamfer of {} mm does not fit here (it is too large for the neighbouring faces)",
        "The radius of {} mm does not fit these edges. The largest radius that works is about {} mm; a neighbouring face limits it.",
        "The chamfer of {} mm does not fit these edges. The largest chamfer that works is about {} mm; a neighbouring face limits it.",
        "The radius of {} mm does not fit these edges, and no radius works here: the edges meet in a way that cannot be rounded",
        "The chamfer of {} mm does not fit these edges, and no chamfer works here: the edges meet in a way that cannot be chamfered",
        "The chamfer of {} mm at {} degrees does not fit these edges. The largest distance that works is about {} mm (the second distance is then {} mm); a neighbouring face limits it. With the two sides swapped (Flip) up to about {} mm works.",
        "The chamfer of {} mm at {} degrees does not fit these edges. The largest distance that works is about {} mm (the second distance is then {} mm); a neighbouring face limits it.",
        "The chamfer of {} mm at {} degrees does not fit these edges. With the two sides swapped (Flip) up to about {} mm works.",
        "The chamfer of {} mm at {} degrees does not fit these edges, and no distance works here: the edges meet in a way that cannot be chamfered with two distances",
        "too large for the neighbouring faces, or the edges meet in a way that cannot be rounded or chamfered",
        "The new volume touches a body only along an edge or at a point, so it stays a separate body. Pull it down into the hole, or so that it shares a face with the body: then it joins",
        "There is no such edge",
        "These edges do not form a closed loop, so there is nothing to fill",
        "These edges do not lie in one plane, so they cannot be filled with a flat region",
        "Several edges meet here, so it is not clear which loop is meant. Select the edges of the loop one by one",
        "There is nothing to fill yet: draw lines or open a model",
        "There are no open lines to join yet: draw lines or a 3D line first",
        "The round or chamfer cannot be changed here: {}",
        "The helix would leave the face. The largest height is {} mm",
        "The rounds next to the new one could not be made together with it ({}); the new one was made alone",
        "The display mesh of the body could not be made, so the view was not changed",
        "The rounds next to the face cannot be moved with it: {}",
        "Internal error while computing the operation; the model was not changed ({})",
        // messages of the mesh library (STL) that reach the status line
        "A neighbouring wall is not square to this wall (a slope or a bevel), so it cannot be moved without bending the mesh. Tick “Move anyway, the neighbouring walls slant” in the panel to allow it",
        "The move would make the mesh cut through itself",
        "The move would turn a neighbouring wall inside out (the wall would be pulled through the opposite one)",
        "The mesh is not closed or not consistent: repair it first",
        "The diameter must be a positive number",
        "The hole is not round enough to change its diameter",
        "The file is not an STL file (no triangles were found)",
        "The file contains numbers that are not finite",
        // texts chosen by an `if` or a `match` inside `tr(...)`
        "Check the mesh",
        "STL and 3MF files (*.stl, *.3mf)",
        "Cylindrical surface",
        "The neighbouring wall is not square to this one: tick “Move anyway” in the panel",
        "Too far: the wall would go through the opposite one",
        "The mesh would cut through itself",
        "The mesh has holes or turned triangles: check and repair it first",
        "This wall cannot be pulled",
        "up",
        "down",
        "towards +X",
        "towards -X",
        "towards +Y",
        "towards -Y",
    ];

    fn all_keys() -> Vec<String> {
        let mut files = Vec::new();
        rs_files(&root().join("crates/rolidium-app/src"), &mut files);
        // every message of the kernel and of the document that can reach the user
        rs_files(&root().join("crates/rolidium-kernel/src"), &mut files);
        rs_files(&root().join("crates/rolidium-doc/src"), &mut files);
        let mut keys = Vec::new();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            if [
                "selftest.rs",
                "bench.rs",
                "i18n.rs",
                "assets_index.rs",
                "logging.rs",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            scan(&std::fs::read_to_string(&f).unwrap(), &mut keys);
        }
        // help cards
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root().join("assets/help/tools.json")).unwrap())
                .unwrap();
        for e in json.as_array().unwrap() {
            for k in ["title", "text"] {
                keys.push(e[k].as_str().unwrap().to_string());
            }
            for t in e["tips"].as_array().unwrap() {
                keys.push(t.as_str().unwrap().to_string());
            }
        }
        keys.extend(EXTRA.iter().map(|s| s.to_string()));
        keys.sort();
        keys.dedup();
        keys.retain(|k| !k.is_empty());
        keys
    }

    /// Lists every text that has no entry in the Czech file. Run `cargo test -p rolidium-app
    /// czech -- --nocapture` to see the list; the test fails while the list is not empty.
    #[test]
    fn czech_file_has_every_text() {
        let cs = Catalog::parse(include_str!("../../../assets/lang/cs.txt"));
        let missing: Vec<String> = all_keys().into_iter().filter(|k| !cs.has(k)).collect();
        if !missing.is_empty() {
            eprintln!("Texts missing in assets/lang/cs.txt ({}):", missing.len());
            for m in &missing {
                eprintln!("{m} => ");
            }
        }
        assert!(
            missing.is_empty(),
            "{} texts are missing in the Czech file",
            missing.len()
        );
    }

    /// The Czech file must not translate texts that no longer exist (it would silently rot).
    #[test]
    fn czech_file_has_no_unused_entries() {
        let keys = all_keys();
        let text = include_str!("../../../assets/lang/cs.txt");
        let unused: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with('@'))
            .filter_map(|l| l.split_once(" => ").map(|(k, _)| k.trim()))
            .filter(|k| !keys.iter().any(|x| x.replace('\n', "\\n") == *k || x == k))
            .collect();
        assert!(unused.is_empty(), "unused entries in the Czech file: {unused:?}");
    }

    /// Czech (any text with Czech letters) may live only in the translation file.
    #[test]
    fn czech_text_only_in_the_translation_file() {
        const CZECH: &str = "\u{e1}\u{10d}\u{10f}\u{e9}\u{11b}\u{ed}\u{148}\u{f3}\u{159}\u{161}\u{165}\u{fa}\u{16f}\u{fd}\u{17e}\u{c1}\u{10c}\u{10e}\u{c9}\u{11a}\u{cd}\u{147}\u{d3}\u{158}\u{160}\u{164}\u{da}\u{16e}\u{dd}\u{17d}"; // escaped so this file stays free of Czech letters
        fn walk(dir: &Path, bad: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if p.is_dir() {
                    if ["target", ".git", "dist", "lang"].contains(&name.as_str()) {
                        continue;
                    }
                    walk(&p, bad);
                } else if name != "Cargo.lock" {
                    if let Ok(t) = std::fs::read_to_string(&p) {
                        if t.chars().any(|c| CZECH.contains(c)) {
                            bad.push(p.display().to_string());
                        }
                    }
                }
            }
        }
        let mut bad = Vec::new();
        walk(&root(), &mut bad);
        assert!(bad.is_empty(), "Czech text outside assets/lang: {bad:?}");
    }

    /// There are six languages: English (the language of the source texts, so it needs no file) and the five of the files. The tests
    /// below go through `EMBEDDED`, so every language that is shipped is checked and a new one cannot be forgotten.
    #[test]
    fn there_are_six_languages_and_the_tests_cover_all_of_them() {
        let mut codes: Vec<&str> = EMBEDDED.iter().map(|(c, _)| *c).collect();
        codes.push("en");
        codes.sort_unstable();
        assert_eq!(codes, vec!["cs", "de", "en", "es", "fr", "ja"]);
        // English is the identity: a message is shown as it is when no translation is loaded
        set_language("en");
        assert_eq!(tr("Undo"), "Undo");
    }

    /// Every language file has exactly the keys of the others, with the same placeholders.
    #[test]
    fn all_language_files_have_the_same_keys_and_placeholders() {
        let files: Vec<(&str, &str)> = EMBEDDED.to_vec();
        let entries = |text: &str| -> Vec<(String, String)> {
            text.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with('@'))
                .filter_map(|l| l.split_once(" => "))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect()
        };
        let base = entries(files[0].1);
        assert!(base.len() > 400);
        for (code, text) in &files[1..] {
            let e = entries(text);
            let missing: Vec<&String> = base
                .iter()
                .map(|(k, _)| k)
                .filter(|k| !e.iter().any(|(x, _)| x == *k))
                .collect();
            let extra: Vec<&String> = e
                .iter()
                .map(|(k, _)| k)
                .filter(|k| !base.iter().any(|(x, _)| x == *k))
                .collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{code}: missing {missing:?}, extra {extra:?}"
            );
            for (k, v) in &e {
                assert_eq!(
                    k.matches("{}").count(),
                    v.matches("{}").count(),
                    "{code}: placeholders of '{k}'"
                );
                assert!(!v.is_empty(), "{code}: empty translation of '{k}'");
            }
            assert!(text.contains("@name"), "{code}: no language name");
        }
    }

    /// The messages that the operations really produce (with their values filled in) are translated in every language file:
    /// none of them may reach the user in English.
    #[test]
    fn real_operation_messages_are_translated_in_every_language() {
        use rolidium_doc::{Command, Document};
        use rolidium_kernel::DVec3;
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let body = d.bodies()[0].id;
        let s = d.body(body).unwrap().solid.clone();
        let edge = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.start.x < 1.0
                    && i.start.y < 1.0
            })
            .unwrap()
            .0;
        let mut messages: Vec<String> = Vec::new();
        for chamfer in [false, true] {
            messages.push(
                d.apply(Command::FilletEdges {
                    body,
                    edges: vec![edge],
                    size: 500.0,
                    chamfer,
                })
                .unwrap_err()
                .to_string(),
            );
        }
        messages.push(
            d.apply(Command::ChamferAngled {
                body,
                edges: vec![edge],
                distance: 500.0,
                angle_deg: 30.0,
                flip: false,
            })
            .unwrap_err()
            .to_string(),
        );
        messages.push(
            d.apply(Command::ChamferAngled {
                body,
                edges: vec![],
                distance: 5.0,
                angle_deg: 30.0,
                flip: false,
            })
            .unwrap_err()
            .to_string(),
        );
        for (code, text) in EMBEDDED.iter().copied() {
            let c = Catalog::parse(text);
            for m in &messages {
                assert_ne!(&c.translate(m), m, "{code}: not translated: {m}");
            }
        }
    }
}
