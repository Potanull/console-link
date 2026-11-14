#!/usr/bin/env python3
"""Fast local feedback, or the complete non-interactive source checks before push."""

import argparse
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full", action="store_true", help="Run strict Clippy, Rust tests and Nintendo helper tests")
    parser.add_argument("--target", help="Optional Rust target triple; defaults to the local machine")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    target = ["--target", args.target] if args.target else []
    commands = [["cargo", "fmt", "--check"]]
    if args.full:
        commands += [
            ["cargo", "clippy", "--locked", "--all-targets", *target, "--", "-D", "warnings"],
            ["cargo", "test", "--locked", *target],
            ["node", "--test", "scripts/nintendo-presence.test.cjs"],
        ]
    else:
        commands.append(["cargo", "check", "--locked", "--all-targets", *target])
    started = time.monotonic()
    for command in commands:
        print("+ " + " ".join(command), flush=True)
        subprocess.run(command, cwd=root, check=True)
    print(f"Checks passed in {time.monotonic() - started:.1f}s. Native visual/hardware checks are separate.")


if __name__ == "__main__":
    main()
