#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
# Axes of the machine: X = the travel of the portal along the long profiles of the frame, Y = the travel along the cross beam of the portal, Z = vertical.
import math, os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts
from parts import staged, copy_to

s = Script("Gantry CNC router, work area 600 x 400 mm: X = the portal on two ball screws along the frame, Y = the carrier on the cross beam, Z = the vertical "
           "axis with a belt drive. Parts of demos 5 to 9 are used again. Every part is screwed to something; the bodies are listed by group.")
OFF = (100.0, 100.0, 100.0)      # the whole machine is moved by this: the bearings are revolved about the axes through the origin, which must be free

def P(x, y, z):
    return (x + OFF[0], y + OFF[1], z + OFF[2])

def put(spec, origin, builder, *args, names=None):
    axis, sign, uv = spec
    return staged(s, axis, sign, P(*origin), builder, *args, uv=uv, names=names)

def box(x0, y0, z0, x1, y1, z1, name):
    s.box(x0 + OFF[0], y0 + OFF[1], z0 + OFF[2], x1 - x0, y1 - y0, z1 - z0)
    s.name(s.n, name)
    return s.n

def cut_hole(axis, ab, c0, c1, d):
    """A round hole along a world axis through (a, b) (the other two coordinates in the order of the sketch plane: z -> x, y; x -> y, z; y -> x, z)."""
    lo, hi = sorted((c0, c1))
    a, b = ab
    if axis == "z":
        s.sketch("xy", lo + OFF[2]); s.circle(a + OFF[0], b + OFF[1], d)
    elif axis == "x":
        s.sketch("yz", lo + OFF[0]); s.circle(a + OFF[1], b + OFF[2], d)
    else:
        s.sketch("xz", -(hi + OFF[1])); s.circle(a + OFF[0], b + OFF[2], d)
    s.pull(hi - lo, "cut")

def xyz(axis, ab, c):
    a, b = ab
    return {"x": (c, a, b), "y": (a, c, b), "z": (a, b, c)}[axis]

# --------------------------------------------------------------------------------------------------------------------------------- screws
KINDS = {"M3": (3.2, 2.9, 5.5, 3, 2.5), "M5": (5.5, 4.9, 8.5, 5, 4), "M6": (6.6, 5.8, 10, 6, 5), "M8": (8.5, 7.8, 13, 8, 6)}   # hole, shank, head, head height, socket
BOLTS = []

def bolt(kind, axis, sgn, ab, plane, length, cut=True):
    """A screw: the head stands on `plane` (a coordinate on `axis`), the shank goes in the direction sgn (+1 / -1) for `length`; the hole is cut through
    everything on its way before the screws are placed (cut=False when the parts have the holes already)."""
    BOLTS.append((kind, axis, sgn, ab, plane, length, cut))

def flush_bolts():
    for (kind, axis, sgn, ab, plane, length, cut) in BOLTS:
        if cut:
            cut_hole(axis, ab, plane, plane + sgn * (length + 0.3), KINDS[kind][0])
    cache = {}
    for (kind, axis, sgn, ab, plane, length, cut) in BOLTS:
        hole, shank, head, hh, sock = KINDS[kind]
        L = length - 0.3
        origin = xyz(axis, ab, plane)
        key = (kind, axis, sgn, round(L, 2))
        if key not in cache:
            b = put((axis, -sgn, None), origin, parts.screw, shank, L, head, hh, sock)[0]
            s.name(b, f"Screw {kind}x{int(round(L))}")
            cache[key] = (b, origin)
        else:
            b0, o0 = cache[key]
            copy_to(s, b0, tuple(origin[k] - o0[k] for k in range(3)))
    BOLTS.clear()

# --------------------------------------------------------------------------------------------------------------------------------- helpers
LINKS = []
def link(a, b, reason):
    LINKS.append((a, b, reason))

