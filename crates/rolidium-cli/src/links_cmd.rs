// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The commands for patterns, mirrors, links and groups (the same words are the tools of the MCP server, see `spec.rs`).
//!
//! ```text
//! linear-pattern <count> <spacing> <dx> <dy> <dz> [total] [from <n>...] [second <count> <spacing> <dx> <dy> <dz> [total]] [add <n> | cut <n>] [name "Text"]
//! circular-pattern <count> <angle | full> <ax> <ay> <az> [through <x> <y> <z>] [from <n>...] [add <n> | cut <n>] [name "Text"]
//! mirror <x | y | z | normal <nx> <ny> <nz>> [at <offset> | through <x> <y> <z>] [from <n>...] [live | keep | merge] [name "Text"]
//! links | edit-link <n> [count <k>] [spacing <s>] [angle <a> | full] [total] | unlink <n> | delete-link <n>
//! group <body n>... [name "Text"] | groups | move-to-group <group n | none> <body n>... | ungroup <n>
//! sketch-pattern linear <count> <spacing> <dx> <dy> [second <count> <spacing> <dx> <dy>] | sketch-pattern circular <count> <angle | full> <cx> <cy>
//! sketch-mirror <ax> <ay> <bx> <by> [keep]
//! ```
//! Bodies, links and groups are numbered from 1 in the order of `info` / `links` / `groups`.

use super::*;
use rolidium_doc::{
    Axis, CircularParams, Direction2, LinearParams, LinkKind, MirrorParams, PatternMode, SketchPattern,
};

fn number(s: &str) -> Result<f64, String> {
    s.replace(',', ".")
        .parse::<f64>()
        .map_err(|_| format!("'{s}' is not a number"))
}

/// The numbers after position `i` up to the next word that is not a number.
fn numbers_from<'a>(w: &[&'a str], i: usize) -> Vec<&'a str> {
    w.iter()
        .skip(i)
        .take_while(|t| t.replace(',', ".").parse::<f64>().is_ok())
        .copied()
        .collect()
}

struct Cursor<'a, 'b> {
    w: &'b [&'a str],
    i: usize,
}

impl<'a, 'b> Cursor<'a, 'b> {
    fn num(&mut self, what: &str) -> Result<f64, String> {
        let t = self
            .w
            .get(self.i)
            .ok_or_else(|| format!("{} is missing: {what}", self.w[0]))?;
        self.i += 1;
        number(t)
    }
    fn vec3(&mut self, what: &str) -> Result<DVec3, String> {
        Ok(DVec3::new(self.num(what)?, self.num(what)?, self.num(what)?))
    }
    fn peek(&self) -> Option<&'a str> {
        self.w.get(self.i).copied()
    }
    fn eat(&mut self, word: &str) -> bool {
        if self.peek() == Some(word) {
            self.i += 1;
            true
        } else {
            false
        }
    }
}

impl Runner {
    fn body_number(&self, n: f64) -> Result<BodyId, String> {
        self.doc
            .bodies()
            .get((n as usize).wrapping_sub(1))
            .map(|b| b.id)
            .ok_or_else(|| {
                format!(
                    "there is no body {n}; the document has {} body(ies)",
                    self.doc.bodies().len()
                )
            })
    }

    fn link_number(&self, n: f64) -> Result<rolidium_doc::LinkId, String> {
        self.doc
            .state()
            .links
            .get((n as usize).wrapping_sub(1))
            .map(|l| l.id)
            .ok_or_else(|| {
                format!(
                    "there is no link {n}; the document has {} link(s)",
                    self.doc.state().links.len()
                )
            })
    }

    fn group_number(&self, n: f64) -> Result<rolidium_doc::GroupId, String> {
        self.doc
            .state()
            .groups
            .get((n as usize).wrapping_sub(1))
            .map(|g| g.id)
            .ok_or_else(|| {
                format!(
                    "there is no group {n}; the document has {} group(s)",
                    self.doc.state().groups.len()
                )
            })
    }

