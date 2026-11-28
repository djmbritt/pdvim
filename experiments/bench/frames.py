#!/usr/bin/env python3
"""What each frame costs while reading: opens a document in a build made
with `--features frame-profile`, with RUNE_FRAME_LOG set, presses keys at a
steady pace (so that each press is a frame of its own, as a held key's
repeats are), quits with `:q` and `y` so that the log is written out, and prints
one JSON line a run: per key, the processor time of the whole process,
how many frames there were, how many built the view again, and their
stages' times in microseconds. A build without the frame log (or iced)
gives the processor time alone.

    python3 experiments/bench/frames.py LABEL BINARY DOC RUNS

Environment: CACHE (a cache folder kept across runs, as measure.py's),
SCRATCH (where the runs' folders go), PACE (seconds between presses,
default 0.05).
"""

import json
import os
import shutil
import statistics
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure  # noqa: E402

PACE = float(os.environ.get("PACE", "0.05"))
# What is pressed, in order: (name, modifiers, key, presses).
STEPS = [("j", "", "j", 50), ("k", "", "k", 50), ("C-f", "CTRL", "f", 20)]
STAGES = ["input", "view", "commit", "layout", "text", "paint", "apply",
          "build", "upload", "encode", "submit", "present", "total", "cpu"]


def run(binary, doc, scratch):
    for d in ("config", "data", "state"):
        os.makedirs(os.path.join(scratch, d), exist_ok=True)
    cache = os.environ.get("CACHE") or f"{scratch}/cache"
    os.makedirs(cache, exist_ok=True)
    pidfile, log = f"{scratch}/pid", f"{scratch}/frames.jsonl"
    env = (f"XDG_CONFIG_HOME={scratch}/config XDG_DATA_HOME={scratch}/data "
           f"XDG_CACHE_HOME={cache} XDG_STATE_HOME={scratch}/state "
           f"RUNE_FRAME_LOG={log}")
    inner = f"echo $$ > {pidfile}; exec env {env} {binary} {doc} >{scratch}/log 2>&1"
    subprocess.run(["hyprctl", "dispatch",
                    f'hl.dsp.exec_cmd("sh -c \'{inner}\'", '
                    '{ float = true, size = "' + measure.SIZE + '", workspace = "1 silent" })'],
                   check=True, capture_output=True)
    while not os.path.exists(pidfile) or not open(pidfile).read().strip():
        time.sleep(0.002)
    pid = int(open(pidfile).read())
    found = None
    while found is None:
        time.sleep(0.05)
        found = measure.window(measure.tree(pid))
    time.sleep(1.5)
    address = found["address"]
    marks = []
    for name, mods, key, presses in STEPS:
        start, before = time.monotonic_ns(), measure.cpu(measure.tree(pid))
        for _ in range(presses):
            measure.press(address, mods, key)
            time.sleep(PACE)
        time.sleep(0.5)
        used = (measure.cpu(measure.tree(pid)) - before) / measure.TICK
        marks.append((name, start, time.monotonic_ns(), used))
    # `:q`, and yes to "quit pdvim?": an orderly exit writes the log out.
    for mods, key in (("SHIFT", "semicolon"), ("", "q"), ("", "Return"), ("", "y")):
        measure.press(address, mods, key)
        time.sleep(0.1)
    for _ in range(100):
        if not os.path.exists(f"/proc/{pid}"):
            break
        time.sleep(0.05)
    # A build that does not quit on `:q` in time is stopped, so that no
    # window is left open to weigh on the next run.
    for p in reversed(measure.tree(pid)):
        try:
            os.kill(p, 9)
        except OSError:
            pass
    # A build without the frame log (or iced) leaves none.
    frames = []
    if os.path.exists(log):
        frames = [json.loads(line) for line in open(log) if line.startswith('{"')]
    return frames, marks


def summary(frames, marks):
    # The log's `t` is the monotonic clock in nanoseconds, as Python's.
    out = {}
    for name, start, end, used in marks:
        these = [f for f in frames if "total" in f and start <= f.get("t", 0) <= end]
        built = [f for f in these if f.get("view", 0) > 0]
        # The processor time of the whole process, every thread, while the
        # keys were pressed and half a second after.
        row = {"process_cpu_s": round(used, 3), "frames": len(these), "built": len(built)}
        for stage in STAGES:
            values = [f.get(stage) or 0 for f in these]
            row[stage] = round(statistics.mean(values) / 1000, 1) if values else None
        row["cpu_sum_ms"] = round(sum(f.get("cpu") or 0 for f in these) / 1e6, 2)
        out[name] = row
    return out


if __name__ == "__main__":
    label, binary, doc, runs = (sys.argv[1], os.path.abspath(sys.argv[2]),
                                os.path.abspath(sys.argv[3]), int(sys.argv[4]))
    base = os.environ.get("SCRATCH", "/tmp")
    for i in range(runs):
        scratch = os.path.join(base, f"frames-{label}-{i}")
        shutil.rmtree(scratch, ignore_errors=True)
        frames, marks = run(binary, doc, scratch)
        print(json.dumps({"label": label, "doc": os.path.basename(doc),
                          "steps": summary(frames, marks)}), flush=True)
        time.sleep(0.5)
