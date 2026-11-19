#!/usr/bin/env python3
# Copyright (c) 2026 Rolid spol. s r.o.
# SPDX-License-Identifier: AGPL-3.0-only
"""Helpers that drive the real Rolidium window with synthetic X11 events (XTest) and take screenshots
with ImageMagick `import` into the directory given as argv[1]. Used by the gui_mouse_smoke*.py scripts."""
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


def key(name, ctrl=False, shift=False, alt=False):
    ks = x11.XStringToKeysym(name.encode())
    kc = x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(ks))
    ctl = x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(x11.XStringToKeysym(b"Control_L")))
    mods = []
    if ctrl:
        mods.append("Control_L")
    if shift:
        mods.append("Shift_L")
    if alt:
        mods.append("Alt_L")
    mk = [x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(x11.XStringToKeysym(m.encode()))) for m in mods]
    for m in mk:
        xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), m, 1, 0)
    xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), kc, 1, 0)
    xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), kc, 0, 0)
    for m in mk:
        xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), m, 0, 0)
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



def hold(name, down):
    ks = x11.XStringToKeysym(name.encode())
    kc = x11.XKeysymToKeycode(ctypes.c_void_p(dpy), ctypes.c_ulong(ks))
    xtst.XTestFakeKeyEvent(ctypes.c_void_p(dpy), kc, 1 if down else 0, 0)
    sync()


def click():
    button(1, True)
    button(1, False)


def double_click():
    button(1, True)
    button(1, False)
    button(1, True)
    button(1, False)


def type_text(text):
    for ch in text:
        name = {".": "period", ",": "comma", "-": "minus", " ": "space", "/": "slash", "_": "underscore", ":": "colon", "~": "asciitilde"}.get(ch, ch)
        key(name)


def close_window():
    """Asks the program window to close the way a window manager's X button does (WM_DELETE_WINDOW)."""
    import struct
    root = x11.XDefaultRootWindow(ctypes.c_void_p(dpy))
    r, p = ctypes.c_ulong(), ctypes.c_ulong()
    kids = ctypes.POINTER(ctypes.c_ulong)()
    n = ctypes.c_uint()
    x11.XQueryTree(ctypes.c_void_p(dpy), ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
    win = kids[n.value - 1]
    x11.XInternAtom.restype = ctypes.c_ulong
    wm = x11.XInternAtom(ctypes.c_void_p(dpy), b"WM_PROTOCOLS", 0)
    dele = x11.XInternAtom(ctypes.c_void_p(dpy), b"WM_DELETE_WINDOW", 0)
    ev = bytearray(192)
    struct.pack_into("i", ev, 0, 33)          # ClientMessage
    struct.pack_into("Q", ev, 24, dpy)
    struct.pack_into("Q", ev, 32, win)
    struct.pack_into("Q", ev, 40, wm)
    struct.pack_into("i", ev, 48, 32)
    struct.pack_into("q", ev, 56, dele)
    buf = (ctypes.c_char * 192).from_buffer(ev)
    x11.XSendEvent(ctypes.c_void_p(dpy), ctypes.c_ulong(win), 0, 0, buf)
    sync()
