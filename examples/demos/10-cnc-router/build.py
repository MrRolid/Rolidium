#!/usr/bin/env python3
# Writes build.txt (run: python3 build.py). The program runs build.txt: rolidium cli run build.txt
import math, os, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
from rdm import Script, Frame
import parts
from parts import staged, copy_to, STAGE

s = Script("Gantry CNC router with a work area of 600 x 400 mm and 80+ mm of Z travel: a frame of aluminium profiles, a slotted table, the portal on two linear guides, "
           "ball screws with motors on all three axes, a spindle. All the parts of demos 5 to 9 are used again. About 190 bodies.")
OFF = (100.0, 100.0, 100.0)      # the whole machine is moved by this, so that the axes through the origin (where the bearings are revolved) are free

def P(x, y, z):
    return (x + OFF[0], y + OFF[1], z + OFF[2])

def box(x0, y0, z0, x1, y1, z1):
    return s.box(x0 + OFF[0], y0 + OFF[1], z0 + OFF[2], x1 - x0, y1 - y0, z1 - z0)

def cut_hole(plane_axis, a, b, c0, c1, d):
    """A round hole along a world axis: through (a, b) (the two other coordinates in the order of the sketch plane), from c0 to c1."""
    if plane_axis == "z":
        s.sketch("xy", c0 + OFF[2]); s.circle(a + OFF[0], b + OFF[1], d); s.pull(c1 - c0, "cut")
    elif plane_axis == "x":
        s.sketch("yz", c0 + OFF[0]); s.circle(a + OFF[1], b + OFF[2], d); s.pull(c1 - c0, "cut")
    else:
        s.sketch("xz", -(c1 + OFF[1])); s.circle(a + OFF[0], b + OFF[2], d); s.pull(c1 - c0, "cut")

UV_XZ = None
FRAMES = {      # (axis, sign, uv) of the frames used for the guides and parts, see rdm.Frame
    "y": ("y", 1, None),                                  # along Y, up = +Z, across = +X
    "x_front": ("x", 1, (("z", 1), ("y", -1))),           # along X, up = -Y (the front face of the crossbeam), across = +Z
    "z_front": ("z", 1, (("x", 1), ("y", -1))),           # along Z, up = -Y, across = +X
}

def build_at(spec, origin, builder, *args):
    axis, sign, uv = spec
    return staged(s, axis, sign, P(*origin), builder, *args, uv=uv)

# -------------------------------------------------------------------------------------------------------------------- the frame
s.comment("FRAME: two long profiles 40 x 80 x 1000, two end members 40 x 80 x 520, two middle members 40 x 40 x 520 (one profile section drawn once, extruded to each length).")
for x0 in (0, 560):
    build_at(("y", 1, None), (x0, 0, 20), parts.alu_profile, 40, 80, 1000, [(20, 20), (20, 60)])
for y0 in (0, 960):
    build_at(("x", 1, None), (40, y0, 20), parts.alu_profile, 40, 80, 520, [(20, 20), (20, 60)])
for y0 in (290, 670):
    build_at(("x", 1, None), (40, y0, 60), parts.alu_profile, 40, 40, 520, [(20, 20)])
s.comment("Eight corner connectors (30 mm blocks in the inner corners) and four feet.")
for (cx, cy) in [(40, 40), (530, 40), (40, 930), (530, 930)]:
    for z0 in (20, 70):
        box(cx, cy, z0, cx + 30, cy + 30, z0 + 30)
for (fx, fy) in [(-10, -10), (550, -10), (-10, 950), (550, 950)]:
    box(fx, fy, 0, fx + 60, fy + 60, 20)

s.comment("TABLE: a plate 400 x 600 x 15 with five T-slots (the work area), lying on the middle members.")
tb = box(100, 200, 100, 500, 800, 115)
for x in (140, 220, 300, 380, 460):
    # a T-slot: an opening 10 wide, 6 deep, a cavity 20 wide below it (4 deep), cut along Y
    s.sketch("xz", -(800 + OFF[1] + 1))
    pts = [(x - 5, 116), (x - 5, 109), (x - 10, 109), (x - 10, 105), (x + 10, 105), (x + 10, 109), (x + 5, 109), (x + 5, 116)]
    s.poly([(px + OFF[0], pz + OFF[2]) for px, pz in pts])
    s.pull(602, "cut")

