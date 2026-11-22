// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Patterns and mirrors of the shapes of a sketch (lines, rectangles, circles, arcs). They make new shapes at once (no link: a sketch has no history);
//! the regions are found again by the sketch, so several shapes can be pulled together right away.

use super::*;
use rolidium_sketch::{DVec2, EntityId};

/// A pattern of sketch shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchPattern {
    Linear {
        dir: DVec2,
        count: u32,
        spacing: f64,
        by_total: bool,
        /// A second direction (a grid): direction, count, spacing, `by_total`.
        second: Option<(DVec2, u32, f64, bool)>,
    },
    Circular {
        center: DVec2,
        count: u32,
        angle_deg: f64,
        full: bool,
    },
}

#[derive(Debug, Clone, Copy)]
enum Map2 {
    Move(DVec2),
    Turn { center: DVec2, angle: f64 },
    /// Mirror in the line through `a` with the direction angle `theta`.
    Mirror { a: DVec2, theta: f64 },
}

impl Map2 {
    fn point(self, p: DVec2) -> DVec2 {
        match self {
            Map2::Move(d) => p + d,
            Map2::Turn { center, angle } => {
                let (s, c) = angle.sin_cos();
                let v = p - center;
                center + DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
            }
            Map2::Mirror { a, theta } => {
                let (s, c) = (2.0 * theta).sin_cos();
                let v = p - a;
                a + DVec2::new(v.x * c + v.y * s, v.x * s - v.y * c)
            }
        }
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The shapes that `e` becomes (a rectangle that is not axis-aligned any more becomes four lines).
fn map_entity(e: &Entity, m: Map2) -> Vec<Entity> {
    match e {
        Entity::Line { a, b } => vec![Entity::Line { a: m.point(*a), b: m.point(*b) }],
        Entity::Circle { center, radius } => vec![Entity::Circle { center: m.point(*center), radius: *radius }],
        Entity::Arc { center, radius, start, sweep } => {
            let c = m.point(*center);
            let (start2, sweep2) = match m {
                Map2::Move(_) => (*start, *sweep),
                Map2::Turn { angle, .. } => (*start + angle, *sweep),
                // an angle f becomes 2 theta - f, and the arc is still counter-clockwise: it starts where the old one ended
                Map2::Mirror { theta, .. } => (2.0 * theta - (*start + *sweep), *sweep),
            };
            vec![Entity::Arc { center: c, radius: *radius, start: start2, sweep: sweep2 }]
        }
        Entity::Rect { a, b } => {
            let corners = [DVec2::new(a.x, a.y), DVec2::new(b.x, a.y), DVec2::new(b.x, b.y), DVec2::new(a.x, b.y)];
            let q: Vec<DVec2> = corners.iter().map(|p| m.point(*p)).collect();
            let axis_aligned = (near(q[0].y, q[1].y) && near(q[1].x, q[2].x) && near(q[2].y, q[3].y) && near(q[3].x, q[0].x))
                || (near(q[0].x, q[1].x) && near(q[1].y, q[2].y) && near(q[2].x, q[3].x) && near(q[3].y, q[0].y));
            if axis_aligned {
                vec![Entity::Rect { a: q[0], b: q[2] }]
            } else {
                (0..4).map(|i| Entity::Line { a: q[i], b: q[(i + 1) % 4] }).collect()
            }
        }
    }
}

fn step_of(count: u32, spacing: f64, by_total: bool) -> f64 {
    if by_total && count > 1 {
        spacing / (count - 1) as f64
    } else {
        spacing
    }
}

fn chosen(sk: &SketchDoc, ids: &[EntityId]) -> Result<Vec<Entity>, DocError> {
    let all = sk.sketch.entities();
    let list: Vec<Entity> = if ids.is_empty() {
        all.iter().map(|(_, e)| e.clone()).collect()
    } else {
        ids.iter()
            .map(|i| all.iter().find(|(x, _)| x == i).map(|(_, e)| e.clone()).ok_or_else(|| KernelError::Invalid("a shape of the sketch no longer exists".into()))
            .map_err(DocError::from))
            .collect::<Result<_, _>>()?
    };
    if list.is_empty() {
        return Err(KernelError::Invalid("the sketch has no shapes to repeat".into()).into());
    }
    Ok(list)
}

/// Adds the copies of the shapes (`ids`, empty: all of them); returns how many shapes were added.
pub(crate) fn pattern(sk: &mut SketchDoc, ids: &[EntityId], p: &SketchPattern) -> Result<usize, DocError> {
    let src = chosen(sk, ids)?;
    let mut maps = Vec::new();
    match p {
        SketchPattern::Linear { dir, count, spacing, by_total, second } => {
            if *count < 1 || *count as usize > MAX_PIECES || dir.length() < 1e-12 {
                return Err(KernelError::Invalid(format!("the number of pieces must be between 1 and {MAX_PIECES} and the direction must have a length")).into());
            }
            let d = dir.normalize();
            let step = step_of(*count, *spacing, *by_total);
            let (c2, d2, s2) = match second {
                Some((dd, c, sp, bt)) => {
                    if *c < 1 || dd.length() < 1e-12 || (*c as usize) * (*count as usize) > MAX_PIECES {
                        return Err(KernelError::Invalid(format!("a pattern can have at most {MAX_PIECES} pieces")).into());
                    }
                    (*c, dd.normalize(), step_of(*c, *sp, *bt))
                }
                None => (1, DVec2::Y, 0.0),
            };
            for j in 0..c2 {
                for i in 0..*count {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    maps.push(Map2::Move(d * step * i as f64 + d2 * s2 * j as f64));
                }
            }
        }
        SketchPattern::Circular { center, count, angle_deg, full } => {
            if *count < 1 || *count as usize > MAX_PIECES {
                return Err(KernelError::Invalid(format!("the number of pieces must be between 1 and {MAX_PIECES}")).into());
            }
            let step = if *full { std::f64::consts::TAU / *count as f64 } else { angle_deg.to_radians() };
            for i in 1..*count {
                maps.push(Map2::Turn { center: *center, angle: step * i as f64 });
            }
        }
    }
    let mut added = 0;
    for m in maps {
        for e in &src {
            for n in map_entity(e, m) {
                sk.sketch.add(n)?;
                added += 1;
            }
        }
    }
    Ok(added)
}

/// Mirrors the shapes (`ids`, empty: all of them) in the line through `a` and `b`; with `keep` the originals stay, else they are replaced.
pub(crate) fn mirror(sk: &mut SketchDoc, ids: &[EntityId], a: DVec2, b: DVec2, keep: bool) -> Result<usize, DocError> {
    let d = b - a;
    if d.length() < 1e-9 {
        return Err(KernelError::Invalid("the mirror line has no length".into()).into());
    }
    let src = chosen(sk, ids)?;
    let m = Map2::Mirror { a, theta: d.y.atan2(d.x) };
    if !keep {
        let drop: Vec<EntityId> = if ids.is_empty() { sk.sketch.entities().iter().map(|(i, _)| *i).collect() } else { ids.to_vec() };
        sk.sketch.remove(&drop);
    }
    let mut added = 0;
    for e in &src {
        for n in map_entity(e, m) {
            sk.sketch.add(n)?;
            added += 1;
        }
    }
    Ok(added)
}