def guide_set(spec, rail_origin, length, centres_w, rail_name, car_name):
    """A rail and its carriages (each with two end caps and a nipple). The first carriage set is built, the others are copies. Returns (rail, [carriage...])."""
    axis, sign, uv = spec
    f = Frame(s, axis, sign, P(*rail_origin), uv)
    rail = put(spec, rail_origin, parts.linear_rail, length, names=[rail_name])[0]
    first, cars = None, []
    for k, wc in enumerate(centres_w):
        co = f.world(0, 0, wc - 30); co = tuple(co[i] - OFF[i] for i in range(3))
        nm = f"{car_name} {k + 1}"
        if first is None:
            bodies = [put(spec, co, parts.carriage, 60, names=[nm])[0]]
            f2 = Frame(s, axis, sign, P(*co), uv)
            for (wl, builder, suffix) in ((-6, parts.end_cap, "end cap front"), (60, parts.end_cap, "end cap rear")):
                po = f2.world(0, 0, wl); po = tuple(po[i] - OFF[i] for i in range(3))
                bodies.append(put(spec, po, builder, names=[f"{nm} {suffix}"])[0])
            po = f2.world(0, 19, 66); po = tuple(po[i] - OFF[i] for i in range(3))
            bodies.append(put(spec, po, parts.grease_nipple, names=[f"{nm} grease nipple"])[0])
            first = (bodies, P(*co))
            cars.append(bodies[0])
        else:
            d = tuple(P(*co)[i] - first[1][i] for i in range(3))
            nb = []
            for j, b in enumerate(first[0]):
                suffix = ["", " end cap front", " end cap rear", " grease nipple"][j]
                nb.append(copy_to(s, b, d, name=f"{nm}{suffix}"))
            cars.append(nb[0])
    for c in cars:
        link(rail, c, "the carriage rides on the rail with 0.1 mm clearance (the balls are not modelled)")
    return rail, cars

def bearing(spec, origin, prefix, slot_u=26):
    names = [f"{prefix} housing", f"{prefix} bearing inner ring", f"{prefix} bearing outer ring"] + [f"{prefix} bearing ball {i}" for i in range(1, 8)]
    b = put(spec, origin, parts.bearing_block, slot_u, names=names)
    BALLS.extend(b[3:])
    link(b[1], b[2], "the inner and the outer ring are held apart by the balls")
    return b

BALLS = []

def screw_with_nut(spec_screw, origin, w4, name, spec_nut, nut_origin, nut_name, holder_bounds, holder_name, bore_axis, bore_ab, nut_len=20):
    sc = put(spec_screw, origin, parts.ball_screw, *w4, names=[name])[0]
    box(*holder_bounds, holder_name)
    h = s.n
    return sc, h

def drive(spec, origin, prefix, slot_u=20, tension=False, coupling=True):
    """The motor, its bracket, a coupling and the screws that hold the motor to the bracket (demo 6)."""
    axis, sign, uv = spec
    m = put(spec, origin, parts.nema23_motor, names=[f"{prefix} motor"])[0]
    br = put(spec, origin, parts.motor_bracket, slot_u, tension, names=[f"{prefix} motor bracket"])[0]
    f = Frame(s, axis, sign, P(*origin), uv)
    cp = None
    if coupling:
        co = f.world(0, 0, 6); co = tuple(co[i] - OFF[i] for i in range(3))
        cp = put(spec, co, parts.shaft_coupling, names=[f"{prefix} shaft coupling"])[0]
    first = None
    for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)]:
        pos = f.world(sx * 23.57, sy * 23.57, 5)
        if first is None:
            first = staged(s, axis, sign, pos, parts.screw, 4.9, 9.5, 8.5, 5, 4, uv=uv, names=["Screw M5x9"])[0]; fpos = pos
        else:
            copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
    if coupling:
        cf = Frame(s, axis, sign, f.world(0, 0, 6), uv)
        nv, sv = cf.names[1], cf.signs[1]
        first = None
        for w in parts.COUPLING_SCREWS:
            pos = cf.world(8.5, 8, w)
            if first is None:
                first = staged(s, nv, sv, pos, parts.screw, 2.9, 17, 5.5, 3, 2.5, names=["Screw M3x17"])[0]; fpos = pos
            else:
                copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
    link(m, br, "the motor flange lies on the bracket plate")
    return m, br, cp

FX = ("x", 1, None)        # along X, up = +Z, across = +Y
FY = ("y", 1, None)        # along Y, up = +Z, across = +X
JOURNALS = []

