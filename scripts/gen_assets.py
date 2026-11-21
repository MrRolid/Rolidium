#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Generates the icon set (24x24 SVG), help-card illustrations, tools.json and icons-preview.html.

Colour tokens (#NEUTRAL, #ACCENT) are replaced by the application according to the theme.
Run from the repository root:  python3 scripts/gen_assets.py
"""
import json, math, os

N, A = "#NEUTRAL", "#ACCENT"
HEAD = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" '
        'stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75">')


# The icons of the File block are drawn with thin lines (about half of the usual width).
FILE_BLOCK_ICONS = {"file_new", "file_open", "file_save", "file_save_as", "import_step", "export_step", "export_stl"}


def neutral(*els):
    return f'<g stroke="{N}">' + "".join(els) + "</g>"


def accent(*els):
    return f'<g stroke="{A}">' + "".join(els) + "</g>"


def path(d, fill=None, extra=""):
    f = f' fill="{fill[0]}" fill-opacity="{fill[1]}"' if fill else ""
    return f'<path d="{d}"{f}{extra}/>'


CUBE_TOP = "M12 3 L20 7.5 L12 12 L4 7.5 Z"
CUBE_LEFT = "M4 7.5 L12 12 L12 21 L4 16.5 Z"
CUBE_RIGHT = "M12 12 L20 7.5 L20 16.5 L12 21 Z"
ACC_FILL = (A, ".3")


def gear_path(teeth=8, r_out=9.5, r_in=7.2, cx=12, cy=12):
    pts = []
    for i in range(teeth):
        a0 = 2 * math.pi * i / teeth
        w = 2 * math.pi / teeth
        for frac, r in ((0.0, r_in), (0.22, r_out), (0.5, r_out), (0.72, r_in)):
            a = a0 + frac * w
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + " Z"


GEAR = gear_path()

def letter(ch, x, y, w, h):
    """Block letters as stroked paths, so the exchange icons do not depend on any font."""
    x, y, w, h = (round(v, 2) for v in (x, y, w, h))
    if ch == "S":
        return f"M{x+w} {y} H{x} V{y+h/2} H{x+w} V{y+h} H{x}"
    if ch == "T":
        return f"M{x} {y} H{x+w} M{x+w/2} {y} V{y+h}"
    if ch == "E":
        return f"M{x+w} {y} H{x} V{y+h} H{x+w} M{x} {y+h/2} H{x+w*0.8}"
    if ch == "P":
        return f"M{x} {y+h} V{y} H{x+w} V{y+h/2} H{x}"
    if ch == "L":
        return f"M{x} {y} V{y+h} H{x+w}"
    raise ValueError(ch)


def word(text, x0, y, w, h, pitch):
    return " ".join(letter(c, x0 + i * pitch, y, w, h) for i, c in enumerate(text))


# The exchange icons: a small sheet with an arrow on top, the format name in large letters BELOW it (not inside it), with a margin
# to the edge of the icon, so that it stays readable when the icon is drawn large.
SHEET = "M7 1.5 H14 L18.5 6 V14.5 H7 Z M14 1.5 V6 H18.5"
ARROW_UP = "M12.75 12.5 V8 M10.9 9.9 L12.75 8 L14.6 9.9"
ARROW_DOWN = "M12.75 8 V12.5 M10.9 10.6 L12.75 12.5 L14.6 10.6"
THICK = ' stroke-width="1.0"'


def exchange(text, arrow, w, pitch, x0):
    return neutral(path(SHEET), path(word(text, x0, 16.6, w, 6, pitch), extra=THICK)) + accent(path(arrow))


icons = {
    "select": neutral(path("M6 3 L6 18 L10 14.2 L13 20.5 L15.4 19.4 L12.5 13.2 L18 12.8 Z")) +
              accent(path("M15 3.5 H20 V8.5 H15 Z", ACC_FILL)),
    "pull": neutral(path("M4 16 L12 20 L20 16 L12 12 Z")) +
            accent(path("M12 12 V3 M8.5 6.5 L12 3 L15.5 6.5")),
    "sketch": neutral(path("M3 18 L7.5 9 H21 L16.5 18 Z")) +
              accent(path("M8.5 15 C10.5 9.5 13.5 13.5 17 10.5")),
    "line": neutral(path("M4 17.5 H7 V20.5 H4 Z"), path("M17 3.5 H20 V6.5 H17 Z")) +
            accent(path("M7 17.5 L17 6.5")),
    "rect": neutral(path("M3.5 15.5 H6.5 V18.5 H3.5 Z"), path("M17.5 5.5 H20.5 V8.5 H17.5 Z")) +
            accent(path("M6.5 7 H17.5 V17 H6.5 Z", ACC_FILL)),
    "circle": neutral(path("M12 9.5 V14.5 M9.5 12 H14.5")) +
              accent(path("M12 4.5 A7.5 7.5 0 1 1 11.99 4.5 Z", ACC_FILL)),
    "plan_view": neutral(path("M4 8 V4 H8 M16 4 H20 V8 M20 16 V20 H16 M8 20 H4 V16")) +
                 accent(path("M8 8 H16 V16 H8 Z", ACC_FILL)),
    "mode_3d": neutral(path(CUBE_LEFT), path(CUBE_RIGHT)) + accent(path(CUBE_TOP, ACC_FILL)),
    "undo": neutral(path("M9.5 5.5 L4 10.5 L9.5 15.5")) + accent(path("M4 10.5 H14.5 A5 5 0 0 1 14.5 20.5 H9")),
    "redo": neutral(path("M14.5 5.5 L20 10.5 L14.5 15.5")) + accent(path("M20 10.5 H9.5 A5 5 0 0 0 9.5 20.5 H15")),
    "view_top": neutral(path(CUBE_LEFT), path(CUBE_RIGHT)) + accent(path(CUBE_TOP, (A, ".9"))),
    "view_front": neutral(path(CUBE_TOP), path(CUBE_RIGHT)) + accent(path(CUBE_LEFT, (A, ".9"))),
    "view_right": neutral(path(CUBE_TOP), path(CUBE_LEFT)) + accent(path(CUBE_RIGHT, (A, ".9"))),
    "view_iso": accent(path(CUBE_TOP, (A, ".85")), path(CUBE_LEFT, (A, ".5")), path(CUBE_RIGHT, (A, ".22"))),
    "fit_all": neutral(path("M3.5 8 V3.5 H8 M16 3.5 H20.5 V8 M20.5 16 V20.5 H16 M8 20.5 H3.5 V16")) +
               accent(path("M12 8 L16 10.2 L12 12.4 L8 10.2 Z M8 10.2 V14 L12 16.2 V12.4 M16 10.2 V14 L12 16.2")),
    "export_stl": exchange("STL", ARROW_UP, 4.8, 7.0, 2.6),
    "round": neutral(path("M3 21 V3 H21", extra=' stroke-dasharray="2 3"'), path("M14 14 L6.2 6.2"), path("M13 14 H15 M14 13 V15")) +
             accent(path("M3 21 V14 A11 11 0 0 1 14 3 H21 V21 Z", ACC_FILL)),
    "chamfer": neutral(path("M3 21 V3 H21", extra=' stroke-dasharray="2 3"'), path("M3 10 H7 M10 3 V7")) +
               accent(path("M3 21 V14 L14 3 H21 V21 Z", ACC_FILL)),
    "file_new": neutral(path("M6 3 H14 L19 8 V21 H6 Z M14 3 V8 H19")) + accent(path("M12.5 12 V18 M9.5 15 H15.5")),
    "file_open": neutral(path("M3 7 H10 L12 9 H20 V19 H3 Z")) + accent(path("M3 19 L6 12 H22 L19 19", ACC_FILL)),
    "file_save": neutral(path("M4 4 H17 L20 7 V20 H4 Z")) + accent(path("M8 4 V9 H15 V4 M7 20 V13 H17 V20")),
    "file_save_as": neutral(path("M4 4 H17 L20 7 V20 H4 Z"), path("M8 4 V9 H15 V4")) + accent(path("M15 17 H22 M18.5 13.5 V20.5")),
    "import_step": exchange("STEP", ARROW_DOWN, 4.1, 5.65, 1.45),
    "export_step": exchange("STEP", ARROW_UP, 4.1, 5.65, 1.45),
    "eye_on": neutral(path("M2 12 C5 6 19 6 22 12 C19 18 5 18 2 12 Z")) + accent(path("M12 9 A3 3 0 1 1 11.99 9 Z", ACC_FILL)),
    "eye_off": neutral(path("M2 12 C5 6 19 6 22 12 C19 18 5 18 2 12 Z")) + accent(path("M4 20 L20 4")),
    "move": neutral(path("M4 16 L12 20 L20 16 L12 12 Z")) + accent(path("M12 3 V12 M12 3 L9.5 5.5 M12 3 L14.5 5.5 M12 12 L5 16 M12 12 L19 16")),
    "revolve": neutral(path("M3 4 V20")) + accent(path("M7 6 H12 V18 H7 Z", ACC_FILL), path("M14 6 A5 5 0 0 1 20 11 M20 11 L17.5 10.5 M20 11 L21 8.5")),
    "arc": neutral(path("M4 18.5 H7 V21.5 H4 Z"), path("M17 18.5 H20 V21.5 H17 Z")) + accent(path("M7 20 C7 6 17 6 17 20")),
    "combine": neutral(path("M3 8 H14 V19 H3 Z")) + accent(path("M10 4 H21 V15 H10 Z", ACC_FILL)),
    "donate": neutral(path("M12 20 C4 14 3 9 7 6.5 C9.5 5 11.5 6.5 12 8 C12.5 6.5 14.5 5 17 6.5 C21 9 20 14 12 20 Z")) + accent(path("M12 8 V16 M9.5 11 H14.5", extra=' ')),
    "about": neutral(path("M12 3 A9 9 0 1 1 11.99 3 Z")) + accent(path("M12 11 V17 M12 7.5 V8")),
    "loft": neutral(path("M4 19 H20 M7 5 H17")) + accent(path("M7 5 L4 19 M17 5 L20 19", ), path("M7 5 H17 L20 19 H4 Z", ACC_FILL)),
    "helix": neutral(path("M12 3 V21", extra=' stroke-dasharray="2 3"')) + accent(path("M5 5 C19 6 19 9 5 10 C19 11 19 14 5 15 C19 16 19 19 5 20")),
    "sweep": neutral(path("M3 19 C10 19 9 7 21 6")) + accent(path("M2 16 A3 3 0 1 1 8 16 A3 3 0 1 1 2 16 Z", ACC_FILL)),
    "finish": neutral(path("M3 20 H21")) + accent(path("M4.5 11.5 L9.5 16.5 L19.5 6.5")),
    "fill": neutral(path("M12 3 A9 9 0 1 1 11.99 3 Z", extra=' stroke-dasharray="3 3"')) + accent(path("M12 7.5 A4.5 4.5 0 1 1 11.99 7.5 Z", ACC_FILL)),
    "join": neutral(path("M3.5 18.5 H8 M16 5.5 H20.5 M3.5 16 V21 M20.5 3 V8")) + accent(path("M8 18.5 L16 5.5")),
    "measure": neutral(path("M3 17 H21 M3 14 V20 M21 14 V20")) + accent(path("M7 10 L3 10 M17 10 L21 10 M7 10 L10 7.5 M7 10 L10 12.5 M17 10 L14 7.5 M17 10 L14 12.5 M7 10 H17")),
    "line3d": neutral(path("M4 19 L10 14 L14 18 L20 7")) + accent(path("M3 19 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0 M19 7 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0 M13 18 a1 1 0 1 0 2 0 a1 1 0 1 0 -2 0")),
    "sketch_plane": neutral(path("M3 17 L8 11 H21 L16 17 Z")) + accent(path("M12 14 V3 M8.5 6.5 L12 3 L15.5 6.5")),
    "settings": neutral(path(GEAR)) + accent(path("M12 9 A3 3 0 1 1 11.99 9 Z")),
}

# --------------------------- help illustrations (before / after) ---------------------------
S = 22.0
C30 = math.cos(math.radians(30))


def P(ox, oy, x, y, z, s=S):
    return (ox + (x - y) * C30 * s, oy + (x + y) * 0.5 * s - z * s)


def poly(pts, fill=None, fo=0.1, stroke=N, w=1.6, dash=""):
    d = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
    f = f'fill="{fill}" fill-opacity="{fo}"' if fill else 'fill="none"'
    return f'<polygon points="{d}" {f} stroke="{stroke}" stroke-width="{w}" stroke-linejoin="round"{dash}/>'


def box(ox, oy, w, d, h, z0=0.0, top=None, stroke=N, s=S):
    out = []
    p = lambda x, y, z: P(ox, oy, x, y, z + z0, s)
    out.append(poly([p(0, d, 0), p(w, d, 0), p(w, d, h), p(0, d, h)], N, 0.06, stroke))      # front-left
    out.append(poly([p(w, d, 0), p(w, 0, 0), p(w, 0, h), p(w, d, h)], N, 0.12, stroke))      # right
    out.append(poly([p(0, 0, h), p(w, 0, h), p(w, d, h), p(0, d, h)], top or N, 0.45 if top else 0.03, top or stroke))
    return "".join(out)


def ellipse_on_top(ox, oy, cx, cy, r, z, s=S, color=A):
    pts = []
    for i in range(36):
        a = i / 36 * 2 * math.pi
        pts.append(P(ox, oy, cx + r * math.cos(a), cy + r * math.sin(a), z, s))
    return poly(pts, color, 0.3, color, 2)


def arrow(x0, y0, x1, y1, color=N, w=2):
    ang = math.atan2(y1 - y0, x1 - x0)
    h = 7
    a1 = (x1 - h * math.cos(ang - 0.45), y1 - h * math.sin(ang - 0.45))
    a2 = (x1 - h * math.cos(ang + 0.45), y1 - h * math.sin(ang + 0.45))
    return (f'<path d="M{x0:.1f} {y0:.1f} L{x1:.1f} {y1:.1f} M{a1[0]:.1f} {a1[1]:.1f} L{x1:.1f} {y1:.1f} L{a2[0]:.1f} {a2[1]:.1f}" '
            f'stroke="{color}" stroke-width="{w}" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')


def plane(ox, oy, w=2.2, d=1.6, fill=A, fo=0.1):
    return poly([P(ox, oy, 0, 0, 0), P(ox, oy, w, 0, 0), P(ox, oy, w, d, 0), P(ox, oy, 0, d, 0)], fill, fo, N, 1.4)


def rect2d(x, y, w, h, color=N, fill=None, fo=0.3, r=0):
    f = f'fill="{fill}" fill-opacity="{fo}"' if fill else 'fill="none"'
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" {f} stroke="{color}" stroke-width="1.8"/>'


L, R = (62, 92), (178, 92)   # panel anchors


def scene(before, after):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 140" width="240" height="140">'
            f'<g>{before}</g><g>{arrow(106, 70, 134, 70)}</g><g>{after}</g></svg>')


def b0(ox, oy, h=1.0, top=None):
    return box(ox, oy, 1.6, 1.2, h, top=top)


def pa(ox, oy, **k):
    return b0(ox - 18, oy - 4, **k)


tx = lambda x, y, t: f'<text x="{x}" y="{y}" fill="{N}" font-size="11" text-anchor="middle" font-family="sans-serif">{t}</text>'

help_art = {
    "feedback": ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 140" width="240" height="140">'
                 f'<rect x="150" y="14" width="76" height="112" rx="4" fill="none" stroke="{N}" stroke-width="2"/>'
                 f'<rect x="156" y="96" width="64" height="12" rx="3" fill="none" stroke="{A}" stroke-width="2"/>'
                 f'<rect x="156" y="62" width="64" height="26" rx="3" fill="none" stroke="{N}" stroke-width="2"/>'
                 f'<path d="M160 70 H200 M160 78 H188" stroke="{N}" stroke-width="2" stroke-linecap="round"/>'
                 f'<rect x="156" y="114" width="26" height="8" rx="3" fill="{A}" stroke="none"/>'
                 f'<path d="M24 70 H120 M108 60 L120 70 L108 80" fill="none" stroke="{N}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>'
                 f'<path d="M30 40 a16 16 0 1 0 32 0 a16 16 0 1 0 -32 0 M46 32 V44 M46 48 V50" fill="none" stroke="{A}" stroke-width="3" stroke-linecap="round"/></svg>'),
    "select": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24, top=A)),
    "pull": scene(b0(L[0] - 18, L[1] - 20, top=A) + arrow(L[0] - 18 + 0, L[1] - 20 - 22 - 16, L[0] - 18, L[1] - 20 - 22 - 40, A),
                  b0(R[0] - 18, R[1] - 20, h=1.0) + box(R[0] - 18, R[1] - 20, 1.6, 1.2, 0.8, z0=1.0, top=A, stroke=A)),
    "finish": scene(b0(L[0] - 18, L[1] - 24) + ellipse_on_top(L[0] - 18, L[1] - 24, 0.8, 0.6, 0.42, 1.0), b0(R[0] - 18, R[1] - 24)),
    "sketch": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24) + ellipse_on_top(R[0] - 18, R[1] - 24, 0.8, 0.6, 0.42, 1.0)),
    "line": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26) + f'<path d="M{P(R[0]-22,R[1]-26,0.3,0.3,0)[0]:.1f} {P(R[0]-22,R[1]-26,0.3,0.3,0)[1]:.1f} L{P(R[0]-22,R[1]-26,1.9,1.3,0)[0]:.1f} {P(R[0]-22,R[1]-26,1.9,1.3,0)[1]:.1f}" stroke="{A}" stroke-width="2.4" stroke-linecap="round"/>'),
    "rect": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26) + poly([P(R[0]-22,R[1]-26,0.4,0.3,0), P(R[0]-22,R[1]-26,1.8,0.3,0), P(R[0]-22,R[1]-26,1.8,1.3,0), P(R[0]-22,R[1]-26,0.4,1.3,0)], A, 0.3, A, 2)),
    "circle": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26) + ellipse_on_top(R[0] - 22, R[1] - 26, 1.1, 0.8, 0.5, 0)),
    "plan_view": scene(b0(L[0] - 18, L[1] - 24), rect2d(R[0] - 32, R[1] - 52, 64, 48, A, A, 0.25)),
    "mode_3d": scene(plane(L[0] - 22, L[1] - 26) + ellipse_on_top(L[0] - 22, L[1] - 26, 1.1, 0.8, 0.5, 0), b0(R[0] - 18, R[1] - 24, top=A)),
    "undo": scene(b0(L[0] - 18, L[1] - 24, h=1.3, top=A), b0(R[0] - 18, R[1] - 24, h=0.9)),
    "redo": scene(b0(L[0] - 18, L[1] - 24, h=0.9), b0(R[0] - 18, R[1] - 24, h=1.3, top=A)),
    "view_top": scene(b0(L[0] - 18, L[1] - 24), rect2d(R[0] - 32, R[1] - 52, 64, 48, A, A, 0.25)),
    "view_front": scene(b0(L[0] - 18, L[1] - 24), rect2d(R[0] - 32, R[1] - 46, 64, 36, A, A, 0.25)),
    "view_right": scene(b0(L[0] - 18, L[1] - 24), rect2d(R[0] - 24, R[1] - 46, 48, 36, A, A, 0.25)),
    "view_iso": scene(rect2d(L[0] - 32, L[1] - 46, 64, 36), b0(R[0] - 18, R[1] - 24, top=A)),
    "fit_all": scene(box(L[0] - 40, L[1] - 10, 0.8, 0.6, 0.5, s=18), b0(R[0] - 18, R[1] - 26, top=A)),
    "export_stl": scene(b0(L[0] - 18, L[1] - 24, top=A),
                        f'<g stroke="{A}" stroke-width="1.6" fill="{A}" fill-opacity=".15" stroke-linejoin="round"><rect x="{R[0]-30}" y="{R[1]-56}" width="60" height="52" rx="3"/>'
                        f'<path d="M{R[0]-30} {R[1]-30} L{R[0]} {R[1]-56} L{R[0]+30} {R[1]-30} L{R[0]} {R[1]-4} Z M{R[0]-30} {R[1]-30} H{R[0]+30} M{R[0]} {R[1]-56} V{R[1]-4}" fill="none"/></g>'),
    **{k: scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24, top=A)) for k in ("file_new", "file_open", "file_save", "file_save_as", "import_step", "export_step")},
    "move": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18 + 14, R[1] - 24 - 6, top=A)),
    "revolve": scene(rect2d(L[0] - 30, L[1] - 50, 60, 46) , rect2d(R[0] - 30, R[1] - 50, 60, 46, A, A, 0.25)),
    "eye_on": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24)),
    "eye_off": scene(b0(L[0] - 18, L[1] - 24), ""),
    "arc": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26) + f'<path d="M{R[0]-30} {R[1]-30} Q{R[0]} {R[1]-62} {R[0]+30} {R[1]-30}" stroke="{A}" stroke-width="2.4" fill="none" stroke-linecap="round"/>'),
    "combine": scene(b0(L[0] - 30, L[1] - 24) + b0(L[0] - 6, L[1] - 30, top=A), b0(R[0] - 18, R[1] - 24, top=A)),
    "donate": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24, top=A)),
    "about": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24)),
    "loft": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26)),
    "helix": scene(f'<path d="M{L[0]} {L[1]-60} V{L[1]}" stroke="{N}" stroke-width="2" stroke-dasharray="4 4" fill="none"/>', f'<path d="M{R[0]-22} {R[1]-52} C{R[0]+30} {R[1]-50} {R[0]+30} {R[1]-38} {R[0]-22} {R[1]-34} C{R[0]+30} {R[1]-32} {R[0]+30} {R[1]-20} {R[0]-22} {R[1]-16} C{R[0]+30} {R[1]-14} {R[0]+30} {R[1]-2} {R[0]-22} {R[1]+2}" stroke="{A}" stroke-width="5" fill="none"/>'),
    "fill": scene(f'<circle cx="{L[0]}" cy="{L[1]-30}" r="22" fill="none" stroke="{N}" stroke-width="3" stroke-dasharray="6 5"/>', f'<circle cx="{R[0]}" cy="{R[1]-30}" r="22" fill="{A}" fill-opacity=".3" stroke="{A}" stroke-width="3"/>'),
    "join": scene(f'<path d="M{L[0]-34} {L[1]-12} V{L[1]-50} H{L[0]-6} M{L[0]+8} {L[1]-50} H{L[0]+34} V{L[1]-12} H{L[0]-34}" fill="none" stroke="{N}" stroke-width="3" stroke-linejoin="round"/>', f'<path d="M{R[0]-34} {R[1]-12} V{R[1]-50} H{R[0]+34} V{R[1]-12} H{R[0]-34}" fill="none" stroke="{N}" stroke-width="3" stroke-linejoin="round"/><path d="M{R[0]-6} {R[1]-50} H{R[0]+8}" stroke="{A}" stroke-width="4" fill="none" stroke-linecap="round"/>'),
    "measure": scene(f'<rect x="{L[0]-40}" y="{L[1]-40}" width="80" height="50" fill="none" stroke="{N}" stroke-width="3"/>', f'<rect x="{R[0]-40}" y="{R[1]-40}" width="80" height="50" fill="none" stroke="{N}" stroke-width="3"/><path d="M{R[0]-40} {R[1]-52} H{R[0]+40} M{R[0]-40} {R[1]-58} V{R[1]-46} M{R[0]+40} {R[1]-58} V{R[1]-46}" stroke="{A}" stroke-width="3" fill="none"/>'),
    "sweep": scene(f'<ellipse cx="{L[0]-24}" cy="{L[1]-6}" rx="12" ry="18" fill="none" stroke="{A}" stroke-width="3"/><path d="M{L[0]-24} {L[1]-6} C{L[0]+10} {L[1]-6} {L[0]+10} {L[1]-60} {L[0]+40} {L[1]-60}" stroke="{N}" stroke-width="3" fill="none"/>', f'<path d="M{R[0]-24} {R[1]-6} C{R[0]+10} {R[1]-6} {R[0]+10} {R[1]-60} {R[0]+40} {R[1]-60}" stroke="{A}" stroke-width="16" stroke-opacity=".55" fill="none"/>'),
    "line3d": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 26) + f'<path d="M{R[0]-30} {R[1]-10} L{R[0]-6} {R[1]-30} L{R[0]+10} {R[1]-8} L{R[0]+34} {R[1]-52}" stroke="{A}" stroke-width="4" fill="none"/>'),
    "sketch_plane": scene(plane(L[0] - 22, L[1] - 26), plane(R[0] - 22, R[1] - 50)),
    "settings": scene(b0(L[0] - 18, L[1] - 24), b0(R[0] - 18, R[1] - 24) + f'<path d="{gear_path(8, 14, 10.5, R[0] + 20, R[1] - 70)}" stroke="{A}" stroke-width="2" fill="{A}" fill-opacity=".2"/>'),
    "round": scene(rect2d(L[0] - 30, L[1] - 50, 60, 46) + f'<circle cx="{L[0]+30}" cy="{L[1]-50}" r="4" fill="{A}"/>',
                   f'<path d="M{R[0]-30} {R[1]-4} V{R[1]-50} H{R[0]+14} A16 16 0 0 1 {R[0]+30} {R[1]-34} V{R[1]-4} Z" fill="{N}" fill-opacity=".04" stroke="{N}" stroke-width="1.8"/>'
                   f'<path d="M{R[0]+14} {R[1]-50} A16 16 0 0 1 {R[0]+30} {R[1]-34}" stroke="{A}" stroke-width="3" fill="none"/>'),
    "chamfer": scene(rect2d(L[0] - 30, L[1] - 50, 60, 46) + f'<circle cx="{L[0]+30}" cy="{L[1]-50}" r="4" fill="{A}"/>',
                     f'<path d="M{R[0]-30} {R[1]-4} V{R[1]-50} H{R[0]+16} L{R[0]+30} {R[1]-36} V{R[1]-4} Z" fill="{N}" fill-opacity=".04" stroke="{N}" stroke-width="1.8"/>'
                     f'<path d="M{R[0]+16} {R[1]-50} L{R[0]+30} {R[1]-36}" stroke="{A}" stroke-width="3" fill="none"/>'),
}

tools = [
    ("select", "Select", "S", "Click a face, an edge, a sketch shape or a sketch region to select it. Ctrl+click adds to the selection. The object under the cursor is highlighted.",
     ["Click empty space to clear the selection.", "A small panel appears next to the selection: sizes of holes, pins, rounds and sketch shapes, and Move by for flat faces."]),
    ("pull", "Pull", "P", "Drag a flat face or a sketch region along its normal: out adds material, in removes it. Drag a selected edge to round it. Type a number to enter an exact value, Enter applies, Esc cancels.",
     ["The distance snaps to other faces and vertices (also of other bodies) and otherwise to the grid. Hold Shift while dragging to turn snapping off.", "U (Up to) pulls the selected face to a face, edge, vertex or point you click.", "The panel's Mode forces Add or Cut whichever way you pull."]),
    ("sketch", "Sketch", "K", "Starts a sketch on the selected flat face, or on the base plane (XY) if no flat face is selected. Draw with Line, Rectangle and Circle, then leave with D or P.",
     ["Closed shapes get a light fill: that is what Pull can extrude.", "Snapping markers show what the cursor is attached to, also points of 3D features: hover a vertex, an edge midpoint or a circle centre of a body and the point is projected into the sketch plane (a thin line shows where it comes from).", "Dotted guides show alignment with other points, extensions of lines and edges, and crossings; a placed point can be typed as two distances from a reference (Tab).", "K without a selected face asks for the plane: XY, XZ or YZ."]),
    ("line", "Line", "L", "Click the start point, then the end point. The next line starts where the last one ended. Type a length and an angle if you want exact values.",
     ["Tab switches between the length and angle fields.", "Esc or a right click stops drawing.", "A line snaps parallel or perpendicular to existing lines and edges (marked on screen)."]),
    ("rect", "Rectangle", "R", "Click one corner, then the opposite corner. Type width and height for exact sizes, Tab switches the field, Enter confirms.",
     ["Rectangles are always parallel to the sketch axes.", "Before the first corner is placed, Tab opens two fields with the distances from the marked reference point; type them and press Enter to place the corner exactly."]),
    ("circle", "Circle", "C", "Click the centre, then a point on the circle. Or type the diameter and press Enter.",
     ["Snap to a circle's centre to draw concentric circles.", "A hole 5 mm from a corner: put the cursor on the corner (it becomes the reference), press Tab, type 5, Tab, 5 and Enter.", "On a rectangular face weak helper points show the mirrored positions of an existing centre."]),
    ("plan_view", "Plan view", "V", "Turns the view straight onto the sketch plane so you can draw without perspective distortion.",
     ["Mouse wheel zooms towards the cursor."]),
    ("mode_3d", "3D mode", "D", "Leaves the sketch and returns to normal 3D work. P does the same and switches to Pull.",
     []),
    ("undo", "Undo", "Ctrl+Z", "Takes back the last change: a drawn shape, a pull, a rounding. There is no limit on the number of steps.", []),
    ("redo", "Redo", "Ctrl+Y", "Re-applies a change that was undone. Making a new change clears the redo steps.", []),
    ("view_top", "Top view", "", "Looks at the model from above.", []),
    ("view_front", "Front view", "", "Looks at the model from the front.", []),
    ("view_right", "Right view", "", "Looks at the model from the right side.", []),
    ("view_iso", "Isometric view", "", "A three-quarter view showing top, front and right at once.", []),
    ("fit_all", "Fit all", "", "Zooms so that everything in the scene is visible.", []),
    ("export_stl", "Export STL", "", "Opens the export dialog: choose where to save and how fine the triangles should be. The dialog shows the resulting number of triangles and file size.",
     ["Binary STL is much smaller than ASCII.", "Finer settings give smoother circles and larger files."]),
    ("file_new", "New", "Ctrl+N", "Starts an empty document. If there are unsaved changes you are asked first.", []),
    ("file_open", "Open", "Ctrl+O", "Opens a saved .rolidium document. Rounds and chamfers can still be edited afterwards. The undo history is not saved.", ["A file can also be opened with: rolidium file.rolidium", "Recent files are listed under the File buttons."]),
    ("file_save", "Save", "Ctrl+S", "Saves the document. The previous version stays next to it as a .bak file. Saving is atomic: if it is interrupted the old file is untouched.", ["The window title shows * when there are unsaved changes.", "Unsaved work is also saved automatically every few minutes (see Settings)."]),
    ("file_save_as", "Save As", "Ctrl+Shift+S", "Saves the document under a new name.", []),
    ("import_step", "Import STEP", "", "Adds the solids of a STEP file to the document as new bodies. Anything that is not a valid solid is skipped and reported.", []),
    ("export_step", "Export STEP", "", "Writes all visible bodies into one STEP file.", []),
    ("move", "Move", "M", "Select bodies (click, Ctrl+click for more, or in the tree) and drag an arrow to move along an axis or a ring to rotate about it. Type a number and press Enter for an exact value. The panel has X, Y, Z, an angle and Copy.", ["Moving snaps to the grid step; hold Shift to move freely.", "Sketches drawn on a body move with it; rounds and chamfers stay editable."]),
    ("revolve", "Revolve", "", "Turns a sketch region or a flat face around an axis: press Revolve in the panel, click the axis (a sketch line, a straight edge or a coordinate axis), then drag for the angle or type it. 360 makes a full turn.", ["The axis must lie in the plane of the profile and may touch it but not cut through it.", "Mode Cut makes grooves, Add and Auto add material."]),
    ("eye_on", "Visibility", "", "The eye in the document tree hides or shows a body or a sketch. Hidden objects cannot be selected and are not exported.", []),
    ("eye_off", "Hidden", "", "This object is hidden. Click the eye to show it again.", []),
    ("arc", "Arc", "A", "Click the start point, the end point, then a point on the arc. Type a radius after the end point and press Enter for an exact arc.", ["If the start lies at the end of a line or an arc, the arc can continue it tangentially (marked on screen).", "After an arc the next one continues from its end; Esc stops."]),
    ("combine", "Combine", "I", "Click the target body, then one or more other bodies. Choose Merge, Cut (subtract the others from the target) or Intersect, optionally keep the tool bodies, and press Apply or Enter.", ["Target and tools are highlighted in different colours.", "A cut that splits the target makes separate bodies."]),
    ("donate", "Support development", "", "Opens the About window at the section with a Bitcoin address and its QR code. Nothing is sent anywhere and there are no reminders.", []),
    ("about", "About", "", "Name, version and build of this program, and how to support its development.", []),
    ("loft", "Loft", "L", "Click two or more profiles (sketch regions or flat faces of a body) in the order they should be joined. Choose Ruled (straight lines between profiles) or Smooth, and New body, Add or Cut, then Apply or Enter. A translucent preview shows the result.", ["Profiles with holes (rings) are joined with their holes; all profiles must have the same number of holes.", "Use the Offset field of the sketch to draw profiles on planes at different heights.", "A ruled loft between two coaxial circles is a real cone."]),
    ("helix", "Helix", "H", "Makes a helix curve around an axis. Click the axis: a cylindrical face (its diameter is filled in), a straight edge or a sketch line, or a point for a vertical axis. Then set the diameter, the pitch, the height or the number of turns (one follows from the other), the direction and optionally a start angle and a cone angle. The helix is drawn as you type; Create makes it a Curve in the document tree.", ["A helix is the path for a thread or a spring: use it with Sweep.", "A cone angle makes a tapered helix."]),
    ("sweep", "Sweep", "W", "Pulls a profile along a path. Click the profile (a sketch region, also with a hole for a pipe, or a flat face), then the path: a Curve, a chain of sketch lines and arcs, or a chain of edges. One click takes the whole chain that continues. Choose Auto, New body, Add or Cut; the corners of the path can be sharp or rounded.", ["Along a helix the profile keeps its orientation to the axis, so a thread comes out right.", "If the path turns more sharply than the profile allows, nothing is changed and a message says so."]),
    ("finish", "Finish sketch", "K", "Leaves the sketch and returns to the view you had before it. The shapes you drew stay in the document as a sketch. K, D and P do the same.", ["The view turns back by itself (the setting 'Turn the view square to the sketch automatically' switches this off)."]),
    ("fill", "Fill", "F", "Makes a flat shape out of a closed loop. Click an edge of a body (the edge of a hole, the inner circle of a pipe) or the lines of a shape; Shift adds more edges when the loop is made of several. The shape appears on the plane of the loop with exactly the same geometry (a circle stays a circle) and the Pull tool starts with it, so you can close the hole or make a plug.", ["A negative distance goes into the body and fills the hole; a positive one makes a plug on top.", "If the edges are not closed or not flat, a message says why."]),
    ("join", "Join ends", "J", "Joins open lines and curves that almost meet. Select the lines you want (nothing selected means the whole sketch), choose the largest gap to bridge, and the dashed preview shows the new connections. Enter confirms, Esc cancels. Tiny gaps are closed by moving the ends together; larger ones get a short line.", ["Ends are paired starting with the closest ones; every end is used at most once.", "Ends that stay free are highlighted in the preview."]),
    ("feedback", "Report a bug or an idea", "", "The folded row at the bottom of the right column opens a panel. Choose Bug or Idea, write what happened or what you would like (up to 1000 characters) and, if you wish, leave an e-mail for a reply. When you click the row, the program takes a picture of its own window (what you were looking at, before the panel opens); it stays in memory until you send. The diagnostics and the window picture are attached by default to a bug and not to an idea (you can change it); Take again takes the picture again with the panel hidden; Show what is sent lists every file with a check box (window.png first): a file you switch off is not sent. The diagnostics are made in the background from what the program already has in memory (about 2 seconds at most, never more than 5). The test scene of the other graphics interfaces is only in About > Save diagnostics. Nothing is sent before you press Send. If the server does not answer, your text stays and you can save the report to a file.", ["An idea needs some text; a bug can be sent without text only with the diagnostics.", "The diagnostics hold no models, no names of models, no name of the computer and no user name."]),
    ("measure", "Measure", "E", "Click two elements: a vertex, a point of a sketch line or a curve, an edge or a face. The distance appears in the view as a dimension line, with its X, Y and Z parts in a panel; two flat faces or straight edges also give their angle, two parallel faces their distance and two cylindrical faces the distance of their axes. After the first click the Selection panel shows that element alone (length, area, diameter).", ["Click a value in the panel to copy it. Several measurements can stay on the screen; Esc clears them.", "Measurements are temporary and are not saved in the file."]),
    ("line3d", "3D line", "", "Switch on 3D in the toolbar next to the Line tool: the points are no longer tied to the sketch plane. They snap to vertices, edge midpoints, circle centres and points of sketches and curves. X, Y or Z locks a direction and a typed length sets the segment; a corner radius rounds the corners (the path of a bent tube). Enter or a double click finishes the line as a Curve.", ["Without a snap and without a locked direction the point lies in the current sketch plane.", "Use the curve as the path of a Sweep."]),
    ("sketch_plane", "Move sketch plane", "", "Shows an arrow on the sketch plane. Drag it to move the plane (steps of the grid; Shift: free), or type a distance and press Enter, or press U and click a vertex or a face to bring the plane there. If the sketch already has shapes, a new sketch is started on the moved plane and the old one stays.", ["The Offset field next to this button shows the current distance and can be typed into.", "It is not available while a shape is being drawn: finish or cancel the shape first."]),
    ("settings", "Settings", "", "Opens the settings: language, and the soft tone played when an operation fails.",
     ["Language changes at once, no restart needed.", "Other languages are plain text files in the lang folder that you can edit."]),
    ("round", "Round", "", "Rounds the selected edges. Select an edge, choose Round in the panel, then drag with the Pull tool or type the radius. Click a finished round to change its radius; 0 brings the sharp edge back.",
     ["If the radius is too large, nothing changes and you get a message."]),
    ("chamfer", "Chamfer", "", "Cuts the selected edges at an angle. Select an edge, choose Chamfer in the panel, then drag or type the distance. Click a finished chamfer to change its distance; 0 brings the sharp edge back.", []),
]


def write(path, text):
    with open(path, "w") as f:
        f.write(text)


os.makedirs("assets/icons", exist_ok=True)
os.makedirs("assets/help", exist_ok=True)
for name, body in icons.items():
    head = HEAD.replace('stroke-width="1.75"', 'stroke-width="0.9"') if name in FILE_BLOCK_ICONS else HEAD
    write(f"assets/icons/{name}.svg", head + body + "</svg>\n")
for name, svg in help_art.items():
    write(f"assets/help/{name}.svg", svg + "\n")
write("assets/help/tools.json", json.dumps([
    {"id": i, "title": t, "shortcut": k, "text": x, "tips": tips, "illustration": f"{i}.svg", "animation": None}
    for i, t, k, x, tips in tools], indent=2) + "\n")

# preview page: the three application icon candidates first
def sized(svg_text, px):
    return svg_text.replace('width="256" height="256"', f'width="{px}" height="{px}"', 1)


app_icons = ""
app_titles = {"a": "A: Atom over a drawing grid", "b": "B: Part with orbits (default; 16-32 px use the simplified form on the right)", "c": "C: Blueprint"}
for key in "abc":
    svg_text = open(f"assets/appicon/rolidium-{key}.svg").read()
    row = ""
    for bg in ("#f2f4f7", "#1e2127"):
        row += f'<div class="bg" style="background:{bg}">' + "".join(sized(svg_text, px) for px in (32, 64, 256)) + "</div>"
    if key == "b":
        small_svg = open("assets/appicon/rolidium-small.svg").read()
        row += '<div class="bg" style="background:#f2f4f7">' + "".join(sized(small_svg, px) for px in (16, 24, 32)) + "</div>"
        row += '<div class="bg" style="background:#1e2127">' + "".join(sized(small_svg, px) for px in (16, 24, 32)) + "</div>"
    app_icons += f'<div class="ai"><div class="t">{app_titles[key]}</div><div class="rw">{row}</div></div>'
neutral_c, accent_c = "#d6dae0", "#ffb347"
cells = []
for name in icons:
    svg = open(f"assets/icons/{name}.svg").read().replace(N, neutral_c).replace(A, accent_c)
    s24 = svg.replace('width="24" height="24"', 'width="24" height="24"')
    s48 = svg.replace('width="24" height="24"', 'width="48" height="48"')
    cells.append(f'<div class="c"><div class="r">{s24}{s48}</div><span>{name}</span></div>')
write("assets/icons-preview.html", f"""<!doctype html><html><head><meta charset="utf-8"><title>Rolidium icons</title>
<style>body{{background:#1e2127;color:#aab;font-family:sans-serif;margin:24px}}h1{{font-size:16px;font-weight:600}}
.g{{display:flex;flex-wrap:wrap;gap:14px}}.c{{background:#262a31;border-radius:8px;padding:12px 14px;min-width:110px}}
.r{{display:flex;gap:14px;align-items:center;margin-bottom:8px}}span{{font-size:12px}}\n.ai{{margin-bottom:18px}}.t{{font-size:13px;color:#dde;margin-bottom:6px}}.rw{{display:flex;gap:14px;flex-wrap:wrap}}.bg{{display:flex;gap:16px;align-items:flex-end;padding:14px;border-radius:8px}}</style></head>
<body><h1>Application icon: three candidates (32, 64 and 256 px, on a light and a dark background)</h1>{app_icons}
<h1>Rolidium icons (24 px and 48 px, dark theme)</h1><div class="g">{''.join(cells)}</div></body></html>
""")
print("icons:", len(icons), "help:", len(help_art))

# Rust index of the embedded assets
lines = ["// Copyright (c) 2026 Rolid spol. s r.o.", "// SPDX-License-Identifier: AGPL-3.0-only", "// Generated by scripts/gen_assets.py, do not edit.", "",
         "pub static ICONS: &[(&str, &str)] = &["]
lines += [f'    ("{n}", include_str!("../../../assets/icons/{n}.svg")),' for n in icons]
lines += ["];", "", "pub static HELP_ART: &[(&str, &str)] = &["]
lines += [f'    ("{n}", include_str!("../../../assets/help/{n}.svg")),' for n in help_art]
lines += ["];", "", 'pub static HELP_JSON: &str = include_str!("../../../assets/help/tools.json");', ""]
write("crates/rolidium-app/src/assets_index.rs", "\n".join(lines))
