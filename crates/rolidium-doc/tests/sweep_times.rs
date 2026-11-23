// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! How long a circle of 1 mm swept along a helix takes for 20 to 300 turns (the thread of a rod M8, pitch 1.25 mm, as the report on the limits asks), with the
//! faces of the body and the display mesh. Slow: `cargo test --release -p rolidium-doc --test sweep_times -- --ignored --nocapture`.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{DVec3, HelixDef, Path, PathSource};
use std::time::Instant;

fn doc_for(turns: f64) -> (Document, Command) {
    let mut d = Document::new();
    let h = HelixDef {
        origin: DVec3::ZERO,
        axis: DVec3::Z,
        x_dir: DVec3::X,
        radius: 4.0,
        pitch: 1.25,
        turns,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    };
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
                radius: 0.5,
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
#[ignore = "slow: seconds to a minute for the long helices"]
fn the_time_of_a_circle_of_1_mm_along_a_helix_for_20_to_300_turns() {
    println!("turns | sweep ms | faces | display mesh ms | triangles | memory MB");
    for turns in [20.0, 50.0, 100.0, 200.0, 300.0] {
        let (mut d, cmd) = doc_for(turns);
        let t = Instant::now();
        let r = d.apply(cmd);
        let sweep = t.elapsed().as_secs_f64() * 1e3;
        match r {
            Ok(_) => {
                let b = &d.bodies()[0];
                let t = Instant::now();
                let m = b.solid.display_mesh().unwrap();
                let mesh = t.elapsed().as_secs_f64() * 1e3;
                let rss = std::fs::read_to_string("/proc/self/statm")
                    .ok()
                    .and_then(|s| s.split_whitespace().nth(1).and_then(|p| p.parse::<u64>().ok()))
                    .unwrap_or(0)
                    * 4096
                    / (1024 * 1024);
                println!(
                    "{turns:5} | {sweep:8.0} | {:5} | {mesh:8.0} | {:9} | {rss}",
                    b.solid.face_count(),
                    m.triangle_count()
                );
            }
            Err(e) => println!("{turns:5} | {sweep:8.0} | failed: {e}"),
        }
    }
}