# ========================================================================================================================================= 1 FRAME
s.comment("1 FRAME: two long profiles 40 x 80 x 1000 (along X), two end members 40 x 80 x 520, two middle members 40 x 40 x 520 (one T-slot section, drawn once and extruded to every length),")
s.comment("eight corner connectors screwed into the profiles, four feet screwed under the long profiles.")
frame_ids = {}
frame_ids["long left"] = put(FX, (0, 0, 20), parts.alu_profile, 40, 80, 1000, [(20, 20), (20, 60)], names=["Frame profile long left"])[0]
frame_ids["long right"] = put(FX, (0, 560, 20), parts.alu_profile, 40, 80, 1000, [(20, 20), (20, 60)], names=["Frame profile long right"])[0]
frame_ids["end front"] = put(FY, (0, 40, 20), parts.alu_profile, 40, 80, 520, [(20, 20), (20, 60)], names=["Frame profile end front"])[0]
frame_ids["end rear"] = put(FY, (960, 40, 20), parts.alu_profile, 40, 80, 520, [(20, 20), (20, 60)], names=["Frame profile end rear"])[0]
frame_ids["mid front"] = put(FY, (290, 40, 60), parts.alu_profile, 40, 40, 520, [(20, 20)], names=["Frame profile middle front"])[0]
frame_ids["mid rear"] = put(FY, (670, 40, 60), parts.alu_profile, 40, 40, 520, [(20, 20)], names=["Frame profile middle rear"])[0]
for xs, xn in ((40, "front"), (930, "rear")):
    for ys, yn in ((40, "left"), (530, "right")):
        for z0, zn in ((20, "lower"), (70, "upper")):
            box(xs, ys, z0, xs + 30, ys + 30, z0 + 30, f"Frame corner connector {xn} {yn} {zn}")
            if zn == "lower":      # lower connectors are screwed into the long profile (along Y), at the middle of the T-slot (z = 40)
                for dx in (8, 22):
                    bolt("M5", "y", -1 if ys == 40 else 1, (xs + dx, 40), 70 if ys == 40 else 530, 35, cut=True)
            else:                  # upper connectors are screwed into the end member (along X), z = 80
                for dy in (8, 22):
                    bolt("M5", "x", -1 if xs == 40 else 1, (ys + dy, 80), 70 if xs == 40 else 930, 35, cut=True)
for (fx, fy, nm) in [(-10, -10, "front left"), (-10, 550, "front right"), (950, -10, "rear left"), (950, 550, "rear right")]:
    box(fx, fy, 0, fx + 60, fy + 60, 20, f"Frame foot {nm}")
    bolt("M6", "z", 1, (fx + 30, fy + 30), 0, 26, cut=True)
flush_bolts()

# ========================================================================================================================================= 2 TABLE
s.comment("2 TABLE: a plate 600 x 400 x 15 with five T-slots along X, screwed to the middle members.")
box(200, 100, 100, 800, 500, 115, "Table plate")
for y in (140, 220, 300, 380, 460):
    s.sketch("yz", 199 + OFF[0])
    pts = [(y - 5, 116), (y - 5, 109), (y - 10, 109), (y - 10, 105), (y + 10, 105), (y + 10, 109), (y + 5, 109), (y + 5, 116)]
    s.poly([(py + OFF[1], pz + OFF[2]) for py, pz in pts])
    s.pull(602, "cut")
for x in (310, 690):
    for y in (180, 420):
        bolt("M6", "z", -1, (x, y), 115, 20, cut=True)
flush_bolts()

# ========================================================================================================================================= 3 X AXIS
s.comment("3 X AXIS (the portal along the frame): two rails of size 15 on the long profiles, four carriages, and on the OUTSIDE of each long profile a ball screw 16 x 851")
s.comment("in two bearing blocks screwed to the profile, a stepper motor with a bracket and a coupling at the front end, and a nut with its holder that hangs under the portal.")
xrails = {}
for side, y in (("left", 20), ("right", 580)):
    xrails[side] = guide_set(FX, (50, y, 100), 900, [390, 510], f"X rail {side}", f"X carriage {side}")
