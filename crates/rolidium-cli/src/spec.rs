// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The table of commands: the single source of truth for the script language of `rolidium-cli` and for the tools of the
//! MCP server. Every row gives the script word, the tool name, a description meant for a language model, the parameters
//! (type, unit, range) and how the parameters are written in a script line. The MCP server turns a tool call into such a
//! line and runs it through the very same [`crate::Runner::run_line`] that runs scripts, so the two cannot diverge; the
//! JSON schema of a tool is generated from its row.

use serde_json::{json, Map, Value};

/// What a parameter holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A real number.
    Num,
    /// A non-negative whole number.
    Int,
    /// A word or a text without spaces.
    Text,
    /// One of the listed words.
    Enum(&'static [&'static str]),
    /// A switch: written as its keyword when true.
    Flag,
    /// A path of a file, resolved under the root of the server.
    Path,
    /// A list of numbers written one after another.
    Nums,
    /// Several words copied into the line as they are (for example `point 1 2 3`).
    Words,
}

#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    /// Meaning for the model, with the unit.
    pub doc: &'static str,
    /// mm, degrees, g/cm3 ... or "".
    pub unit: &'static str,
    pub required: bool,
    /// Keyword written before the value in a script line (`radius 5`); `None`: the value alone, by position.
    pub word: Option<&'static str>,
    /// Allowed range for numbers (inclusive).
    pub range: Option<(f64, f64)>,
    /// For lists: the least and the most items.
    pub items: Option<(usize, usize)>,
}

const fn p(name: &'static str, kind: Kind, doc: &'static str) -> Param {
    Param {
        name,
        kind,
        doc,
        unit: "",
        required: true,
        word: None,
        range: None,
        items: None,
    }
}

impl Param {
    const fn unit(mut self, u: &'static str) -> Self {
        self.unit = u;
        self
    }
    const fn opt(mut self) -> Self {
        self.required = false;
        self
    }
    const fn word(mut self, w: &'static str) -> Self {
        self.word = Some(w);
        self
    }
    const fn range(mut self, lo: f64, hi: f64) -> Self {
        self.range = Some((lo, hi));
        self
    }
    const fn items(mut self, lo: usize, hi: usize) -> Self {
        self.items = Some((lo, hi));
        self
    }
}

/// How a row is carried out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Exec {
    /// A script line run by the runner.
    Line,
    /// The server does it itself (the picture is returned, not written to a file).
    Special,
}

#[derive(Debug, Clone, Copy)]
pub struct Cmd {
    /// The script word.
    pub name: &'static str,
    /// The name of the MCP tool.
    pub tool: &'static str,
    /// Description for the model: what it does, in which units, what to do next.
    pub summary: &'static str,
    pub params: &'static [Param],
    /// The operation changes the model (false: it only reads).
    pub changes: bool,
    /// The operation writes or reads files.
    pub files: bool,
    pub exec: Exec,
    /// A script line that works in a fresh document after the `setup` lines (used by the tests).
    pub example: &'static str,
}

const AXIS: Kind = Kind::Enum(&["x", "y", "z"]);
const MODE: Kind = Kind::Enum(&["auto", "add", "cut"]);
const BODY: Param = p(
    "body",
    Kind::Int,
    "Number of the body (1, 2, ... as listed by describe_model). Default: the first body.",
)
.opt()
.word("body")
.range(1.0, 100000.0);

const fn c(
    name: &'static str,
    tool: &'static str,
    summary: &'static str,
    params: &'static [Param],
    changes: bool,
    example: &'static str,
) -> Cmd {
    Cmd {
        name,
        tool,
        summary,
        params,
        changes,
        files: false,
        exec: Exec::Line,
        example,
    }
}

const fn files(mut cmd: Cmd) -> Cmd {
    cmd.files = true;
    cmd
}

const COORD: (f64, f64) = (-1.0e6, 1.0e6);

