// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Two links that work on one body: a hollow body (the Shell) and text raised from a flat face or engraved into it. Both remember their numbers and the place they
//! work at (not the numbers of faces, which change), and are calculated again when a number or the body they start from changes.

use super::*;
use rolidium_kernel::{FaceKind, OpResult};
pub use rolidium_text::Align as TextAlign;

fn bad(msg: impl Into<String>) -> DocError {
    DocError::Kernel(KernelError::Invalid(msg.into()))
}

// ---------------------------------------------------------------- where a face is

/// A face found again by what it is and where it lies (the numbers of faces change with every operation).
#[derive(Debug, Clone, PartialEq)]
pub struct FaceAnchor {
    pub kind: FaceKind,
    /// A plane: the outward normal; a round face: its axis.
    pub normal: DVec3,
    pub origin: DVec3,
    pub radius: f64,
    pub area: f64,
}

impl FaceAnchor {
    pub fn of(solid: &Solid, face: FaceId) -> Result<FaceAnchor, DocError> {
        let i = solid.face_info(face)?;
        Ok(FaceAnchor {
            kind: i.kind,
            normal: i.normal,
            origin: i.origin,
            radius: i.radius,
            area: i.area,
        })
    }

    /// The face of `solid` that this anchor describes best, `None` when nothing is near.
    pub fn find(&self, solid: &Solid) -> Option<FaceId> {
        let mut best: Option<(f64, FaceId)> = None;
        for f in solid.faces() {
            let Ok(i) = solid.face_info(f) else { continue };
            if i.kind != self.kind {
                continue;
            }
            let score = match self.kind {
                FaceKind::Plane => {
                    if i.normal.dot(self.normal) < 0.9999 {
                        continue;
                    }
                    let off = (i.origin - self.origin).dot(self.normal).abs();
                    if off > 1e-3 {
                        continue;
                    }
                    (i.area - self.area).abs() / self.area.max(1e-9)
                }
                _ => {
                    let axis_ok = i
                        .normal
                        .normalize_or_zero()
                        .dot(self.normal.normalize_or_zero())
                        .abs()
                        > 0.9999;
                    let d = (i.origin - self.origin)
                        .cross(self.normal.normalize_or_zero())
                        .length();
                    if !axis_ok || d > 1e-3 || (i.radius - self.radius).abs() > 1e-3 {
                        continue;
                    }
                    (i.area - self.area).abs() / self.area.max(1e-9)
                }
            };
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, f));
            }
        }
        best.map(|b| b.1)
    }
}

