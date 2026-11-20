#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from rdm import Script

W, D, H, T = 120, 80, 60, 3          # outer size and the thickness of the material
s = Script("Finger-joint box for a laser cutter: 120 x 80 x 60, material 3 mm, fingers about 10 mm. Five plates (bottom and four walls), "
           "once folded up and once laid flat.")

V_SEG = 10                            # vertical edges: 6 fingers of exactly 10 mm; front/back walls own the even ones, the side walls the odd ones
def segments(length, start, n):
    w = length / n
    return [(start + i * w, start + (i + 1) * w) for i in range(n)]
SEG_X = segments(W - 2 * T, T, 11)    # bottom edges: 114 mm in 11 parts (10.36 mm each), the bottom has the tabs in the even parts
SEG_Y = segments(D - 2 * T, T, 7)     # 74 mm in 7 parts (10.57 mm each)
TABS_X = [g for i, g in enumerate(SEG_X) if i % 2 == 0]
TABS_Y = [g for i, g in enumerate(SEG_Y) if i % 2 == 0]
OUT = 1.0                             # a slot reaches this far outside the plate (its edge does not coincide with the edge of the plate)

def flat(ox, oy, core, tabs=(), notches=()):
    """A flat plate in z = 0..3 whose own frame (u, v) starts at (ox, oy): a core rectangle, tabs added to it, slots cut out of it."""
    s.sketch("xy")
    u0, v0, u1, v1 = core
    s.rect(ox + u0, oy + v0, ox + u1, oy + v1)
    s.pull(T, "add")
    s.n += 1
    for (a, b, c, d) in tabs:                      # tabs touch the core, so they join it
        s.sketch("xy")
        s.rect(ox + a, oy + b, ox + c, oy + d)
        s.pull(T, "add")
    for (a, b, c, d) in notches:                   # one sketch for each slot: slots that touch would share edges
        s.sketch("xy", -1)
        s.rect(ox + a, oy + b, ox + c, oy + d)
        s.pull(T + 2, "cut")

slots_x = [(a, -OUT, b, T) for a, b in TABS_X]      # in the front and back wall, for the tabs of the bottom
slots_y = [(a, -OUT, b, T) for a, b in TABS_Y]      # in the side walls
odd = [(j * V_SEG, (j + 1) * V_SEG) for j in range(H // V_SEG) if j % 2 == 1]
even = [(j * V_SEG, (j + 1) * V_SEG) for j in range(H // V_SEG) if j % 2 == 0]
front_notches = slots_x + [(u0, v0, u1, v1) for (v0, v1) in odd for (u0, u1) in ((-OUT, T), (W - T, W + OUT))]
side_notches = slots_y + [(u0, v0, u1, v1) for (v0, v1) in even for (u0, u1) in ((-OUT, T), (D - T, D + OUT))]
bottom_tabs = ([(a, 0, b, T) for a, b in TABS_X] + [(a, D - T, b, D) for a, b in TABS_X]
               + [(0, a, T, b) for a, b in TABS_Y] + [(W - T, a, W, b) for a, b in TABS_Y])

s.comment("The five plates laid flat (bodies 1 to 5): bottom, front, back, left, right. Each is a rectangle with the slots cut out of it;")
s.comment("the bottom is a core of 114 x 74 with the tabs added to it.")
OX = W + 40
flat(OX, 0, (T, T, W - T, D - T), tabs=bottom_tabs)
flat(OX, D + 20, (0, 0, W, H), notches=front_notches)
flat(OX, D + 20 + H + 20, (0, 0, W, H), notches=front_notches)
flat(OX + W + 20, 0, (0, 0, D, H), notches=side_notches)
flat(OX + W + 20, H + 20, (0, 0, D, H), notches=side_notches)

s.comment("Copies of the plates stood up into the folded box (bodies 6 to 10): turn about its middle, then move into place.")
centres = [
    (OX + W / 2, D / 2, T / 2),                       # bottom (corner of the frame is at OX, 0; the plate itself starts there)
    (OX + W / 2, D + 20 + H / 2, T / 2),
    (OX + W / 2, D + 20 + H + 20 + H / 2, T / 2),
    (OX + W + 20 + D / 2, H / 2, T / 2),
    (OX + W + 20 + D / 2, H + 20 + H / 2, T / 2),
]
targets = [(W / 2, D / 2, T / 2), (W / 2, T / 2, H / 2), (W / 2, D - T / 2, H / 2), (T / 2, D / 2, H / 2), (W - T / 2, D / 2, H / 2)]
first_copy = s.n + 1
for i, (c, t) in enumerate(zip(centres, targets)):
    s.raw(f"# plate {i + 1}")
    if i == 0:
        s.raw("move 0 0 0 copy body 1")
        s.n += 1
    else:
        s.raw(f"rotate x 90 copy body {i + 1}")
        s.n += 1
        if i >= 3:
            s.raw(f"rotate z 90 body {s.n}")
    dx, dy, dz = (t[k] - c[k] for k in range(3))
    s.move(dx, dy, dz, s.n)
s.header(10, 190296.002, no_overlap="yes")
s.raw("info")
s.write(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build.txt"))
