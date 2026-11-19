// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The helix test matrix: springs (a circle swept along a helix) and threads (a trapezoid swept along a helix) combined with
//! a cylinder in every way. Every case must give the same volume of the common part from merge, cut and intersect, and that
//! volume must be sensible. `cargo test --release -p rolidium-kernel --test helix_matrix -- --ignored --nocapture` prints the
//! table; `ROLIDIUM_MATRIX=quick` runs a small subset. Both tests are ignored by default; they are the yardstick for the exact helix construction.

use rolidium_kernel::{
    BooleanOp, DVec3, HelixDef, LoftInput, Path, PathSource, Profile, ProfileSegment, Solid, SweepOptions,
};

#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub turns: f64,
    pub pitch: f64,
    pub wire: f64,
    pub right: bool,
    pub cone: f64,
    pub trapezoid: bool,
    /// The cylinder is shorter than the helix, so the ends of the spring stick out through its end faces.
    pub short: bool,
}

fn build(c: &Case, radius: f64) -> Option<Solid> {
    let helix = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius,
        pitch: c.pitch,
        turns: c.turns,
        right: c.right,
        start_deg: 0.0,
        cone_deg: c.cone,
    };
    let path = Path::build(PathSource::Helix(helix)).ok()?;
    let r = c.wire / 2.0;
    let seg = if c.trapezoid {
        // an ISO-like tooth: base c.wire wide on the cylinder (radius - r), tip 1/8 of it, height c.wire * 0.6
        let (b, t, h) = (c.wire, c.wire / 8.0, c.wire * 0.6);
        let x0 = radius - h * 0.3;
        let pts = [
            [x0, -b / 2.0],
            [x0 + h, -t / 2.0],
            [x0 + h, t / 2.0],
            [x0, b / 2.0],
        ];
        (0..4)
            .map(|i| ProfileSegment::Line {
                a: pts[i],
                b: pts[(i + 1) % 4],
            })
            .collect()
    } else {
        vec![ProfileSegment::Circle {
            center: [radius, 0.0],
            radius: r,
        }]
    };
    let profile = Profile { loops: vec![seg] };
    // the application draws on the XZ plane with the normal -Y, against the direction of the path
    let input = LoftInput::Profile {
        profile: &profile,
        origin: DVec3::ZERO,
        normal: DVec3::new(0.0, if c.right { -1.0 } else { 1.0 }, 0.0),
        x_dir: DVec3::X,
    };
    Solid::sweep(&input, &path, &SweepOptions::default())
        .ok()
        .map(|r| r.solid)
}

/// `Ok(volume of the common part)` or a description of what went wrong. Uses the raw booleans (no fallback ways).
pub fn run_case(c: &Case) -> Result<f64, String> {
    let radius = 10.0;
    let spring = build(c, radius).ok_or("sweep failed")?;
    let vs = spring.volume().map_err(|e| e.to_string())?;
    if !spring.is_valid() || vs <= 0.0 {
        return Err(format!("invalid solid (volume {vs})"));
    }
    let full = c.pitch * c.turns + c.wire + 2.0;
    let (height, z0) = if c.short {
        (full * 0.6, full * 0.15)
    } else {
        (full, -c.wire)
    };
    let cyl =
        Solid::cylinder(radius, height, DVec3::new(0.0, 0.0, z0), DVec3::Z).map_err(|e| e.to_string())?;
    let vc = cyl.volume().map_err(|e| e.to_string())?;
    let tol = 2e-4 * vs + 1e-3;
    let fuse = cyl
        .boolean(&spring, BooleanOp::Fuse)
        .map_err(|e| format!("merge: {e}"))?;
    let vf = fuse.solid.volume().map_err(|e| e.to_string())?;
    let pieces = cyl
        .boolean_pieces(&spring, BooleanOp::Cut)
        .map_err(|e| format!("cut: {e}"))?;
    let vcut: f64 = pieces.iter().filter_map(|p| p.volume().ok()).sum();
    let com = cyl
        .boolean(&spring, BooleanOp::Common)
        .map_err(|e| format!("intersect: {e}"))?;
    let vi = com.solid.volume().map_err(|e| e.to_string())?;
    let (x1, x2, x3) = (vc + vs - vf, vc - vcut, vi);
    if (x1 - x2).abs() > tol || (x2 - x3).abs() > tol {
        return Err(format!("merge {x1:.3} cut {x2:.3} common {x3:.3}"));
    }
    if x3 < -tol || x3 > vs.min(vc) + tol {
        return Err(format!(
            "common {x3:.3} impossible (spring {vs:.2}, cylinder {vc:.2})"
        ));
    }
    Ok(x3)
}

fn cases(quick: bool) -> Vec<Case> {
    let mut v = Vec::new();
    let turns: &[f64] = if quick {
        &[1.0, 3.0, 10.0]
    } else {
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 15.0, 20.0]
    };
    let shapes: &[(f64, f64)] = if quick {
        &[(6.0, 4.0), (12.0, 3.0)]
    } else {
        &[
            (3.0, 2.0),
            (6.0, 4.0),
            (6.0, 2.0),
            (9.0, 6.0),
            (12.0, 3.0),
            (5.0, 4.5),
        ]
    };
    for &t in turns {
        for &(pitch, wire) in shapes {
            for right in [true, false] {
                for cone in [0.0, 4.0] {
                    for trapezoid in [false, true] {
                        for short in [false, true] {
                            if quick && (cone != 0.0 || (trapezoid && !right)) {
                                continue;
                            }
                            v.push(Case {
                                turns: t,
                                pitch,
                                wire,
                                right,
                                cone,
                                trapezoid,
                                short,
                            });
                        }
                    }
                }
            }
        }
    }
    v
}

#[test]
#[ignore = "the full matrix takes minutes"]
fn the_helix_matrix() {
    let quick = std::env::var("ROLIDIUM_MATRIX").as_deref() == Ok("quick");
    let all = cases(quick);
    let n = all.len();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    let workers: usize = std::env::var("ROLIDIUM_MATRIX_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let r = run_case(&all[i]);
                results.lock().unwrap().push((i, r));
            });
        }
    });
    let mut res = results.into_inner().unwrap();
    res.sort_by_key(|(i, _)| *i);
    let mut ok = 0;
    for (i, r) in &res {
        match r {
            Ok(_) => ok += 1,
            Err(e) => println!("FAIL {:?}: {e}", all[*i]),
        }
    }
    println!("MATRIX {ok} / {n} passed");
    assert_eq!(ok, n, "{} cases failed", n - ok);
}

#[test]
#[ignore = "prepared for the exact helix construction"]
fn a_few_matrix_cases() {
    for c in [
        Case {
            turns: 10.0,
            pitch: 6.0,
            wire: 4.0,
            right: true,
            cone: 0.0,
            trapezoid: false,
            short: false,
        },
        Case {
            turns: 3.0,
            pitch: 6.0,
            wire: 4.0,
            right: false,
            cone: 0.0,
            trapezoid: false,
            short: false,
        },
        Case {
            turns: 5.0,
            pitch: 6.0,
            wire: 4.0,
            right: true,
            cone: 0.0,
            trapezoid: true,
            short: true,
        },
    ] {
        run_case(&c).unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }
}
