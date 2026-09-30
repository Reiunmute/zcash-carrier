"""Generate two 26x26 16-colour pixel portraits and pack them for a 512-byte memo.

Payload layout (395 bytes, prefixed by the carrier header in Rust):
  48 bytes palette (16 x RGB) + 338 bytes pixels (676 x 4-bit, high nibble first)
"""
import random, sys
from PIL import Image

W = H = 26
PAL = [
    (14, 14, 20), (32, 30, 44), (58, 54, 78), (96, 92, 124),   # 0-3 night / hood
    (235, 232, 224), (200, 196, 186), (150, 146, 140),         # 4-6 mask
    (212, 170, 64), (250, 214, 120),                           # 7-8 gold eyes
    (120, 72, 160), (170, 120, 210),                           # 9-10 violet
    (40, 90, 120), (70, 140, 170),                             # 11-12 teal
    (10, 10, 12), (255, 255, 255), (90, 20, 30),               # 13-15
]

def portrait(seed):
    rnd = random.Random(seed)
    px = [[1 for _ in range(W)] for _ in range(H)]
    # background dither
    for y in range(H):
        for x in range(W):
            if (x + y) % 7 == 0 and rnd.random() < 0.4:
                px[y][x] = 2
    # hood (ellipse)
    cx, cy = 13, 14
    for y in range(H):
        for x in range(W):
            if ((x - cx) / 10.5) ** 2 + ((y - cy) / 12.5) ** 2 <= 1:
                px[y][x] = 3 if y < 12 else 2
    # mask (smaller ellipse)
    for y in range(H):
        for x in range(W):
            if ((x - cx) / 6.5) ** 2 + ((y - 14.5) / 8.0) ** 2 <= 1:
                px[y][x] = 4
                if x >= cx + 3:
                    px[y][x] = 5
    # eyes
    eye = 8 if seed % 2 else 10
    for ex in (10, 15):
        px[13][ex] = eye; px[13][ex + 1] = 7 if eye == 8 else 9
    # mask seam + mouth line
    for y in range(8, 22):
        if px[y][cx] in (4, 5):
            px[y][cx] = 6
    for x in range(11, 16):
        px[18][x] = 6
    # trait: forehead sigil
    sig = 12 if seed % 2 else 15
    px[9][13] = sig; px[10][12] = sig; px[10][14] = sig
    # shoulders
    for y in range(22, H):
        for x in range(3, 23):
            px[y][x] = 11 if (x + y) % 2 else 12
    return px

def pack(px):
    out = bytearray()
    for r, g, b in PAL:
        out += bytes((r, g, b))
    flat = [c for row in px for c in row]
    for i in range(0, len(flat), 2):
        out.append((flat[i] << 4) | flat[i + 1])
    assert len(out) == 48 + 338
    return bytes(out)

def save_png(px, path, scale=12):
    im = Image.new("RGB", (W, H))
    im.putdata([PAL[c] for row in px for c in row])
    im.resize((W * scale, H * scale), Image.NEAREST).save(path)

if __name__ == "__main__":
    outdir = sys.argv[1]
    for n in (1, 2, 3):
        px = portrait(n)
        open(f"{outdir}/art{n}.bin", "wb").write(pack(px))
        save_png(px, f"{outdir}/art{n}_source.png")
        print(n, len(pack(px)))
