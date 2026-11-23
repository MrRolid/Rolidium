// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! How long a circle of 1 mm swept along a helix takes for 20 to 300 turns (the thread of a rod M8, pitch 1.25 mm, as the report on the limits asks), with the
//! faces of the body and the display mesh. Slow: `cargo test --release -p rolidium-doc --test sweep_times -- --ignored --nocapture`.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{DVec3, HelixDef, Path, PathSource};
use std::time::Instant;

fn doc_for(turns: f64) -> (Document, Command) {
    doc_for_wire(turns, 0.5)
}

fn doc_for_wire(turns: f64, wire: f64) -> (Document, Command) {
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
                radius: wire,
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

/// The base mesh of a thin wire: triangles and time for 10, 40 and 100 turns (the base mesh follows the size on the screen, not the radius of the wire).
#[test]
#[ignore = "slow: a few seconds"]
fn the_base_mesh_of_a_thin_wire_for_10_40_100_turns() {
    println!("wire radius | turns | faces | triangles | display mesh ms");
    for wire in [0.15, 0.3, 0.5] {
        for turns in [10.0, 40.0, 100.0] {
            let (mut d, cmd) = doc_for_wire(turns, wire);
            d.apply(cmd).unwrap();
            let b = &d.bodies()[0];
            let t = Instant::now();
            let m = b.solid.display_mesh().unwrap();
            println!(
                "{wire:5} | {turns:5} | {:5} | {:9} | {:8.0}",
                b.solid.face_count(),
                m.triangle_count(),
                t.elapsed().as_secs_f64() * 1e3
            );
        }
    }
}

/// Time of the sweep with its display mesh against the estimate that decides the question "is it announced" (thin wires on helices of 4 mm radius).
#[test]
#[ignore = "slow: about a minute"]
fn the_estimate_against_the_measured_time_for_thin_wires() {
    println!("wire radius | turns | estimate s | measured s (sweep + display mesh) | triangles");
    for wire in [0.15, 0.3, 0.5, 1.0] {
        for turns in [40.0, 80.0, 300.0] {
            let (mut d, cmd) = doc_for_wire(turns, wire);
            let t = Instant::now();
            let r = d.apply(cmd);
            if let Err(e) = r {
                println!("{wire:5} | {turns:5} | failed: {e}");
                continue;
            }
            let b = &d.bodies()[0];
            let m = b.solid.display_mesh().unwrap();
            let secs = t.elapsed().as_secs_f64();
            let len = std::f64::consts::TAU * 4.0 * turns;
            println!(
                "{wire:5} | {turns:5} | {:6.1} | {secs:6.1} | {}",
                rolidium_kernel::estimated_sweep_seconds(len, wire),
                m.triangle_count()
            );
        }
    }
}
