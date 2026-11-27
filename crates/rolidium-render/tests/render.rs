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
            stamp: 0,
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
        xray: false,
        clip: None,
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
        xray: false,
        clip: None,
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
            stamp: 0,
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
        xray: false,
        clip: None,
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
            stamp: 0,
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
        xray: false,
        clip: None,
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

#[test]
fn a_section_cuts_the_picture_and_fills_the_cut_and_x_ray_shows_through() {
    // a hollow box seen from the front: with a section through the middle the walls are cut and painted; X-ray changes the picture too
    let (device, queue, mut r) = setup();
    let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 40.0, 40.0)).unwrap();
    let top = b
        .faces()
        .find(|f| b.face_info(*f).unwrap().normal.z > 0.9)
        .unwrap();
    let hollow = b.shell(&[top], 4.0, true, false).unwrap().solid;
    let mesh = hollow.view_mesh(0.05, 200_000, None).unwrap();
    r.set_bodies(
        &device,
        &[BodyDraw {
            id: 1,
            mesh: &mesh,
            color: [0.7, 0.7, 0.75],
            stamp: 0,
        }],
    );
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    cam.fit(glam::Vec3::ZERO, glam::Vec3::splat(40.0));
    let mut frame = Frame {
        camera: cam,
        hover: ID_NONE,
        selected: vec![],
        body_marks: vec![],
        size: [200, 200],
        grid: None,
        render_ids: false,
        grid_levels: None,
        scene_radius: 0.0,
        xray: false,
        clip: None,
    };
    let plain = r.render_to_rgba(&device, &queue, &frame).unwrap();
    // the plane y = 20 and what lies behind it (larger y) stays: the camera in front sees into the cut
    frame.clip = Some([0.0, 1.0, 0.0, -20.0]);
    let cut = r.render_to_rgba(&device, &queue, &frame).unwrap();
    let cap = |img: &[u8]| {
        img.chunks(4)
            .filter(|p| p[0] > 200 && p[1] > 130 && p[1] < 190 && p[2] < 110)
            .count()
    };
    assert_eq!(cap(&plain), 0, "no cut colour without a section");
    assert!(cap(&cut) > 300, "the walls of the cut are painted: {}", cap(&cut));
    frame.clip = None;
    frame.xray = true;
    let xr = r.render_to_rgba(&device, &queue, &frame).unwrap();
    assert_ne!(plain, xr, "X-ray changes the picture");
}
