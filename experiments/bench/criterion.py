#!/usr/bin/env python3
"""Pulls the benchmark names and median times out of `cargo bench` logs and
prints them side by side as a Markdown table.

    criterion.py LABEL=bench.log [LABEL=bench.log ...]
"""
import re, sys

def parse(path):
    times, name = {}, None
    for line in open(path):
        start = re.match(r"^(\S.*?)\s{2,}time:|^(\S.*)$", line.rstrip("\n"))
        if line.startswith("Benchmarking") or line.startswith("Found") or "Warming" in line:
            continue
        m = re.match(r"^(\S[^\[]*?)\s+time:\s+\[\S+ \S+ (\S+ \S+) \S+ \S+\]", line)
        if m:
            times[m.group(1).strip()] = m.group(2)
            continue
        m = re.match(r"^\s+time:\s+\[\S+ \S+ (\S+ \S+) \S+ \S+\]", line)
        if m and name:
            times[name] = m.group(1)
            continue
        m = re.match(r"^([a-z][\w /.-]*\S)\s*$", line)
        if m:
            name = m.group(1)
    return times

runs = [(arg.split("=", 1)[0], parse(arg.split("=", 1)[1])) for arg in sys.argv[1:]]
names = list(dict.fromkeys(n for _, t in runs for n in t))
print("| Benchmark | " + " | ".join(label for label, _ in runs) + " |")
print("| --- | " + " | ".join("---:" for _ in runs) + " |")
for n in names:
    print(f"| {n} | " + " | ".join(t.get(n, "–") for _, t in runs) + " |")