for side, ya, sgny in (("left", -35, -1), ("right", 635, 1)):
    uv = (("z", 1), ("y", sgny))
    prefix = f"X {side}"
    yh0, yh1 = (-55, -4) if side == "left" else (604, 655)
    holder = box(495, yh0, 40, 525, yh1, 124, f"X nut holder {side}")
    cut_hole("x", (ya, 60), 494, 526, 26.2)
    sc = put(FX, (0, ya, 60), parts.ball_screw, 22, 69, 893, 920, names=[f"X ball screw {side}"])[0]
    b1 = bearing(("x", 1, uv), (62, ya, 60), f"X bearing block {side} front")
    b2 = bearing(("x", -1, uv), (900, ya, 60), f"X bearing block {side} rear")
    ys0, ys1 = (-15, 0) if side == "left" else (600, 615)
    box(60, ys0, 25, 88, ys1, 95, f"X bearing block spacer {side} front")
    box(874, ys0, 25, 902, ys1, 95, f"X bearing block spacer {side} rear")
    link(sc, b1[1], "the screw shoulder rests on the inner ring")      # (touches: this link is only a safety)
    face = 0 if side == "left" else 600
    head = face + sgny * 23       # the top of the base of a block (8 thick) on a spacer 15 thick, outside the profile face
    for (xb, sg) in ((74, 1), (888, -1)):
        for dz in (-26, 26):
            bolt("M6", "y", -sgny, (xb, 60 + dz), head, 8 + 15 + 6, cut=True)
    nut = put(FX, (500, ya, 60), parts.screw_nut, 20, names=[f"X nut {side}"])[0]
    link(nut, holder, "the nut sits in the bore of the holder with 0.1 mm clearance")
    link(nut, sc, "the thread of the nut is not modelled (smooth bore)")
    m, br, cp = drive(("x", 1, uv), (0, ya, 60), prefix)
    link(cp, m, "the shaft of the motor is clamped in the coupling (clearance fit)")
    link(cp, sc, "the screw journal is clamped in the coupling (clearance fit)")
    foot = 0 if side == "left" else 600
    for dz in (-20, 20):
        bolt("M6", "y", -sgny, (30, 60 + dz), foot + sgny * 5, 5 + 5, cut=False)
flush_bolts()

# ========================================================================================================================================= 4 PORTAL
s.comment("4 PORTAL: two side plates (an L: a plate 15 mm and a foot that is screwed to the X carriages), the cross beam 40 x 80 screwed to both plates.")
def side_plate(s_, f, pts):
    f.ext("w", 400, 600, ("poly", pts), "add", joins=False)
    return [s_.n]
put(FX, (0, 0, 0), side_plate, [(-14, 124), (50, 124), (50, 139), (27.5, 139), (27.5, 424), (12.5, 424), (12.5, 139), (-14, 139)], names=["Portal side plate left"])
put(FX, (0, 0, 0), side_plate, [(550, 124), (614, 124), (614, 139), (587.5, 139), (587.5, 424), (572.5, 424), (572.5, 139), (550, 139)], names=["Portal side plate right"])
beam = put(FY, (480, 27.5, 250), parts.alu_profile, 40, 80, 545, [(20, 20), (20, 60)], names=["Portal cross beam"])[0]
for side, yr in (("left", 20), ("right", 580)):
    for xc in (440, 560):
        for dx in (-15, 15):
            for dy in (-19, 19):
                bolt("M5", "z", -1, (xc + dx, yr + dy), 139, 24, cut=True)
    for x in (500, 520):
        bolt("M5", "z", -1, (x, -8 if side == "left" else 608), 139, 23, cut=True)
for (y0, sg) in ((12.5, 1), (587.5, -1)):
    for z in (270, 310):
        bolt("M8", "y", sg, (500, z), y0, 35, cut=True)
flush_bolts()

# ========================================================================================================================================= 5 Y AXIS
s.comment("5 Y AXIS (the carrier along the cross beam): two rails on the -X face of the cross beam, four carriages, a ball screw on top of the cross beam in two blocks on risers,")
s.comment("a motor with a bracket and a coupling, a nut with a holder screwed to the Z fixed plate.")
YR = ("y", 1, (("z", 1), ("x", -1)))
yrails = [guide_set(YR, (480, 70, zr), 450, [130, 330], f"Y rail {n}", f"Y carriage {n}") for n, zr in (("lower", 255), ("upper", 305))]
yh = box(456, 285, 347, 528, 315, 383, "Y nut holder")
cut_hole("y", (500, 365), 284, 316, 26.2)
screw_y = put(FY, (500, 0, 365), parts.ball_screw, 112, 169, 533, 560, names=["Y ball screw"])[0]
yb1 = bearing(("y", 1, None), (500, 162, 365), "Y bearing block front")
yb2 = bearing(("y", -1, None), (500, 540, 365), "Y bearing block rear")
box(465, 150, 330, 535, 206, 345, "Y bearing block riser front")
box(465, 496, 330, 535, 552, 345, "Y bearing block riser rear")
for (yb, sg) in ((174, 1), (528, 1)):
    for dx in (-26, 26):
        bolt("M6", "z", -1, (500 + dx, yb), 353, 8 + 10, cut=True)
