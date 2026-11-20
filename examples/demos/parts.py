# Parts shared by the demos (the middle row of the demos is reused in the machine, demo 10). Every function builds a part with the
# commands of `rolidium cli` into the script `s` in the frame `f` (see rdm.Frame), and returns the numbers of the bodies it made.
# Parts are built in an empty place and moved afterwards, because bodies that touch would be joined into one by Pull.
import math
from rdm import Frame, hexagon

STAGE = (5000.0, 5000.0, 5000.0)       # an empty place far from everything, where parts are built before they are moved


def place(s, bodies, d):
    """Moves the bodies (numbers) by d = (dx, dy, dz)."""
    for b in bodies:
        s.move(d[0], d[1], d[2], b)


def copy_to(s, body, d):
    """A copy of a body moved by d; returns the number of the copy."""
    s.raw(f"move {d[0]:.4f} {d[1]:.4f} {d[2]:.4f} copy body {body}")
    s.n += 1
    return s.n


def screw(s, f, d, length, head_d, head_h, socket, sink=0.0):
    """A cap screw with a hex socket, simplified: head and plain shank (the thread is not drawn). Local frame: the head stands on w = 0, the
    shank goes down to w = -length. Returns the body."""
    start = s.n + 1
    f.ext("w", 0, head_h, ("circle", 0, 0, head_d), "add", joins=False)
    f.ext("w", -length, 0, ("circle", 0, 0, d), "add", joins=True)
    f.ext("w", head_h - head_h * 0.5, head_h + 1, ("poly", hexagon(0, 0, socket)), "cut")
    return [start]


def nema23_motor(s, f):
    """Stepper motor NEMA 23: the local origin is the middle of the mounting face of the flange, the shaft points to +w. One body.
    Flange 56.4 x 56.4 x 5 with four holes of 5 on 47.14 and the pilot of 38.1 x 1.6; body 55.6 x 55.6, 56 long with chamfered corners; rear cap;
    a cable box; the shaft of 8 x 21 with a flat."""
    start = s.n + 1
    h = 56.4 / 2
    f.ext("w", -5, 0, ("rect", -h, -h, h, h), "add", joins=False)                  # flange
    f.ext("w", -56, -5, ("rect", -27.8, -27.8, 27.8, 27.8), "add")                  # the body
    f.ext("w", -61, -56, ("rect", -22, -22, 22, 22), "add")                         # the rear cap
    f.ext("w", 0, 1.6, ("circle", 0, 0, 38.1), "add")                               # the pilot
    f.ext("w", 0, 21, ("circle", 0, 0, 7.98), "add")                                # the shaft (8 h6, a hair under 8 so that it does not touch a bore of 8)
    f.ext("u", 3.5, 5, ("rect", -5, 6, 5, 21), "cut")                               # the flat on the shaft (a slice 0.5 deep)
    for sx in (-1, 1):
        for sy in (-1, 1):
            f.ext("w", -4.5, 1, ("circle", sx * 23.57, sy * 23.57, 5), "cut")        # four blind holes 4.5 deep
            f.chamfer(7, sx * 27.8, sy * 27.8, -30)                                  # the corners of the body
    f.ext("w", -50, -38, ("rect", -6, 27.8, 6, 34), "add")                           # the cable box
    return [start]


def motor_bracket(s, f):
    """Angle bracket 60 x 5 (plate 65 high, foot 60 long, 60 wide) for the motor: the plate stands on w = 0..5 (the motor's face meets its back), the foot goes
    forward along w at the bottom (v = -35..-30), low enough to clear the heads of the screws. Hole of 38.5 for the pilot, four holes of 5.5 for the screws, two slots in the foot. One body."""
    start = s.n + 1
    pts = [(0, -35), (60, -35), (60, -30), (5, -30), (5, 30), (0, 30)]
    # the profile lies in (w, v): extrude it along u. Local coordinates of the shape are (the others of u,v,w) = (v, w)
    f.ext("u", -30, 30, ("poly", [(v, w) for (w, v) in pts]), "add", joins=False)
    f.ext("w", -1, 6, ("circle", 0, 0, 38.5), "cut")
    for sx in (-1, 1):
        for sy in (-1, 1):
            f.ext("w", -1, 6, ("circle", sx * 23.57, sy * 23.57, 5.5), "cut")
    for sx in (-1, 1):
        f.ext("v", -36, -29, ("slot", sx * 20, 30, "b", 8, 6.6), "cut")             # two slots (long along w) in the foot
    return [start]


