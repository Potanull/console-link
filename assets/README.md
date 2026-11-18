# Console Link icon

Minimal white gamepad with a D-pad and integrated Triforce on a dark rounded tile.
The canvas outside the tile is transparent, including the corners. The wordmark
is rendered separately by the interface.

Generated with the built-in imagegen tool. Rebuild every platform variant from
`icon.png` with `python3 scripts/package-icons.py` on macOS. Resizing uses `sips`.
ICNS and ICO embed the same PNG payloads, preserving transparent corners at every size.

- icon.png: 1024 px application artwork.
- window-icon.png: 256 px window and navigation artwork.
- icon.icns: macOS bundle.
- icon.ico: Windows executable resource.

Direction: preserve the solid white gamepad, dark D-pad and Triforce. Add a near-black
tile with large smooth rounded corners and a transparent outer margin. Balanced
padding, no lettering, crisp edges and readability at small sizes.
