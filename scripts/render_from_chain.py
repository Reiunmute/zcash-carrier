"""Rebuild PNGs purely from the bytes the verifier pulled out of on-chain memos."""
import sys
from PIL import Image

W = H = 26
for n in sys.argv[1:]:
    b = open(f"out/item{n}_from_chain.bin", "rb").read()
    pal = [tuple(b[i:i + 3]) for i in range(0, 48, 3)]
    idx = []
    for byte in b[48:48 + 338]:
        idx += [byte >> 4, byte & 15]
    im = Image.new("RGB", (W, H))
    im.putdata([pal[i] for i in idx])
    im.resize((W * 12, H * 12), Image.NEAREST).save(f"out/item{n}_from_chain.png")
    src = open(f"art/art{n}.bin", "rb").read()
    print(f"item{n}: {len(b)}B, identical_to_source={b == src}")
