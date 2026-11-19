// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The example scripts in `examples/` are run (without `save` and `render`) and their volume is checked against the
//! number in their header, so they cannot go stale. Header lines: `# volume: <mm3>` and `# bodies: <count>`.

use rolidium_cli::Runner;
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn header(text: &str, key: &str) -> Option<f64> {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("# {key}:")))
        .and_then(|v| v.trim().parse().ok())
}

#[test]
fn every_example_builds_a_valid_part_with_the_documented_volume() {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    scripts.sort();
    assert!(
        scripts.len() >= 5,
        "expected at least five examples, found {}",
        scripts.len()
    );
    for path in scripts {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        let expected = header(&text, "volume").unwrap_or_else(|| panic!("{name}: no '# volume:' header"));
        let bodies =
            header(&text, "bodies").unwrap_or_else(|| panic!("{name}: no '# bodies:' header")) as usize;
        // the pictures and models they write must exist in the repository
        for line in text.lines() {
            let l = line.trim();
            if let Some(out) = l
                .strip_prefix("render ")
                .and_then(|r| r.split_whitespace().next())
            {
                assert!(
                    examples_dir().join("..").join(out).is_file(),
                    "{name}: {out} is missing, run examples/build-all.sh"
                );
            }
        }
        let script: String = text
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("render ") && !t.starts_with("save ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut r = Runner::new();
        r.run_script(&script).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(r.doc.bodies().len(), bodies, "{name}: number of bodies");
        assert!(
            r.doc.bodies().iter().all(|b| b.solid.is_valid()),
            "{name}: a body is not a valid solid"
        );
        let volume: f64 = r.doc.bodies().iter().map(|b| b.solid.volume().unwrap()).sum();
        assert!(
            (volume - expected).abs() < 0.05,
            "{name}: volume {volume} but the header says {expected}"
        );
    }
}
