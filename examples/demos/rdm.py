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
