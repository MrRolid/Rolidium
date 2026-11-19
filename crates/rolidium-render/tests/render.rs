// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use rolidium_kernel::{DVec3, FaceKind, Solid};
use rolidium_render::{
    decode_pick, headless_device, BodyDraw, Camera, Frame, Renderer, StandardView, ID_NONE,
};

fn setup() -> (
    rolidium_render::wgpu::Device,
    rolidium_render::wgpu::Queue,
    Renderer,
) {
    // No silent skipping: without any adapter (hardware or software) this fails loudly.
    let (device, queue, info) = headless_device().expect("no graphics adapter, not even a software one");
    eprintln!("adapter: {} ({:?})", info.name, info.device_type);
    let r = Renderer::new(&device, rolidium_render::wgpu::TextureFormat::Rgba8Unorm);
    (device, queue, r)
}

#[test]
fn id_buffer_identifies_faces() {
    let (device, queue, mut r) = setup();
    let solid = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let mesh = solid.display_mesh().unwrap();
    r.set_bodies(
        &device,
        &[BodyDraw {
            id: 77,
            mesh: &mesh,
            color: [0.7, 0.7, 0.7],
        }],
    );

    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.fit(glam::Vec3::ZERO, glam::Vec3::new(40.0, 30.0, 20.0));
    let (w, h) = (200u32, 160u32);
    let frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size: [w, h],
        grid: None,
        render_ids: true,
        grid_levels: None,
        scene_radius: 0.0,
    };
    let ids = r.read_ids(&device, &queue, &frame).unwrap();

    // Corner pixel is background, centre pixel is the top face.
    assert_eq!(ids[0], ID_NONE);
    let centre = ids[(h / 2 * w + w / 2) as usize];
    let (slot, face) = decode_pick(centre).expect("centre must hit the box");
    assert_eq!(r.slot_body(slot), Some(77));
    let info = solid.face_info(rolidium_kernel::FaceId(face)).unwrap();
    assert_eq!(info.kind, FaceKind::Plane);
    assert!(
        info.normal.z > 0.99,
        "top view must pick the top face, got normal {:?}",
        info.normal
    );

    // Isometric view sees three distinct faces.
    cam.set_view(StandardView::Isometric);
    let frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size: [w, h],
        grid: None,
        render_ids: true,
        grid_levels: None,
        scene_radius: 0.0,
    };
    let ids = r.read_ids(&device, &queue, &frame).unwrap();
    // Ignore single-pixel cracks along shared edges (faces do not share vertices).
    let mut counts = std::collections::BTreeMap::new();
    for i in ids.iter().filter(|i| **i != ID_NONE) {
        *counts.entry(*i).or_insert(0u32) += 1;
    }
    let seen: Vec<u32> = counts
        .into_iter()
        .filter(|(_, c)| *c >= 20)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        seen.len(),
        3,
        "isometric view shows exactly three faces, got {seen:?}"
    );
}

#[test]
fn offscreen_render_is_not_blank_and_highlights() {
    let (device, queue, mut r) = setup();
    let solid = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let mesh = solid.display_mesh().unwrap();
    r.set_bodies(
        &device,
        &[BodyDraw {
            id: 1,
            mesh: &mesh,
            color: [0.7, 0.7, 0.7],
        }],
    );
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.fit(glam::Vec3::ZERO, glam::Vec3::new(40.0, 30.0, 20.0));
    let mut frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size: [160, 120],
        grid: None,
        render_ids: true,
        grid_levels: None,
        scene_radius: 0.0,
    };
    let plain = r.render_to_rgba(&device, &queue, &frame).unwrap();
    assert_eq!(plain.len(), 160 * 120 * 4);
    let bg = &plain[0..4];
    let centre = &plain[((60 * 160 + 80) * 4) as usize..][..4];
    assert_ne!(bg, centre, "box must differ from the background");

    // Selecting the top face changes the centre pixel.
    let ids = r.read_ids(&device, &queue, &frame).unwrap();
    frame.selected = vec![ids[60 * 160 + 80]];
    let sel = r.render_to_rgba(&device, &queue, &frame).unwrap();
    let centre_sel = &sel[((60 * 160 + 80) * 4) as usize..][..4];
    assert_ne!(centre, centre_sel, "selected face must be highlighted");
}

#[test]
fn test_scene_shows_grid_axes_sketch_and_body() {
    let (device, queue, _r) = setup();
    let (rgba, check) = rolidium_render::render_test_scene(&device, &queue, [640, 480]).unwrap();
    if std::env::var_os("ROLIDIUM_SAVE_SCENE").is_some() {
        std::fs::write(
            "../../target/tmp/test-scene.png",
            rolidium_render::encode_png(&rgba, 640, 480).unwrap(),
        )
        .unwrap();
    }
    assert!(check.ok(), "{}", check.describe());
}

/// A 300 mm part seen from a low angle: the grid must reach far beyond the model and must not shimmer.
#[test]
fn grid_is_visible_far_away_at_a_low_viewing_angle() {
    let (device, queue, mut r) = setup();
    let solid = Solid::cuboid(DVec3::new(-150.0, -100.0, 0.0), DVec3::new(150.0, 100.0, 80.0)).unwrap();
    let mesh = solid.display_mesh().unwrap();
    r.set_bodies(
        &device,
        &[BodyDraw {
            id: 1,
            mesh: &mesh,
            color: [0.7, 0.7, 0.75],
        }],
    );
    let mut cam = Camera::default();
    cam.set_view(StandardView::Isometric);
    cam.fit(
        glam::Vec3::new(-150.0, -100.0, 0.0),
        glam::Vec3::new(150.0, 100.0, 80.0),
    );
    cam.pitch = 0.10; // almost horizontal
    let (w, h) = (800u32, 500u32);
    let frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size: [w, h],
        grid: None,
        render_ids: false,
        grid_levels: None,
        scene_radius: 190.0,
    };
    let rgba = r.render_to_rgba(&device, &queue, &frame).unwrap();
    if std::env::var_os("ROLIDIUM_SAVE_SCENE").is_some() {
        std::fs::write(
            "../../target/tmp/low-angle.png",
            rolidium_render::encode_png(&rgba, w, h).unwrap(),
        )
        .unwrap();
    }
    // the rows between the horizon and the top of the model: grid lines are visible in them, beyond the model
    let bg = [rgba[0], rgba[1], rgba[2]];
    let lit = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        let d = (rgba[i] as i32 - bg[0] as i32).abs()
            + (rgba[i + 1] as i32 - bg[1] as i32).abs()
            + (rgba[i + 2] as i32 - bg[2] as i32).abs();
        d > 12
    };
    // find the horizon row: the first row from the top where grid lines show
    let mut rows_with_grid = 0;
    for y in 0..h / 2 {
        if (0..w).filter(|x| lit(*x, y)).count() > 12 {
            rows_with_grid += 1;
        }
    }
    assert!(
        rows_with_grid > 40,
        "only {rows_with_grid} rows of the upper half show the grid: it fades out too early"
    );
}
