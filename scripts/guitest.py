#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Helpers for the scripted window tests (scripts/gui-tests.sh): they drive the real Rolidium window with synthetic X11 mouse and keyboard events
(XTest, inside Xvfb) and read what the program says about itself from the probe file (ROLIDIUM_PROBE, see crates/rolidium-app/src/app/probe.rs):
where points of the model are on the screen, the status line, the bodies with their volumes and holes, the links, the live distances.
Every test saves screenshots into the directory given as GUI_SHOTS (target/gui-shots) and exits with 1 if a check failed."""
import ctypes, json, math, os, subprocess, sys, time

SHOTS = os.environ.get("GUI_SHOTS", "target/gui-shots")
PROBE = os.environ["ROLIDIUM_PROBE"]
os.makedirs(SHOTS, exist_ok=True)
x11 = ctypes.CDLL("libX11.so.6")
xtst = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XStringToKeysym.restype = ctypes.c_ulong
dpy = x11.XOpenDisplay(None)
assert dpy, "cannot open display"
D = ctypes.c_void_p(dpy)
_failures = []
_name = os.path.splitext(os.path.basename(sys.argv[0]))[0]


def sync(t=0.12):
    x11.XFlush(D)
    time.sleep(t)


def move(x, y):
    xtst.XTestFakeMotionEvent(D, 0, int(x), int(y), 0)
    sync()


def button(n, down):
    xtst.XTestFakeButtonEvent(D, n, 1 if down else 0, 0)
    sync()


def click(x=None, y=None):
    if x is not None:
        move(x, y)
        time.sleep(0.25)  # the program picks what is under the pointer one frame later
    button(1, True)
    button(1, False)
    time.sleep(0.25)


def right_click(x=None, y=None, shift=False):
    """A click of the right button (with Shift held when asked)."""
    if x is not None:
        move(x, y)
        time.sleep(0.25)
    sh = keycode("Shift_L")
    if shift:
        xtst.XTestFakeKeyEvent(D, sh, 1, 0)
        sync(0.1)
    button(3, True)
    button(3, False)
    if shift:
        xtst.XTestFakeKeyEvent(D, sh, 0, 0)
    sync(0.1)
    time.sleep(0.25)


def empty_point(p):
    """A place in the model area where there is nothing (near its top left corner)."""
    v = p["viewport"]
    return v[0] + 70, v[1] + 70


def ctrl_click(x=None, y=None):
    """A click with Ctrl held."""
    if x is not None:
        move(x, y)
        time.sleep(0.25)
    c = keycode("Control_L")
    xtst.XTestFakeKeyEvent(D, c, 1, 0)
    sync(0.1)
    button(1, True)
    button(1, False)
    xtst.XTestFakeKeyEvent(D, c, 0, 0)
    sync(0.1)
    time.sleep(0.25)


_presses = 0
_waits = []
_presses_base = None


def set_press_base(v):
    global _presses_base
    _presses_base = v


def fast_click(x, y):
    """The pointer goes to the place and the button is pressed and released at once, with no waiting for anything the program says; then the next two frames are
    waited for (the probe is written at every frame in ROLIDIUM_PROBE_IDLE mode, otherwise a fixed short time passes)."""
    global _presses
    _presses += 1   # the number of presses the program has to have seen when this click is handled
    f0 = None
    try:
        q = json.load(open(PROBE))
        f0 = q["frame"]
        if _presses_base is None:
            set_press_base(q.get("click_total", 0) - (_presses - 1))
    except Exception:
        pass
    xtst.XTestFakeMotionEvent(D, 0, int(x), int(y), 0)
    x11.XFlush(D)
    time.sleep(0.01)   # the motion reaches the window before the button does (10 ms: not a wait for any answer of the program)
    xtst.XTestFakeButtonEvent(D, 1, 1, 0)
    xtst.XTestFakeButtonEvent(D, 1, 0, 0)
    x11.XFlush(D)
    if f0 is None:
        time.sleep(0.15)
        return
    t0 = time.time()
    while time.time() - t0 < 30.0:
        time.sleep(0.004)
        try:
            q = json.load(open(PROBE))
            log = q.get("click_log") or [""]
            want = "press at Some((%s, %s))" % (round(x / q["ppp"]) + 0.0, round(y / q["ppp"]) + 0.0)
            if q["frame"] >= f0 + 3 and log[0].startswith(want):   # the newest press of the program is at the place of this click, and a frame came after it
                time.sleep(0.03)
                _waits.append(time.time() - t0)
                return
        except Exception:
            pass


def keycode(name):
    return x11.XKeysymToKeycode(D, ctypes.c_ulong(x11.XStringToKeysym(name.encode())))


def key(name, ctrl=False, shift=False):
    mods = [keycode(m) for m, on in (("Control_L", ctrl), ("Shift_L", shift)) if on]
    kc = keycode(name)
    for m in mods:
        xtst.XTestFakeKeyEvent(D, m, 1, 0)
    xtst.XTestFakeKeyEvent(D, kc, 1, 0)
    xtst.XTestFakeKeyEvent(D, kc, 0, 0)
    for m in mods:
        xtst.XTestFakeKeyEvent(D, m, 0, 0)
    sync(0.2)


def type_text(text):
    for ch in text:
        key({".": "period", ",": "comma", "-": "minus", " ": "space"}.get(ch, ch))


def drag(x0, y0, x1, y1, steps=14):
    move(x0, y0)
    time.sleep(0.2)
    button(1, True)
    for i in range(1, steps + 1):
        move(x0 + (x1 - x0) * i / steps, y0 + (y1 - y0) * i / steps)
    return lambda: button(1, False)


def focus_app_window():
    """No window manager runs in Xvfb, so give the program window the keyboard focus."""
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    root = x11.XDefaultRootWindow(D)
    r, p = ctypes.c_ulong(), ctypes.c_ulong()
    kids = ctypes.POINTER(ctypes.c_ulong)()
    n = ctypes.c_uint()
    x11.XQueryTree(D, ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
    if n.value:
        x11.XSetInputFocus(D, ctypes.c_ulong(kids[n.value - 1]), 2, 0)
        sync()


def probe():
    for _ in range(60):
        try:
            with open(PROBE) as f:
                return json.load(f)
        except Exception:
            time.sleep(0.1)
    raise RuntimeError("the program does not write the probe file")


def settle(seconds=0.5):
    """Waits until the program has drawn a few more frames and is not computing."""
    time.sleep(seconds)
    f0 = probe()["frame"]
    for _ in range(100):
        p = probe()
        if p["frame"] > f0 + 1 and not p["busy"]:
            return p
        time.sleep(0.1)
    return probe()


def wait_for(pred, what, timeout=15.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        p = probe()
        v = pred(p)
        if v:
            return p
        time.sleep(0.15)
    check(False, f"waited for: {what}")
    return probe()


def screen(p, name):
    """The position of a named point of the model, or of a widget, as given by the probe."""
    v = p["points"].get(name) or p["widgets"].get(name)
    if not v:
        raise RuntimeError(f"the probe does not know '{name}' (points: {list(p['points'])}, widgets: {list(p['widgets'])})")
    return v


def shot(name):
    time.sleep(0.5)
    path = f"{SHOTS}/{_name}-{name}.png"
    subprocess.run(["import", "-window", "root", path], check=True)
    print("shot", path)


def check(cond, msg):
    print(("PASS " if cond else "FAIL ") + msg)
    if not cond:
        _failures.append(msg)


def close(a, b, tol=0.05):
    return abs(a - b) <= tol


def holes_of(p, body=0):
    """The holes of a body as (x, y, z, r), through holes only (a round hole in a face)."""
    return [(h["x"], h["y"], h["z"], h["r"]) for h in p["bodies"][body]["holes"] if h["hole"]]


def has_hole_at(p, x, y, body=0, tol=0.05):
    return any(close(h[0], x, tol) and close(h[1], y, tol) for h in holes_of(p, body))


def first_by(p, f):
    return next((x for x in p["bodies"] if f(x)), None)


def hover_cylinder(p, candidates):
    """Moves the pointer over each candidate point until the program says the face under it is round; returns the point."""
    for name in candidates:
        x, y = screen(probe(), name)
        move(x, y)
        time.sleep(0.35)
        q = probe()
        h = q.get("hover")
        if h and h["kind"] == "Cylinder" and q.get("hover_obj") is None:
            return x, y
    raise RuntimeError("no round face under the candidate points")


def hover_normal(candidates, want):
    """Moves the pointer over each candidate point until the face under it has the normal `want`; returns the screen place."""
    for name in candidates:
        x, y = screen(probe(), name)
        move(x, y)
        time.sleep(0.35)
        h = probe().get("hover")
        if h and h.get("normal") and all(abs(a - b) < 0.02 for a, b in zip(h["normal"], want)):
            return x, y
    raise RuntimeError(f"no face with the normal {want} under the candidate points")


def box_triangles(w, d, h, ox=0.0, oy=0.0, oz=0.0, n=1):
    """The triangles of a box (counter-clockwise seen from outside), every wall divided into n x n squares of two triangles, as triples of (x, y, z)."""
    faces = [((0, 0, 0), (0, d, 0), (w, 0, 0)), ((0, 0, h), (w, 0, 0), (0, d, 0)), ((0, 0, 0), (w, 0, 0), (0, 0, h)),
             ((0, d, 0), (0, 0, h), (w, 0, 0)), ((0, 0, 0), (0, 0, h), (0, d, 0)), ((w, 0, 0), (0, d, 0), (0, 0, h))]
    out = []
    for o, e1, e2 in faces:
        def pt(i, j):
            return tuple(o[k] + e1[k] * i / n + e2[k] * j / n + (ox, oy, oz)[k] for k in range(3))
        for i in range(n):
            for j in range(n):
                out.append((pt(i, j), pt(i + 1, j), pt(i + 1, j + 1)))
                out.append((pt(i, j), pt(i + 1, j + 1), pt(i, j + 1)))
    return out


def write_stl(path, tris, ascii_=False):
    """Writes triangles into a binary (or ASCII) STL file."""
    import struct
    if ascii_:
        with open(path, "w") as f:
            f.write("solid t\n")
            for t in tris:
                f.write("facet normal 0 0 0\n outer loop\n")
                for p in t:
                    f.write("  vertex %r %r %r\n" % tuple(p))
                f.write(" endloop\nendfacet\n")
            f.write("endsolid t\n")
        return
    with open(path, "wb") as f:
        f.write(b"test".ljust(80, b" "))
        f.write(struct.pack("<I", len(tris)))
        for t in tris:
            f.write(struct.pack("<12fH", 0, 0, 0, *[c for p in t for c in p], 0))


def cylinder_triangles(r, h, n, cx=0.0, cy=0.0, oz=0.0):
    """A closed cylinder standing on z = oz (its side made of n flat strips, the ends of fans), outward normals."""
    import math
    b = [(cx + r * math.cos(2 * math.pi * k / n), cy + r * math.sin(2 * math.pi * k / n), oz) for k in range(n)]
    t = [(x, y, oz + h) for x, y, _ in b]
    bc, tc = (cx, cy, oz), (cx, cy, oz + h)
    out = []
    for k in range(n):
        j = (k + 1) % n
        out += [(b[k], b[j], t[j]), (b[k], t[j], t[k]), (bc, b[j], b[k]), (tc, t[k], t[j])]
    return out


def frustum_triangles(bottom, top, h, ox=0.0, oy=0.0, oz=0.0, n=2):
    """A block with a square foot of `bottom` and a square top of `top` (its sides slant), from a box whose points are drawn in towards the middle the higher they are."""
    k = top / bottom

    def mv(p):
        x, y, z = p
        f = 1.0 - (1.0 - k) * z / h
        c = bottom / 2.0
        return (c + (x - c) * f + ox, c + (y - c) * f + oy, z + oz)
    return [tuple(mv(p) for p in tri) for tri in box_triangles(bottom, bottom, h, n=n)]


def quad_triangles(p0, p1, p2, p3, outward, n=2):
    """The quad p0 p1 p2 p3 divided n x n (bilinear), its triangles turned so that they look the way `outward` says."""
    def at(i, j):
        u, v = i / n, j / n
        return tuple((1 - u) * (1 - v) * p0[k] + u * (1 - v) * p1[k] + u * v * p2[k] + (1 - u) * v * p3[k] for k in range(3))
    out = []
    for i in range(n):
        for j in range(n):
            a, b, c, d = at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)
            for t in ((a, b, c), (a, c, d)):
                u = [t[1][k] - t[0][k] for k in range(3)]
                w = [t[2][k] - t[0][k] for k in range(3)]
                nrm = (u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0])
                out.append(t if sum(nrm[k] * outward[k] for k in range(3)) > 0 else (t[0], t[2], t[1]))
    return out


def tray_triangles(ox=0.0, oy=0.0, floor=1.0, w=40.0, d=30.0, h=20.0, wall=5.0):
    """An open tray (a box with a cavity open at the top), the floor `floor` thick, the walls `wall` thick; closed, every wall divided 2 x 2, normals out of the material."""
    def P(x, y, z):
        return (x + ox, y + oy, z)
    iw, id_ = w - wall, d - wall
    t = []
    t += quad_triangles(P(0, 0, 0), P(w, 0, 0), P(w, d, 0), P(0, d, 0), (0, 0, -1))                     # the outer bottom
    t += quad_triangles(P(0, 0, 0), P(w, 0, 0), P(w, 0, h), P(0, 0, h), (0, -1, 0))                     # outer walls
    t += quad_triangles(P(0, d, 0), P(w, d, 0), P(w, d, h), P(0, d, h), (0, 1, 0))
    t += quad_triangles(P(0, 0, 0), P(0, d, 0), P(0, d, h), P(0, 0, h), (-1, 0, 0))
    t += quad_triangles(P(w, 0, 0), P(w, d, 0), P(w, d, h), P(w, 0, h), (1, 0, 0))
    t += quad_triangles(P(0, 0, h), P(w, 0, h), P(iw, wall, h), P(wall, wall, h), (0, 0, 1))              # the rim
    t += quad_triangles(P(w, 0, h), P(w, d, h), P(iw, id_, h), P(iw, wall, h), (0, 0, 1))
    t += quad_triangles(P(w, d, h), P(0, d, h), P(wall, id_, h), P(iw, id_, h), (0, 0, 1))
    t += quad_triangles(P(0, d, h), P(0, 0, h), P(wall, wall, h), P(wall, id_, h), (0, 0, 1))
    t += quad_triangles(P(wall, wall, floor), P(iw, wall, floor), P(iw, id_, floor), P(wall, id_, floor), (0, 0, 1))  # the floor
    t += quad_triangles(P(wall, wall, floor), P(iw, wall, floor), P(iw, wall, h), P(wall, wall, h), (0, 1, 0))      # the walls of the cavity (they look into it)
    t += quad_triangles(P(wall, id_, floor), P(iw, id_, floor), P(iw, id_, h), P(wall, id_, h), (0, -1, 0))
    t += quad_triangles(P(wall, wall, floor), P(wall, id_, floor), P(wall, id_, h), P(wall, wall, h), (1, 0, 0))
    t += quad_triangles(P(iw, wall, floor), P(iw, id_, floor), P(iw, id_, h), P(iw, wall, h), (-1, 0, 0))
    return t


def import_dropped(*paths, units=None):
    """Drops the files into the window, lets the dialog of the import take them as they are and waits for the meshes."""
    n0 = len(probe()["meshes"])
    drop_files(*paths)
    wait_for(lambda q: q["stl_dialog"], "the dialog of the import is open", 60)
    click(*steady("stl_as_mesh"))
    click(*steady("stl_import_ok"))
    return wait_for(lambda q: len(q["meshes"]) >= n0 + len(paths) and q["stl_dialog"] is None, "the meshes were imported", 60)


def drop_files(*paths):
    """The files are put into the window the way the window system does it (the program reads the names from the file named in ROLIDIUM_DROP; a real
    drag and drop of the window system cannot be made by a script, the program handles both with the same code)."""
    target = os.environ["ROLIDIUM_DROP"]
    tmp = target + ".new"
    with open(tmp, "w") as f:
        f.write("\n".join(paths) + "\n")
    os.rename(tmp, target)


def steady(name, timeout=20.0):
    """The screen place of a named point once the view has stopped moving (a turn of the view to a sketch takes a while on a slow machine)."""
    last, same, t0 = None, 0, time.time()
    while time.time() - t0 < timeout:
        q = probe()
        v = q["points"].get(name) or q["widgets"].get(name)
        if v and last and abs(v[0] - last[0]) < 0.6 and abs(v[1] - last[1]) < 0.6:
            same += 1
            if same >= 3:
                return v
        else:
            same = 0
        last = v
        time.sleep(0.4)
    return last


def rss_mb():
    """The resident memory of the program (MB), from /proc."""
    out = subprocess.run(["pgrep", "-xf", os.environ["ROLIDIUM_APP_BIN"] + " " + os.environ["ROLIDIUM_MODEL"]], capture_output=True, text=True).stdout.split()
    if not out:
        return None
    with open(f"/proc/{out[0]}/status") as f:
        for line in f:
            if line.startswith("VmRSS:"):
                return round(int(line.split()[1]) / 1024)
    return None


def orbit(seconds, x=640, y=400, span=140):
    """Turns the model with the middle button for `seconds`, the pointer going to and fro without a pause; returns (frames the program drew, seconds)."""
    move(x, y)
    time.sleep(0.3)
    f0, t0 = probe()["frame"], time.time()
    xtst.XTestFakeButtonEvent(D, 2, 1, 0)
    k = 0
    while time.time() - t0 < seconds:
        k += 1
        xtst.XTestFakeMotionEvent(D, 0, int(x + span * math.sin(k / 6.0)), int(y + span * 0.3 * math.cos(k / 9.0)), 0)
        x11.XFlush(D)
        time.sleep(0.015)
    xtst.XTestFakeButtonEvent(D, 2, 0, 0)
    x11.XFlush(D)
    q = probe()
    return q["frame"] - f0, time.time() - t0


def middle_drag(x0, y0, x1, y1, steps=24):
    """Turns the view: the middle button goes down at (x0, y0), the pointer moves to (x1, y1) and the button is released."""
    move(x0, y0)
    time.sleep(0.2)
    xtst.XTestFakeButtonEvent(D, 2, 1, 0)
    x11.XFlush(D)
    for i in range(1, steps + 1):
        xtst.XTestFakeMotionEvent(D, 0, int(x0 + (x1 - x0) * i / steps), int(y0 + (y1 - y0) * i / steps), 0)
        x11.XFlush(D)
        time.sleep(0.02)
    xtst.XTestFakeButtonEvent(D, 2, 0, 0)
    x11.XFlush(D)
    time.sleep(0.4)


def big_mesh_perf(stl, label, edit=False, cancel=False):
    """A mesh of millions of triangles in the window: how long the reading, the import, the display and the search for walls and holes take, whether the window answers
    meanwhile, the frames a second while the model is turned, and the time of a pick under the pointer; the numbers go to GUI_SHOTS/perf-<label>.json."""
    res = {"label": label, "file_mb": round(os.path.getsize(stl) / 1e6, 1)}
    time.sleep(1.0)
    focus_app_window()
    p = settle()
    res["rss_start_mb"] = rss_mb()
    t0 = time.time()
    drop_files(stl)
    p = wait_for(lambda q: q["stl_dialog"], "the dialog of the import is open", 180)
    res["read_and_check_s"] = round(time.time() - t0, 2)
    res["triangles"] = p["stl_dialog"]["items"][0]["triangles"]
    res["check"] = p["stl_dialog"]["items"][0]["check"]
    shot("01-dialog")
    click(*steady("stl_import_ok"))
    t1 = time.time()
    p = wait_for(lambda q: len(q["meshes"]) == 1 and q["meshes"][0]["shown"], "the mesh is shown", 180)
    res["import_to_shown_s"] = round(time.time() - t1, 2)
    res["rss_shown_mb"] = rss_mb()
    # the search for walls and holes runs in the background: the window keeps drawing and the progress and the Cancel button are there
    samples, frames, t2 = [], [], time.time()
    cancel_clicked = None
    while time.time() - t2 < 240:
        q = probe()
        job = q["mesh_job"]
        if job is not None:
            samples.append(job)
            frames.append((time.time(), q["frame"]))
        if cancel and job is not None and job > 0.1 and "mesh_job_cancel" in q["widgets"] and cancel_clicked is None:
            shot("02-progress")
            click(*screen(q, "mesh_job_cancel"))
            cancel_clicked = time.time()
        if q["meshes"] and (q["meshes"][0]["walls"] is not None) and job is None:
            break
        if cancel_clicked and job is None:
            break
        time.sleep(0.1)
    res["regions_s"] = round(time.time() - t2, 2)
    res["progress_samples"] = len(samples)
    res["progress_max"] = max(samples) if samples else None
    if len(frames) > 2:
        res["frames_during_search_per_s"] = round((frames[-1][1] - frames[0][1]) / max(frames[-1][0] - frames[0][0], 1e-6), 1)
    p = settle(0.5)
    res["walls"] = p["meshes"][0]["walls"]
    res["holes"] = len(p["meshes"][0]["holes"] or [])
    res["cancelled"] = bool(cancel_clicked)
    res["status_after_search"] = p["status"]
    res["rss_regions_mb"] = rss_mb()
    shot("03-shown")
    # the frames a second while the model is turned
    n, secs = orbit(6.0)
    res["orbit_frames_per_s"] = round(n / secs, 1)
    settle(0.5)
    shot("04-turned")
    # picks under the pointer: the time of each, and when the hierarchy of boxes is ready
    vx, vy, vw, vh = p["viewport"]
    picks, t3, bvh_at = [], time.time(), None
    for k in range(24):
        fast_click(vx + vw * (0.35 + 0.3 * ((k * 7) % 10) / 10), vy + vh * (0.35 + 0.3 * ((k * 3) % 10) / 10))
        q = probe()
        picks.append((q["pick"]["ms"], q["pick"]["bvh"]))
        if q["pick"]["bvh"] and bvh_at is None:
            bvh_at = time.time() - t1
        time.sleep(0.4 if not q["pick"]["bvh"] else 0.05)
    res["pick_ms_without_bvh"] = [a for a, b in picks if not b][:4]
    res["pick_ms_with_bvh"] = sorted(a for a, b in picks if b)[len([1 for a, b in picks if b]) // 2] if any(b for a, b in picks) else None
    res["bvh_ready_after_import_s"] = round(bvh_at, 1) if bvh_at else None
    res["rss_end_mb"] = rss_mb()
    if edit and p["meshes"][0]["walls"]:
        # a pull of the top wall by 1 mm, typed
        key("p")
        settle(0.4)
        x, y = steady("top")
        click(x, y)
        p = wait_for(lambda q: len(q["selection"]) == 1, "the top wall is selected for the pull", 90)
        type_text("1")
        wait_for(lambda q: not q["busy"], "the program is idle", 120)
        stamp0 = probe()["meshes"][0]["stamp"]
        t4 = time.time()
        key("Return")
        q = wait_for(lambda q: q["status"].startswith("Pull +") or q["status_error"], "the pull is done", 120)
        check(not q["status_error"], f"the pull was not refused: {q['status']}")
        res["pull_done_s"] = round(time.time() - t4, 2)
        res["pull_status"] = q["status"]
        q = wait_for(lambda q: q["meshes"][0]["stamp"] != stamp0, "the new display is there", 120)
        res["pull_display_s"] = round(time.time() - t4, 2)
        q = wait_for(lambda q: q["meshes"][0]["walls"] is not None and q["mesh_job"] is None, "the walls are found again", 240)
        res["walls_again_s"] = round(time.time() - t4, 2)
        key("z", ctrl=True)
        t5 = time.time()
        q = wait_for(lambda q: q["status"] == "Undo", "undo", 60)
        res["undo_s"] = round(time.time() - t5, 2)
        q = wait_for(lambda q: q["meshes"][0]["stamp"] == stamp0, "the display before the pull is back", 60)
        res["undo_display_s"] = round(time.time() - t5, 2)
        shot("05-after-undo")
    with open(f"{SHOTS}/perf-{label}.json", "w") as f:
        json.dump(res, f, indent=1)
    for k, v in res.items():
        print("PERF", k, v)


def finish():
    if _failures:
        print(f"{len(_failures)} check(s) failed")
        sys.exit(1)
    print("all checks passed")


def _on_error(kind, value, tb):
    """A test that stops with an exception leaves a screenshot and the last probe for the person who reads the log."""
    import traceback
    traceback.print_exception(kind, value, tb)
    try:
        subprocess.run(["import", "-window", "root", f"{SHOTS}/{_name}-FAILED.png"], check=False)
        print("last probe:", json.dumps(probe())[:3000])
    except Exception:
        pass
    sys.exit(2)


sys.excepthook = _on_error


def scale_check(scale):
    """The interface scale (pixels per point): a point placed by a click lies under the pointer, the snaps to the grid and to the corner of the face work, and the
    face and the edge under the pointer are the ones that were aimed at. The model is a plate 100 x 60 x 10 and the program runs with WINIT_X11_SCALE_FACTOR."""
    time.sleep(1.0)
    focus_app_window()
    p = settle()
    check(close(p["ppp"], scale, 0.01), f"the scale of the interface is {scale}: {p['ppp']}")
    # the faces and the edge under the pointer (Select tool): the top face, the front face, an edge
    for name, want in (("top", (0, 0, 1)), ("front", (0, -1, 0))):
        x, y = screen(p, name)
        move(x, y)
        time.sleep(0.5)
        h = probe()["hover"]
        check(h and h["normal"] and all(abs(a - b) < 0.02 for a, b in zip(h["normal"], want)), f"the pointer on '{name}' hovers the face with the normal {want}: {h}")
    x, y = screen(p, "edge")
    move(x, y)
    time.sleep(0.5)
    q = probe()
    check(q["hover_obj"] is not None and "Edge" in q["hover_obj"], f"the pointer on the edge hovers the edge: {q['hover_obj']}")
    shot("01-hover")
    # a sketch on the top face: a circle by two clicks, centre at (30, 20), radius 6 (the grid step is 1 mm)
    fast_click(*screen(p, "top"))   # at once after the move: what is under the pointer must be found for this place, not taken from an older one
    settle(0.4)
    key("k")
    settle(0.4)
    key("c")
    p = settle(0.4)
    check(p["sketching"] == "Circle", f"the Circle tool is on: {p['sketching']}")
    click(*screen(p, "c1"))
    click(*screen(p, "c2"))
    p = settle(0.5)
    c = p["circles"][0] if p["circles"] else None
    check(c is not None and close(c["x"], 30, 0.6) and close(c["y"], 20, 0.6) and close(c["r"], 6, 0.6), f"the circle lies where it was clicked, (30, 20) r 6: {c}")
    # the corner of the face: a click 3 pixels from it snaps to the vertex (Esc first: the panel of the finished circle opens next to it and could cover the corner)
    key("Escape")
    p = settle(0.6)
    cx, cy = screen(p, "corner")
    click(cx + 3, cy - 2)
    click(*screen(p, "c3"))
    p = settle(0.5)
    cs = p["circles"]
    check(len(cs) == 2 and close(cs[1]["x"], 0, 0.01) and close(cs[1]["y"], 0, 0.01), f"the centre snapped to the corner of the face (0, 0): {cs}")
    shot("02-circles")
    finish()


def rapid_clicks(scale):
    """50 clicks at once after a move each, at different places of 20 little bodies and of the background, in a random order, with no waiting for the pick of the
    program: every click takes the body under the place it was made at (or nothing over the background), none is lost."""
    import random
    time.sleep(1.5)
    focus_app_window()
    # the view is quiet when no frame was drawn for 2 seconds (the probe is written at every frame in this mode) and the program has drawn some
    p = probe()
    for _ in range(120):
        time.sleep(2.0)
        q = probe()
        if q["frame"] == p["frame"] and q["frame"] > 5:
            break
        p = q
    check(close(p["ppp"], scale, 0.01), f"the scale of the interface is {scale}: {p['ppp']}")
    bodies = {b["id"]: b["bbox"] for b in p["bodies"]}
    check(len(bodies) == 20, f"20 bodies: {len(bodies)}")
    targets = [(f"p{i}", None) for i in range(20)] + [(f"v{i}", None) for i in range(4)]
    vx, vy, vw, vh = p["viewport"]
    # places of the background, inside the model area (the corners of the view, where no body is)
    for k, (fx, fy) in enumerate(((0.06, 0.08), (0.94, 0.08), (0.06, 0.92), (0.94, 0.92))):
        p["points"][f"v{k}"] = [vx + vw * fx, vy + vh * fy]
    rnd = random.Random(7)
    seq = []
    last = None
    while len(seq) < 50:
        t = rnd.choice(targets)[0]
        if t != last:
            seq.append(t)
            last = t
    wrong, lost = [], []
    for name in seq:
        x, y = screen(p, name)
        fast_click(x, y)
        q = probe()
        sel = q["selection"]
        # the face that was taken opens a panel next to it (Face: Move by, Distance...), which would cover some of the next places: Esc closes it, and the frames that
        # follow are waited for (egui looks for what is under the pointer in the layout of the frame before)
        f_before = probe()["frame"]
        key("Escape")
        for _ in range(400):   # two frames after the key: it must not be handled in the same frame as the next click (which it would then undo)
            time.sleep(0.01)
            if probe()["frame"] >= f_before + 2:
                break
        if name.startswith("v"):
            if sel:
                wrong.append((name, sel))
            continue
        cx, cy = P_XY[name]
        want = next(i for i, bb in bodies.items() if bb[0] - 1e-6 <= cx <= bb[3] + 1e-6 and bb[1] - 1e-6 <= cy <= bb[4] + 1e-6)
        if not sel:
            lost.append(name)
            print("lost", name, "sent", (x, y), "log", q.get("click_log", [])[:3])
        elif sel[0][0] != want:
            wrong.append((name, sel, want))
            print("click", name, "sent", (x, y), "log", q.get("click_log", [])[:2])
    if wrong or lost:
        print("viewport", p["viewport"])
        for w in wrong[:6]:
            print("wrong", w, "at", screen(p, w[0]), "wanted bbox", bodies.get(w[2]) if len(w) > 2 else None, "got bbox", bodies.get(w[1][0][0]))
        print("click log (newest first):", *probe().get("click_log", [])[:30], sep="\n  ")
    print("slowest wait for a click to be handled: %.2f s, mean %.2f s" % (max(_waits or [0]), sum(_waits) / max(1, len(_waits))))
    shot("01-after-50-clicks")
    check(not lost, f"no click was lost: {lost}")
    check(not wrong, f"no click took another place: {wrong}")
    finish()


P_XY = {f"p{j * 5 + i}": (5 + 14 * i, 5 + 14 * j) for j in range(4) for i in range(5)}