pub static COMMANDS: &[Cmd] = &[
    c(
        "new",
        "new_document",
        "Start an empty document. Everything not saved is dropped.",
        &[],
        true,
        "new",
    ),
    files(c(
        "open",
        "open_document",
        "Open a .rolidium file (a path under the root directory of the server). Replaces the current document.",
        &[p("path", Kind::Path, "File to open, for example part.rolidium")],
        true,
        "",
    )),
    files(c(
        "save",
        "save_document",
        "Save the document as a .rolidium file under the root directory. Without a path the file it was opened from or last saved to is used.",
        &[p("path", Kind::Path, "File to write, for example part.rolidium").opt()],
        false,
        "",
    )),
    files(c(
        "export-stl",
        "export_stl",
        "Write a body as a binary STL file (for 3D printing). Units are mm.",
        &[
            p("path", Kind::Path, "File to write, for example part.stl"),
            p("deviation", Kind::Num, "Largest distance of the triangles from the true surface")
                .unit("mm")
                .opt()
                .word("--deviation")
                .range(0.001, 10.0),
            p("angle", Kind::Num, "Largest angle between neighbouring triangles")
                .unit("degrees")
                .opt()
                .word("--angle")
                .range(1.0, 90.0),
            p("ascii", Kind::Flag, "Write text STL instead of binary").opt().word("--ascii"),
            BODY,
        ],
        false,
        "",
    )),
    files(c(
        "export-step",
        "export_step",
        "Write a body as a STEP file (for other CAD programs).",
        &[p("path", Kind::Path, "File to write, for example part.step"), BODY],
        false,
        "",
    )),
    files(c(
        "import-step",
        "import_step",
        "Add the solids of a STEP file as new bodies.",
        &[p("path", Kind::Path, "STEP file to read")],
        true,
        "",
    )),
    c(
        "box",
        "add_box",
        "Add a box (a body) with one corner at the origin and the opposite corner at (x, y, z). Units mm.",
        &[
            p("x", Kind::Num, "Size along X").unit("mm").range(0.001, 1.0e5),
            p("y", Kind::Num, "Size along Y").unit("mm").range(0.001, 1.0e5),
            p("z", Kind::Num, "Size along Z").unit("mm").range(0.001, 1.0e5),
        ],
        true,
        "box 10 20 30",
    ),
    c(
        "sketch",
        "start_sketch",
        "Choose where the next sketch shapes (line, rect, circle, arc) are drawn: a base plane (xy, xz or yz, optionally moved along its normal) or a flat face of a body \
(top, bottom, front, back, left, right = the highest planar face looking that way, or #N for the face with id N from describe_model). \
Shapes drawn afterwards belong to one sketch until the next start_sketch. Plane coordinates: on xy the sketch x is X and y is Y; on xz x is X and y is Z; on yz x is Y and y is Z.",
        &[
            p("on", Kind::Text, "xy, xz, yz, top, bottom, front, back, left, right or #N (face id)"),
            p("offset", Kind::Num, "Distance of the plane from the base plane along its normal (only for xy, xz, yz)")
                .unit("mm")
                .opt()
                .range(-1.0e5, 1.0e5),
            BODY,
        ],
        false,
        "sketch xy",
    ),
    c(
        "line",
        "sketch_line",
        "Draw a straight line in the current sketch. Coordinates are in the plane of the sketch, in mm. Lines that close a loop make a region that can be pulled.",
        &[
            p("x1", Kind::Num, "Start x").unit("mm").range(COORD.0, COORD.1),
            p("y1", Kind::Num, "Start y").unit("mm").range(COORD.0, COORD.1),
            p("x2", Kind::Num, "End x").unit("mm").range(COORD.0, COORD.1),
            p("y2", Kind::Num, "End y").unit("mm").range(COORD.0, COORD.1),
        ],
        true,
        "line 0 0 10 0",
    ),
    c(
        "rect",
        "sketch_rectangle",
        "Draw an axis-aligned rectangle in the current sketch from one corner to the opposite one (a closed region).",
        &[
            p("x1", Kind::Num, "First corner x").unit("mm").range(COORD.0, COORD.1),
            p("y1", Kind::Num, "First corner y").unit("mm").range(COORD.0, COORD.1),
            p("x2", Kind::Num, "Opposite corner x").unit("mm").range(COORD.0, COORD.1),
            p("y2", Kind::Num, "Opposite corner y").unit("mm").range(COORD.0, COORD.1),
        ],
        true,
        "rect 0 0 10 5",
    ),
    c(
        "circle",
        "sketch_circle",
        "Draw a circle in the current sketch (a closed region). The size is the DIAMETER, not the radius.",
        &[
            p("cx", Kind::Num, "Centre x").unit("mm").range(COORD.0, COORD.1),
            p("cy", Kind::Num, "Centre y").unit("mm").range(COORD.0, COORD.1),
            p("diameter", Kind::Num, "Diameter").unit("mm").range(0.001, 1.0e5),
        ],
        true,
        "circle 0 0 10",
    ),
    c(
        "arc",
        "sketch_arc",
        "Draw a circular arc in the current sketch through three points: start, a point on the arc, end. The points must not lie on one line.",
        &[
            p("x1", Kind::Num, "Start x").unit("mm").range(COORD.0, COORD.1),
            p("y1", Kind::Num, "Start y").unit("mm").range(COORD.0, COORD.1),
            p("xm", Kind::Num, "Point on the arc x").unit("mm").range(COORD.0, COORD.1),
            p("ym", Kind::Num, "Point on the arc y").unit("mm").range(COORD.0, COORD.1),
            p("x2", Kind::Num, "End x").unit("mm").range(COORD.0, COORD.1),
            p("y2", Kind::Num, "End y").unit("mm").range(COORD.0, COORD.1),
        ],
        true,
        "arc 0 0 5 5 10 0",
    ),
    c(
        "regions",
        "list_regions",
        "List the closed regions of the current sketch (index, area in mm2, holes). The index is what pull_region, sweep and revolve take.",
        &[],
        false,
        "",
    ),
    c(
        "pull-region",
        "pull_region",
        "Extrude a closed region of the current sketch along the normal of its plane by a distance (mm). A positive distance goes along the normal, a negative one against it. \
Mode auto joins the result to a body it touches or makes a new body, add joins, cut removes material from the bodies the region passes through (use it for holes and pockets).",
        &[
            p("region", Kind::Int, "Region index from list_regions (0 is the first)").range(0.0, 10000.0),
            p("distance", Kind::Num, "Extrusion distance, may be negative").unit("mm").range(-1.0e5, 1.0e5),
            p("mode", MODE, "auto, add or cut").opt(),
        ],
        true,
        "",
    ),
    c(
        "pull",
        "pull_face",
        "Push or pull a flat face of a body along its normal by a distance (mm): positive grows the body outwards, negative cuts into it. \
Mode auto lets the program decide, add forces adding material, cut forces removing it.",
        &[
            p("face", Kind::Text, "top, bottom, front, back, left, right or #N (face id from describe_model)"),
            p("distance", Kind::Num, "Distance along the outward normal").unit("mm").range(-1.0e5, 1.0e5),
            p("mode", MODE, "auto, add or cut").opt(),
            BODY,
        ],
        true,
        "",
    ),
    c(
        "round",
        "round_edges",
        "Round (fillet) edges with a radius in mm. Give the edge ids from describe_model or select_geometry (edges), or a point near the edge. \
Edges that continue each other smoothly are rounded together. A radius too large for the edge is refused: try a smaller one.",
        &[
            p("size", Kind::Num, "Radius").unit("mm").range(0.001, 1.0e4),
            p("edges", Kind::Nums, "Edge ids").opt().word("edges").items(1, 200),
            p("near", Kind::Nums, "A point x y z near the edge (instead of edges)").opt().word("near").items(3, 3),
            BODY,
        ],
        true,
        "",
    ),
    c(
        "chamfer",
        "chamfer_edges",
        "Chamfer (bevel) edges: cut the corner by a distance in mm, or with an angle (a second distance = distance times tan(angle)). \
Give the edge ids from describe_model or select_geometry, or a point near the edge.",
        &[
            p("size", Kind::Num, "Chamfer distance").unit("mm").range(0.001, 1.0e4),
            p("edges", Kind::Nums, "Edge ids").opt().word("edges").items(1, 200),
            p("near", Kind::Nums, "A point x y z near the edge (instead of edges)").opt().word("near").items(3, 3),
            p("angle", Kind::Num, "Angle of an unequal chamfer, between 1 and 89 degrees")
                .unit("degrees")
                .opt()
                .word("angle")
                .range(1.0, 89.0),
            p("flip", Kind::Flag, "Measure the distance on the other face").opt().word("flip"),
            BODY,
        ],
        true,
        "",
    ),
    c(
        "combine",
        "combine_bodies",
        "Combine bodies: merge (union) the tools into the target, cut (subtract) the tools from the target, or intersect (keep the common part). \
Bodies are numbered from 1 as in describe_model. Refused with an explanation when the bodies do not overlap or the result is impossible; the model is then unchanged.",
        &[
            p("op", Kind::Enum(&["merge", "cut", "intersect"]), "merge, cut or intersect"),
            p("target", Kind::Int, "Body number of the target").range(1.0, 100000.0),
            p("tools", Kind::Nums, "Body numbers of the tools").items(1, 100),
            p("keep", Kind::Flag, "Keep the tool bodies (cut and intersect)").opt().word("keep"),
        ],
        true,
        "",
    ),
    c(
        "move",
        "move_body",
        "Move a body by a vector (mm), or move a copy of it.",
        &[
            p("dx", Kind::Num, "Shift along X").unit("mm").range(COORD.0, COORD.1),
            p("dy", Kind::Num, "Shift along Y").unit("mm").range(COORD.0, COORD.1),
            p("dz", Kind::Num, "Shift along Z").unit("mm").range(COORD.0, COORD.1),
            p("copy", Kind::Flag, "Leave the original and move a copy").opt().word("copy"),
            BODY,
        ],
        true,
        "",
    ),
    c(
        "rotate",
        "rotate_body",
        "Rotate a body about an axis parallel to X, Y or Z through the centre of its bounding box (or through a given pivot), by an angle in degrees (right-hand rule).",
        &[
            p("axis", AXIS, "x, y or z"),
            p("angle", Kind::Num, "Angle").unit("degrees").range(-3600.0, 3600.0),
            p("pivot", Kind::Nums, "Pivot point x y z (default: the centre of the body)").opt().word("pivot").items(3, 3),
            p("copy", Kind::Flag, "Leave the original and rotate a copy").opt().word("copy"),
            BODY,
        ],
        true,
        "",
    ),
    c(
        "delete",
        "delete_item",
        "Delete a body, a sketch or a curve by its number (as listed by describe_model). Undo brings it back.",
        &[
            p("what", Kind::Enum(&["body", "sketch", "curve"]), "body, sketch or curve"),
            p("number", Kind::Int, "Its number, from 1").range(1.0, 100000.0),
        ],
        true,
        "",
    ),
    c(
        "curve",
        "add_3d_line",
        "Add a 3D line (a path, not a body) through points x y z; with a radius the corners are rounded. Use it as the path of sweep_along_path.",
        &[
            p("radius", Kind::Num, "Corner rounding radius (0: sharp corners)").unit("mm").opt().word("radius").range(0.0, 1.0e5),
            p("points", Kind::Nums, "Flat list x1 y1 z1 x2 y2 z2 ... (at least two points)").unit("mm").items(6, 3000),
        ],
        true,
        "curve 0 0 0 10 0 0 10 10 0",
    ),
    c(
        "helix",
        "add_helix",
        "Add a helix (a path, not a body): a spiral about an axis, for springs and threads. Give the radius or diameter, the pitch (rise per turn) and the number of turns or the total height. \
The helix starts at the +X side of its axis at the origin (move it with at). Use it as the path of sweep_along_path.",
        &[
            p("radius", Kind::Num, "Radius of the helix").unit("mm").opt().word("radius").range(0.001, 1.0e5),
            p("diameter", Kind::Num, "Diameter (instead of radius)").unit("mm").opt().word("diameter").range(0.002, 2.0e5),
            p("pitch", Kind::Num, "Rise per turn").unit("mm").word("pitch").range(0.001, 1.0e5),
            p("turns", Kind::Num, "Number of turns (may be fractional)").opt().word("turns").range(0.01, 1000.0),
            p("height", Kind::Num, "Total height (instead of turns)").unit("mm").opt().word("height").range(0.001, 1.0e6),
            p("left", Kind::Flag, "Left-handed").opt().word("left"),
            p("start", Kind::Num, "Start angle").unit("degrees").opt().word("start").range(-3600.0, 3600.0),
            p("cone", Kind::Num, "Half-angle of a conical helix").unit("degrees").opt().word("cone").range(-80.0, 80.0),
            p("at", Kind::Nums, "Origin x y z").unit("mm").opt().word("at").items(3, 3),
            p("axis", Kind::Nums, "Axis direction x y z (default 0 0 1)").opt().word("axis").items(3, 3),
        ],
        true,
        "helix radius 10 pitch 5 turns 3",
    ),
    c(
        "sweep",
        "sweep_along_path",
        "Sweep a closed region of the current sketch along a path: a 3D line / helix from add_3d_line / add_helix (curve number), or a chain of shapes of another sketch. \
The profile should sit at the start of the path and be perpendicular to it. Result: a new body (new), added to a body (add), or removed from one (cut). \
Fails with an explanation when the swept body would cut through itself (profile wider than the pitch, path bent too sharply).",
        &[
            p("region", Kind::Int, "Region index of the profile in the current sketch").range(0.0, 10000.0),
            p("curve", Kind::Int, "Curve number (from describe_model) used as the path").opt().word("curve").range(1.0, 100000.0),
            p("chain", Kind::Nums, "Alternative path: sketch number and shape number of a sketch chain").opt().word("chain").items(2, 2),
            p("mode", Kind::Enum(&["auto", "new", "add", "cut"]), "auto, new, add or cut").opt(),
            p("rounded", Kind::Flag, "Round the corners of the path").opt().word("rounded"),
        ],
        true,
        "",
    ),
    c(
        "loft",
        "loft_profiles",
        "Join the first closed region of every sketch, in the order the sketches were made, into one body (ruled = straight between profiles, smooth = a smooth surface).",
        &[
            p("kind", Kind::Enum(&["ruled", "smooth"]), "ruled or smooth"),
            p("mode", Kind::Enum(&["new", "add", "cut"]), "new, add or cut").opt(),
        ],
        true,
        "",
    ),
    c(
        "revolve",
        "revolve_region",
        "Revolve a closed region of the current sketch about an axis through the origin parallel to X, Y or Z (in the coordinates of the model, not of the sketch) by an angle. \
The axis must not pass through the region.",
        &[
            p("region", Kind::Int, "Region index in the current sketch").range(0.0, 10000.0),
            p("axis", AXIS, "x, y or z"),
            p("angle", Kind::Num, "Angle (360 for a full turn)").unit("degrees").range(0.1, 360.0),
            p("mode", MODE, "auto, add or cut").opt(),
        ],
        true,
        "",
    ),
    c(
        "measure",
        "measure",
        "Measure the distance (and angle, parallel-face distance, axes distance where they apply) between two elements of a body. Each element is written as one of: \
`point X Y Z`, `face top` / `face #N`, `edge #N` (ids from describe_model).",
        &[
            p("a", Kind::Words, "First element, for example `face top` or `edge #4` or `point 0 0 0`"),
            p("b", Kind::Words, "Second element"),
            BODY,
        ],
        false,
        "",
    ),
    c(
        "material",
        "set_material",
        "Set the material of a body, which decides its mass: steel, aluminium, brass, copper, PLA, PETG, ABS, or a density in g/cm3.",
        &[p("material", Kind::Text, "A material name or a density in g/cm3"), BODY],
        true,
        "",
    ),
    c(
        "mass",
        "mass",
        "Report the mass (g) of every body from its volume and material.",
        &[],
        false,
        "",
    ),
    c("undo", "undo", "Undo the last change of the model.", &[], true, ""),
    c("redo", "redo", "Redo the change that was undone.", &[], true, ""),
    c(
        "describe",
        "describe_model",
        "Describe the whole model with the numbers and ids other tools refer to: bodies (number, volume, mass, bounding box, faces with id, kind, area, centre, normal or axis; edges with id, kind, length, ends, the faces they join), \
sketches (plane, regions) and curves. Call it after each change that you want to build on. With a body number only that body is described in full; without it large bodies are summarised.",
        &[
            BODY,
            p("detail", Kind::Enum(&["summary", "full"]), "summary lists faces and edges only for small bodies; full always").opt(),
        ],
        false,
        "describe",
    ),
    c(
        "select",
        "select_geometry",
        "Find faces or edges of a body by a query and return their ids and geometry, for example the highest face looking up (`kind face`, `normal +z`, `extreme +z`) \
or the edges of a face (`kind edge`, `of_face 3`) or all cylindrical faces (`kind face`, `surface cylinder`). Filters combine.",
        &[
            p("kind", Kind::Enum(&["face", "edge"]), "face or edge"),
            p("normal", Kind::Text, "Planar faces looking this way: +x -x +y -y +z -z").opt().word("normal"),
            p("surface", Kind::Enum(&["plane", "cylinder", "cone", "sphere", "torus", "other"]), "Kind of surface (faces)")
                .opt()
                .word("surface"),
            p("extreme", Kind::Text, "Only the item farthest along this direction: +x -x +y -y +z -z").opt().word("extreme"),
            p("of_face", Kind::Int, "Only the edges of this face id").opt().word("of_face").range(0.0, 1.0e7),
            p("edge_kind", Kind::Enum(&["line", "circle", "other"]), "Kind of edge (edges)").opt().word("edge_kind"),
            p("min_area", Kind::Num, "Only faces at least this large").unit("mm2").opt().word("min_area").range(0.0, 1.0e12),
            p("limit", Kind::Int, "Most items returned (default 50)").opt().word("limit").range(1.0, 1000.0),
            BODY,
        ],
        false,
        "",
    ),
    Cmd {
        name: "render",
        tool: "render_view",
        summary: "Draw the model to a PNG picture and return it as an image. Works without a window or a graphics card. \
View: iso, top, bottom, front, back, left, right, or custom with azimuth and elevation. The camera frames the whole model. \
Optionally highlight faces and edges (orange) and label them with their ids (F12 = face 12, E5 = edge 5), which helps to check what a later operation will refer to. Sketch lines are drawn yellow, 3D lines violet.",
        params: &[
            p("view", Kind::Enum(&["iso", "top", "bottom", "front", "back", "left", "right", "custom"]), "Direction of view (default iso)").opt(),
            p("azimuth", Kind::Num, "Custom view: turn about Z (0 = looking from -Y, positive turns the camera to the right)")
                .unit("degrees")
                .opt()
                .range(-360.0, 360.0),
            p("elevation", Kind::Num, "Custom view: height of the camera above the XY plane")
                .unit("degrees")
                .opt()
                .range(-89.0, 89.0),
            p("width", Kind::Int, "Picture width in pixels (default 800)").opt().range(64.0, 2048.0),
            p("height", Kind::Int, "Picture height in pixels (default 600)").opt().range(64.0, 2048.0),
            p("highlight_faces", Kind::Nums, "Face ids to highlight and label").opt().items(1, 200),
            p("highlight_edges", Kind::Nums, "Edge ids to highlight and label").opt().items(1, 200),
            p("body", Kind::Int, "Number of the body the highlighted ids belong to (default 1)").opt().range(1.0, 100000.0),
            p("path", Kind::Path, "Also save the PNG here (under the root directory)").opt(),
        ],
        changes: false,
        files: false,
        exec: Exec::Special,
        example: "",
    },
];

