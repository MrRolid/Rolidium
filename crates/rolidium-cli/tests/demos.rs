// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The demo models in `examples/demos/NN-name/`: every `build.txt` is built (without `save`, `render` and the other files it writes) and
//! checked against the header of the script:
//!
//! * `# bodies: <count>` and `# volume: <mm3>` (the sum over all bodies),
//! * `# no-overlap: yes` for an assembly: no two bodies share volume (touching with a face is fine),
//! * `# connected: yes` (assemblies): every body touches another one with a face (an edge or a point does not count) and the whole assembly is
//!   one connected piece. Contacts that are not there on purpose are written in the script: `# link: <a> <b> <reason>` (bodies a and b count as
//!   connected, for example a nut that sits in its holder with 0.1 mm clearance) and `# exempt: <a,b,c...> <reason>` (bodies that float, for example
//!   the balls of a bearing),
//! * every body is a valid closed solid,
//! * the model survives a STEP export and import with the same volume, and a save to a `.rolidium` file and an open.
//!
//! The quick demos run in `cargo test`; the slow ones (`# slow: yes` in the header) are in a test marked `#[ignore]`, run with
//! `cargo test -p rolidium-cli --test demos -- --ignored`.

use rolidium_cli::Runner;
use rolidium_kernel::{BooleanOp, KernelError, Solid};
use std::path::{Path, PathBuf};

fn demos_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/demos")
}

fn header(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("# {key}:")))
        .map(|v| v.trim().to_string())
}

fn demo_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(demos_dir())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("build.txt").is_file())
        .collect();
    v.sort();
    v
}

/// The script without the pictures. The `save` / `export-*` lines stay: they write into `target/demos/` (paths relative to the
/// repository root, where the test runs), which is also where a demo finds the parts of the demos it `# needs:`.
fn script_of(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("render "))
        .collect::<Vec<_>>()
        .join("\n")
}

static BUILT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Builds the demos in the `# needs:` header first (each at most once per run), so that their files exist.
fn build_needs(text: &str) {
    for n in header(text, "needs").unwrap_or_default().split_whitespace() {
        let dir = demo_dirs()
            .into_iter()
            .find(|d| d.file_name().unwrap().to_string_lossy().starts_with(n))
            .unwrap_or_else(|| panic!("no demo {n}"));
        check_demo(&dir);
    }
}

fn overlapping_pairs(solids: &[Solid], names: &[String]) -> Vec<(String, String, f64)> {
    let boxes: Vec<_> = solids.iter().map(|s| s.props().unwrap()).collect();
    let mut out = Vec::new();
    for i in 0..solids.len() {
        for j in i + 1..solids.len() {
            let (a, b) = (&boxes[i], &boxes[j]);
            let eps = 1e-6;
            let apart = (0..3).any(|k| a.max[k] <= b.min[k] + eps || b.max[k] <= a.min[k] + eps);
            if apart {
                continue;
            }
            match solids[i].boolean(&solids[j], BooleanOp::Common) {
                Ok(r) => {
                    let v = r.solid.volume().unwrap_or(0.0);
                    if v > 1e-6 {
                        out.push((
                            format!("{} {}", i + 1, names[i]),
                            format!("{} {}", j + 1, names[j]),
                            v,
                        ));
                    }
                }
                // nothing in common (they only touch, or the boxes overlap but the shapes do not)
                Err(KernelError::InvalidResult) => {}
                // the common part is in several pieces: the bodies overlap in more than one place
                Err(KernelError::SplitResult) => out.push((
                    format!("{} {}", i + 1, names[i]),
                    format!("{} {}", j + 1, names[j]),
                    -1.0,
                )),
                Err(e) => panic!("common of bodies {i} and {j}: {e}"),
            }
        }
    }
    out
}

/// Two bodies touch with a face when their fusion is one solid (an edge or a point gives two pieces, overlapping bodies do not occur here).
fn touch_by_face(a: &Solid, b: &Solid) -> bool {
    if !a.may_be_within(b, 1e-6) {
        return false;
    }
    match a.boolean(b, BooleanOp::Fuse) {
        Ok(_) => true,
        Err(KernelError::SplitResult) => false,
        Err(e) => panic!("fusing two bodies to see whether they touch: {e}"),
    }
}