def shaft_coupling(s, f):
    """Clamp coupling 25 x 30 with two bores of 8, a slit of 1 mm along one side and two M3 pockets; the screws are separate. One body.
    Local origin: the middle of one end, the coupling goes to +w; the slit and the screws are on the +u side, the screws go along v."""
    start = s.n + 1
    f.ext("w", 0, 30, ("circle", 0, 0, 25), "add", joins=False)
    f.ext("w", -1, 31, ("circle", 0, 0, 8), "cut")
    f.ext("v", -0.5, 0.5, ("rect", 0, -1, 13.5, 31), "cut")                          # the slit: a slice at v = 0 from the bore out to +u
    for w in COUPLING_SCREWS:
        f.ext("v", -14, 14, ("circle", 8.5, w, 3.2), "cut")                           # the holes of the screws
        f.ext("v", 8, 14, ("circle", 8.5, w, 6.2), "cut")                             # the pockets for the heads, on the +v side
    return [start]


COUPLING_SCREWS = (7.5, 22.5)    # positions of the clamp screws along the coupling


def staged(s, axis, sign, final, builder, *args):
    """Builds a part at the staging area (frame axis/sign), then moves it so that its local origin is at `final` (a world point)."""
    f = Frame(s, axis, sign, STAGE)
    bodies = builder(s, f, *args)
    place(s, bodies, tuple(final[i] - STAGE[i] for i in range(3)))
    return bodies


# ---- linear guide (size 15) ------------------------------------------------------------------------------------------------------------
# Local frame: w along the rail, u across, v up; the rail stands on v = 0 and is symmetric about u = 0.
RAIL_HALF = [(7.5, 0), (7.5, 4), (6.3, 5.5), (7.5, 7), (7.5, 9), (6.3, 10.5), (7.5, 12), (7.5, 15)]   # right side, from the foot to the top
CAR_HALF = [(7.6, 4.0), (7.6, 4.6), (6.5, 5.5), (7.6, 6.4), (7.6, 9.6), (6.5, 10.5), (7.6, 11.4), (7.6, 15.1)]   # the channel of the carriage, 0.1 mm clear


def mirrored(half):
    """The closed outline: the right side going up, then the left side coming down."""
    return half + [(-u, v) for (u, v) in reversed(half)]


def linear_rail(s, f, length=300, pitch=60, first=30):
    """Rail of size 15 (15 x 15, grooves for the balls on both sides), `length` long; counterbored holes (4.5 and 7.5 x 5.3 deep) every `pitch`. One body."""
    start = s.n + 1
    f.ext("w", 0, length, ("poly", mirrored(RAIL_HALF)), "add", joins=False)
    w = first
    while w < length - 10:
        f.ext("v", -1, 16, ("circle", 0, w, 4.5), "cut")
        f.ext("v", 15 - 5.3, 16, ("circle", 0, w, 7.5), "cut")
        w += pitch
    return [start]


def carriage(s, f, length=60):
    """Carriage of size 15: 47 wide, `length` long, 24 above the foot of the rail, with a channel that clears the rail by 0.1 mm, four holes for M5
    (4.2 on 38 x 30). One body. Local w = 0 .. length."""
    start = s.n + 1
    # the outline: the left corner, the channel (left side up, right side down) and the right corner, then up and over the top
    chan = [(-u, v) for (u, v) in CAR_HALF] + [(u, v) for (u, v) in reversed(CAR_HALF)]
    outline = [(-23.5, 4)] + chan + [(23.5, 4), (23.5, 24), (-23.5, 24)]
    f.ext("w", 0, length, ("poly", outline), "add", joins=False)
    for sx in (-19, 19):
        for dw in (-15, 15):
            f.ext("v", 14, 25, ("circle", sx, length / 2 + dw, 4.2), "cut")
    return [start]


def end_cap(s, f):
    """End cap of a carriage: 47 wide, 18 high above the foot of the rail, 6 thick (w = 0..6), with a channel for the rail. One body."""
    start = s.n + 1
    outline = [(-23.5, 4), (-7.7, 4), (-7.7, 15.2), (7.7, 15.2), (7.7, 4), (23.5, 4), (23.5, 22), (-23.5, 22)]
    f.ext("w", 0, 6, ("poly", outline), "add", joins=False)
    return [start]


def grease_nipple(s, f):
    """A grease nipple: a cylinder of 5 x 4 on a neck of 3 x 2 (the local origin is where it meets the cap, it points to +w). One body."""
    start = s.n + 1
    f.ext("w", 0, 2, ("circle", 0, 0, 3), "add", joins=False)
    f.ext("w", 2, 6, ("circle", 0, 0, 5), "add")
    return [start]
