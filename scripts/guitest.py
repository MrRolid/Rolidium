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
    # the view is quiet when the places of the points of two readings are the same
    p = probe()
    for _ in range(40):
        time.sleep(0.5)
        q = probe()
        if q["points"] == p["points"]:
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