for yb in (156, 196, 502, 546):
    bolt("M6", "z", -1, (500, yb), 345, 15 + 4, cut=True)
ynut = put(FY, (500, 290, 365), parts.screw_nut, 20, names=["Y nut"])[0]
link(ynut, yh, "the nut sits in the bore of the holder with 0.1 mm clearance")
link(ynut, screw_y, "the thread of the nut is not modelled (smooth bore)")
ym, ybr, ycp = drive(("y", 1, None), (500, 90, 365), "Y", slot_u=12)
link(ycp, ym, "the shaft of the motor is clamped in the coupling (clearance fit)")
link(ycp, screw_y, "the screw journal is clamped in the coupling (clearance fit)")
for xx in (488, 512):
    bolt("M6", "z", -1, (xx, 120), 335, 5 + 12, cut=True)
flush_bolts()

# ========================================================================================================================================= 6 Z AXIS
s.comment("6 Z AXIS, from the back to the front: the fixed plate on the four Y carriages; on its front two spacer strips with the vertical rails; the ball screw exactly between them,")
s.comment("in two blocks screwed to the fixed plate; the moving plate on four carriages (the screw is hidden behind it, the nut holder is screwed to its back; the heads of its screws are sunk in")
s.comment("counterbores); the motor on the back of the fixed plate with the shaft up, a toothed belt to a pulley on the top of the screw (the belt passes a window in the plate and one in the bracket).")
def cbore(axis, ab, plane, sgn, depth, d):
    cut_hole(axis, ab, plane, plane + sgn * depth, d)
fixed = box(441, 150, 230, 456, 450, 600, "Z fixed plate")
s.sketch("yz", 440 + OFF[0])
s.rect(278 + OFF[1], 523 + OFF[2], 322 + OFF[1], 539 + OFF[2])
s.pull(17, "cut")                                  # the window for the belt
for zr in (255, 305):
    for yc in (200, 400):
        for dz in (-19, 19):
            for dy in (-15, 15):
                bolt("M5", "x", 1, (yc + dy, zr + dz), 441, 24, cut=True)
sp = [box(429, 240, 240, 441, 260, 540, "Z rail spacer left"), box(429, 340, 240, 441, 360, 540, "Z rail spacer right")]
for y in (250, 350):
    for z in (260, 520):
        bolt("M5", "x", -1, (y, z), 456, 25, cut=True)
ZR = ("z", 1, (("y", 1), ("x", -1)))
zrails = [guide_set(ZR, (429, yr, 240), 300, [100, 180], f"Z rail {n}", f"Z carriage {n}") for n, yr in (("left", 250), ("right", 350))]
mov = box(390, 205, 300, 405, 395, 460, "Z moving plate")
CB = 5.2
for yr in (250, 350):
    for zc in (340, 420):
        for dy in (-19, 19):
            for dz in (-15, 15):
                cbore("x", (yr + dy, zc + dz), 390, 1, CB, 9.5)
                bolt("M5", "x", 1, (yr + dy, zc + dz), 390 + CB, 15 - CB + 9, cut=True)
for dy in (-19, 19):
    cbore("x", (300 + dy, 385), 390, 1, CB, 9.5)
    bolt("M5", "x", 1, (300 + dy, 385), 390 + CB, 15 - CB + 10, cut=True)
zh = box(405, 276, 365, 438, 324, 405, "Z nut holder")
cut_hole("z", (421, 300), 364, 406, 26.2)
screw_z = put(("z", 1, None), (421, 300, 0), parts.ball_screw, 228, 243, 517, 545, names=["Z ball screw"])[0]
UVZ = (("y", 1), ("x", -1))
zb1 = bearing(("z", 1, UVZ), (421, 300, 236), "Z bearing block lower")
zb2 = bearing(("z", -1, UVZ), (421, 300, 524), "Z bearing block upper")
for zz in (248, 512):
    for dy in (-26, 26):
        bolt("M6", "x", 1, (300 + dy, zz), 433, 8 + 8, cut=True)
