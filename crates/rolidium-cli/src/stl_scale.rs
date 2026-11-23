// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The scale check of the STL export: the script `examples/scale-check/cube-20.txt` makes a cube of 20 mm and exports it (binary and
//! ASCII); the files are read back with a parser written here for the purpose (not the STL import of the program) and the bounding box of
//! the triangles is measured. STL has no units, Rolidium writes millimetres, so the box must be 20 x 20 x 20 mm to 0.001 mm.

use crate::Runner;
use std::path::Path;

/// The script of the check (also in `examples/scale-check/`, CC0).
pub const SCRIPT: &str = include_str!("../../../examples/scale-check/cube-20.txt");
/// What the cube of the script must measure.
pub const EXPECTED_MIN: [f64; 3] = [0.0, 0.0, 0.0];
pub const EXPECTED_MAX: [f64; 3] = [20.0, 20.0, 20.0];
pub const TOLERANCE: f64 = 0.001;

/// Bounding box (min, max) and the number of triangles of an STL file, binary or ASCII.
pub fn read_stl_bbox(bytes: &[u8]) -> ([f64; 3], [f64; 3], usize) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    let mut add = |v: [f64; 3]| {
        for i in 0..3 {
            lo[i] = lo[i].min(v[i]);
            hi[i] = hi[i].max(v[i]);
        }
    };
    let mut triangles = 0;
    let binary = bytes.len() >= 84 && {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        bytes.len() == 84 + 50 * n
    };
    if binary {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        for t in 0..n {
            let at = 84 + 50 * t + 12; // after the normal
            for v in 0..3 {
                let f = |k: usize| {
                    let o = at + v * 12 + k * 4;
                    f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as f64
                };
                add([f(0), f(1), f(2)]);
            }
            triangles += 1;
        }
    } else {
        let text = String::from_utf8_lossy(bytes);
        for l in text.lines() {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w.len() == 4 && w[0] == "vertex" {
                if let (Ok(x), Ok(y), Ok(z)) = (w[1].parse(), w[2].parse(), w[3].parse()) {
                    add([x, y, z]);
                }
            }
            if w.first() == Some(&"endfacet") {
                triangles += 1;
            }
        }
    }
    (lo, hi, triangles)
}

fn triple(v: [f64; 3]) -> String {
    format!("({:.4}, {:.4}, {:.4})", v[0], v[1], v[2])
}

/// Runs the script in `dir`, measures both files and returns the report (always the expected and the measured box) and whether
/// every size is within the tolerance.
pub fn run_cube_check(dir: &Path) -> Result<(String, bool), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // the files of the script are written into `dir`
    let script: String = SCRIPT
        .lines()
        .map(|l| match l.strip_prefix("export-stl ") {
            Some(rest) => format!("export-stl {}/{rest}", dir.display()),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut r = Runner::new();
    r.run_script(&script).map_err(|e| format!("{e:?}"))?;
    let mut report = String::new();
    let mut ok = true;
    let expected_size = [
        EXPECTED_MAX[0] - EXPECTED_MIN[0],
        EXPECTED_MAX[1] - EXPECTED_MIN[1],
        EXPECTED_MAX[2] - EXPECTED_MIN[2],
    ];
    for (name, kind) in [("cube-20.stl", "binary"), ("cube-20-ascii.stl", "ASCII")] {
        let bytes = std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        let (lo, hi, n) = read_stl_bbox(&bytes);
        let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let diff = [
            (size[0] - expected_size[0]).abs(),
            (size[1] - expected_size[1]).abs(),
            (size[2] - expected_size[2]).abs(),
        ];
        let fine = n >= 12 && diff.iter().all(|d| *d <= TOLERANCE);
        ok &= fine;
        report.push_str(&format!(
            "STL scale check, {name} ({kind}, {n} triangles)\n  expected  min {}  max {}  size {}\n  measured  min {}  max {}  size {}\n  difference of the size {}  (tolerance {TOLERANCE} mm)  {}\n",
            triple(EXPECTED_MIN),
            triple(EXPECTED_MAX),
            triple(expected_size),
            triple(lo),
            triple(hi),
            triple(size),
            triple(diff),
            if fine { "OK" } else { "WRONG SCALE" },
        ));
    }
    Ok((report, ok))
}
