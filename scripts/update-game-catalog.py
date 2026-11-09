#!/usr/bin/env python3
"""Rebuild bundled game metadata. Build-time tool only; no Python at runtime."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import urllib.request

REVISION = "d424690446eec6f4484209b9a1c067c7f7511398"
REGIONS = ["US.en", "GB.en", "JP.ja", "RU.ru", "FR.fr", "DE.de"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    args = parser.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    games = {}
    sources = []
    for region in REGIONS:
        url = f"https://raw.githubusercontent.com/blawar/titledb/{REVISION}/{region}.json"
        path = args.cache / f"{REVISION}-{region}.json"
        if not path.exists():
            request = urllib.request.Request(url, headers={"User-Agent": "ConsoleLink-Catalog/1"})
            temporary = path.with_suffix(".part")
            with urllib.request.urlopen(request, timeout=60) as response, temporary.open("wb") as out:
                while chunk := response.read(1024 * 1024):
                    out.write(chunk)
                    if out.tell() > 120 * 1024 * 1024:
                        raise ValueError("Unexpectedly large source catalog")
            temporary.replace(path)
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        sources.append({"region": region, "sha256": digest, "url": url})
        data = json.loads(path.read_text())
        for item in data.values():
            game_id = item.get("id")
            name = item.get("name")
            if not isinstance(game_id, str) or not re.fullmatch(r"[0-9a-fA-F]{16}", game_id):
                continue
            if int(game_id, 16) & 0x1FFF or item.get("isDemo") or not isinstance(name, str) or not name.strip():
                continue
            game_id = game_id.upper()
            if game_id not in games:
                cover = item.get("iconUrl") or item.get("frontBoxArt")
                games[game_id] = {"id": game_id, "name": name.strip(), "aliases": [],
                    "cover_url": cover if isinstance(cover, str) and cover.startswith("https://") else None,
                    # No public product URL is supplied by TitleDB. Do not invent slugs.
                    "game_url": None}
            game = games[game_id]
            if name.strip() != game["name"] and name.strip() not in game["aliases"]:
                game["aliases"].append(name.strip())
        print(f"{region}: {len(games)} cumulative base games", flush=True)
        del data
    root = Path(__file__).resolve().parents[1]
    output = sorted(games.values(), key=lambda game: (game["name"].casefold(), game["id"]))
    (root / "assets/games.json").write_text(json.dumps(output, ensure_ascii=False, separators=(",", ":")) + "\n")
    (root / "assets/games-sources.json").write_text(json.dumps({"revision": REVISION, "count": len(output), "sources": sources}, indent=2) + "\n")
    print(f"Wrote {len(output)} games")


if __name__ == "__main__":
    main()
