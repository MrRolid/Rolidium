// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A cancelled mesh is never taken for a finished one. The cancel flag is global and the next job clears it; a mesher that was stopped half way
//! and then looked at the cleared flag used to hand back a mesh with missing faces (a body of 0 triangles after "Cancelled: the model is unchanged").
//! A test binary of its own: the flag is global.
use glam::DVec3;
use rolidium_kernel::*;

fn thread() -> Solid {
    let half = ((20.0_f64 - 10.0) / 30.0).atan().to_degrees();
    let h = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 20.0,
        pitch: 3.0,
        turns: 10.0,
        right: true,
        start_deg: 0.0,
        cone_deg: -half,
    };
    let path = Path::build(PathSource::Helix(h)).unwrap();
    let t = path.start_tangent;
    let disc = Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: 0.5,
        }]],
    };
    let input = LoftInput::Profile {
        profile: &disc,
        origin: path.start,
        normal: t,
        x_dir: t.cross(DVec3::Z).normalize(),
    };
    Solid::sweep(&input, &path, &SweepOptions::default()).unwrap().solid
}

#[test]
fn a_mesh_that_was_stopped_is_an_error_even_when_the_flag_is_cleared_at_once() {
    reset_cancel();
    let s = thread();
    let n = s.face_count();
    let (mut stopped, mut finished) = (0, 0);
    for delay_ms in [1u64, 3, 8, 15, 30, 60, 120, 250] {
        let solid = s.clone();
        let job = std::thread::spawn(move || solid.display_mesh());
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        request_cancel();
        // the next job clears the flag soon, before the mesher has finished its way out
        std::thread::sleep(std::time::Duration::from_millis(60));
        reset_cancel();
        match job.join().unwrap() {
            Err(KernelError::Cancelled) => stopped += 1,
            Err(e) => panic!("{e}"),
            Ok(m) => {
                finished += 1;
                let area = m.area_per_face(n);
                for (i, f) in s.faces().enumerate() {
                    let exact = s.face_info(f).unwrap().area;
                    assert!(
                        area[i] >= 0.9 * exact,
                        "delay {delay_ms} ms: face {i} has {:.0} % of its area",
                        100.0 * area[i] / exact
                    );
                }
            }
        }
    }
    eprintln!("{stopped} meshes stopped, {finished} finished");
    reset_cancel();
    assert!(stopped > 0, "at least one mesh was stopped");
    assert!(s.display_mesh().is_ok(), "the next mesh works");
}