# -------------------------------------------------------------------------------------------------------------------- guides
def guide_set(spec, rail_origin, length, carriage_ws, w_of_origin=0):
    """A rail and its carriages (each with two end caps and a nipple): the first set is built, the others are copies."""
    axis, sign, uv = spec
    f = Frame(s, axis, sign, P(*rail_origin), uv)
    build_at(spec, rail_origin, parts.linear_rail, length)
    first = None
    for wc in carriage_ws:
        # the carriage centre at w = wc: the carriage is 60 long, the caps are 6 thick on each end, the nipple on the far cap
        co = f.world(0, 0, wc - 30)
        co = (co[0] - OFF[0], co[1] - OFF[1], co[2] - OFF[2])
        if first is None:
            bodies = [build_at(spec, co, parts.carriage, 60)[0]]
            f2 = Frame(s, axis, sign, P(*co), uv)
            for (wl, builder) in ((-6, parts.end_cap), (60, parts.end_cap)):
                po = f2.world(0, 0, wl); po = (po[0] - OFF[0], po[1] - OFF[1], po[2] - OFF[2])
                bodies.append(build_at(spec, po, builder)[0])
            po = f2.world(0, 19, 66); po = (po[0] - OFF[0], po[1] - OFF[1], po[2] - OFF[2])
            bodies.append(build_at(spec, po, parts.grease_nipple)[0])
            first = (bodies, P(*co))
        else:
            d = tuple(P(*co)[k] - first[1][k] for k in range(3))
            for b in first[0]:
                copy_to(s, b, d)

# the Y guides: rails 900 long on the long profiles, two carriages on each (centres at y = 440 and 560)
s.comment("Y AXIS: two rails of size 15 (900 long), four carriages with end caps and nipples.")
for x0 in (20, 580):
    guide_set(FRAMES["y"], (x0, 50, 100), 900, [390, 510])      # w = y - 50
s.comment("PORTAL: two side plates 15 x 200 x 300 on the carriages and the cross beam (a profile 40 x 80 section, 545 long).")
box(12.5, 400, 124, 27.5, 600, 424)
box(572.5, 400, 124, 587.5, 600, 424)
build_at(("x", 1, None), (27.5, 480, 250), parts.alu_profile, 40, 80, 545, [(20, 20), (20, 60)])

s.comment("X AXIS: two rails on the front of the cross beam (500 long), four carriages.")
for zr in (265, 315):
    guide_set(FRAMES["x_front"], (50, 480, zr), 500, [190, 310])    # w = x - 50, centres at x = 240 and 360
s.comment("Z AXIS: the carrier plate on the X carriages, two vertical rails (230 long), four carriages, the spindle plate.")
box(200, 441, 230, 455, 456, 480)
for xr in (250, 350):
    guide_set(FRAMES["z_front"], (xr, 441, 240), 230, [60, 160])    # w = z - 240, centres at z = 300 and 400
box(220, 402, 265, 400, 417, 435)

# -------------------------------------------------------------------------------------------------------------------- the spindle
s.comment("SPINDLE: the parts of demo 9 (housing, nut, cutter, cap, gland, clamp mount, two clamp screws) on the spindle plate; the tool points down.")
SP = ("z", -1, (("x", 1), ("y", -1)))
SP_ORIGIN = (300, 357, 430)
for builder in (parts.spindle_body, parts.spindle_nut, parts.cutter, parts.spindle_cap, parts.cable_gland, parts.spindle_mount):
    build_at(SP, SP_ORIGIN, builder)
mf = Frame(s, SP[0], SP[1], P(*SP_ORIGIN), SP[2])
first = None
for w in parts.MOUNT_SCREWS:
    pos = mf.world(38, 38, w)
    if first is None:
        first = staged(s, "x", 1, pos, parts.screw, 5.8, 70, 10, 6, 5)[0]
        fpos = pos
    else:
        copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))