pub fn find_by_name(name: &str) -> Option<&'static Cmd> {
    COMMANDS.iter().find(|c| c.name == name)
}

pub fn find_by_tool(tool: &str) -> Option<&'static Cmd> {
    COMMANDS.iter().find(|c| c.tool == tool)
}

fn kind_schema(p: &Param) -> Value {
    let mut o = Map::new();
    let mut desc = p.doc.to_string();
    if !p.unit.is_empty() {
        desc.push_str(&format!(" [{}]", p.unit));
    }
    match p.kind {
        Kind::Num => {
            o.insert("type".into(), json!("number"));
        }
        Kind::Int => {
            o.insert("type".into(), json!("integer"));
        }
        Kind::Text | Kind::Path | Kind::Words => {
            o.insert("type".into(), json!("string"));
        }
        Kind::Flag => {
            o.insert("type".into(), json!("boolean"));
        }
        Kind::Enum(values) => {
            o.insert("type".into(), json!("string"));
            o.insert("enum".into(), json!(values));
        }
        Kind::Nums => {
            o.insert("type".into(), json!("array"));
            o.insert("items".into(), json!({"type": "number"}));
            if let Some((lo, hi)) = p.items {
                o.insert("minItems".into(), json!(lo));
                o.insert("maxItems".into(), json!(hi));
            }
        }
    }
    if let Some((lo, hi)) = p.range {
        if matches!(p.kind, Kind::Num | Kind::Int) {
            o.insert("minimum".into(), json!(lo));
            o.insert("maximum".into(), json!(hi));
        }
    }
    o.insert("description".into(), json!(desc));
    Value::Object(o)
}

