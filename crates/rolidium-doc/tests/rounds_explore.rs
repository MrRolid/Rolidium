use rolidium_doc::*;
use rolidium_kernel::{DVec3, EdgeId};

#[test]
fn explore() {
    for mode in ["together", "one by one"] {
        for n in [2usize, 3, 4, 12] {
            let mut d = Document::new();
            d.apply(Command::AddBox { name: "B".into(), min: DVec3::ZERO, max: DVec3::new(40.0, 30.0, 20.0) }).unwrap();
            let id = d.bodies()[0].id;
            let all: Vec<u32> = if n == 3 {
                let s = &d.body(id).unwrap().solid;
                s.edges().filter(|e| { let i = s.edge_info(*e).unwrap(); i.start.length() < 1e-6 || i.end.length() < 1e-6 }).map(|e| e.0).collect()
            } else { (0..n as u32).collect() };
            let mut ok = true;
            if mode == "together" {
                if let Err(e) = d.apply(Command::FilletEdges { body: id, edges: all.clone(), size: 3.0, chamfer: false }) { eprintln!("{mode} n={n} fillet: {e}"); ok = false; }
            } else {
                // the edge ids change after every operation; pick by midpoints
                let mids: Vec<(DVec3, DVec3)> = { let s = &d.body(id).unwrap().solid; all.iter().map(|e| { let i = s.edge_info(EdgeId(*e)).unwrap(); (i.start, (i.end - i.start).normalize()) }).collect() };
                for (p0, dir) in mids {
                    let e = { let s = &d.body(id).unwrap().solid; s.edges().find(|e| { let i = s.edge_info(*e).unwrap(); let m = (i.start + i.end) * 0.5; let v = m - p0; let off = v - dir * v.dot(dir); let straight = i.start.distance(i.end) > 1e-6 && ((i.end - i.start).normalize().dot(dir)).abs() > 0.9999; straight && off.length() < 1e-6 }) };
                    match e { Some(e) => if let Err(er) = d.apply(Command::FilletEdges { body: id, edges: vec![e.0], size: 3.0, chamfer: false }) { eprintln!("{mode} n={n} step: {er}"); ok = false; }, None => { eprintln!("{mode} n={n}: edge lost"); ok = false; } }
                }
            }
            if !ok { continue; }
            let b = d.body(id).unwrap();
            eprintln!("{mode} n={n}: faces {} features {:?}", b.solid.face_count(), b.features.iter().map(|f| f.faces.len()).collect::<Vec<_>>());
            let feats: Vec<Vec<_>> = b.features.iter().map(|f| f.faces.clone()).collect();
            for (k, f) in feats.iter().enumerate() {
                let r = d.apply(Command::EditBlend { body: id, face: f[0], size: 2.0 });
                eprintln!("  feature {k} resize 2: {:?}", r.map(|_| ()).map_err(|e| e.to_string()));
                d.undo();
            }
            let f0 = feats[0][0];
            let r = d.apply(Command::EditBlend { body: id, face: f0, size: 0.0 });
            eprintln!("  zero: {:?} faces {}", r.map(|_| ()).map_err(|e| e.to_string()), d.body(id).unwrap().solid.face_count());
        }
    }
}
