// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    hover: u32,
    selected_count: u32,
    pad0: u32,
    pad1: u32,
    selected: array<vec4<u32>, 2>,
    body_marks: array<vec4<u32>, 4>,
    grid_step: vec4<f32>,
    grid_o: vec4<f32>,
    grid_u: vec4<f32>,
    grid_v: vec4<f32>,
    viewport: vec4<f32>,
}
@group(0) @binding(0) var<uniform> g: Globals;

struct Body {
    color: vec4<f32>,
}
@group(1) @binding(0) var<uniform> body: Body;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) id: u32,
}
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) id: u32,
}

@vertex
fn vs_mesh(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.normal = v.normal;
    o.id = v.id;
    return o;
}

fn is_selected(id: u32) -> bool {
    for (var i = 0u; i < g.selected_count; i = i + 1u) {
        if (g.selected[i / 4u][i % 4u] == id) { return true; }
    }
    return false;
}

fn body_tint(id: u32, c: vec3<f32>) -> vec3<f32> {
    let slot = (id >> 20u);
    var out = c;
    for (var i = 0u; i < 16u; i = i + 1u) {
        let e = g.body_marks[i / 4u][i % 4u];
        if (e != 0u && (e & 0xffffffu) == slot) {
            let k = e >> 24u;
            if (k == 1u) { out = mix(c, vec3<f32>(0.20, 0.50, 1.0), 0.55); }
            else if (k == 2u) { out = mix(c, vec3<f32>(0.20, 0.85, 1.0), 0.55); }
            else if (k == 3u) { out = mix(c, vec3<f32>(0.95, 0.35, 0.95), 0.55); }
            else { out = mix(c, vec3<f32>(1.0, 0.72, 0.25), 0.50); }
        }
    }
    return out;
}

@fragment
fn fs_mesh(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(i.normal);
    let view = normalize(g.eye.xyz - i.world);
    if (dot(n, view) < 0.0) { n = -n; }
    let key = normalize(vec3<f32>(0.4, -0.5, 0.8));
    let diffuse = 0.30 + 0.50 * max(dot(n, view), 0.0) + 0.25 * max(dot(n, key), 0.0);
    var c = body.color.rgb * diffuse;
    if (i.id == g.hover) { c = mix(c, vec3<f32>(1.0, 0.72, 0.25), 0.55); }
    if (is_selected(i.id)) { c = mix(c, vec3<f32>(0.20, 0.50, 1.0), 0.65); }
    c = body_tint(i.id, c);
    return vec4<f32>(c, 1.0);
}

// ---- face ID pass ----
@fragment
fn fs_id(i: VOut) -> @location(0) u32 {
    return i.id;
}

// ---- lines ----
struct LIn { @location(0) pos: vec3<f32> }
struct LOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec4<f32> }

struct LineStyle { color: vec4<f32> }
@group(1) @binding(0) var<uniform> line_style: LineStyle;

@vertex
fn vs_line(v: LIn) -> LOut {
    var o: LOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    // Pull lines slightly towards the camera so they win over coplanar faces.
    o.clip.z = o.clip.z - 0.00005 * o.clip.w;
    o.color = line_style.color;
    return o;
}

@fragment
fn fs_line(i: LOut) -> @location(0) vec4<f32> {
    return i.color;
}

// ---- thick lines: one instance per segment, expanded into a screen-space quad ----
struct TIn {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @builtin(vertex_index) vi: u32,
}

@vertex
fn vs_thick(i: TIn) -> LOut {
    var o: LOut;
    let ca = g.view_proj * vec4<f32>(i.a, 1.0);
    let cb = g.view_proj * vec4<f32>(i.b, 1.0);
    let na = ca.xy / ca.w;
    let nb = cb.xy / cb.w;
    let d = (nb - na) * g.viewport.xy;
    var dir = vec2<f32>(1.0, 0.0);
    if (length(d) > 1e-6) { dir = normalize(d); }
    let nrm = vec2<f32>(-dir.y, dir.x);
    // corner k of the two triangles of the quad: end b for k = 1, 2, 4; side -1 for k = 0, 1, 3 and +1 otherwise
    // (written without local arrays: dynamic indexing of those misbehaves on some Direct3D drivers)
    let k = i.vi % 6u;
    let at_b = (k == 1u) || (k == 2u) || (k == 4u);
    var side = 1.0;
    if ((k == 0u) || (k == 1u) || (k == 3u)) { side = -1.0; }
    var base = ca;
    if (at_b) { base = cb; }
    let off = nrm * side * g.viewport.z * 0.5 * 2.0 / g.viewport.xy * base.w;
    o.clip = vec4<f32>(base.xy + off, base.z - 0.0003 * base.w, base.w);
    o.color = line_style.color;
    return o;
}

