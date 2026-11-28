#!/usr/bin/env python3
"""Prints Markdown tables of the medians in measure.py's output, one row a
document, one column a build, for each measure.

    summarise.py results.jsonl [more.jsonl ...]
"""
import json, statistics, sys

MEASURES = [
    ("window_s", "Seconds until the window appears", 2),
    ("ready_s", "Seconds until the document is open and drawn", 2),
    ("open_cpu_s", "Processor seconds to open", 2),
    ("open_pss_mb", "Memory once open (PSS, MB)", 0),
    ("threads", "Threads once open", 0),
    ("idle_cpu_s", "Processor seconds over 5 s idle", 2),
    ("scroll_cpu_s", "Processor seconds for 50 presses of j", 2),
    ("page_cpu_s", "Processor seconds for 20 presses of Ctrl-f", 2),
    ("jump_s", "Seconds to go to the last page (G)", 2),
    ("jump_cpu_s", "Processor seconds to go to the last page", 2),
    ("end_pss_mb", "Memory after all of that (PSS, MB)", 0),
]
DOCS = ["small.pdf", "manual.pdf", "pictures.pdf", "long.pdf"]
NAMES = {"small.pdf": "1 page", "manual.pdf": "477 pages",
         "pictures.pdf": "42 pages, pictures", "long.pdf": "1,908 pages"}

rows = [json.loads(line) for path in sys.argv[1:] for line in open(path) if line.strip()]
labels = list(dict.fromkeys(r["label"] for r in rows))
for key, title, places in MEASURES:
    print(f"\n{title} (median of runs):\n")
    print("| Document | " + " | ".join(labels) + " |")
    print("| --- | " + " | ".join("---:" for _ in labels) + " |")
    for doc in DOCS:
        cells = []
        for label in labels:
            values = [r[key] for r in rows if r["label"] == label and r["doc"] == doc and r.get(key) is not None]
            cells.append(f"{statistics.median(values):.{places}f}" if values else "–")
        print(f"| {NAMES.get(doc, doc)} | " + " | ".join(cells) + " |")
counts = {(r["label"], r["doc"]) for r in rows}
print("\nRuns: " + ", ".join(f"{label} {sum(1 for r in rows if r['label'] == label)}" for label in labels))
