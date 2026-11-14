#!/usr/bin/env python3
"""Run the same presentation checks against development and packaged binaries."""
import argparse, os, pathlib, re, subprocess

parser = argparse.ArgumentParser(description="Verify native GPU presentation and window modes.")
parser.add_argument("executable", type=pathlib.Path)
args = parser.parse_args()
executable = args.executable
log = pathlib.Path(os.environ.get('RUNNER_TEMP', 'dist')) / 'console-link-gpui-smoke.log'
log.parent.mkdir(parents=True, exist_ok=True)
arguments = [str(executable.resolve()), '--synthetic', '--smoke-cycle', '--smoke-seconds', '14']
try:
    result = subprocess.run(arguments, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace", timeout=30)
except subprocess.TimeoutExpired as error:
    output = error.stdout or b''
    if isinstance(output, bytes):
        output = output.decode('utf-8', errors='replace')
    log.write_text(output, encoding='utf-8')
    print(output)
    raise SystemExit('Native GPUI smoke exceeded 30 seconds; no headless-success or CPU fallback is accepted')
log.write_text(result.stdout, encoding='utf-8')
print(result.stdout)
if result.returncode != 0:
    raise SystemExit(f'Native GPUI smoke failed with exit code {result.returncode}; inspect window/GPU/DirectComposition diagnostics above')
counters = re.findall(r'GPUI_SHUTDOWN presents=(\d+) gpu_readback_playback=0', result.stdout)
if not counters or int(counters[-1]) <= 0:
    raise SystemExit('Native GPUI smoke did not confirm a presented GPU frame without playback readback')
for mode in ('Borderless', 'Fullscreen', 'Windowed'):
    if f'mode={mode}' not in result.stdout:
        raise SystemExit(f'Native smoke did not exercise {mode}')
borderless = [line for line in result.stdout.splitlines() if 'mode=Borderless fullscreen=false' in line]
if not borderless:
    raise SystemExit('Borderless mode was not confirmed outside native fullscreen')
# Native style changes notify GPUI on the next event-loop turn.
# Check the settled sample, while rejecting persistent viewport drift.
line = borderless[-1]
sizes = re.findall(r'Size \{ ([0-9.]+)px × ([0-9.]+)px \}', line)
if len(sizes) < 2:
    raise SystemExit(f'Could not parse borderless native bounds and viewport: {line}')
if sizes[0] != sizes[1]:
    raise SystemExit(f'Borderless native bounds and GPUI viewport differ: {line}')
