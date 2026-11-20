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
            f.chamfer(7, sx * 27.8, sy * 27.8, -30, body=start)                                 # the corners of the body
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


def staged(s, axis, sign, final, builder, *args, uv=None):
    """Builds a part at the staging area (frame axis/sign/uv, see rdm.Frame), then moves it so that its local origin is at `final` (a world point)."""
    f = Frame(s, axis, sign, STAGE, uv)
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


# ---- spindle with a clamp mount -------------------------------------------------------------------------------------------------------
# Local frame: w along the spindle, from the top (w = 0) toward the tool (w = 200 is the end of the body); the tool end is the `nose`.
def spindle_body(s, f):
    """The housing of a router spindle: a cylinder of 65 x 200 and a nose of 40 x 15 with a bore of 6.2 for the
    cutter (the nose runs from w = 200 to 215). One body."""
    start = s.n + 1
    f.ext("w", 0, 200, ("circle", 0, 0, 65), "add", joins=False)
    f.ext("w", 200, 215, ("circle", 0, 0, 40), "add")
    f.ext("w", 205, 216, ("circle", 0, 0, 6.2), "cut")
    return [start]


def spindle_nut(s, f):
    """The collet nut: 30 x 22 on the nose (w = 215 .. 237), bore 6.2 for the cutter. One body."""
    start = s.n + 1
    f.ext("w", 215, 237, ("circle", 0, 0, 30), "add", joins=False)
    f.ext("w", 214, 238, ("circle", 0, 0, 6.2), "cut")
    return [start]


def cutter(s, f):
    """An end mill of 6 mm: 55 long, from w = 205 (inside the nose) to 260, a smooth cylinder (the flutes are not drawn). One body."""
    start = s.n + 1
    f.ext("w", 205, 260, ("circle", 0, 0, 6), "add", joins=False)
    return [start]


def spindle_cap(s, f):
    """The cap on the top of the spindle: 60 x 12 (w = -12 .. 0). One body."""
    start = s.n + 1
    f.ext("w", -12, 0, ("circle", 0, 0, 60), "add", joins=False)
    return [start]


def cable_gland(s, f):
    """A cable gland of 16 x 14 on the cap (w = -26 .. -12), 18 mm off the axis along +u. One body."""
    start = s.n + 1
    f.ext("w", -26, -12, ("circle", 18, 0, 16), "add", joins=False)
    f.ext("w", -26, -22, ("circle", 18, 0, 8), "cut")
    return [start]


def spindle_mount(s, f):
    """The clamp mount of the spindle: a block 90 x 90 x 60 (u, v from -45 to 45, w = 70 .. 130) with a clamping bore of 65.1, a slit of 1.5 mm
    toward +v, two holes across the slit for M6 clamp screws (heads in pockets on +u) and four holes of 6.6 to bolt it down (along v, at u = +-38).
    One body."""
    start = s.n + 1
    f.ext("w", 70, 130, ("rect", -45, -45, 45, 45), "add", joins=False)
    f.ext("w", 69, 131, ("circle", 0, 0, 65.1), "cut")
    f.ext("w", 69, 131, ("rect", -0.75, 0, 0.75, 46), "cut")                       # the slit from the bore to the front face (+v)
    for w in MOUNT_SCREWS:
        f.ext("u", -46, 46, ("circle", 38, w, 6.4), "cut")                          # across the slit at v = 38
        f.ext("u", 38, 46, ("circle", 38, w, 11), "cut")                            # the pocket for the head of the screw on +u
    for sx in (-38, 38):
        for w in (80, 120):
            f.ext("v", -46, 46, ("circle", sx, w, 6.6), "cut")
    return [start]


MOUNT_SCREWS = (85, 115)    # where the two clamp screws are along the spindle


# ---- ball bearing 608 in a pillow block ------------------------------------------------------------------------------------------------
R_BALL = 3.969 / 2
R_PITCH = 7.75          # the balls run on a circle of this radius
GROOVE = 2.05           # radius of the raceway grooves


