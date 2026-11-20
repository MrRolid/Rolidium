#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import math, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from rdm import Script

s = Script("Pillow block with a 608 ball bearing (8 x 22 x 7 mm): housing, inner and outer ring with raceways, seven balls of 3.969 mm. "
           "The shaft axis is along Y at a height of 20 mm.")
X0 = 200   # the housing is built here, far from the origin, because the bearing is revolved about an axis through the origin; it is moved back at the end
AX = 20    # height of the axis

s.comment("The bearing (bodies 1 to 9) is revolved about the Y axis first, at the origin, where nothing else is yet.")
R_BALL = 3.969 / 2
R_PITCH = 7.75                         # the balls run on a circle of this radius
G = 2.05                               # radius of the raceway grooves
dz = math.sqrt(G * G - (R_PITCH - 6.25) ** 2)   # the groove meets the outer face of the inner ring (r = 6.25) at +-dz from the middle
def groove_ring(r_face, r_mid, sign):
    """Half section of a ring (radius from r_face_a to r_face_b is drawn by the caller); returns the points of the groove."""
    return (r_face, 3.5 - dz), (r_mid, 3.5), (r_face, 3.5 + dz)

s.comment("Inner ring: bore 8, outer face r = 6.25 with a groove (an arc).")
s.sketch("xy")
s.line(4, 0, 6.25, 0)
s.line(6.25, 0, 6.25, 3.5 - dz)
s.arc(6.25, 3.5 - dz, R_PITCH - G, 3.5, 6.25, 3.5 + dz)
s.line(6.25, 3.5 + dz, 6.25, 7)
s.line(6.25, 7, 4, 7)
s.line(4, 7, 4, 0)
s.raw("revolve 0 y 360 add")
s.comment("Outer ring: outer diameter 22, inner face r = 9.25 with a groove.")
s.sketch("xy")
s.line(9.25, 0, 11, 0)
s.line(11, 0, 11, 7)
s.line(11, 7, 9.25, 7)
s.line(9.25, 7, 9.25, 3.5 + dz)
s.arc(9.25, 3.5 + dz, R_PITCH + G, 3.5, 9.25, 3.5 - dz)
s.line(9.25, 3.5 - dz, 9.25, 0)
s.raw("revolve 0 y 360 add")
s.comment("A ball: a half disc revolved about its diameter, then copied to seven places on the pitch circle (and the first one moved to its place).")
s.sketch("xy")
s.arc(0, -R_BALL, R_BALL, 0, 0, R_BALL)
s.line(0, R_BALL, 0, -R_BALL)
s.raw("revolve 0 y 360 add")
ball = 3
for k in range(7):
    a = 2 * math.pi * k / 7
    x, y, z = R_PITCH * math.cos(a), 3.5, AX + R_PITCH * math.sin(a)
    if k < 6:
        s.raw(f"move {x:.4f} {y} {z:.4f} copy body {ball}")
    else:
        s.raw(f"move {x:.4f} {y} {z:.4f} body {ball}")
s.n = 9
s.move(0, 0, AX, 1)
s.move(0, 0, AX, 2)

s.comment("The housing (body 10), built at x = 200 and moved back at the end: a base 70 x 24 x 8, the block over the axis, the bearing seat and the bore.")
s.box(X0 - 35, 0, 0, 70, 24, 8)
s.sketch("xy", 8)
s.rect(X0 - 18, 0, X0 + 18, 24)
s.pull(28, "add")
for sx in (-1, 1):
    s.round(3, X0 + sx * 35, 0, 4, body=10)
    s.round(3, X0 + sx * 35, 24, 4, body=10)
    s.round(3, X0 + sx * 18, 12, 36, body=10)
s.comment("Bore 19 through, and the seat for the bearing: 22.05 wide (0.025 mm clearance), 7 mm deep from the front face y = 0.")
s.sketch("xz", -25)
s.circle(X0, AX, 19)
s.pull(26, "cut")
s.sketch("xz", -7)
s.circle(X0, AX, 22.05)
s.pull(8, "cut")
s.comment("Two oval slots for M6 screws, 52 mm apart (6.6 wide, centres 4 mm apart) in the base.")
r = 3.3
for sx in (-1, 1):
    cx, cy = X0 + sx * 26, 12
    s.sketch("xy", -1)
    s.line(cx - 2, cy + r, cx + 2, cy + r)
    s.arc(cx + 2, cy + r, cx + 2 + r, cy, cx + 2, cy - r)
    s.line(cx + 2, cy - r, cx - 2, cy - r)
    s.arc(cx - 2, cy - r, cx - 2 - r, cy, cx - 2, cy + r)
    s.pull(10, "cut")
s.comment("A lubrication hole of diameter 6 from the top into the bore.")
s.hole("xy", 28, [(X0, 12)], 6, 10)
s.move(-X0, 0, 0, 10)
# the picture file of the section (outputs.txt, used by build-all.sh): the program has no section view, so a half is cut away
here = os.path.dirname(os.path.abspath(__file__))
out = ["# a half (x > 0) of every body cut away: the cut face is seen from the right. Balls that lie wholly in the removed half are deleted.",
       "box 80 60 80", "move 0 -30 -10 body 11"]
ball_bodies = {k: (4 + k if k < 6 else 3) for k in range(7)}
cuts, deletes = [10, 1, 2], []
for k, b in ball_bodies.items():
    cx = R_PITCH * math.cos(2 * math.pi * k / 7)
    if cx - R_BALL > 0:
        deletes.append(b)
    elif abs(cx) < R_BALL:
        cuts.append(b)
for b in cuts:
    out.append(f"combine cut {b} 11 keep")
out.append("delete body 11")
for b in sorted(deletes, reverse=True):
    out.append(f"delete body {b}")
out += ["save @OUT@/@NAME@-section.rolidium", "render @OUT@/@NAME@-detail.png 1920 1080 right"]
open(os.path.join(here, "outputs.txt"), "w").write("\n".join(out) + "\n")
s.raw("export-step target/demos/05-bearing-block/05-bearing-block.step")
s.header(10, 30240.269, no_overlap="yes")
s.raw("info")
s.write(os.path.join(os.path.dirname(os.path.abspath(__file__)), "build.txt"))
