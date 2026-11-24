// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: CC0-1.0
//! Writes the example meshes of `examples/stl/` (own geometry, CC0): `cargo run -p rolidium-mesh --example make_examples`.
use rolidium_mesh::{gen, stl};

fn main() {
    let dir = std::path::Path::new("examples/stl");
    std::fs::create_dir_all(dir).unwrap();
    // a plate 60 x 40 x 10 mm with a round hole of 5 mm diameter (a little box of the kind the edits of the program are tried on)
    let plate = gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, 48);
    std::fs::write(dir.join("plate-with-hole.stl"), stl::write_binary(&plate)).unwrap();
    std::fs::write(dir.join("plate-with-hole-ascii.stl"), stl::write_ascii(&plate, "plate")).unwrap();
    // a cube of 20 mm, each face divided in 4 x 4 squares
    std::fs::write(dir.join("cube-20.stl"), stl::write_binary(&gen::box_mesh(20.0, 20.0, 20.0, 4))).unwrap();
}
