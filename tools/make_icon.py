#!/usr/bin/env python3
"""Builds assets/icon.ico for the Windows .exe from assets/icon_256.png. Standard library only."""
import struct
import sys
import zlib


def read_png(path):
    d = open(path, "rb").read()
    p, idat = 8, b""
    while p < len(d):
        n = struct.unpack(">I", d[p:p + 4])[0]
        tag, body = d[p + 4:p + 8], d[p + 8:p + 8 + n]
        p += 12 + n
        if tag == b"IHDR":
            w, h, depth, kind = struct.unpack(">IIBB", body[:10])
            if depth != 8 or kind != 6:
                sys.exit("expected an 8-bit RGBA PNG")
        elif tag == b"IDAT":
            idat += body
    raw, rows, prev, o = zlib.decompress(idat), [], bytearray(w * 4), 0
    for _ in range(h):
        f, line = raw[o], bytearray(raw[o + 1:o + 1 + w * 4])
        o += 1 + w * 4
        for i in range(w * 4):
            a = line[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, h, rows


def shrink(w, rows, size):
    """Box filter weighted by alpha, so transparent corners do not darken the edges."""
    out = []
    for y in range(size):
        y0, y1 = y * w // size, max((y + 1) * w // size, y * w // size + 1)
        line = bytearray()
        for x in range(size):
            x0, x1 = x * w // size, max((x + 1) * w // size, x * w // size + 1)
            r = g = b = a = n = 0
            for yy in range(y0, y1):
                row = rows[yy]
                for xx in range(x0, x1):
                    i = xx * 4
                    al = row[i + 3]
                    r += row[i] * al
                    g += row[i + 1] * al
                    b += row[i + 2] * al
                    a += al
                    n += 1
            line += bytes((r // a, g // a, b // a, a // n)) if a else bytes(4)
        out.append(line)
    return out


def write_png(rows, size):
    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))
    raw = b"".join(b"\0" + bytes(r) for r in rows)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


w, h, rows = read_png("assets/icon_256.png")
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [write_png(rows if s == w else shrink(w, rows, s), s) for s in sizes]
offset = 6 + 16 * len(sizes)
head = struct.pack("<HHH", 0, 1, len(sizes))
for s, img in zip(sizes, images):
    head += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(img), offset)
    offset += len(img)
open("assets/icon.ico", "wb").write(head + b"".join(images))
print("assets/icon.ico: %d sizes, %d KB" % (len(sizes), offset // 1024))