znut = put(("z", 1, None), (421, 300, 375), parts.screw_nut, 20, names=["Z nut"])[0]
link(znut, zh, "the nut sits in the bore of the holder with 0.1 mm clearance")
link(znut, screw_z, "the thread of the nut is not modelled (smooth bore)")
ZM = ("z", 1, (("y", 1), ("x", 1)))
ZO = (491, 300, 520)
zm = put(ZM, ZO, parts.nema23_motor, names=["Z motor"])[0]
zbr = put(ZM, ZO, parts.motor_bracket, 27, True, (21, 5, 22), names=["Z motor bracket"])[0]
fm = Frame(s, "z", 1, P(*ZO), ZM[2])
first = None
for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)]:
    pos = fm.world(sx * 23.57, sy * 23.57, 5)
    if first is None:
        first = staged(s, "z", 1, pos, parts.screw, 4.9, 9.5, 8.5, 5, 4, uv=ZM[2], names=["Screw M5x9"])[0]; fpos = pos
    else:
        copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
link(zm, zbr, "the motor flange lies on the bracket plate")
for dy in (-27, 27):
    bolt("M6", "x", -1, (300 + dy, 550), 461, 5 + 10, cut=True)
pul_m = put(("z", 1, None), (491, 300, 526), parts.pulley, names=["Z motor pulley"])[0]
pul_s = put(("z", 1, None), (421, 300, 526), parts.pulley, names=["Z ball screw pulley"])[0]
link(pul_m, zm, "the pulley sits on the shaft of the motor (clearance fit)")
link(pul_s, screw_z, "the pulley sits on the journal of the screw (clearance fit)")
s.sketch("xy", 527 + OFF[2])
def stadium(r):
    x0, x1, yc = 421 + OFF[0], 491 + OFF[0], 300 + OFF[1]
    s.line(x0, yc - r, x1, yc - r)
    s.arc(x1, yc - r, x1 + r, yc, x1, yc + r)
    s.line(x1, yc + r, x0, yc + r)
    s.arc(x0, yc + r, x0 - r, yc, x0, yc - r)
stadium(19.2)
stadium(18.2)
s.pull(8, "new", region=0)
s.n += 1
s.name(s.n, "Z timing belt")
belt = s.n
link(belt, pul_m, "the belt runs on the teeth of the pulley (0.2 mm clearance)")
link(belt, pul_s, "the belt runs on the teeth of the pulley (0.2 mm clearance)")
flush_bolts()

# ========================================================================================================================================= 7 SPINDLE
s.comment("7 SPINDLE: the clamp mount (demo 9) on the front of the moving plate, the spindle in it, the tool points down.")
ZS = ("z", -1, (("y", 1), ("x", -1)))   # (axis, sign, uv)
SO = (345, 300, 470)
sb = {}
for key, builder, nm in (("mount", parts.spindle_mount, "Spindle clamp mount"), ("body", parts.spindle_body, "Spindle housing"), ("nut", parts.spindle_nut, "Spindle collet nut"),
                         ("cutter", parts.cutter, "End mill 6 mm"), ("cap", parts.spindle_cap, "Spindle cap"), ("gland", parts.cable_gland, "Spindle cable gland")):
    sb[key] = put(ZS, SO, builder, names=[nm])[0]
link(sb["cutter"], sb["body"], "the cutter sits in the bore of the nose (0.1 mm clearance)")
link(sb["cutter"], sb["nut"], "the cutter passes the collet nut (0.1 mm clearance)")
link(sb["mount"], sb["body"], "the housing sits in the clamping bore of the mount (0.05 mm clearance)")
for dy in (-38, 38):
    for w in (80, 120):
        bolt("M6", "x", 1, (300 + dy, SO[2] - w), 300, 100, cut=True)
mf = Frame(s, "z", -1, P(*SO), ZS[2])
first = None
for w in parts.MOUNT_SCREWS:
    pos = mf.world(38, 38, w)
    if first is None:
        first = staged(s, "y", 1, pos, parts.screw, 5.8, 70, 10, 6, 5, names=["Screw M6x70"])[0]; fpos = pos
    else:
        copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
link(first, sb["mount"], "the clamp screws sit in the pockets of the mount")
flush_bolts()

# ========================================================================================================================================= checks of the test
s.exempt(BALLS, "the balls of the bearings float in their raceways (0.065 mm clearance)")
for (a, b, why) in LINKS:
    s.link(a, b, why)
s.header(340, 22228681.143, no_overlap="yes", connected="yes", slow="yes", user_model="yes")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
print("bodies counted by the script:", s.n)