# -------------------------------------------------------------------------------------------------------------------- the drives
def drive(spec, origin):
    """The motor with its bracket, the clamp coupling and screws (demo 6): `origin` is the middle of the mounting face of the flange, the shaft points
    along the local +w of `spec`."""
    axis, sign, uv = spec
    for builder in (parts.nema23_motor, parts.motor_bracket):
        build_at(spec, origin, builder)
    f = Frame(s, axis, sign, P(*origin), uv)
    co = f.world(0, 0, 6); co = (co[0] - OFF[0], co[1] - OFF[1], co[2] - OFF[2])
    build_at(spec, co, parts.shaft_coupling)
    first = None
    for (sx, sy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)]:
        pos = f.world(sx * 23.57, sy * 23.57, 5)
        if first is None:
            first = staged(s, axis, sign, pos, parts.screw, 4.9, 9.5, 8.5, 5, 4, uv=uv)[0]; fpos = pos
        else:
            copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))
    # the two M3 screws of the coupling go along the local v (down from the pocket side), the head on the +v side
    cf = Frame(s, axis, sign, f.world(0, 0, 6), uv)
    first = None
    nv, sv = cf.names[1], cf.signs[1]
    for w in parts.COUPLING_SCREWS:
        pos = cf.world(8.5, 8, w)
        if first is None:
            first = staged(s, nv, sv, pos, parts.screw, 2.9, 17, 5.5, 3, 2.5)[0]; fpos = pos
        else:
            copy_to(s, first, tuple(pos[k] - fpos[k] for k in range(3)))

def bearing_blocks(spec1, origin1, spec2, origin2, riser1, riser2):
    for spec, origin, riser in ((spec1, origin1, riser1), (spec2, origin2, riser2)):
        build_at(spec, origin, parts.bearing_block)
        box(*riser)

def nut_and_holder(spec, origin, holder, bore_axis, bore_center, length=20):
    """The nut (26 x length) of a screw and its holder (a block with a bore of 26.2): cut the bore before the screw is there."""
    x0, y0, z0, x1, y1, z1 = holder
    box(*holder)
    c0, c1 = (x0, x1) if bore_axis == "x" else (y0, y1) if bore_axis == "y" else (z0, z1)
    cut_hole(bore_axis, bore_center[0], bore_center[1], c0 - 1, c1 + 1, 26.2)
    build_at(spec, origin, parts.screw_nut, length)

def lead_screw(spec, origin, wa, wb, wc, wd):
    build_at(spec, origin, parts.ball_screw, wa, wb, wc, wd)

# Y axis: the screw along Y at x = 60, z = 135; bearing blocks on risers on the end members, the motor in front
s.comment("Y AXIS DRIVE: ball screw 16 x 920 along Y, two bearing blocks of demo 5 on risers, a motor with a bracket and a coupling (demo 6), a nut with its holder on the portal.")
YA = ("y", 1, None)
nut_and_holder(YA, (60, 490, 135), (27.5, 485, 118, 92, 515, 152), "y", (60, 135))
bearing_blocks(YA, (60, 16, 135), ("y", -1, None), (60, 984, 135), (25, 14, 100, 95, 40, 115), (25, 960, 100, 95, 986, 115))
lead_screw(YA, (60, 0, 135), -24, 40, 960, 990)
drive(YA, (60, -46, 135))

# X axis: the screw along X at y = 500, z = 365 above the cross beam; the motor between the portal plate and the first bearing block
s.comment("X AXIS DRIVE: the same parts along X on the cross beam; the nut holder hangs on the carrier plate of the Z axis.")
XA = ("x", 1, None)
nut_and_holder(XA, (290, 500, 365), (285, 456, 347, 315, 528, 383), "x", (500, 365))
bearing_blocks(XA, (162, 500, 365), ("x", -1, None), (540, 500, 365), (160, 465, 330, 188, 535, 345), (514, 465, 330, 542, 535, 345))
lead_screw(XA, (0, 500, 365), 122, 186, 516, 560)
drive(XA, (100, 500, 365))

# Z axis: the screw along Z at x = 420, y = 406 in front of the carrier plate; the motor on top
s.comment("Z AXIS DRIVE: the same parts along Z; the bearing blocks sit on risers on the carrier plate, the motor on top, the nut holder on the spindle plate.")
ZA = ("z", 1, (("x", 1), ("y", -1)))
ZB = ("z", -1, (("x", 1), ("y", -1)))
nut_and_holder(ZA, (420, 406, 345), (400, 388, 335, 440, 424, 375), "z", (420, 406))
bearing_blocks(ZA, (420, 406, 236), ZB, (420, 406, 464), (385, 426, 236, 455, 441, 260), (385, 426, 440, 455, 441, 464))
lead_screw(ZA, (420, 406, 0), 228, 260, 440, 518)
drive(ZB, (420, 406, 540))
s.header(188, 20324348.795, no_overlap="yes", slow="yes")
s.raw("info")
s.write(os.path.join(here, "build.txt"))
