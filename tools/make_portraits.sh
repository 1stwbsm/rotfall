#!/bin/sh
# Renders the bestiary and dossier portraits with the game's own renderer, then recompresses them:
# the game writes PNGs without compression, and these are built into every copy of it.
set -e
cd "$(dirname "$0")/.."
cargo build --release
./target/release/rotfall --portraits assets --no-audio
python3 -I - assets/portrait_*.png <<'PY'
import struct, sys, zlib
for path in sys.argv[1:]:
    d = open(path, "rb").read()
    p, idat, head = 8, b"", None
    while p < len(d):
        n = struct.unpack(">I", d[p:p + 4])[0]
        tag, body = d[p + 4:p + 8], d[p + 8:p + 8 + n]
        p += 12 + n
        if tag == b"IHDR":
            head = body
        elif tag == b"IDAT":
            idat += body
    w, h = struct.unpack(">II", head[:8])
    raw = zlib.decompress(idat)
    # Halve it, which smooths the edges and the film grain, drop the alpha channel, which is
    # always opaque, and filter each row against the one above.
    src = [raw[y * (w * 4 + 1) + 1:(y + 1) * (w * 4 + 1)] for y in range(h)]
    w, h = w // 2, h // 2
    rows, prev = [], bytes(w * 3)
    for y in range(h):
        a, b = src[y * 2], src[y * 2 + 1]
        # Sixty-four levels a channel: the grain hides the steps, and the files come out far smaller.
        rgb = bytes(min(255, ((a[x * 8 + c] + a[x * 8 + 4 + c] + b[x * 8 + c] + b[x * 8 + 4 + c] + 2) // 4 + 2) & 0xFC) for x in range(w) for c in range(3))
        rows.append(b"\x02" + bytes((p - q) & 255 for p, q in zip(rgb, prev)))
        prev = rgb
    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))
    out = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    out += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(out)
    print("%s: %d KB" % (path, len(out) // 1024))
PY
cargo build --release
