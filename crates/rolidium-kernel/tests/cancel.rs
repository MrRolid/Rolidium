// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Cancelling a long operation. This is a test binary of its own, with one test: the cancel flag is global, so it must not
//! run beside other tests that use the kernel.

use rolidium_kernel::{
    request_cancel, reset_cancel, BooleanOp, DVec3, HelixDef, KernelError, LoftInput, Path, PathSource,
    Profile, ProfileSegment, Solid, SweepOptions,
};
use std::time::{Duration, Instant};

/// A thread M10 x 1.5 groove swept along a helix, and the cylinder it is cut from (the slow step is the cut).
fn thread_parts() -> (Solid, Solid) {
    let cylinder = Solid::cylinder(5.0, 20.0, DVec3::ZERO, DVec3::Z).unwrap();
    let helix = HelixDef {
        origin: DVec3::new(0.0, 0.0, -1.5),
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 5.0,
        pitch: 1.5,
        turns: 23.0 / 1.5,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    };
    let path = Path::build(PathSource::Helix(helix)).unwrap();
    let v = |x: f64, y: f64| [x, y];
    let profile = Profile {
        loops: vec![vec![
            ProfileSegment::Line {
                a: v(4.08, -1.5),
                b: v(5.3, -0.796),
            },
            ProfileSegment::Line {
                a: v(5.3, -0.796),
                b: v(5.3, -2.204),
            },
            ProfileSegment::Line {
                a: v(5.3, -2.204),
                b: v(4.08, -1.5),
            },
        ]],
    };
    // the profile lies in the XZ plane at the start of the helix
    let input = LoftInput::Profile {
        profile: &profile,
        origin: DVec3::ZERO,
        normal: DVec3::Y,
        x_dir: DVec3::X,
    };
    let tool = Solid::sweep(&input, &path, &SweepOptions::default())
        .unwrap()
        .solid;
    (cylinder, tool)
}

#[test]
fn a_long_cut_can_be_cancelled_and_the_same_cut_still_gives_the_same_result() {
    reset_cancel();
    let (cylinder, tool) = thread_parts();
    // 1. not cancelled: the reference result
    let t = Instant::now();
    let reference = cylinder
        .boolean(&tool, BooleanOp::Cut)
        .unwrap()
        .solid
        .volume()
        .unwrap();
    let full = t.elapsed();
    assert!(reference > 1300.0 && reference < 1570.0, "{reference}");
    // 2. cancelled after a short time: stops soon, with the cancel error, nothing returned
    let canceller = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(150));
        request_cancel();
    });
    let t = Instant::now();
    let r = cylinder.boolean(&tool, BooleanOp::Cut);
    let stopped = t.elapsed();
    canceller.join().unwrap();
    eprintln!("full cut {full:?}, cancelled after {stopped:?}");
    reset_cancel();
    if full > Duration::from_millis(500) {
        assert!(
            matches!(r, Err(KernelError::Cancelled)),
            "the cut should have been cancelled (it takes {full:?}), got {:?}",
            r.map(|_| ())
        );
        assert!(
            stopped < Duration::from_millis(1000) + Duration::from_millis(150),
            "stopping took {stopped:?} after the request"
        );
    }
    // 3. the same cut again: the same result as before
    let again = cylinder
        .boolean(&tool, BooleanOp::Cut)
        .unwrap()
        .solid
        .volume()
        .unwrap();
    assert!((again - reference).abs() < 1e-9, "{again} vs {reference}");
}
