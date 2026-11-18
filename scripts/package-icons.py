#!/usr/bin/env python3
"""Build all app icon formats from assets/icon.png on macOS using sips."""

from pathlib import Path
import struct
import subprocess
import tempfile


def main():
    assets = Path(__file__).resolve().parent.parent / "assets"
    source = assets / "icon.png"
    with tempfile.TemporaryDirectory(prefix="console-link-icons-") as temporary:
        root = Path(temporary)
        rasters = {}
        for size in (16, 32, 48, 64, 128, 256, 512, 1024):
            raster = root / f"{size}.png"
            subprocess.run(
                ["sips", "-z", str(size), str(size), str(source), "--out", str(raster)],
                check=True, stdout=subprocess.DEVNULL,
            )
            rasters[size] = raster.read_bytes()
        # Modern ICNS elements contain PNG data directly, including its alpha channel.
        elements = ((b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128),
                    (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024))
        payload = b"".join(
            kind + struct.pack(">I", len(rasters[size]) + 8) + rasters[size]
            for kind, size in elements
        )
        (assets / "icon.icns").write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)
        sizes = (16, 32, 48, 64, 128, 256)
        offset = 6 + 16 * len(sizes)
        entries = []
        for size in sizes:
            data = rasters[size]
            entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0,
                                       1, 32, len(data), offset))
            offset += len(data)
        (assets / "icon.ico").write_bytes(
            struct.pack("<HHH", 0, 1, len(sizes)) + b"".join(entries)
            + b"".join(rasters[size] for size in sizes)
        )
        (assets / "window-icon.png").write_bytes(rasters[256])
    print("Updated macOS, Windows and window icons from assets/icon.png.")


if __name__ == "__main__":
    main()