// ---- preview triangles (translucent) ----
@vertex
fn vs_prev(v: LIn) -> LOut {
    var o: LOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.clip.z = o.clip.z - 0.0001 * o.clip.w;
    o.color = line_style.color;
    return o;
}

// ---- infinite grid ----
struct GOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_grid(@builtin(vertex_index) vi: u32) -> GOut {
    // one big triangle covering the screen: (-1,-1), (3,-1), (-1,3)
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u)) * 2.0 - vec2<f32>(1.0, 1.0);
    var o: GOut;
    o.clip = vec4<f32>(p, 0.0, 1.0);
    o.ndc = p;
    return o;
}

struct GridOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// Strength of the lines of one grid level with the given spacing. Per axis: lines closer than about 8 pixels
// on the screen fade out completely (so a grid seen at a grazing angle never turns into moire), full strength
// from 16 pixels. A coarser level then takes over, which is why the grid stays readable far away.
fn grid_level(coord: vec2<f32>, step: f32) -> f32 {
    let c = coord / step;
    let w = fwidth(c);
    let a = abs(fract(c - 0.5) - 0.5) / max(w, vec2<f32>(1e-6));
    let line = vec2<f32>(1.0) - min(a, vec2<f32>(1.0));
    let density = vec2<f32>(1.0) - smoothstep(vec2<f32>(1.0 / 16.0), vec2<f32>(1.0 / 8.0), w);
    return max(line.x * density.x, line.y * density.y);
}

@fragment
fn fs_grid(i: GOut) -> GridOut {
    var out: GridOut;
    let a = g.inv_view_proj * vec4<f32>(i.ndc, 0.0, 1.0);
    let b = g.inv_view_proj * vec4<f32>(i.ndc, 1.0, 1.0);
    let p0 = a.xyz / a.w;
    let p1 = b.xyz / b.w;
    let d = p1 - p0;
    let gn = cross(g.grid_u.xyz, g.grid_v.xyz);
    let den = dot(d, gn);
    // No early return anywhere: the derivatives below must be computed by every pixel (uniform control flow).
    let ok_den = abs(den) >= 1e-9;
    var safe_den = den;
    if (!ok_den) { safe_den = 1.0; }
    let t = dot(g.grid_o.xyz - p0, gn) / safe_den;
    let visible = ok_den && (t >= 0.0);
    var tt = t;
    if (!visible) { tt = 0.0; }
    let pw = p0 + d * tt;
    let rel = pw - g.grid_o.xyz;
    let p = vec3<f32>(dot(rel, g.grid_u.xyz), dot(rel, g.grid_v.xyz), 0.0);
    let clip = g.view_proj * vec4<f32>(pw, 1.0);
    var depth = 1.0;
    if (visible) { depth = clamp(clip.z / clip.w, 0.0, 1.0); }
    out.depth = depth;

    let s = g.grid_step.x;
    let major_step = g.grid_step.z;
    // minor, major and four coarser levels (x10 each), the coarse ones a little stronger
    var strength = grid_level(p.xy, s) * 0.16;
    strength = max(strength, grid_level(p.xy, major_step) * 0.30);
    strength = max(strength, grid_level(p.xy, major_step * 10.0) * 0.34);
    strength = max(strength, grid_level(p.xy, major_step * 100.0) * 0.38);
    strength = max(strength, grid_level(p.xy, major_step * 1000.0) * 0.40);
    strength = max(strength, grid_level(p.xy, major_step * 10000.0) * 0.40);
    let dist = length(pw - g.eye.xyz);
    let fade = 1.0 - smoothstep(g.grid_step.y * 0.5, g.grid_step.y, dist);

    var col = vec3<f32>(0.62, 0.64, 0.68);
    var alpha = strength * fade;

    let wy = max(fwidth(p.y), 1e-6);
    let wx = max(fwidth(p.x), 1e-6);
    let ax = 1.0 - min(abs(p.y) / wy, 1.0);   // X axis lies along y = 0
    let ay = 1.0 - min(abs(p.x) / wx, 1.0);   // Y axis lies along x = 0
    if (ax > 0.0) { col = mix(col, vec3<f32>(0.90, 0.25, 0.25), ax); alpha = max(alpha, ax * 0.95); }
    if (ay > 0.0) { col = mix(col, vec3<f32>(0.30, 0.78, 0.30), ay); alpha = max(alpha, ay * 0.95); }
    if (!visible) { alpha = 0.0; }
    out.color = vec4<f32>(col, alpha);
    return out;
}