/// The JSON schema of the arguments of a tool.
pub fn input_schema(cmd: &Cmd) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for p in cmd.params {
        props.insert(p.name.into(), kind_schema(p));
        if p.required {
            required.push(p.name);
        }
    }
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

/// A short usage line of the script syntax, for the documentation and for error messages.
pub fn usage(cmd: &Cmd) -> String {
    let mut s = cmd.name.to_string();
    for p in cmd.params {
        let body = match p.kind {
            Kind::Flag => p.word.unwrap_or(p.name).to_string(),
            Kind::Nums => format!(
                "{}<{}...>",
                p.word.map(|w| format!("{w} ")).unwrap_or_default(),
                p.name
            ),
            _ => format!(
                "{}<{}>",
                p.word.map(|w| format!("{w} ")).unwrap_or_default(),
                p.name
            ),
        };
        if p.required {
            s.push_str(&format!(" {body}"));
        } else {
            s.push_str(&format!(" [{body}]"));
        }
    }
    s
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn arg_num(v: &Value, name: &str) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("'{name}' must be a number"))
}

/// Checks the arguments against the row and writes them as a script line. `resolve` turns a path into the checked path.
pub fn to_line(
    cmd: &Cmd,
    args: &Value,
    resolve: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    let empty = Map::new();
    let obj = match args {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return Err("the arguments must be an object".into()),
    };
    for k in obj.keys() {
        if !cmd.params.iter().any(|p| p.name == k) {
            let names: Vec<&str> = cmd.params.iter().map(|p| p.name).collect();
            return Err(format!(
                "unknown argument '{k}' for {}; the arguments are: {}",
                cmd.tool,
                names.join(", ")
            ));
        }
    }
    let mut words: Vec<String> = vec![cmd.name.to_string()];
    for p in cmd.params {
        let Some(v) = obj.get(p.name).filter(|v| !v.is_null()) else {
            if p.required {
                return Err(format!("the argument '{}' is missing ({})", p.name, p.doc));
            }
            continue;
        };
        let range_check = |x: f64| -> Result<(), String> {
            if let Some((lo, hi)) = p.range {
                if x < lo || x > hi {
                    return Err(format!(
                        "'{}' = {} is outside the allowed range {} to {}{}",
                        p.name,
                        fmt_num(x),
                        fmt_num(lo),
                        fmt_num(hi),
                        if p.unit.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", p.unit)
                        }
                    ));
                }
            }
            Ok(())
        };
        match p.kind {
            Kind::Num => {
                let x = arg_num(v, p.name)?;
                range_check(x)?;
                words.extend(p.word.map(String::from));
                words.push(fmt_num(x));
            }
            Kind::Int => {
                let x = v
                    .as_u64()
                    .or_else(|| {
                        v.as_f64()
                            .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                            .map(|f| f as u64)
                    })
                    .ok_or_else(|| format!("'{}' must be a whole number", p.name))?;
                range_check(x as f64)?;
                words.extend(p.word.map(String::from));
                words.push(x.to_string());
            }
            Kind::Flag => {
                if v.as_bool()
                    .ok_or_else(|| format!("'{}' must be true or false", p.name))?
                {
                    words.push(p.word.unwrap_or(p.name).to_string());
                }
            }
            Kind::Text => {
                let s = v.as_str().ok_or_else(|| format!("'{}' must be a text", p.name))?;
                check_word(s, p.name)?;
                words.extend(p.word.map(String::from));
                words.push(s.to_string());
            }
            Kind::Enum(values) => {
                let s = v.as_str().ok_or_else(|| format!("'{}' must be a text", p.name))?;
                if !values.contains(&s) {
                    return Err(format!("'{}' must be one of: {}", p.name, values.join(", ")));
                }
                words.extend(p.word.map(String::from));
                words.push(s.to_string());
            }
            Kind::Path => {
                let s = v.as_str().ok_or_else(|| format!("'{}' must be a text", p.name))?;
                let resolved = resolve(s)?;
                words.extend(p.word.map(String::from));
                words.push(format!("\"{resolved}\""));
            }
            Kind::Words => {
                let s = v.as_str().ok_or_else(|| format!("'{}' must be a text", p.name))?;
                if s.chars().any(|c| c.is_control() || c == '"' || c == '#') {
                    return Err(format!("'{}' contains characters that are not allowed", p.name));
                }
                words.extend(p.word.map(String::from));
                words.push(s.to_string());
            }
            Kind::Nums => {
                let arr = v
                    .as_array()
                    .ok_or_else(|| format!("'{}' must be a list of numbers", p.name))?;
                if let Some((lo, hi)) = p.items {
                    if arr.len() < lo || arr.len() > hi {
                        return Err(format!(
                            "'{}' needs {} to {} numbers, got {}",
                            p.name,
                            lo,
                            hi,
                            arr.len()
                        ));
                    }
                }
                words.extend(p.word.map(String::from));
                for x in arr {
                    let n = arg_num(x, p.name)?;
                    if n.abs() > 1.0e6 {
                        return Err(format!("'{}' has a value outside +-1000000", p.name));
                    }
                    words.push(fmt_num(n));
                }
            }
        }
    }
    Ok(words.join(" "))
}

fn check_word(s: &str, name: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 64
        || s.chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '#' && !s.starts_with('#'))
    {
        return Err(format!("'{name}' is not a valid word"));
    }
    Ok(())
}

/// The script commands the runner handles besides the rows of the table (kept for the old scenario scripts).
pub const SCRIPT_ONLY: &[&str] = &["curves", "features", "edit-feature", "info"];