/// The parts of the assembly that are not connected to each other (empty when it is one piece), as lists of body names.
fn loose_groups(
    solids: &[Solid],
    names: &[String],
    links: &[(usize, usize)],
    exempt: &[usize],
) -> Vec<Vec<String>> {
    let n = solids.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        if p[i] != i {
            let r = find(p, p[i]);
            p[i] = r;
        }
        p[i]
    }
    for &(a, b) in links {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        parent[ra] = rb;
    }
    for i in 0..n {
        for j in i + 1..n {
            if exempt.contains(&i) || exempt.contains(&j) {
                continue;
            }
            if touch_by_face(&solids[i], &solids[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<String>> = Default::default();
    for (i, nm) in names.iter().enumerate().take(n) {
        if exempt.contains(&i) {
            continue;
        }
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(format!("{} {}", i + 1, nm));
    }
    if groups.len() <= 1 {
        return Vec::new();
    }
    groups.into_values().collect()
}

fn check_demo(dir: &Path) {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    if BUILT.lock().unwrap().contains(&name) {
        return;
    }
    let text = std::fs::read_to_string(dir.join("build.txt")).unwrap();
    build_needs(&text);
    // the scripts are written for the repository root (`target/demos/...`)
    std::env::set_current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    std::fs::create_dir_all(format!("target/demos/{name}")).unwrap();
    let bodies: usize = header(&text, "bodies")
        .unwrap_or_else(|| panic!("{name}: no '# bodies:' header"))
        .parse()
        .unwrap();
    let volume: f64 = header(&text, "volume")
        .unwrap_or_else(|| panic!("{name}: no '# volume:' header"))
        .parse()
        .unwrap();
    let mut r = Runner::new();
    r.quiet = true;
    // scripts may read parts made by the demos before them: they are looked up from the demo folder's `out/` (see build-all.sh)
    let t = std::time::Instant::now();
    r.run_script(&script_of(&text))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let build_s = t.elapsed().as_secs_f64();
    let solids: Vec<Solid> = r.doc.bodies().iter().map(|b| b.solid.clone()).collect();
    assert_eq!(solids.len(), bodies, "{name}: number of bodies");
    assert!(
        solids.iter().all(|s| s.is_valid()),
        "{name}: a body is not a valid solid"
    );
    let total: f64 = solids.iter().map(|s| s.volume().unwrap()).sum();
    assert!(
        (total - volume).abs() <= volume * 1e-4 + 0.05,
        "{name}: volume {total} but the header says {volume}"
    );
    if header(&text, "no-overlap").as_deref() == Some("yes") {
        let names: Vec<String> = r.doc.bodies().iter().map(|b| b.name.clone()).collect();
        let pairs = overlapping_pairs(&solids, &names);
        assert!(
            pairs.is_empty(),
            "{name}: bodies share volume ({} pairs): {:?}",
            pairs.len(),
            &pairs[..pairs.len().min(12)]
        );
    }
    if header(&text, "connected").as_deref() == Some("yes") {
        let names: Vec<String> = r.doc.bodies().iter().map(|b| b.name.clone()).collect();
        let num = |t: &str| {
            t.parse::<usize>()
                .unwrap_or_else(|_| panic!("{name}: bad body number '{t}' in a link or exempt line"))
                - 1
        };
        let links: Vec<(usize, usize)> = text
            .lines()
            .filter_map(|l| l.strip_prefix("# link:"))
            .map(|l| {
                let mut w = l.split_whitespace();
                (num(w.next().unwrap()), num(w.next().unwrap()))
            })
            .collect();
        let exempt: Vec<usize> = text
            .lines()
            .filter_map(|l| l.strip_prefix("# exempt:"))
            .flat_map(|l| {
                l.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .split(',')
                    .filter(|t| !t.is_empty())
                    .map(num)
                    .collect::<Vec<_>>()
            })
            .collect();
        let loose = loose_groups(&solids, &names, &links, &exempt);
        let report: Vec<String> = loose
            .iter()
            .map(|g| {
                format!(
                    "[{} bodies: {}]",
                    g.len(),
                    g.iter().take(4).cloned().collect::<Vec<_>>().join("; ")
                )
            })
            .collect();
        assert!(
            loose.is_empty(),
            "{name}: the assembly is not one piece: {}",
            report.join(" ")
        );
        println!(
            "demo {name}: connected (links {}, exempt {})",
            links.len(),
            exempt.len()
        );
    }
    // STEP round trip: the same bodies come back with the same volume
    let step = Solid::write_step_many(&solids).unwrap();
    let (back, skipped) = Solid::read_step_all(&step).unwrap();
    assert!(skipped.is_empty(), "{name}: STEP: {skipped:?}");
    assert_eq!(back.len(), solids.len(), "{name}: STEP bodies");
    let vb: f64 = back.iter().map(|s| s.volume().unwrap()).sum();
    assert!(
        (vb - total).abs() <= total * 1e-5 + 1e-6,
        "{name}: STEP volume {vb} vs {total}"
    );
    // save and open
    let tmp = std::env::temp_dir().join(format!("rolidium-demo-{name}-{}.rolidium", std::process::id()));
    rolidium_doc::file::save_file(&r.doc, &tmp, None, "test").unwrap_or_else(|e| panic!("{name}: save: {e}"));
    let opened = rolidium_doc::file::load_file(&tmp).unwrap_or_else(|e| panic!("{name}: open: {e}"));
    let _ = std::fs::remove_file(&tmp);
    assert_eq!(
        opened.state.bodies.len(),
        solids.len(),
        "{name}: bodies after open"
    );
    BUILT.lock().unwrap().push(name.clone());
    println!(
        "demo {name}: {} bodies, volume {total:.3}, built in {build_s:.1} s",
        solids.len()
    );
}

fn is_slow(dir: &Path) -> bool {
    let text = std::fs::read_to_string(dir.join("build.txt")).unwrap();
    header(&text, "slow").as_deref() == Some("yes")
}

/// The checks change the working directory, so they must not run at the same time.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn the_quick_demos_build_and_match_their_headers() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dirs = demo_dirs();
    assert!(!dirs.is_empty(), "no demos found");
    for d in dirs.iter().filter(|d| !is_slow(d)) {
        check_demo(d);
    }
}

#[test]
#[ignore = "slow: the big demos, run with --ignored"]
fn the_slow_demos_build_and_match_their_headers() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for d in demo_dirs().iter().filter(|d| is_slow(d)) {
        check_demo(d);
    }
}