    /// The sources named after `from` (all the numbers that follow), else the first body (or `body N`).
    fn sources_of(&self, w: &[&str]) -> Result<Vec<BodyId>, String> {
        match w.iter().position(|t| *t == "from") {
            Some(i) => {
                let ns = numbers_from(w, i + 1);
                if ns.is_empty() {
                    return Err("'from' needs the numbers of the bodies".into());
                }
                ns.iter().map(|n| self.body_number(number(n)?)).collect()
            }
            None => Ok(vec![self.first_body()?]),
        }
    }

    fn mode_of(&self, w: &[&str]) -> Result<PatternMode, String> {
        for (word, add) in [("add", true), ("cut", false)] {
            if let Some(i) = w.iter().position(|t| *t == word) {
                let n = number(
                    w.get(i + 1)
                        .ok_or_else(|| format!("'{word}' needs the number of a body"))?,
                )?;
                let id = self.body_number(n)?;
                return Ok(if add {
                    PatternMode::Add(id)
                } else {
                    PatternMode::Cut(id)
                });
            }
        }
        Ok(PatternMode::New)
    }

    fn name_of(w: &[&str]) -> String {
        w.iter()
            .position(|t| *t == "name")
            .and_then(|i| w.get(i + 1))
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn report_link(&mut self, what: &str) {
        let st = self.doc.state().clone();
        if let Some(l) = st.links.last() {
            let bodies = l.results.len();
            self.say(format!(
                "{what}: link {} '{}' ({} body(ies) made, {} in the document){}",
                st.links.len(),
                l.name,
                bodies,
                st.bodies.len(),
                if l.warnings.is_empty() {
                    String::new()
                } else {
                    format!("; {}", l.warnings.join("; "))
                }
            ));
        }
    }

    /// Runs the commands of this module. `Ok(false)`: the word is not one of them.
    pub(crate) fn run_links_command(&mut self, w: &[&str]) -> Result<bool, String> {
        let err = |e: rolidium_doc::DocError| e.to_string();
        match w[0] {
            "linear-pattern" => {
                let mut c = Cursor { w, i: 1 };
                let count = c.num("the number of pieces")? as u32;
                let spacing = c.num("the spacing")?;
                let dir = c.vec3("the direction")?;
                let by_total = c.eat("total");
                let second = match w.iter().position(|t| *t == "second") {
                    Some(i) => {
                        let mut c2 = Cursor { w, i: i + 1 };
                        let (n, s) = (c2.num("the second count")? as u32, c2.num("the second spacing")?);
                        let d = c2.vec3("the second direction")?;
                        Some(Direction2 {
                            axis: Axis {
                                origin: DVec3::ZERO,
                                dir: d,
                            },
                            count: n,
                            spacing: s,
                            by_total: c2.eat("total"),
                        })
                    }
                    None => None,
                };
                let kind = LinkKind::Linear(
                    LinearParams {
                        axis: Axis {
                            origin: DVec3::ZERO,
                            dir,
                        },
                        count,
                        spacing,
                        by_total,
                        second,
                    },
                    self.mode_of(w)?,
                );
                let sources = self.sources_of(w)?;
                self.doc
                    .apply(Command::AddLink {
                        kind,
                        sources,
                        name: Self::name_of(w),
                    })
                    .map_err(err)?;
                self.report_link("linear-pattern");
            }
            "circular-pattern" => {
                let mut c = Cursor { w, i: 1 };
                let count = c.num("the number of pieces")? as u32;
                let (angle_deg, full) = if c.eat("full") {
                    (0.0, true)
                } else {
                    (c.num("the angle")?, false)
                };
                let dir = c.vec3("the axis")?;
                let origin = match w.iter().position(|t| *t == "through") {
                    Some(i) => Cursor { w, i: i + 1 }.vec3("the point of the axis")?,
                    None => DVec3::ZERO,
                };
                let kind = LinkKind::Circular(
                    CircularParams {
                        axis: Axis { origin, dir },
                        count,
                        angle_deg,
                        full,
                    },
                    self.mode_of(w)?,
                );
                let sources = self.sources_of(w)?;
                self.doc
                    .apply(Command::AddLink {
                        kind,
                        sources,
                        name: Self::name_of(w),
                    })
                    .map_err(err)?;
                self.report_link("circular-pattern");
            }
            "mirror" => {
                let mut c = Cursor { w, i: 1 };
                let normal = match c.peek() {
                    Some("x") => DVec3::X,
                    Some("y") => DVec3::Y,
                    Some("z") => DVec3::Z,
                    Some("normal") => {
                        c.i += 1;
                        c.vec3("the normal")?
                    }
                    _ => return Err("mirror needs the plane: x, y, z or normal <nx> <ny> <nz>".into()),
                };
                let origin = if let Some(i) = w.iter().position(|t| *t == "through") {
                    Cursor { w, i: i + 1 }.vec3("the point of the plane")?
                } else if let Some(i) = w.iter().position(|t| *t == "at") {
                    normal.normalize() * number(w.get(i + 1).ok_or("'at' needs an offset")?)?
                } else {
                    DVec3::ZERO
                };
                let sources = self.sources_of(w)?;
                let (live, keep, merge) = (w.contains(&"live"), w.contains(&"keep"), w.contains(&"merge"));
                if live {
                    let kind = LinkKind::Mirror(MirrorParams { origin, normal });
                    self.doc
                        .apply(Command::AddLink {
                            kind,
                            sources,
                            name: Self::name_of(w),
                        })
                        .map_err(err)?;
                    self.report_link("mirror");
                } else {
                    self.doc
                        .apply(Command::Mirror {
                            bodies: sources,
                            origin,
                            normal,
                            keep: keep || merge,
                            merge,
                        })
                        .map_err(err)?;
                    self.say(format!("mirror: {} body(ies)", self.doc.bodies().len()));
                }
            }
            "links" => {
                let st = self.doc.state().clone();
                if st.links.is_empty() {
                    self.say("no links");
                }
                for (i, l) in st.links.iter().enumerate() {
                    let nums = |ids: &[BodyId]| -> String {
                        ids.iter()
                            .filter_map(|id| st.bodies.iter().position(|b| b.id == *id))
                            .map(|p| (p + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    let what = match &l.kind {
                        LinkKind::Linear(p, _) => format!(
                            "linear, {} pieces, spacing {}{}{}",
                            p.count,
                            p.spacing,
                            if p.by_total { " (total)" } else { "" },
                            if p.second.is_some() { ", grid" } else { "" }
                        ),
                        LinkKind::Circular(p, _) => format!(
                            "circular, {} pieces, {}",
                            p.count,
                            if p.full {
                                "full circle".to_string()
                            } else {
                                format!("{} degrees apart", p.angle_deg)
                            }
                        ),
                        LinkKind::Mirror(_) => "mirror".to_string(),
                        LinkKind::Loft { profiles, .. } => format!("loft through {} profiles", profiles.len()),
                        LinkKind::Sweep { .. } => "sweep along a path".to_string(),
                    };
                    self.say(format!(
                        "link {}: '{}' {what}; from body {}; made body {}{}{}",
                        i + 1,
                        l.name,
                        nums(&l.sources),
                        nums(&l.results),
                        match l.kind.mode() {
                            PatternMode::New => String::new(),
                            PatternMode::Add(_) => " (added to it)".to_string(),
                            PatternMode::Cut(_) => " (cut from it)".to_string(),
                        },
                        l.error
                            .as_ref()
                            .map(|e| format!("; ERROR: {e}"))
                            .unwrap_or_default()
                    ));
                }
            }
            "edit-link" => {
                let mut c = Cursor { w, i: 1 };
                let n = c.num("the number of the link")?;
                let id = self.link_number(n)?;
                let l = self
                    .doc
                    .state()
                    .links
                    .iter()
                    .find(|l| l.id == id)
                    .unwrap()
                    .clone();
                let val = |word: &str| -> Result<Option<f64>, String> {
                    match w.iter().position(|t| *t == word) {
                        Some(i) => Ok(Some(number(
                            w.get(i + 1).ok_or_else(|| format!("'{word}' needs a number"))?,
                        )?)),
                        None => Ok(None),
                    }
                };
                let name = w
                    .iter()
                    .position(|t| *t == "name")
                    .and_then(|i| w.get(i + 1))
                    .map(|s| s.to_string());
                let kind = match &l.kind {
                    LinkKind::Linear(p, m) => {
                        let mut p = p.clone();
                        if let Some(k) = val("count")? {
                            p.count = k as u32;
                        }
                        if let Some(s) = val("spacing")? {
                            p.spacing = s;
                        }
                        if w.contains(&"total") {
                            p.by_total = true;
                        }
                        Some(LinkKind::Linear(p, *m))
                    }
                    LinkKind::Circular(p, m) => {
                        let mut p = p.clone();
                        if let Some(k) = val("count")? {
                            p.count = k as u32;
                        }
                        if w.contains(&"full") {
                            p.full = true;
                        } else if let Some(a) = val("angle")? {
                            p.angle_deg = a;
                            p.full = false;
                        }
                        Some(LinkKind::Circular(p, *m))
                    }
                    LinkKind::Mirror(m) => {
                        let mut m = m.clone();
                        if let Some(i) = w.iter().position(|t| *t == "through") {
                            m.origin = Cursor { w, i: i + 1 }.vec3("the point of the plane")?;
                        }
                        Some(LinkKind::Mirror(m))
                    }
                    // the profiles and the path are changed where they are (the sketch, the curve), not here
                    LinkKind::Loft { .. } | LinkKind::Sweep { .. } => None,
                };
                self.doc
                    .apply(Command::EditLink { link: id, kind, name })
                    .map_err(err)?;
                let l = self
                    .doc
                    .state()
                    .links
                    .iter()
                    .find(|l| l.id == id)
                    .unwrap()
                    .clone();
                self.say(format!(
                    "edit-link: '{}' now has {} body(ies); {} in the document{}",
                    l.name,
                    l.results.len(),
                    self.doc.bodies().len(),
                    if l.warnings.is_empty() {
                        String::new()
                    } else {
                        format!("; {}", l.warnings.join("; "))
                    }
                ));
            }
            "unlink" | "delete-link" => {
                let id = self.link_number(number(
                    w.get(1)
                        .ok_or_else(|| format!("{} needs the number of a link", w[0]))?,
                )?)?;
                let cmd = if w[0] == "unlink" {
                    Command::Unlink { link: id }
                } else {
                    Command::DeleteLink { link: id }
                };
                self.doc.apply(cmd).map_err(err)?;
                self.say(format!(
                    "{}: {} body(ies), {} link(s)",
                    w[0],
                    self.doc.bodies().len(),
                    self.doc.state().links.len()
                ));
            }
            "group" => {
                let ns = numbers_from(w, 1);
                if ns.is_empty() {
                    return Err("group needs the numbers of the bodies".into());
                }
                let bodies: Vec<BodyId> = ns
                    .iter()
                    .map(|n| self.body_number(number(n)?))
                    .collect::<Result<_, _>>()?;
                let parent = match w.iter().position(|t| *t == "in") {
                    Some(i) => Some(
                        self.group_number(number(w.get(i + 1).ok_or("'in' needs the number of a group")?)?)?,
                    ),
                    None => None,
                };
                self.doc
                    .apply(Command::CreateGroup {
                        bodies,
                        name: Self::name_of(w),
                        parent,
                    })
                    .map_err(err)?;
                let g = self.doc.state().groups.last().unwrap().clone();
                self.say(format!(
                    "group {}: '{}' with {} body(ies)",
                    self.doc.state().groups.len(),
                    g.name,
                    g.bodies.len()
                ));
            }
            "groups" => {
                let st = self.doc.state().clone();
                if st.groups.is_empty() {
                    self.say("no groups");
                }
                for (i, g) in st.groups.iter().enumerate() {
                    let nums: Vec<String> = g
                        .bodies
                        .iter()
                        .filter_map(|id| st.bodies.iter().position(|b| b.id == *id))
                        .map(|p| (p + 1).to_string())
                        .collect();
                    let parent = g
                        .parent
                        .and_then(|p| st.groups.iter().position(|x| x.id == p))
                        .map(|p| format!(", in group {}", p + 1))
                        .unwrap_or_default();
                    self.say(format!(
                        "group {}: '{}'{parent}, bodies {}{}",
                        i + 1,
                        g.name,
                        nums.join(" "),
                        if g.visible { "" } else { " (hidden)" }
                    ));
                }
            }
            "move-to-group" => {
                let target = match w.get(1) {
                    Some(&"none") => None,
                    Some(n) => Some(self.group_number(number(n)?)?),
                    None => {
                        return Err("move-to-group needs a group number (or none) and body numbers".into())
                    }
                };
                let ns = numbers_from(w, 2);
                if ns.is_empty() {
                    return Err("move-to-group needs the numbers of the bodies".into());
                }
                let bodies: Vec<BodyId> = ns
                    .iter()
                    .map(|n| self.body_number(number(n)?))
                    .collect::<Result<_, _>>()?;
                self.doc
                    .apply(Command::MoveToGroup {
                        bodies,
                        group: target,
                    })
                    .map_err(err)?;
                self.say(format!("move-to-group: {} body(ies) moved", ns.len()));
            }
            "ungroup" => {
                let id =
                    self.group_number(number(w.get(1).ok_or("ungroup needs the number of a group")?)?)?;
                self.doc.apply(Command::Ungroup { group: id }).map_err(err)?;
                self.say(format!(
                    "ungroup: {} group(s) left",
                    self.doc.state().groups.len()
                ));
            }
            "sketch-pattern" => {
                let sketch = self.sketch.ok_or("there is no current sketch")?;
                let mut c = Cursor { w, i: 1 };
                let kind = c.peek().ok_or("sketch-pattern needs linear or circular")?;
                c.i += 1;
                let pattern = match kind {
                    "linear" => {
                        let (count, spacing) = (c.num("the number of pieces")? as u32, c.num("the spacing")?);
                        let dir = rolidium_doc::rolidium_sketch::DVec2::new(c.num("dx")?, c.num("dy")?);
                        let by_total = c.eat("total");
                        let second = match w.iter().position(|t| *t == "second") {
                            Some(i) => {
                                let mut c2 = Cursor { w, i: i + 1 };
                                let (n, s) =
                                    (c2.num("the second count")? as u32, c2.num("the second spacing")?);
                                let d =
                                    rolidium_doc::rolidium_sketch::DVec2::new(c2.num("dx")?, c2.num("dy")?);
                                Some((d, n, s, c2.eat("total")))
                            }
                            None => None,
                        };
                        SketchPattern::Linear {
                            dir,
                            count,
                            spacing,
                            by_total,
                            second,
                        }
                    }
                    "circular" => {
                        let count = c.num("the number of pieces")? as u32;
                        let (angle_deg, full) = if c.eat("full") {
                            (0.0, true)
                        } else {
                            (c.num("the angle")?, false)
                        };
                        let center = rolidium_doc::rolidium_sketch::DVec2::new(c.num("cx")?, c.num("cy")?);
                        SketchPattern::Circular {
                            center,
                            count,
                            angle_deg,
                            full,
                        }
                    }
                    o => return Err(format!("unknown pattern '{o}' (linear or circular)")),
                };
                self.doc
                    .apply(Command::SketchPattern {
                        sketch,
                        entities: vec![],
                        pattern,
                    })
                    .map_err(err)?;
                let n = self
                    .doc
                    .sketch(sketch)
                    .map(|s| s.sketch.entities().len())
                    .unwrap_or(0);
                self.say(format!(
                    "sketch-pattern: the sketch has {n} shape(s), {} region(s)",
                    self.doc
                        .sketch(sketch)
                        .map(|s| s.sketch.regions().len())
                        .unwrap_or(0)
                ));
            }
            "sketch-mirror" => {
                let sketch = self.sketch.ok_or("there is no current sketch")?;
                let mut c = Cursor { w, i: 1 };
                let a = rolidium_doc::rolidium_sketch::DVec2::new(c.num("ax")?, c.num("ay")?);
                let b = rolidium_doc::rolidium_sketch::DVec2::new(c.num("bx")?, c.num("by")?);
                self.doc
                    .apply(Command::SketchMirror {
                        sketch,
                        entities: vec![],
                        a,
                        b,
                        keep: c.eat("keep"),
                    })
                    .map_err(err)?;
                self.say(format!(
                    "sketch-mirror: the sketch has {} shape(s)",
                    self.doc
                        .sketch(sketch)
                        .map(|s| s.sketch.entities().len())
                        .unwrap_or(0)
                ));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
