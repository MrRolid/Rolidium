// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! `rolidium-svg2png in.svg out.png|out.rgba size` renders the SVG, scaled to `size` pixels wide, with a transparent background.

use resvg::{tiny_skia, usvg};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: rolidium-svg2png in.svg out.png size");
        std::process::exit(2);
    }
    let svg = std::fs::read_to_string(&args[1]).expect("cannot read the SVG file");
    let size: u32 = args[3].parse().expect("size must be a number");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("not a valid SVG");
    let scale = size as f32 / tree.size().width();
    let height = (tree.size().height() * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(size, height).expect("bad size");
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    if args[2].ends_with(".rgba") {
        // raw straight (not premultiplied) RGBA, for the window icon
        let mut raw = Vec::with_capacity(pixmap.pixels().len() * 4);
        for p in pixmap.pixels() {
            let c = p.demultiply();
            raw.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        std::fs::write(&args[2], raw).expect("cannot write the RGBA file");
    } else {
        pixmap.save_png(&args[2]).expect("cannot write the PNG file");
    }
}
