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
        h = None
        for _ in range(40):   # the read-back of what is under the pointer takes a few frames, more at a big window and a slow drawing
            time.sleep(0.25)
            h = probe()["hover"]
            if h and h["normal"] and all(abs(a - b) < 0.02 for a, b in zip(h["normal"], want)):
                break
        check(h and h["normal"] and all(abs(a - b) < 0.02 for a, b in zip(h["normal"], want)), f"the pointer on '{name}' hovers the face with the normal {want}: {h}")
    x, y = screen(p, "edge")
    move(x, y)
    time.sleep(0.5)
    q = probe()
    check(q["hover_obj"] is not None and "Edge" in q["hover_obj"], f"the pointer on the edge hovers the edge: {q['hover_obj']}")
    shot("01-hover")
    # a sketch on the top face: a circle by two clicks, centre at (30, 20), radius 6 (the grid step is 1 mm)
    x, y = screen(p, "top")
    move(x, y)
    for _ in range(40):   # a click takes the face the read-back of the pick reported: wait until it is the one aimed at
        time.sleep(0.25)
        h = probe()["hover"]
        if h and h["normal"] and abs(h["normal"][2] - 1) < 0.02:
            break
    click()
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
    # the corner of the face: a click 3 pixels from it snaps to the vertex
    cx, cy = screen(p, "corner")
    click(cx + 3, cy - 2)
    click(*screen(p, "c3"))
    p = settle(0.5)
    cs = p["circles"]
    check(len(cs) == 2 and close(cs[1]["x"], 0, 0.01) and close(cs[1]["y"], 0, 0.01), f"the centre snapped to the corner of the face (0, 0): {cs}")
    shot("02-circles")
    finish()
