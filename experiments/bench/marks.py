#!/usr/bin/env python3
"""Puts launch.py's traces side by side: each mark's median time from the
process's start, in milliseconds, with the fastest and slowest run, one
column a file, the marks in the order they were reached.

    python3 experiments/bench/marks.py data/launch-a.jsonl data/launch-b.jsonl
"""
import json
import os
import statistics as st
import sys

files = sys.argv[1:]
runs = {f: [json.loads(line) for line in open(f)] for f in files}
times = {}
for f in files:
    for run in runs[f]:
        for name, at in run.get("marks", {}).items():
            times.setdefault(name, []).append(at)
names = sorted(times, key=lambda name: st.median(times[name]))
label = lambda f: os.path.basename(f).removesuffix(".jsonl")
print("| Mark | " + " | ".join(label(f) for f in files) + " |")
print("| --- | " + " | ".join("---:" for _ in files) + " |")
for name in names:
    row = []
    for f in files:
        values = [run["marks"][name] / 1000 for run in runs[f] if name in run.get("marks", {})]
        row.append(f"{st.median(values):.0f} ({min(values):.0f}–{max(values):.0f})" if values else "–")
    print(f"| `{name}` | " + " | ".join(row) + " |")
