#!/usr/bin/env python3
"""Where a Rune build's launch goes: opens a document RUNS times, each in a
floating window on workspace 1 that takes no focus, with
PDVIM_LAUNCH_MARKS and RUNE_GPU_LOG set, and prints one JSON line a run:
the marks (microseconds from the process's start), and the GPU log's
lines.

    python3 experiments/bench/launch.py LABEL BINARY DOC RUNS

Environment:
    CACHE   a folder for XDG_CACHE_HOME kept across runs (a warm shader
            cache); without it each run gets an empty one, as measure.py
            gives
    EXTRA   more variables for the process, "NAME=value NAME=value"
    WAIT    seconds to let each run go before it is stopped (default 5)
    SCRATCH where the runs' folders go (default /tmp)
"""

import json
import os
import shutil
import subprocess
import sys
import time

WAIT = float(os.environ.get("WAIT", "5"))
SIZE = os.environ.get("SIZE", "1000 750")


def tree(pid):
    out = subprocess.run(["ps", "-o", "pid=", "--ppid", str(pid)],
                         capture_output=True, text=True).stdout.split()
    pids = [pid]
    for child in out:
        pids += tree(int(child))
    return pids


def run(binary, doc, scratch):
    for d in ("config", "data", "cache", "state"):
        os.makedirs(os.path.join(scratch, d), exist_ok=True)
    cache = os.environ.get("CACHE") or f"{scratch}/cache"
    os.makedirs(cache, exist_ok=True)
    pidfile, marks = f"{scratch}/pid", f"{scratch}/marks.jsonl"
    env = (f"XDG_CONFIG_HOME={scratch}/config XDG_DATA_HOME={scratch}/data "
           f"XDG_CACHE_HOME={cache} XDG_STATE_HOME={scratch}/state "
           f"PDVIM_LAUNCH_MARKS={marks} RUNE_GPU_LOG=1 "
           + os.environ.get("EXTRA", ""))
    inner = f"echo $$ > {pidfile}; exec env {env} {binary} {doc} >{scratch}/log 2>&1"
    subprocess.run(["hyprctl", "dispatch",
                    f'hl.dsp.exec_cmd("sh -c \'{inner}\'", '
                    '{ float = true, size = "' + SIZE + '", workspace = "1 silent" })'],
                   check=True, capture_output=True)
    while not os.path.exists(pidfile) or not open(pidfile).read().strip():
        time.sleep(0.002)
    pid = int(open(pidfile).read())
    time.sleep(WAIT)
    pids = tree(pid)
    for p in reversed(pids):
        try:
            os.kill(p, 15)
        except OSError:
            pass
    time.sleep(0.5)
    for p in pids:
        try:
            os.kill(p, 9)
        except OSError:
            pass
    result = {}
    if os.path.exists(marks):
        line = open(marks).readline().strip()
        if line:
            result = json.loads(line)
    log = open(f"{scratch}/log", errors="replace").read().splitlines()
    result["log"] = [line for line in log if line.strip()][:80]
    return result


if __name__ == "__main__":
    label, binary, doc, runs = (sys.argv[1], os.path.abspath(sys.argv[2]),
                                os.path.abspath(sys.argv[3]), int(sys.argv[4]))
    base = os.environ.get("SCRATCH", "/tmp")
    for i in range(runs):
        scratch = os.path.join(base, f"launch-{label}-{i}")
        shutil.rmtree(scratch, ignore_errors=True)
        result = run(binary, doc, scratch)
        result["label"] = label
        print(json.dumps(result), flush=True)
        time.sleep(0.5)
