# Helpers that write `rolidium cli` scenario lines. Every demo has a `build.py` that uses them and writes its `build.txt`;
# the scenario is what the program runs (`rolidium cli run build.txt`), the Python only saves typing for repeated shapes.
#
# Sketch planes: xy (normal +Z, sketch x = X, sketch y = Y), xz (normal -Y, x = X, y = Z), yz (normal +X, x = Y, y = Z).
# A positive distance pulls along the normal of the plane; the offset moves the plane along its normal.

class Script:
    def __init__(self, title, lines=()):
        self.out = [f"# {l}" for l in ([title] + list(lines))]
        self.n = 0  # number of bodies (the caller keeps it right when a pull joins bodies)

    def raw(self, line):
        self.out.append(line)

    def comment(self, text):
        self.out.append(f"# {text}")

    def header(self, bodies, volume, **meta):
        """The checks of the test (tests/demos.rs): body count, total volume, and `no_overlap='yes'`, `slow='yes'`."""
        lines = [f"# bodies: {bodies}", f"# volume: {volume}"]
        for k, v in meta.items():
            lines.append(f"# {k.replace('_', '-')}: {v}")
        for l in lines:
            self.out.insert(self._head_end(), l)

    def _head_end(self):
        i = 0
        while i < len(self.out) and self.out[i].startswith("# "):
            i += 1
        return i

    # ---- drawing -------------------------------------------------------------------------------------------------
    def sketch(self, plane, off=0.0):
        self.out.append(f"sketch {plane} {fmt(off)}" if off else f"sketch {plane}")

    def rect(self, x1, y1, x2, y2):
        self.out.append(f"rect {fmt(x1)} {fmt(y1)} {fmt(x2)} {fmt(y2)}")

    def circle(self, x, y, d):
        self.out.append(f"circle {fmt(x)} {fmt(y)} {fmt(d)}")

    def line(self, x1, y1, x2, y2):
        self.out.append(f"line {fmt(x1)} {fmt(y1)} {fmt(x2)} {fmt(y2)}")

    def poly(self, pts, closed=True):
        n = len(pts)
        for i in range(n if closed else n - 1):
            a, b = pts[i], pts[(i + 1) % n]
            self.line(a[0], a[1], b[0], b[1])

    def arc(self, x1, y1, xm, ym, x2, y2):
        self.out.append(f"arc {fmt(x1)} {fmt(y1)} {fmt(xm)} {fmt(ym)} {fmt(x2)} {fmt(y2)}")

    def pull(self, dist, mode="add", region=0, times=1):
        for _ in range(times):
            self.out.append(f"pull-region {region} {fmt(dist)} {mode}")

    # ---- bodies --------------------------------------------------------------------------------------------------
    def box(self, x, y, z, w, d, h):
        """A new box body with its corner at (x, y, z)."""
        self.out.append(f"box {fmt(w)} {fmt(d)} {fmt(h)}")
        self.n += 1
        if x or y or z:
            self.out.append(f"move {fmt(x)} {fmt(y)} {fmt(z)} body {self.n}")
        return self.n

    def cyl(self, plane, off, x, y, d, dist, mode="add"):
        self.sketch(plane, off)
        self.circle(x, y, d)
        self.pull(dist, mode)

    def hole(self, plane, off, pts, d, depth, body=None):
        """Circles of diameter d at the sketch points, cut `depth` along the normal from the plane."""
        self.sketch(plane, off)
        for x, y in pts:
            self.circle(x, y, d)
        self.pull(depth, "cut", times=len(pts))

    def move(self, x, y, z, body):
        self.out.append(f"move {fmt(x)} {fmt(y)} {fmt(z)} body {body}")

    def rotate(self, axis, deg, body):
        self.out.append(f"rotate {axis} {fmt(deg)} body {body}")

    def round(self, r, x, y, z, body=None):
        self.out.append(f"round {fmt(r)} near {fmt(x)} {fmt(y)} {fmt(z)}" + (f" body {body}" if body else ""))

    def chamfer(self, c, x, y, z, body=None, angle=None):
        a = f" angle {fmt(angle)}" if angle else ""
        self.out.append(f"chamfer {fmt(c)} near {fmt(x)} {fmt(y)} {fmt(z)}{a}" + (f" body {body}" if body else ""))

    def text(self):
        return "\n".join(self.out) + "\n"

    def write(self, path):
        with open(path, "w") as f:
            f.write(self.text())


def fmt(v):
    s = f"{v:.4f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


# ---------------------------------------------------------------------------------------------------------------------------------
# Parts in any direction. A `Frame` maps the local coordinates of a part (u, v, w; w is its axis) to the world: the axis of the part is the
# world axis `axis` (x, y or z), local (u, v) are the other two world axes in the order (x, y), (y, z), (x, z), and w runs along the axis
# (against it with sign = -1). `origin` is the world position of the local origin. Everything is built where the frame says; the caller
# builds in an empty place (the "staging area") and moves the finished bodies, because bodies that touch are joined by Pull.
_AXES = {"z": ("x", "y", "z"), "x": ("y", "z", "x"), "y": ("x", "z", "y")}
_PLANE = {"z": "xy", "x": "yz", "y": "xz"}          # sketch plane perpendicular to a world axis
_PLANE_COORDS = {"z": (0, 1), "x": (1, 2), "y": (0, 2)}   # which world coordinates are the sketch x and y in that plane
_IDX = {"x": 0, "y": 1, "z": 2}


