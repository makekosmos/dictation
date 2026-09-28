"""Regenerate gpui/windows/app.ico from gpui/windows/dictation.png.

Requires Pillow (pip install pillow). Every entry is a PNG-compressed icon
image (Vista+); sizes match what Windows requests for the exe/taskbar —
16/24/32/48/64/128/256. Deterministic: same input → same output bytes.

    python scripts/make_icon.py          # write gpui/windows/app.ico
    python scripts/make_icon.py --check  # fail if app.ico is out of date
"""

import argparse
import io
import struct
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "gpui" / "windows" / "dictation.png"
TARGET = ROOT / "gpui" / "windows" / "app.ico"
SIZES = (16, 24, 32, 48, 64, 128, 256)


def build_ico(source: Path) -> bytes:
    image = Image.open(source)
    if image.mode != "RGBA":
        image = image.convert("RGBA")
    entries = []
    for size in SIZES:
        resized = image.resize((size, size), Image.LANCZOS)
        buffer = io.BytesIO()
        resized.save(buffer, format="PNG")
        entries.append(buffer.getvalue())

    header = struct.pack("<HHH", 0, 1, len(entries))
    directory = b""
    offset = 6 + 16 * len(entries)
    for size, blob in zip(SIZES, entries):
        # Width/height bytes store 256 as 0. PNG entries carry no colour
        # count/planes — bpp is informational for PNG-compressed icons.
        field = 0 if size == 256 else size
        directory += struct.pack("<BBBBHHII", field, field, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    return header + directory + b"".join(entries)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="fail if app.ico is not what the script would write")
    args = parser.parse_args()

    expected = build_ico(SOURCE)
    if args.check:
        actual = TARGET.read_bytes() if TARGET.is_file() else b""
        if actual != expected:
            print(f"{TARGET} is out of date — run python scripts/make_icon.py", file=sys.stderr)
            return 1
        print(f"{TARGET} is up to date")
        return 0
    TARGET.write_bytes(expected)
    print(f"Wrote {TARGET} ({len(expected)} bytes, sizes {SIZES})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
