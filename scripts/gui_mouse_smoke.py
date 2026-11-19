#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Drives the real Rolidium window with synthetic X11 mouse and keyboard events (XTest).

Run inside Xvfb, with the app already started, e.g. via scripts/gui-mouse-smoke.sh.
Takes screenshots with ImageMagick `import` into the directory given as argv[1].
"""
import ctypes, subprocess, sys, time

out = sys.argv[1]
x11 = ctypes.CDLL("libX11.so.6")
xtst = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XStringToKeysym.restype = ctypes.c_ulong
dpy = x11.XOpenDisplay(None)
assert dpy, "cannot open display"


def sync():
    x11.XFlush(ctypes.c_void_p(dpy))
    time.sleep(0.15)


def move(x, y):
    xtst.XTestFakeMotionEvent(ctypes.c_void_p(dpy), 0, int(x), int(y), 0)
    sync()


def button(n, down):
    xtst.XTestFakeButtonEvent(ctypes.c_void_p(dpy), n, 1 if down else 0, 0)
    sync()


def key(name, ctrl=False):
    ks = x11.XStringToKeysym(name.encode())
    kc = x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(ks))
    ctl = x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(x11.XStringToKeysym(b"Control_L")))
    if ctrl:
        xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), ctl, 1, 0)
    xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), kc, 1, 0)
    xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), kc, 0, 0)
    if ctrl:
        xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), ctl, 0, 0)
    sync()


def shot(name):
    time.sleep(0.6)
    subprocess.run(["import", "-window", "root", f"{out}/{name}.png"], check=True)
    print("shot", name)


def drag(x0, y0, x1, y1, steps=12, btn=1):
    move(x0, y0)
    button(btn, True)
    for i in range(1, steps + 1):
        move(x0 + (x1 - x0) * i / steps, y0 + (y1 - y0) * i / steps)
    return lambda: button(btn, False)


def focus_app_window():
    """No window manager runs in Xvfb, so give the app window keyboard focus ourselves."""
    root = x11.XDefaultRootWindow(ctypes.c_void_p(dpy))
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    root = x11.XDefaultRootWindow(ctypes.c_void_p(dpy))
    r, p = ctypes.c_ulong(), ctypes.c_ulong()
    kids = ctypes.POINTER(ctypes.c_ulong)()
    n = ctypes.c_uint()
    x11.XQueryTree(ctypes.c_void_p(dpy), ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
    if n.value:
        x11.XSetInputFocus(ctypes.c_void_p(dpy), ctypes.c_ulong(kids[n.value - 1]), 2, 0)
        sync()


time.sleep(1.0)
focus_app_window()
move(540, 300)
time.sleep(0.5)
shot("m1-hover")                      # face under the cursor is highlighted
button(1, True); button(1, False)     # Select tool: click selects
shot("m2-selected")
key("p")                              # Pull tool
release = drag(540, 300, 540, 190)    # drag up along the normal
shot("m3-dragging")                   # preview visible
release()                             # release commits
time.sleep(1.0)
shot("m4-pulled")
key("z", ctrl=True)                   # undo
time.sleep(1.0)
shot("m5-undone")
# orbit with the right button, zoom with the wheel
d = drag(600, 400, 700, 380, btn=3)
d()
for _ in range(3):
    xtst.XTestFakeButtonEvent(ctypes.c_void_p(dpy), 4, 1, 0)
    xtst.XTestFakeButtonEvent(ctypes.c_void_p(dpy), 4, 0, 0)
    sync()
shot("m6-orbit-zoom")
key("F12")                            # debug overlay
shot("m7-overlay")

# ---- sketch mode with real mouse and keyboard ----
key("Escape")
time.sleep(0.5)
click = lambda: (button(1, True), button(1, False))
key("i")                              # iso view is not bound; harmless
move(540, 300); time.sleep(0.4)
click()                               # Select tool: select the top face
key("k")                              # sketch mode on the selected face
key("v")                              # face the sketch
key("c")                              # circle tool
move(577, 387); time.sleep(0.4)
shot("s1-sketch-snap")
click()                               # circle centre
move(640, 387); time.sleep(0.4)
shot("s2-circle-rubber")
for ch in ["1", "0"]:
    key(ch)                           # type diameter 10
shot("s3-circle-typed")
key("Return")                         # confirm
key("r")                              # rectangle tool
move(700, 200); time.sleep(0.3); click()
move(800, 300); time.sleep(0.3); click()
shot("s4-shapes")
key("p")                              # leave sketch, Pull tool
move(577, 387); time.sleep(0.5)
shot("s5-region-hover")
release = drag(577, 387, 577, 300)
shot("s6-region-pull-preview")
release()
time.sleep(1.5)
shot("s7-region-pulled")

# ---- phase 1b: help card, edge hover/select/round, size panel, export dialog ----
key("Escape"); key("s"); time.sleep(0.5)
move(70, 70); time.sleep(1.6)
shot("t1-help-card")                  # help card of the Pull tool
move(600, 600); time.sleep(0.3)
for _ in range(14):
    key("z", ctrl=True)               # undo everything
time.sleep(0.5)
key("y", ctrl=True)                   # redo only the creation of the box
time.sleep(1.0)
move(70, 420); click(); time.sleep(0.4)   # Isometric
move(70, 451); click(); time.sleep(0.6)   # Fit all
move(535, 392); time.sleep(0.8)           # on the top-front edge
shot("t2-edge-hover")
click(); time.sleep(0.5)
shot("t3-edge-selected-panel")        # size panel next to the edge
key("p"); time.sleep(0.3)
d = drag(535, 392, 585, 440); shot("t4-edge-drag-preview"); d()
time.sleep(1.5)
shot("t5-rounded")
move(70, 509); time.sleep(0.4); click(); time.sleep(0.8)
shot("t6-export-dialog")
