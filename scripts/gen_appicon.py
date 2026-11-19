#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Generates the three candidate application icons (SVG) and the PNG sizes of the chosen one.

Run from the repository root:  python3 scripts/gen_appicon.py   (needs `cargo build --release -p rolidium-svg2png` first)
All three combine an atom (nucleus and orbits) with technical-drawing marks (grid, dimension line, centre lines, part outline).
"""
import os
import subprocess

OUT = "assets/appicon"
DEFAULT = "b"
# at 16-32 px the full drawing is too fine: the same motif with fewer, thicker parts
SMALL_MAX = 32
SIZES = [16, 24, 32, 48, 64, 128, 256, 512]

ORANGE, BLUE, WHITE = "#ffb347", "#5ab0ff", "#eef2f8"


def svg(body, size=256):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 256 256">'
            f'{body}</svg>\n')


def orbit(cx, cy, rx, ry, rot, color, w):
    return (f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="none" stroke="{color}" stroke-width="{w}" '
            f'transform="rotate({rot} {cx} {cy})"/>')


def line(x1, y1, x2, y2, color, w, extra=""):
    return f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{w}" stroke-linecap="round"{extra}/>'


def dimension(x1, x2, y, color, w=7):
    """A dimension line with arrow heads and end ticks."""
    a = 16
    return (line(x1, y, x2, y, color, w)
            + f'<path d="M{x1} {y} l{a} -{a*0.55:.1f} v{a*1.1:.1f} Z" fill="{color}"/>'
            + f'<path d="M{x2} {y} l-{a} -{a*0.55:.1f} v{a*1.1:.1f} Z" fill="{color}"/>'
            + line(x1, y - 16, x1, y + 16, color, w - 2) + line(x2, y - 16, x2, y + 16, color, w - 2))


def variant_a():
    """Atom over a drawing grid with a dimension line underneath."""
    b = '<rect x="0" y="0" width="256" height="256" rx="52" fill="#1b2330"/>'
    for i in range(1, 8):
        b += line(i * 32, 20, i * 32, 236, "#2c3748", 2) + line(20, i * 32, 236, i * 32, "#2c3748", 2)
    cx, cy = 128, 104
    b += orbit(cx, cy, 92, 34, 0, WHITE, 11) + orbit(cx, cy, 92, 34, 60, BLUE, 11) + orbit(cx, cy, 92, 34, 120, ORANGE, 11)
    b += f'<circle cx="{cx}" cy="{cy}" r="21" fill="{ORANGE}"/>'
    b += dimension(50, 206, 222, "#aab6c8")
    return svg(b)


def variant_b():
    """A part (isometric block) with the atom around it."""
    b = '<rect x="0" y="0" width="256" height="256" rx="52" fill="#13202f"/>'
    hexa = "128,44 200,86 200,170 128,212 56,170 56,86"
    b += f'<polygon points="{hexa}" fill="#1f3550" stroke="{WHITE}" stroke-width="9" stroke-linejoin="round"/>'
    b += line(128, 128, 128, 212, WHITE, 7) + line(128, 128, 56, 86, WHITE, 7) + line(128, 128, 200, 86, WHITE, 7)
    b += orbit(128, 128, 112, 40, -28, BLUE, 10) + orbit(128, 128, 112, 40, 28, ORANGE, 10)
    b += f'<circle cx="128" cy="128" r="18" fill="{ORANGE}"/>'
    b += line(24, 92, 24, 168, "#aab6c8", 6) + line(14, 92, 34, 92, "#aab6c8", 5) + line(14, 168, 34, 168, "#aab6c8", 5)
    return svg(b)


def variant_b_small():
    """Variant B for 16 to 32 px: the block, two bold orbits and the nucleus, nothing finer."""
    b = '<rect x="0" y="0" width="256" height="256" rx="52" fill="#13202f"/>'
    hexa = "128,38 206,83 206,173 128,218 50,173 50,83"
    b += f'<polygon points="{hexa}" fill="#27466b" stroke="{WHITE}" stroke-width="16" stroke-linejoin="round"/>'
    b += line(128, 128, 128, 218, WHITE, 13) + line(128, 128, 50, 83, WHITE, 13) + line(128, 128, 206, 83, WHITE, 13)
    b += orbit(128, 128, 120, 46, -28, BLUE, 20) + orbit(128, 128, 120, 46, 28, ORANGE, 20)
    b += f'<circle cx="128" cy="128" r="28" fill="{ORANGE}"/>'
    return svg(b)


def variant_c():
    """Blueprint: centre lines, a construction circle, orbits and a dimension line."""
    b = '<rect x="0" y="0" width="256" height="256" rx="52" fill="#1d4a86"/>'
    for i in range(1, 16):
        b += line(i * 16, 16, i * 16, 240, "#2a5c9e", 1.5) + line(16, i * 16, 240, i * 16, "#2a5c9e", 1.5)
    b += line(128, 16, 128, 240, "#cfe4ff", 4, ' stroke-dasharray="22 8 4 8"') + line(16, 112, 240, 112, "#cfe4ff", 4, ' stroke-dasharray="22 8 4 8"')
    b += f'<circle cx="128" cy="112" r="74" fill="none" stroke="#cfe4ff" stroke-width="6"/>'
    b += orbit(128, 112, 88, 30, 32, WHITE, 10) + orbit(128, 112, 88, 30, -32, ORANGE, 10)
    b += f'<circle cx="128" cy="112" r="19" fill="{ORANGE}"/>'
    b += dimension(54, 202, 224, "#cfe4ff")
    return svg(b)


SMALL = {"b": variant_b_small}
VARIANTS = {"a": ("Atom over a drawing grid", variant_a), "b": ("Part with orbits", variant_b), "c": ("Blueprint", variant_c)}


def write(path, text):
    with open(path, "w") as f:
        f.write(text)


TOOL = "target/release/rolidium-svg2png"


def render(svg_text, size, path):
    """PNG of `size` pixels through the resvg based helper (cargo build --release -p rolidium-svg2png)."""
    tmp = path + ".svg"
    write(tmp, svg_text)
    subprocess.run([TOOL, tmp, path, str(size)], check=True)
    os.remove(tmp)


def write_ico(path, sizes):
    """An .ico file whose images are the PNG files of the default icon."""
    import struct
    pngs = [open(f"{OUT}/png/rolidium-{s}.png", "rb").read() for s in sizes]
    head = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries, data = b"", b""
    for s, png in zip(sizes, pngs):
        entries += struct.pack("<BBBBHHII", 0 if s >= 256 else s, 0 if s >= 256 else s, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    with open(path, "wb") as f:
        f.write(head + entries + data)


if __name__ == "__main__":
    os.makedirs(f"{OUT}/png", exist_ok=True)
    for key, (title, fn) in VARIANTS.items():
        write(f"{OUT}/rolidium-{key}.svg", fn())
    default = VARIANTS[DEFAULT][1]()
    small = SMALL[DEFAULT]() if DEFAULT in SMALL else default
    write(f"{OUT}/rolidium.svg", default)
    write(f"{OUT}/rolidium-small.svg", small)
    for s in SIZES:
        render(small if s <= SMALL_MAX else default, s, f"{OUT}/png/rolidium-{s}.png")
    render(default, 128, f"{OUT}/rolidium-128.rgba")      # the window icon, embedded in the program
    write_ico(f"{OUT}/rolidium.ico", [16, 32, 48, 64, 128, 256])   # the icon of rolidium.exe
    print("app icons written to", OUT)
