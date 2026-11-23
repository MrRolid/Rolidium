use rolidium_kernel::*;
#[test]
fn dbg() {
    let s = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let es: Vec<EdgeId> = s.edges().filter(|e| { let i = s.edge_info(*e).unwrap(); i.start.z > 19.99 && i.end.z > 19.99 }).collect();
    let r = s.chamfer(&es, 3.0).unwrap().solid;
    for c in &r.blend_map().components { eprintln!("comp {:?}", c.faces); for ro in &c.roundings { eprintln!("   {:?} size {} angle {} pairs {:?} legs {:?} corners {:?}", ro.faces, ro.size, ro.angle_deg, ro.pairs, ro.legs, ro.corners); } }
    for f in r.faces() { let i = r.face_info(f).unwrap(); eprintln!("face {} {:?} n {:?} area {}", f.0, i.kind, i.normal, i.area); }
}
