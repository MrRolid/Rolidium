// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A helix stays on its cone (it never passes the apex) and has a sane number of turns.
use rolidium_kernel::*;

fn cone_helix(height: f64, pitch: f64) -> HelixDef {
    // a cone of 20 mm radius that narrows to a point 30 mm up (half angle atan(20/30))
    HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 20.0,
        pitch,
        turns: height / pitch,
        right: true,
        start_deg: 0.0,
        cone_deg: -(20.0f64 / 30.0).atan().to_degrees(),
    }
}

#[test]
fn a_helix_on_a_cone_may_not_pass_the_apex() {
    // inside the cone: fine, and the radius at the end is what the cone has there
    let ok = cone_helix(24.0, 2.0);
    let p = Path::build(PathSource::Helix(ok)).expect("inside the cone");
    let r_end = (p.end.x * p.end.x + p.end.y * p.end.y).sqrt();
    assert!((r_end - ok.end_radius()).abs() < 1e-3, "{r_end} vs {}", ok.end_radius());
    assert!(r_end > 3.0);
    // past the apex (110 turns of 2 mm = 220 mm on a cone 30 mm high): refused with the largest height
    let e = Path::build(PathSource::Helix(cone_helix(220.0, 2.0))).err().expect("past the apex");
    let m = e.to_string();
    eprintln!("{m}");
    assert!(m.contains("apex") && m.contains("largest height"), "{m}");
    // exactly the apex height is refused too (the radius would reach zero)
    assert!(Path::build(PathSource::Helix(cone_helix(30.0, 2.0))).is_err());
    // the largest height the message names works
    let max = cone_helix(10.0, 2.0).apex_height().unwrap() * 0.99;
    assert!(Path::build(PathSource::Helix(cone_helix(max - 0.01, 2.0))).is_ok());
    // a cone that widens, and a cylinder, have no apex
    let mut wide = cone_helix(100.0, 2.0);
    wide.cone_deg = 20.0;
    assert!(Path::build(PathSource::Helix(wide)).is_ok());
    let mut cyl = cone_helix(100.0, 2.0);
    cyl.cone_deg = 0.0;
    assert!(Path::build(PathSource::Helix(cyl)).is_ok());
}

#[test]
fn a_helix_with_too_many_turns_is_refused() {
    let mut h = cone_helix(10.0, 1.0);
    h.cone_deg = 0.0;
    h.turns = HELIX_MAX_TURNS;
    assert!(Path::build(PathSource::Helix(h)).is_ok());
    h.turns = HELIX_MAX_TURNS + 1.0;
    let e = Path::build(PathSource::Helix(h)).err().expect("too many");
    assert!(e.to_string().contains("too large"), "{e}");
    h.turns = 1000.0;
    assert!(Path::build(PathSource::Helix(h)).is_err());
}
