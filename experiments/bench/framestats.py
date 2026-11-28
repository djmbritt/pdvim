#!/usr/bin/env python3
"""Puts frames.py's runs side by side as Markdown tables: per document and
key, the median over runs of each build's frames, how many built the view
again, the mean time of each stage of a frame in microseconds, and the
process's processor time.

    python3 experiments/bench/framestats.py data/frames-*.jsonl
"""
import json
import statistics as st
import sys

STAGES = [("built", "built again"), ("view", "view"), ("commit", "commit"),
          ("layout", "layout"), ("paint", "paint"), ("apply", "apply"),
          ("build", "build, all"), ("upload", "upload"), ("encode", "encode"),
          ("submit", "submit"), ("present", "present"), ("total", "frame")]

rows = [json.loads(line) for path in sys.argv[1:] for line in open(path)]
docs = sorted({r["doc"] for r in rows})
labels = list(dict.fromkeys(r["label"] for r in rows))
keys = list(dict.fromkeys(k for r in rows for k in r["steps"]))

for doc in docs:
    for key in keys:
        print(f"\n{doc}, {key}:\n")
        print("| | " + " | ".join(labels) + " |")
        print("| --- | " + " | ".join("---:" for _ in labels) + " |")
        cells = {}
        for label in labels:
            runs = [r["steps"][key] for r in rows if r["doc"] == doc and r["label"] == label]
            cells[label] = runs
        print("| frames | " + " | ".join(
            f"{st.median([x['frames'] for x in cells[l]]):.0f}" if cells[l] and cells[l][0]["frames"] else "–"
            for l in labels) + " |")
        for stage, name in STAGES:
            out = []
            for label in labels:
                values = [x[stage] for x in cells[label] if x.get(stage) is not None]
                out.append(f"{st.median(values):.0f}" if values else "–")
            print(f"| {name} | " + " | ".join(out) + " |")
        print("| process, all threads (ms) | " + " | ".join(
            f"{1000 * st.median([x['process_cpu_s'] for x in cells[l]]):.0f}" if cells[l] else "–"
            for l in labels) + " |")
