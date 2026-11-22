// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! STL has no units: Rolidium writes it in millimetres, always. A cube of 20 mm made with the command-line tool is exported to STL (binary and
//! ASCII), the file is read back and its bounding box must be 20 x 20 x 20 mm to 0.001 mm.
use rolidium_cli::Runner;

/// The bounding box of the triangles of an STL file (binary or ASCII), without the help of the program that wrote it.
fn read_stl_bbox(bytes: &[u8]) -> ([f64; 3], [f64; 3], usize) {
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
                add([w[1].parse().unwrap(), w[2].parse().unwrap(), w[3].parse().unwrap()]);
            }
            if w.first() == Some(&"endfacet") {
                triangles += 1;
            }
        }
    }
    (lo, hi, triangles)
}

#[test]
fn a_cube_of_20_mm_is_20_mm_in_the_stl_file() {
    let dir = std::env::temp_dir().join(format!("rolidium-stl-scale-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, ascii) in [("cube-bin.stl", false), ("cube-ascii.stl", true)] {
        let path = dir.join(name);
        let mut r = Runner::new();
        r.run_script(&format!(
            "box 20 20 20\nexport-stl {}{}\n",
            path.display(),
            if ascii { " --ascii" } else { "" }
        ))
        .unwrap();
        let (lo, hi, n) = read_stl_bbox(&std::fs::read(&path).unwrap());
        assert!(n >= 12, "{name}: {n} triangles");
        for i in 0..3 {
            let side = hi[i] - lo[i];
            assert!((side - 20.0).abs() <= 0.001, "{name}: side {i} is {side} mm, not 20");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
