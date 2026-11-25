// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Lists the planar walls of an STL file: normal, offset, area and the area-weighted mean of the normals of its triangles (they must agree).
use rolidium_mesh::{regions, stl};
fn main() {
    let path = std::env::args().nth(1).expect("an STL file");
    let m = stl::parse(&std::fs::read(path).unwrap()).unwrap();
    let r = regions::find(&m, 3.0, 4);
    println!("{} triangles, volume {:.1}", m.triangles.len(), m.signed_volume());
    for (i, p) in r.planes.iter().enumerate() {
        let mut s = [0.0; 3];
        for t in &p.triangles {
            let (n, a) = m.normal_area(*t as usize);
            for k in 0..3 {
                s[k] += n[k] * a;
            }
        }
        let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-12);
        println!(
            "plane {i}: normal {:.3?} offset {:.3} area {:.1} tris {} mean-of-triangles {:.3?}",
            p.normal,
            p.offset,
            p.area,
            p.triangles.len(),
            [s[0] / l, s[1] / l, s[2] / l]
        );
    }
}
