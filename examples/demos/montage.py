#!/usr/bin/env python3
# Puts the preview pictures of the demos side by side in one picture (prehled.png). Only copies pixels of the pictures the program drew (no drawing, no editing):
# python3 montage.py out.png preview1.png preview2.png ...   (5 per row; every picture is scaled by simple pixel skipping when needed)
import struct, sys, zlib

def read_png(path):
    d = open(path, "rb").read()
    assert d[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, w = 8, b"", None
    while pos < len(d):
        n, t = struct.unpack(">I4s", d[pos:pos + 8]); body = d[pos + 8:pos + 8 + n]; pos += 12 + n
        if t == b"IHDR":
            w, h, depth, ctype, _, _, il = struct.unpack(">IIBBBBB", body)
            assert depth == 8 and ctype in (2, 6) and il == 0, "unsupported PNG"
            bpp = 3 if ctype == 2 else 4
        elif t == b"IDAT":
            idat += body
    raw = zlib.decompress(idat); stride = w * bpp; rows = []; prev = bytearray(stride); p = 0
    for _ in range(h):
        f = raw[p]; line = bytearray(raw[p + 1:p + 1 + stride]); p += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0; b = prev[i]; c = prev[i - bpp] if i >= bpp else 0
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if pa <= pb and pa <= pc else b if pb <= pc else c
                line[i] = (line[i] + pr) & 255
        rows.append(bytes(line)); prev = line
    return w, h, bpp, rows

def write_png(path, w, h, rows):
    raw = b"".join(b"\x00" + r for r in rows)
    def chunk(t, b): return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

def main():
    out, files = sys.argv[1], sys.argv[2:]
    cols, tw, th = 5, 480, 320
    nrows = (len(files) + cols - 1) // cols
    canvas = [bytearray(b"\x20\x22\x2a" * (tw * cols)) for _ in range(th * nrows)]
    for k, f in enumerate(files):
        w, h, bpp, rows = read_png(f)
        sx, sy = w / tw, h / th
        for y in range(th):
            src = rows[min(h - 1, int(y * sy))]; dst = canvas[(k // cols) * th + y]; x0 = (k % cols) * tw
            for x in range(tw):
                i = min(w - 1, int(x * sx)) * bpp
                dst[(x0 + x) * 3:(x0 + x) * 3 + 3] = src[i:i + 3]
    write_png(out, tw * cols, th * nrows, [bytes(r) for r in canvas])

main()
