// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use crate::*;
use std::f64::consts::PI;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Rect {
        a: v(x0, y0),
        b: v(x1, y1),
    }
}
fn circle(x: f64, y: f64, r: f64) -> Entity {
    Entity::Circle {
        center: v(x, y),
        radius: r,
    }
}
fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line {
        a: v(x0, y0),
        b: v(x1, y1),
    }
}
fn sketch(es: &[Entity]) -> Sketch {
    let mut s = Sketch::new();
    for e in es {
        s.add(*e).unwrap();
    }
    s
}
fn areas(s: &Sketch) -> Vec<f64> {
    let mut a: Vec<f64> = s.regions().iter().map(|r| r.area()).collect();
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    a
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn single_rectangle() {
    let s = sketch(&[rect(0.0, 0.0, 40.0, 30.0)]);
    let r = s.regions();
    assert_eq!(r.len(), 1);
    close(r[0].area(), 1200.0);
    assert!(r[0].holes.is_empty());
    assert_eq!(r[0].exclusive_entities, vec![1]);
}

#[test]
fn circle_in_rectangle_gives_annulus_and_disc() {
    let s = sketch(&[rect(0.0, 0.0, 40.0, 30.0), circle(20.0, 15.0, 5.0)]);
    let r = s.regions();
    assert_eq!(r.len(), 2);
    let a = areas(&s);
    close(a[0], PI * 25.0);
    close(a[1], 1200.0 - PI * 25.0);
    let ring = r.iter().find(|x| !x.holes.is_empty()).unwrap();
    assert_eq!(ring.holes.len(), 1);
    // the circle borders both regions, so it is not exclusive to either
    assert_eq!(ring.exclusive_entities, vec![1]);
    let disc = r.iter().find(|x| x.holes.is_empty() && x.area() < 100.0).unwrap();
    assert!(disc.exclusive_entities.is_empty());
    assert!(ring.contains(v(1.0, 1.0)) && !ring.contains(v(20.0, 15.0)) && disc.contains(v(20.0, 15.0)));
}

#[test]
fn rect_circle_rect_nesting() {
    let s = sketch(&[
        rect(0.0, 0.0, 100.0, 100.0),
        circle(50.0, 50.0, 40.0),
        rect(40.0, 40.0, 60.0, 60.0),
    ]);
    let a = areas(&s);
    assert_eq!(a.len(), 3);
    close(a[0], 400.0);
    close(a[1], PI * 1600.0 - 400.0);
    close(a[2], 10000.0 - PI * 1600.0);
}

#[test]
fn overlapping_rectangles_give_three_regions() {
    let s = sketch(&[rect(0.0, 0.0, 20.0, 20.0), rect(10.0, 10.0, 30.0, 30.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 3);
    close(a[0], 100.0);
    close(a[1], 300.0);
    close(a[2], 300.0);
}

#[test]
fn circle_crossing_rectangle() {
    let s = sketch(&[rect(0.0, 0.0, 20.0, 20.0), circle(20.0, 10.0, 5.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 3);
    close(a.iter().sum::<f64>(), 400.0 + PI * 25.0 / 2.0);
}

#[test]
fn two_overlapping_circles() {
    let s = sketch(&[circle(0.0, 0.0, 10.0), circle(10.0, 0.0, 10.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 3);
    close(a.iter().sum::<f64>(), 2.0 * PI * 100.0 - (a[0]));
}

#[test]
fn touching_rectangles_share_an_edge() {
    let s = sketch(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 0.0, 20.0, 10.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 2);
    close(a[0], 100.0);
    close(a[1], 100.0);
}

#[test]
fn touching_circles_are_separate_regions() {
    let s = sketch(&[circle(0.0, 0.0, 5.0), circle(10.0, 0.0, 5.0)]);
    assert_eq!(s.regions().len(), 2);
}

#[test]
fn circle_tangent_inside_circle() {
    let s = sketch(&[circle(0.0, 0.0, 10.0), circle(5.0, 0.0, 5.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 2);
    close(a[0], PI * 25.0);
    close(a[1], PI * 75.0);
}

#[test]
fn line_through_rectangle_splits_it() {
    let s = sketch(&[rect(0.0, 0.0, 20.0, 10.0), line(10.0, -5.0, 10.0, 15.0)]);
    let a = areas(&s);
    assert_eq!(a.len(), 2);
    close(a[0], 100.0);
}

#[test]
fn closed_triangle_from_lines() {
    let s = sketch(&[
        line(0.0, 0.0, 10.0, 0.0),
        line(10.0, 0.0, 0.0, 10.0),
        line(0.0, 10.0, 0.0, 0.0),
    ]);
    let a = areas(&s);
    assert_eq!(a.len(), 1);
    close(a[0], 50.0);
    // crossing lines (an X inside a square frame of lines) make four triangles
    let s = sketch(&[
        rect(0.0, 0.0, 10.0, 10.0),
        line(0.0, 0.0, 10.0, 10.0),
        line(0.0, 10.0, 10.0, 0.0),
    ]);
    assert_eq!(s.regions().len(), 4);
}

#[test]
fn open_chains_are_ignored() {
    let s = sketch(&[
        line(0.0, 0.0, 10.0, 0.0),
        line(10.0, 0.0, 10.0, 10.0),
        line(10.0, 10.0, 0.0, 10.0),
    ]);
    assert!(s.regions().is_empty());
    assert!(Sketch::new().regions().is_empty());
}

#[test]
fn dangling_and_bridging_lines_do_not_change_regions() {
    let base = sketch(&[rect(0.0, 0.0, 10.0, 10.0), circle(30.0, 5.0, 4.0)]);
    let with = sketch(&[
        rect(0.0, 0.0, 10.0, 10.0),
        circle(30.0, 5.0, 4.0),
        line(10.0, 5.0, 26.0, 5.0), // bridge between the two shapes
        line(5.0, 5.0, 5.0, 8.0),   // dangling inside the rectangle
        line(-5.0, 0.0, 0.0, 0.0),  // dangling outside
    ]);
    assert_eq!(areas(&base).len(), 2);
    let (a, b) = (areas(&base), areas(&with));
    assert_eq!(b.len(), 2);
    close(a[0], b[0]);
    close(a[1], b[1]);
}

#[test]
fn identical_shapes_do_not_duplicate_regions() {
    let s = sketch(&[
        rect(0.0, 0.0, 10.0, 10.0),
        rect(0.0, 0.0, 10.0, 10.0),
        circle(5.0, 5.0, 2.0),
        circle(5.0, 5.0, 2.0),
    ]);
    assert_eq!(s.regions().len(), 2);
}

#[test]
fn removing_exclusive_entities_keeps_the_rest() {
    let mut s = sketch(&[rect(0.0, 0.0, 40.0, 30.0), circle(20.0, 15.0, 5.0)]);
    let ring = s.regions().into_iter().find(|r| !r.holes.is_empty()).unwrap();
    s.remove(&ring.exclusive_entities);
    let left = s.regions();
    assert_eq!(left.len(), 1);
    close(left[0].area(), PI * 25.0);
}

#[test]
fn invalid_entities_are_rejected() {
    let mut s = Sketch::new();
    assert_eq!(s.add(line(1.0, 1.0, 1.0, 1.0)), Err(SketchError::Degenerate));
    assert_eq!(s.add(rect(0.0, 0.0, 0.0, 5.0)), Err(SketchError::Degenerate));
    assert_eq!(s.add(circle(0.0, 0.0, 0.0)), Err(SketchError::Degenerate));
    assert_eq!(s.add(circle(f64::NAN, 0.0, 1.0)), Err(SketchError::NotFinite));
    assert!(s.is_empty());
}

#[test]
fn triangulation_covers_the_area() {
    let s = sketch(&[rect(0.0, 0.0, 40.0, 30.0), circle(20.0, 15.0, 5.0)]);
    let ring = s.regions().into_iter().find(|r| !r.holes.is_empty()).unwrap();
    let tri: f64 = ring
        .triangulate(0.05)
        .iter()
        .map(|t| 0.5 * ((t[1] - t[0]).perp_dot(t[2] - t[0])).abs())
        .sum();
    assert!((tri - ring.area()).abs() < 1.0, "{tri} vs {}", ring.area());
}

#[test]
fn plane_axes_follow_world_axes() {
    use glam::DVec3;
    let top = PlaneDef::from_face(DVec3::new(3.0, 4.0, 20.0), DVec3::Z);
    assert_eq!(top.origin, DVec3::new(0.0, 0.0, 20.0));
    assert_eq!(top.x_dir, DVec3::X);
    assert_eq!(top.y_dir(), DVec3::Y);
    let front = PlaneDef::from_face(DVec3::new(3.0, 0.0, 5.0), -DVec3::Y);
    assert_eq!(front.x_dir, DVec3::X);
    assert_eq!(front.y_dir(), DVec3::Z);
    let right = PlaneDef::from_face(DVec3::new(40.0, 1.0, 5.0), DVec3::X);
    assert_eq!(right.x_dir, DVec3::Y);
    assert_eq!(right.y_dir(), DVec3::Z);
    let p = top.to_3d(v(7.0, 8.0));
    assert!((top.to_2d(p) - v(7.0, 8.0)).length() < 1e-12);
}

#[test]
fn arcs_close_regions_with_lines() {
    use std::f64::consts::PI;
    // an oval: two lines of length 20 and two half circles of radius 5
    let mut s = Sketch::new();
    s.add(Entity::Line {
        a: DVec2::new(0.0, 0.0),
        b: DVec2::new(20.0, 0.0),
    })
    .unwrap();
    s.add(Entity::Line {
        a: DVec2::new(0.0, 10.0),
        b: DVec2::new(20.0, 10.0),
    })
    .unwrap();
    s.add(
        Entity::arc_through(
            DVec2::new(20.0, 0.0),
            DVec2::new(25.0, 5.0),
            DVec2::new(20.0, 10.0),
        )
        .unwrap(),
    )
    .unwrap();
    s.add(Entity::arc_through(DVec2::new(0.0, 10.0), DVec2::new(-5.0, 5.0), DVec2::new(0.0, 0.0)).unwrap())
        .unwrap();
    let r = s.regions();
    assert_eq!(r.len(), 1);
    assert!(
        (r[0].area() - (200.0 + PI * 25.0)).abs() < 1e-6,
        "{}",
        r[0].area()
    );
    // a line across an arc splits the disc-like region into two
    let mut s = Sketch::new();
    s.add(
        Entity::arc_through(
            DVec2::new(10.0, 0.0),
            DVec2::new(0.0, 10.0),
            DVec2::new(-10.0, 0.0),
        )
        .unwrap(),
    )
    .unwrap();
    s.add(Entity::Line {
        a: DVec2::new(-10.0, 0.0),
        b: DVec2::new(10.0, 0.0),
    })
    .unwrap();
    assert_eq!(s.regions().len(), 1);
    s.add(Entity::Line {
        a: DVec2::new(0.0, -1.0),
        b: DVec2::new(0.0, 12.0),
    })
    .unwrap();
    assert_eq!(s.regions().len(), 2);
    // an open arc alone bounds nothing
    let mut s = Sketch::new();
    s.add(
        Entity::arc_through(
            DVec2::new(10.0, 0.0),
            DVec2::new(0.0, 10.0),
            DVec2::new(-10.0, 0.0),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(s.regions().is_empty());
    // the arc builders
    let e = Entity::arc_chord_radius(
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        5.0,
        DVec2::new(5.0, 6.0),
    )
    .unwrap();
    match e {
        Entity::Arc {
            radius,
            sweep,
            center,
            ..
        } => {
            assert!((radius - 5.0).abs() < 1e-9 && (sweep.abs() - PI).abs() < 1e-6);
            assert!(center.distance(DVec2::new(5.0, 0.0)) < 1e-6);
        }
        _ => panic!(),
    }
    assert!(
        Entity::arc_chord_radius(DVec2::ZERO, DVec2::new(10.0, 0.0), 4.0, DVec2::new(5.0, 1.0)).is_none()
    );
    assert!(Entity::arc_through(DVec2::ZERO, DVec2::new(1.0, 0.0), DVec2::new(2.0, 0.0)).is_none());
}

#[test]
fn an_auxiliary_axis_makes_no_region_and_does_not_close_a_profile() {
    // a rectangle with an axis through it and another axis that closes nothing: still one region, the axis is no shape of the outline
    let ax = Entity::Axis {
        a: v(5.0, -5.0),
        b: v(5.0, 15.0),
    };
    let s = sketch(&[rect(0.0, 0.0, 10.0, 10.0), ax]);
    let r = s.regions();
    assert_eq!(r.len(), 1);
    assert!(
        (r[0].area() - 100.0).abs() < 1e-9,
        "the axis does not split the rectangle"
    );
    // an axis alone, and three lines that would close with an axis as the third side
    assert!(sketch(&[ax]).regions().is_empty());
    let open = sketch(&[
        line(0.0, 0.0, 10.0, 0.0),
        line(10.0, 0.0, 10.0, 10.0),
        Entity::Axis {
            a: v(10.0, 10.0),
            b: v(0.0, 0.0),
        },
    ]);
    assert!(open.regions().is_empty());
    // a line and an axis are the same line drawn two ways
    let l = line(0.0, 0.0, 3.0, 4.0);
    assert_eq!(l.as_axis().as_line(), l);
    assert!(l.as_axis().is_axis() && !l.is_axis());
    let (p, d) = ax.axis_line().unwrap();
    assert_eq!(p, v(5.0, -5.0));
    assert!((d - v(0.0, 1.0)).length() < 1e-12);
    assert!(circle(0.0, 0.0, 1.0).axis_line().is_none());
}
