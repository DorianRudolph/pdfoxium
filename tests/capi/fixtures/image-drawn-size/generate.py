#!/usr/bin/env python3
"""Optional synthetic fixture generation; no runtime/test-runner codec dependency."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
SIDE = 2048

def tiny_fixture(temporary):
    ppm = Path(temporary) / "gray-32.ppm"
    ppm.write_bytes(b"P6\n32 32\n255\n" + bytes([128, 128, 128]) * (32 * 32))
    image = HERE / "gray-32-no-decomposition.jp2"
    subprocess.run(["opj_compress", "-i", str(ppm), "-o", str(image),
                    "-n", "1", "-r", "1"], check=True)
    data = image.read_bytes()
    cod = data.index(b"\xff\x52")
    assert data[cod + 9] == 0, "COD must contain zero decomposition levels"
    return image


def report(paths):
    print(json.dumps({p.name: dict(bytes=p.stat().st_size,
        sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tiny-only", action="store_true")
    args = parser.parse_args()
    if args.tiny_only:
        with tempfile.TemporaryDirectory(prefix="pdfoxium-drawn-image-") as temporary:
            report([tiny_fixture(temporary)])
        return
    # Equal RGB samples eliminate chroma-conversion ambiguity while the JPEG
    # still has genuine three-component 4:2:0 sampling. A two-pixel checkerboard
    # retains full-resolution contrast but loses it at a quarter-size decode.
    pixels = bytearray()
    for y in range(SIDE):
        for x in range(SIDE):
            value = 64 if ((x // 2 + y // 2) & 1) == 0 else 192
            pixels.extend((value, value, value))
    with tempfile.TemporaryDirectory(prefix="pdfoxium-drawn-image-") as temporary:
        ppm = Path(temporary) / "checker.ppm"
        ppm.write_bytes(f"P6\n{SIDE} {SIDE}\n255\n".encode() + pixels)
        jpeg = HERE / "checker-2048-420.jpg"
        jpx = HERE / "checker-2048-lossless.jp2"
        subprocess.run(["cjpeg", "-quality", "95", "-sample", "2x2,1x1,1x1",
                        "-baseline", "-outfile", str(jpeg), str(ppm)], check=True)
        subprocess.run(["opj_compress", "-i", str(ppm), "-o", str(jpx),
                        "-n", "6", "-r", "1"], check=True)
        tiny = tiny_fixture(temporary)
    # Verify the condition that native reduced JPEG decoding requires, rather
    # than relying only on the encoder option spelling.
    data = jpeg.read_bytes()
    offset = 2
    while offset + 4 <= len(data):
        assert data[offset] == 0xff
        marker = data[offset + 1]
        length = int.from_bytes(data[offset + 2:offset + 4], "big")
        if marker == 0xc0:
            frame = data[offset + 4:offset + 2 + length]
            assert frame[:6] == bytes([8, 8, 0, 8, 0, 3])
            assert [frame[7], frame[10], frame[13]] == [0x22, 0x11, 0x11]
            break
        assert marker != 0xda, "missing baseline SOF"
        offset += 2 + length
    else:
        raise AssertionError("missing baseline SOF")
    report([jpeg, jpx, tiny])

if __name__ == "__main__":
    main()
