// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A heavy sweep along a helix that is cancelled leaves the document exactly as it was (bodies, curves, sketches, the file that is saved), and a helix
//! with 1000 turns on a cone is refused by the limits instead of being calculated. A test binary of its own: the cancel flag is global.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{request_cancel, reset_cancel, DVec3, HelixDef, KernelError, Path, PathSource};
use std::time::{Duration, Instant};

fn helix(turns: f64, pitch: f64, radius: f64, cone_deg: f64) -> HelixDef {
    HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius,
        pitch,
        turns,
        right: true,
        start_deg: 0.0,
        cone_deg,
    }
}

/// The document of a heavy sweep that has not been calculated yet: a cylinder, a helix of 250 turns and a small circle across its start.
fn heavy_doc() -> (Document, Command) {
    let mut d = Document::new();
    let h = helix(250.0, 0.3, 5.0, 0.0);
    let curve = d
        .apply(Command::AddCurve {
            def: CurveDef::Helix(h),
        })
        .unwrap()
        .created_curve
        .unwrap();
    let path = Path::build(PathSource::Helix(h)).unwrap();
    let x = path.start_tangent.cross(DVec3::Z).normalize();
    let sk = d
        .apply(Command::AddEntity {
            target: SketchTarget::New {
                plane: PlaneDef {
                    origin: path.start,
                    normal: path.start_tangent,
                    x_dir: x,
                },
                body: None,
            },
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: 0.1,
            },
        })
        .unwrap()
        .created_sketch
        .unwrap();
    let cmd = Command::AddLink {
        kind: LinkKind::Sweep {
            profile: LoftRef::Region {
                sketch: sk,
                region: 0,
            },
            path: SweepPath::Curve(curve),
            rounded: false,
            mode: PatternMode::New,
        },
        sources: Vec::new(),
        name: String::new(),
    };
    (d, cmd)
}

#[test]
fn a_cancelled_heavy_sweep_leaves_the_document_as_it_was() {
    reset_cancel();
    let (d, cmd) = heavy_doc();
    let before_bytes = file::save_to_bytes(&d, None, "t").unwrap();
    let before_state = d.state().clone();
    let (bodies, curves, sketches) = (d.bodies().len(), d.curves().len(), d.sketches().len());
    let job = d.plan(cmd);
    let started = Instant::now();
    let worker = std::thread::spawn(move || job.run().map(|_| ()));
    std::thread::sleep(Duration::from_millis(400));
    request_cancel();
    let r = worker.join().unwrap();
    let took = started.elapsed();
    reset_cancel();
    eprintln!(
        "the sweep stopped after {took:?}: {:?}",
        r.as_ref().err().map(|e| e.to_string())
    );
    match r {
        Err(DocError::Kernel(KernelError::Cancelled)) => {}
        Err(e) => panic!("another error: {e}"),
        Ok(()) => panic!("the sweep was not heavy enough to be cancelled ({took:?})"),
    }
    // the document is the very same one
    assert!(std::sync::Arc::ptr_eq(&before_state, d.state()));
    assert_eq!(
        (d.bodies().len(), d.curves().len(), d.sketches().len()),
        (bodies, curves, sketches)
    );
    assert_eq!(
        file::save_to_bytes(&d, None, "t").unwrap(),
        before_bytes,
        "the saved file is byte for byte the same"
    );
    // and it still works
    let mut d = d;
    d.apply(Command::AddBox {
        name: "b".into(),
        min: DVec3::ZERO,
        max: DVec3::splat(5.0),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 1);
}

#[test]
fn a_helix_of_1000_turns_on_a_cone_is_refused_not_calculated() {
    let mut d = Document::new();
    let half = -(20.0f64 / 30.0).atan().to_degrees();
    let started = Instant::now();
    let e = d
        .apply(Command::AddCurve {
            def: CurveDef::Helix(helix(1000.0, 3.0, 20.0, half)),
        })
        .expect_err("refused");
    eprintln!("{e}");
    assert!(started.elapsed() < Duration::from_secs(2), "refused at once");
    assert!(d.curves().is_empty());
    // within the cone it is accepted, past the apex it is not (110 turns of 3 mm would be 330 mm on a cone of 30 mm)
    assert!(d
        .apply(Command::AddCurve {
            def: CurveDef::Helix(helix(8.0, 3.0, 20.0, half)),
        })
        .is_ok());
    assert!(d
        .apply(Command::AddCurve {
            def: CurveDef::Helix(helix(110.0, 3.0, 20.0, half)),
        })
        .is_err());
    assert_eq!(d.curves().len(), 1);
}
