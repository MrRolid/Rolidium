// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! STL has no units: Rolidium writes it in millimetres, always. The script `examples/scale-check/cube-20.txt` makes a cube of 20 mm and exports it to
//! STL (binary and ASCII); the files are read back with a parser of their own and the bounding box of the triangles must be 20 x 20 x 20 mm to 0.001 mm.
//! The expected and the measured box are always printed (run with `--nocapture`, or use `scripts/stl-scale-check.sh`), so a wrong scale is seen at once.
use rolidium_cli::stl_scale::{read_stl_bbox, run_cube_check};

#[test]
fn a_cube_of_20_mm_is_20_mm_in_the_stl_file() {
    let dir = std::env::temp_dir().join(format!("rolidium-stl-scale-{}", std::process::id()));
    let (report, ok) = run_cube_check(&dir).unwrap();
    println!("{report}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "the STL file has the wrong size:\n{report}");
}

#[test]
fn the_parser_reads_both_kinds_of_file_and_a_wrong_size_is_seen() {
    // a hand-made ASCII file: one triangle 1 x 2 x 3 mm
    let text = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 1 2 3\nendloop\nendfacet\nendsolid t\n";
    let (lo, hi, n) = read_stl_bbox(text.as_bytes());
    assert_eq!((lo, hi, n), ([0.0, 0.0, 0.0], [1.0, 2.0, 3.0], 1));
    // the same triangle in inches would measure 25.4 times less: such a file must not pass for 20 mm
    let mut bin = vec![0u8; 80];
    bin.extend_from_slice(&1u32.to_le_bytes());
    bin.extend_from_slice(&[0u8; 12]);
    for v in [
        [0.0f32, 0.0, 0.0],
        [20.0 / 25.4, 0.0, 0.0],
        [0.0, 20.0 / 25.4, 20.0 / 25.4],
    ] {
        for c in v {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    bin.extend_from_slice(&[0u8; 2]);
    let (lo, hi, n) = read_stl_bbox(&bin);
    assert_eq!(n, 1);
    assert!(
        (hi[0] - lo[0] - 20.0).abs() > 0.001,
        "an inch-sized file is detected"
    );
}