def bearing_block(s, f):
    """Pillow block with a 608 ball bearing (demo 5): the shaft axis is the local w axis through u = v = 0, the bearing sits in the seat at w = 0 .. 7
    (so its open side is on w = 0), the housing runs from w = 0 to 24, its base is below (v = -20 .. -12, 70 wide). Returns the bodies: housing, inner ring,
    outer ring, seven balls. The rings and balls are revolved about the world axis through the origin, so that place must be empty."""
    A, sg = f.names[2], f.signs[2]
    dz = math.sqrt(GROOVE ** 2 - (R_PITCH - 6.25) ** 2)

    def pc(r, a):       # (radial, axial) -> sketch coordinates in the sketch plane that contains the axis
        return (a, r) if A == "x" else (r, a)
    plane = "xz" if A == "z" else "xy"

    def ring(segs):
        s.sketch(plane)
        for seg in segs:
            if seg[0] == "line":
                (r1, a1), (r2, a2) = seg[1], seg[2]
                s.line(*pc(r1, sg * a1), *pc(r2, sg * a2))
            else:
                (r1, a1), (rm, am), (r2, a2) = seg[1], seg[2], seg[3]
                s.arc(*pc(r1, sg * a1), *pc(rm, sg * am), *pc(r2, sg * a2))
        s.raw(f"revolve 0 {A} 360 add")
        s.n += 1
    housing = s.n + 1
    f.ext("w", 0, 24, ("rect", -35, -20, 35, -12), "add", joins=False)
    f.ext("w", 0, 24, ("rect", -18, -12, 18, 16), "add")
    for sx in (-35, 35):
        for w in (0, 24):
            f.round(3, sx, -16, w, body=housing)
    for sx in (-18, 18):
        f.round(3, sx, 16, 12, body=housing)
    f.ext("w", -1, 25, ("circle", 0, 0, 19), "cut")
    f.ext("w", -1, 7, ("circle", 0, 0, 22.05), "cut")
    for sx in (-26, 26):
        f.ext("v", -21, -11, ("slot", sx, 12, "a", 4, 6.6), "cut")
    f.ext("v", 8, 17, ("circle", 0, 12, 6), "cut")
    first = s.n + 1
    ring([("line", (4, 0), (6.25, 0)), ("line", (6.25, 0), (6.25, 3.5 - dz)),
          ("arc", (6.25, 3.5 - dz), (R_PITCH - GROOVE, 3.5), (6.25, 3.5 + dz)),
          ("line", (6.25, 3.5 + dz), (6.25, 7)), ("line", (6.25, 7), (4, 7)), ("line", (4, 7), (4, 0))])
    ring([("line", (9.25, 0), (11, 0)), ("line", (11, 0), (11, 7)), ("line", (11, 7), (9.25, 7)),
          ("line", (9.25, 7), (9.25, 3.5 + dz)),
          ("arc", (9.25, 3.5 + dz), (R_PITCH + GROOVE, 3.5), (9.25, 3.5 - dz)),
          ("line", (9.25, 3.5 - dz), (9.25, 0))])
    # the ball: a half disc whose diameter lies on the axis, revolved about it
    s.sketch(plane)
    s.arc(*pc(0, -R_BALL), *pc(R_BALL, 0), *pc(0, R_BALL))
    s.line(*pc(0, R_BALL), *pc(0, -R_BALL))
    s.raw(f"revolve 0 {A} 360 add")
    s.n += 1
    ball0 = s.n
    others = [n for n in ("x", "y", "z") if n != A]
    for k in range(7):
        a = 2 * math.pi * k / 7
        pos = [0.0, 0.0, 0.0]
        pos[_IDX_[others[0]]] = R_PITCH * math.cos(a)
        pos[_IDX_[others[1]]] = R_PITCH * math.sin(a)
        pos[_IDX_[A]] = sg * 3.5
        d = tuple(pos[i] + f.origin[i] for i in range(3))
        if k < 6:
            copy_to(s, ball0, d)
        else:
            s.move(d[0], d[1], d[2], ball0)
    # the rings go to the frame's origin too (they were made at the world origin)
    s.move(*f.origin, first)
    s.move(*f.origin, first + 1)
    balls = [ball0] + list(range(s.n - 5, s.n + 1))
    return [housing, first, first + 1] + sorted(set(balls))


_IDX_ = {"x": 0, "y": 1, "z": 2}


# ---- aluminium profile, ball screw, nut ---------------------------------------------------------------------------------------------------
def alu_outline(width, height, slots):
    """Outline of a T-slot aluminium profile, width (u) x height (v), as a closed polygon. `slots` is a dict with the centres of the T-slots on each side:
    {'bottom': [...], 'right': [...], 'top': [...], 'left': [...]} (the positions along the side, counted from the start of the side going round
    counterclockwise: bottom left to right, right bottom to top, top right to left, left top to bottom). A T-slot: opening 8 wide through a lip 3 thick,
    a cavity 16 wide to a depth of 9."""
    pts = []
    corners = [((0, 0), (1, 0), width), ((width, 0), (0, 1), height), ((width, height), (-1, 0), width), ((0, height), (0, -1), height)]
    for (name, (o, e, length)) in zip(("bottom", "right", "top", "left"), corners):
        n = (-e[1], e[0])            # inward: to the left of the direction of travel
        pts.append(o)
        for c in sorted(slots.get(name, [])):
            for (t, d) in [(c - 4, 0), (c - 4, 3), (c - 8, 3), (c - 8, 9), (c + 8, 9), (c + 8, 3), (c + 4, 3), (c + 4, 0)]:
                pts.append((o[0] + e[0] * t + n[0] * d, o[1] + e[1] * t + n[1] * d))
    return pts


def alu_profile(s, f, width, height, length, bores):
    """A T-slot profile `width` x `height` (u, v from 0), `length` long along w, with the given centre bores (list of (u, v)), slots in the middle of the sides
    (two on the long sides of an 80 mm side). One body."""
    start = s.n + 1
    def centres(length_side):
        return [20] if length_side <= 40 else [20, length_side - 20]
    slots = {"bottom": centres(width), "right": centres(height), "top": centres(width), "left": centres(height)}
    f.ext("w", 0, length, ("poly", alu_outline(width, height, slots)), "add", joins=False)
    for (u, v) in bores:
        f.ext("w", -1, length + 1, ("circle", u, v, 8.5), "cut")
    return [start]


def ball_screw(s, f, w_a, w_b, w_c, w_d):
    """A ball screw drawn as a smooth cylinder of 16 from w_b to w_c with journals of 7.98 from w_a to w_b and from w_c to w_d. One body."""
    start = s.n + 1
    f.ext("w", w_b, w_c, ("circle", 0, 0, 16), "add", joins=False)
    f.ext("w", w_a, w_b, ("circle", 0, 0, 7.98), "add")
    f.ext("w", w_c, w_d, ("circle", 0, 0, 7.98), "add")
    return [start]


def screw_nut(s, f, length=20):
    """A nut for the ball screw: 26 x length (w = 0 .. length), a smooth bore of 16.1. One body."""
    start = s.n + 1
    f.ext("w", 0, length, ("circle", 0, 0, 26), "add", joins=False)
    f.ext("w", -1, length + 1, ("circle", 0, 0, 16.1), "cut")
    return [start]