class Frame:
    def __init__(self, s, axis="z", sign=1, origin=(0.0, 0.0, 0.0), uv=None):
        """`uv` chooses where the local u and v point: ((name, sign), (name, sign)) with world axis names, e.g. (("z", 1), ("y", -1)) for u -> +Z
        and v -> -Y (then `axis` must be the third axis, x). Without it u and v follow the cyclic order x, y, z."""
        self.s, self.axis, self.sign, self.origin = s, axis, sign, origin
        if uv is None:
            self.names = _AXES[axis]            # world axis names of local u, v, w
            self.signs = (1, 1, sign)
        else:
            (nu, su), (nv, sv) = uv
            assert {nu, nv, axis} == {"x", "y", "z"}, "u, v and the axis must be the three axes"
            self.names = (nu, nv, axis)
            self.signs = (su, sv, sign)

    def world(self, u, v, w):
        """The world point of the local point (u, v, w)."""
        p = [0.0, 0.0, 0.0]
        for i, val in enumerate((u, v, w)):
            p[_IDX[self.names[i]]] = val * self.signs[i]
        return tuple(p[i] + self.origin[i] for i in range(3))

    def _lw(self, local):
        """(u, v, w) local axis letter -> (world axis letter, factor)."""
        i = "uvw".index(local)
        return self.names[i], self.signs[i]

    def ext(self, local, lo, hi, shape, mode="add", joins=True, times=1):
        """Extrudes `shape` along the local axis from lo to hi. shape: ('rect', a0, b0, a1, b1), ('circle', a, b, d) or ('poly', [(a, b), ...])
        in the other two local coordinates (in the order of u, v, w without the extruded one). mode: add or cut.
        joins=False says that the new volume is a body of its own (counted); circles in a list are several regions pulled one by one."""
        wa, f = self._lw(local)
        others = [c for c in "uvw" if c != local]
        def to_world(a, b, c):
            vals = {others[0]: a, others[1]: b, local: c}
            return self.world(vals["u"], vals["v"], vals["w"])
        c0, c1 = sorted((to_world(0, 0, lo)[_IDX[wa]], to_world(0, 0, hi)[_IDX[wa]]))
        px, py = _PLANE_COORDS[wa]
        s = self.s
        if wa == "y":
            s.sketch("xz", -c1)
        else:
            s.sketch(_PLANE[wa], c0)
        shapes = shape if isinstance(shape, list) else [shape]
        for sh in shapes:
            if sh[0] == "rect":
                _, a0, b0, a1, b1 = sh
                p = [to_world(a0, b0, lo), to_world(a1, b1, lo)]
                xs = sorted((p[0][px], p[1][px])); ys = sorted((p[0][py], p[1][py]))
                s.rect(xs[0], ys[0], xs[1], ys[1])
            elif sh[0] == "circle":
                _, a, b, d = sh
                p = to_world(a, b, lo)
                s.circle(p[px], p[py], d)
            elif sh[0] == "poly":
                pts = [to_world(a, b, lo) for a, b in sh[1]]
                s.poly([(p[px], p[py]) for p in pts])
            elif sh[0] == "slot":      # an oval: centre (a, b), long along local axis 'a' or 'b', length between the centres, width
                _, a, b, along, length, width = sh
                r = width / 2
                h = length / 2
                if along == "a":
                    ends = [(a - h, b), (a + h, b)]
                    pts = [(a - h, b + r), (a + h, b + r), (a + h + r, b), (a + h, b - r), (a - h, b - r), (a - h - r, b)]
                else:
                    ends = [(a, b - h), (a, b + h)]
                    pts = [(a + r, b - h), (a + r, b + h), (a, b + h + r), (a - r, b + h), (a - r, b - h), (a, b - h - r)]
                P = [to_world(a_, b_, lo) for a_, b_ in pts]
                P = [(p[px], p[py]) for p in P]
                # straight sides and two end arcs (pts[2] and pts[5] are the tips of the arcs)
                s.line(*P[0], *P[1]); s.arc(*P[1], *P[2], *P[3]); s.line(*P[3], *P[4]); s.arc(*P[4], *P[5], *P[0])
            else:
                raise ValueError(sh)
        n = len(shapes)
        s.pull(c1 - c0, mode, times=n)
        if mode == "add" and not joins:
            s.n += n

    def box(self, u0, v0, w0, u1, v1, w1):
        """A new box body (a body of its own, made by the `box` command and moved)."""
        a = self.world(u0, v0, w0); b = self.world(u1, v1, w1)
        lo = [min(a[i], b[i]) for i in range(3)]; hi = [max(a[i], b[i]) for i in range(3)]
        return self.s.box(lo[0], lo[1], lo[2], hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2])

    def round(self, r, u, v, w, body=None):
        p = self.world(u, v, w)
        self.s.round(r, *p, body=body)

    def chamfer(self, c, u, v, w, body=None):
        p = self.world(u, v, w)
        self.s.chamfer(c, *p, body=body)


def hexagon(a, b, across_flats):
    """Corner points of a regular hexagon with the given width across the flats (for a hex socket)."""
    import math
    r = across_flats / math.sqrt(3)
    return [(a + r * math.cos(math.radians(60 * k)), b + r * math.sin(math.radians(60 * k))) for k in range(6)]