// ---------------------------------------------------------------- the shell

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellSide {
    /// The outside of the body stays, the walls grow inwards.
    #[default]
    Inwards,
    /// The inside stays, the walls grow outwards.
    Outwards,
    /// Half of the wall on each side of the surface.
    Centre,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellParams {
    /// The body that is hollowed.
    pub target: BodyId,
    /// The faces that are taken away (the openings); none: a closed hollow body.
    pub open: Vec<FaceAnchor>,
    pub thickness: f64,
    pub side: ShellSide,
    /// Round the corners that stick out (otherwise they are mitred).
    pub round_corners: bool,
    /// Walls that are thicker (positive) or thinner (negative) than the rest, by this much; the wall is found by the face of the body it grew from.
    pub heavier: Vec<(FaceAnchor, f64)>,
}

impl ShellParams {
    pub fn new(target: BodyId, thickness: f64) -> ShellParams {
        ShellParams {
            target,
            open: Vec::new(),
            thickness,
            side: ShellSide::Inwards,
            round_corners: false,
            heavier: Vec::new(),
        }
    }
}

fn shell_once(base: &Solid, p: &ShellParams, open: &[FaceId]) -> Result<OpResult, DocError> {
    Ok(match p.side {
        ShellSide::Inwards => base.shell(open, p.thickness, true, p.round_corners)?,
        ShellSide::Outwards => base.shell(open, p.thickness, false, p.round_corners)?,
        ShellSide::Centre => base.shell_centre(open, p.thickness, p.round_corners)?,
    })
}

/// The shell of `base`, with the remarks about how it was made. `Err` when it cannot be made.
pub fn shell(base: &Solid, p: &ShellParams) -> Result<(Solid, Vec<String>), DocError> {
    if !(p.thickness.is_finite() && p.thickness > 1e-6) {
        return Err(bad("The wall thickness must be a positive number"));
    }
    let mut open = Vec::new();
    for a in &p.open {
        open.push(
            a.find(base)
                .ok_or_else(|| bad("A face that was to be taken away is not on the body any more"))?,
        );
    }
    let mut r = shell_once(base, p, &open)?;
    let mut notes = Vec::new();
    // thicker or thinner single walls: the face of the wall that moves (the cavity side for walls growing inwards, the outer side for walls growing outwards)
    for (anchor, extra) in &p.heavier {
        if extra.abs() < 1e-9 {
            continue;
        }
        let Some(face) = anchor.find(base) else {
            notes.push("A wall that was to be thickened is not on the body any more".to_string());
            continue;
        };
        if open.contains(&face) {
            notes.push("A wall that is an opening cannot be thickened".to_string());
            continue;
        }
        let n = anchor.normal.normalize_or_zero();
        let want_normal = if p.side == ShellSide::Outwards { n } else { -n };
        let succ = r.history.successors(0, face);
        let moved = succ.into_iter().find(|f| {
            r.solid
                .face_info(*f)
                .map(|i| i.kind == FaceKind::Plane && i.normal.dot(want_normal) > 0.9999)
                .unwrap_or(false)
        });
        let Some(f) = moved else {
            notes.push("A wall that was to be thickened could not be found in the shell (only flat walls can be changed one by one)".to_string());
            continue;
        };
        if p.thickness + extra <= 1e-6 {
            return Err(bad("A wall would be thinner than nothing"));
        }
        let next = r.solid.pull_face(f, *extra)?;
        // the history of the later steps is not needed: the walls are found by their anchors in the base
        r = OpResult {
            solid: next.solid,
            history: History::default(),
        };
    }
    Ok((r.solid, notes))
}

/// A hole or a pin of the body taken out so that the shell does not have to follow it (it is put back afterwards).
struct Detail {
    tool: Solid,
    is_hole: bool,
}

/// The holes and pins of `solid` (round faces that are features) taken out one by one: the plain body and the tools that make them again.
fn without_details(solid: &Solid) -> (Solid, Vec<Detail>) {
    let mut cur = solid.clone();
    let mut tools = Vec::new();
    let mut skip: Vec<FaceAnchor> = Vec::new();
    for _ in 0..400 {
        let mut found = None;
        for f in cur.faces() {
            let Ok(i) = cur.face_info(f) else { continue };
            if !matches!(i.kind, FaceKind::Cylinder | FaceKind::Cone) {
                continue;
            }
            let a = FaceAnchor::of(&cur, f);
            if a.as_ref().is_ok_and(|a| skip.contains(a)) {
                continue;
            }
            if let Ok(t) = cur.feature_tool(f) {
                if let Ok(r) = cur.remove_feature(f) {
                    found = Some((a, t, r.solid));
                    break;
                }
            }
            if let Ok(a) = a {
                skip.push(a);
            }
        }
        let Some((_, t, next)) = found else { break };
        tools.push(Detail {
            tool: t.solid,
            is_hole: t.is_hole,
        });
        cur = next;
    }
    (cur, tools)
}

/// The shell with the small features dealt with: when it cannot be made as the body is, the text of the links `texts` and the holes and pins are taken
/// off, the shell is made of the plain body, and they are put back (a text that sticks out or is engraved into a wall is thinner than the wall, so no
/// offset can follow it). Returns the solid, the remarks.
pub fn shell_with_details(
    base: &Solid,
    p: &ShellParams,
    plain: Option<&Solid>,
    texts: &[TextParams],
) -> Result<(Solid, Vec<String>), DocError> {
    match shell(base, p) {
        Ok(r) => Ok(r),
        Err(first) => {
            // the text: start again from the body before it (the first link of text)
            let mut notes = Vec::new();
            let mut start = plain.cloned().unwrap_or_else(|| base.clone());
            let (stripped, tools) = without_details(&start);
            let had_details = !tools.is_empty();
            if had_details {
                start = stripped;
            }
            if texts.is_empty() && !had_details {
                return Err(first);
            }
            let (mut s, mut n) = shell(&start, p).map_err(|e| {
                // the first message is the better one when nothing was taken off
                if plain.is_none() && !had_details {
                    first
                } else {
                    e
                }
            })?;
            notes.append(&mut n);
            for d in &tools {
                s = s
                    .boolean_checked(
                        &d.tool,
                        if d.is_hole {
                            BooleanOp::Cut
                        } else {
                            BooleanOp::Fuse
                        },
                    )?
                    .solid;
            }
            if had_details {
                notes.push(format!(
                    "{} hole(s) or pin(s) were taken off for the shell and put back",
                    tools.len()
                ));
            }
            for t in texts {
                s = apply_text(&s, t)?;
            }
            if !texts.is_empty() {
                notes.push("The text was taken off for the shell and put back".to_string());
            }
            Ok((s, notes))
        }
    }
}

// ---------------------------------------------------------------- the text

#[derive(Debug, Clone, PartialEq)]
pub struct TextParams {
    /// The body the text is raised from or engraved into.
    pub target: BodyId,
    pub text: String,
    /// The font family (`rolidium_text::FAMILIES`).
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    /// The height of the capital letters, mm.
    pub height: f64,
    pub align: TextAlign,
    /// Degrees, about the point where the text stands.
    pub rotation: f64,
    /// mm added after each letter.
    pub spacing: f64,
    /// How far the letters stand out of the face, or go into it, mm.
    pub depth: f64,
    /// Engraved into the body (otherwise raised from it).
    pub engrave: bool,
    /// The plane of the face the text lies on: a point (where the text starts, or its middle or end for the other alignments), the outward normal and the
    /// direction of the writing.
    pub origin: DVec3,
    pub normal: DVec3,
    pub x_dir: DVec3,
}

impl TextParams {
    pub fn style(&self) -> rolidium_text::Style {
        rolidium_text::Style {
            family: self.font.clone(),
            bold: self.bold,
            italic: self.italic,
            height: self.height,
            align: self.align,
            spacing: self.spacing,
            line_spacing: 1.6,
            rotation: self.rotation,
        }
    }
}

/// How far a tool reaches out of the face so that the cut or the fuse does not leave a skin.
const OVER: f64 = 0.05;

/// The solid of the letters: prisms of the shapes of the text standing on the plane (going up from it for raised text, down into the body for engraved text).
pub fn text_tool(p: &TextParams) -> Result<Solid, DocError> {
    if !(p.depth.is_finite() && p.depth > 1e-6) {
        return Err(bad("The depth of the text must be a positive number"));
    }
    let n = p.normal.normalize_or_zero();
    let x = (p.x_dir - n * p.x_dir.dot(n)).normalize_or_zero();
    if n == DVec3::ZERO || x == DVec3::ZERO {
        return Err(bad("The plane of the text is not valid"));
    }
    let (shapes, _) = rolidium_text::text_shapes(&p.text, &p.style()).map_err(|e| bad(e.to_string()))?;
    let (start, dist) = if p.engrave {
        (p.origin + n * OVER, -(p.depth + OVER))
    } else {
        (p.origin - n * OVER, p.depth + OVER)
    };
    // the outlines are the curves of the font (exact Bezier curves), so a letter has a face for every piece of it, not for every short line
    let to_loop = |segs: &[rolidium_text::Seg]| -> Vec<ProfileSegment> {
        use rolidium_text::Run;
        let q = |p: glam::DVec2| [p.x, p.y];
        rolidium_text::merge_runs(segs)
            .into_iter()
            .map(|g| match g {
                Run::Line(a, b) => ProfileSegment::Line { a: q(a), b: q(b) },
                Run::Quad(a, c, b) => ProfileSegment::Quad { a: q(a), c: q(c), b: q(b) },
                Run::Cubic(a, c1, c2, b) => ProfileSegment::Cubic { a: q(a), c1: q(c1), c2: q(c2), b: q(b) },
                Run::Spline2(p, k) => ProfileSegment::Spline2 {
                    poles: p.into_iter().map(q).collect(),
                    knots: k,
                },
            })
            .collect()
    };
    let mut solids: Vec<Solid> = Vec::new();
    for (k, s) in shapes.iter().enumerate() {
        crate::progress::set("Making the letters", 0.05 + 0.5 * k as f32 / shapes.len().max(1) as f32);
        let mut loops = vec![to_loop(&s.outer_curves)];
        loops.extend(s.hole_curves.iter().map(|h| to_loop(h)));
        let prism = Solid::extrude_profile(&Profile { loops }, start, n, x, dist)?;
        solids.push(prism);
    }
    if solids.len() == 1 {
        return Ok(solids.remove(0));
    }
    // letters that touch (the parts of an accented letter that overlap) are fused first, so that the tool is a clean set of separate solids; only the pairs whose
    // boxes meet are looked at (the boxes are made once)
    let mut boxes: Vec<(DVec3, DVec3)> = Vec::with_capacity(solids.len());
    for s in &solids {
        boxes.push(s.bounding_box()?);
    }
    let meet = |a: &(DVec3, DVec3), b: &(DVec3, DVec3)| {
        let g = DVec3::splat(1e-6);
        (a.0 - g).cmple(b.1).all() && (b.0 - g).cmple(a.1).all()
    };
    let mut i = 0;
    while i < solids.len() {
        let mut merged = false;
        for j in (i + 1)..solids.len() {
            if meet(&boxes[i], &boxes[j]) && solids[i].distance_to(&solids[j]).map(|d| d < 1e-6).unwrap_or(false) {
                let f = solids[i].boolean_checked(&solids[j], BooleanOp::Fuse)?.solid;
                boxes[i] = f.bounding_box()?;
                solids[i] = f;
                solids.remove(j);
                boxes.remove(j);
                merged = true;
                break;
            }
        }
        if !merged {
            i += 1;
        }
    }
    let refs: Vec<&Solid> = solids.iter().collect();
    Ok(Solid::compound(&refs)?)
}

/// The text raised from `base` or engraved into it.
pub fn apply_text(base: &Solid, p: &TextParams) -> Result<Solid, DocError> {
    let tool = text_tool(p)?;
    crate::progress::set("Joining the letters to the body", 0.6);
    let op = if p.engrave {
        BooleanOp::Cut
    } else {
        BooleanOp::Fuse
    };
    // the faces of the letters are not coplanar with anything that could be merged: no unifying of faces (it took a tenth of the time for nothing)
    Ok(base.boolean_unify_opt(&tool, op, false)?.solid)
}
