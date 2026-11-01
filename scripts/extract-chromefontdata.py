#!/usr/bin/env python3
"""Extract PDFium's pinned Foxit byte arrays as Rust include_bytes! assets."""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "upstream/pdfium/core/fxge/fontdata/chromefontdata"
DEST = ROOT / "crates/pdfoxium-core/assets/chromefontdata"


def main() -> None:
    DEST.mkdir(parents=True, exist_ok=True)
    for source in sorted(SOURCE.glob("Foxit*.cpp")):
        text = source.read_text()
        match = re.search(
            r"const std::array<uint8_t,\s*(\d+)>\s+\w+\s*=\s*\{\{(.*?)\}\};",
            text,
            re.DOTALL,
        )
        if match is None:
            raise ValueError(f"Missing byte array: {source}")
        data = bytes(int(value, 16) for value in re.findall(r"0x[0-9a-fA-F]+", match[2]))
        if len(data) != int(match[1]):
            raise ValueError(f"Length mismatch: {source}")
        (DEST / f"{source.stem}.bin").write_bytes(data)


if __name__ == "__main__":
    main()
