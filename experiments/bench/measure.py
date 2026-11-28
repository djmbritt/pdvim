#!/usr/bin/env python3
"""Measures one pdvim binary opening and reading a document, for comparing
builds (iced against Rune). Adapted from the harness behind
docs/comparison.md, so the figures line up with that report.

Each run starts the binary in a floating 1000 x 750 window on workspace 1,
without focus ("1 silent"), with empty config, data, cache and state
folders of its own, then:

  open    the time until its window maps, the time until it stops using
          the processor (opened and drawn), the processor time that took,
          and its memory (PSS) and threads then;
  idle    the processor time used over IDLE seconds of doing nothing;
  scroll  SCROLL presses of `j`, then the processor time until settled;
  page    PAGES presses of `<C-f>` (a screen down), the same;
  jump    `G` (the last page): the time and processor time until settled,
          and the memory after.

Keys go to the test window alone, found by the pid this script started,
never by its class: the person at the machine may have pdvim open too.

    measure.py LABEL BINARY DOC RUNS  >> results.jsonl

With CACHE=DIR the runs share DIR as their cache folder instead, so the
GPU driver's compiled shaders are kept from one run to the next.
"""
import json, os, shutil, subprocess, sys, time

SETTLE = 1.0   # seconds without processor use that count as settled
LIMIT = 30.0   # give up waiting after this long
IDLE = float(os.environ.get("IDLE", "5"))
SCROLL = int(os.environ.get("SCROLL", "50"))
PAGES = int(os.environ.get("PAGES", "20"))
SIZE = os.environ.get("SIZE", "1000 750")
TICK = os.sysconf("SC_CLK_TCK")


def ready(samples):
    """When all but the last two ticks (20 ms) of the work was done, so that
    a stray tick long after is not counted."""
    if not samples:
        return None
    final = samples[-1][1]
    for t, c in samples:
        if c >= final - 2:
            return t
    return samples[-1][0]


def tree(pid):
    pids, todo = [], [pid]
    while todo:
        p = todo.pop()
        pids.append(p)
        try:
            with open(f"/proc/{p}/task/{p}/children") as f:
                todo += [int(c) for c in f.read().split()]
        except OSError:
            pass
    return pids


def cpu(pids):
    total = 0
    for p in pids:
        try:
            fields = open(f"/proc/{p}/stat").read().rsplit(")", 1)[1].split()
            total += int(fields[11]) + int(fields[12])
        except (OSError, IndexError):
            pass
    return total


def pss_kb(pids):
    total = 0
    for p in pids:
        try:
            for line in open(f"/proc/{p}/smaps_rollup"):
                if line.startswith("Pss:"):
                    total += int(line.split()[1])
        except OSError:
            pass
    return total


def threads(pids):
    return sum(len(os.listdir(f"/proc/{p}/task")) for p in pids if os.path.exists(f"/proc/{p}"))


def window(pids):
    for c in json.loads(subprocess.check_output(["hyprctl", "clients", "-j"])):
        if c["pid"] in pids and c["mapped"]:
            return c
    return None


def press(address, mods, key):
    subprocess.run(["hyprctl", "dispatch",
                    f'hl.dsp.send_shortcut({{ mods = "{mods}", key = "{key}", '
                    f'window = "address:{address}" }})'],
                   check=True, capture_output=True)


def settle(pid, began, before):
    """Waits until the process has used no processor time for SETTLE
    seconds; gives (seconds until the work was done, processor seconds)."""
    last_cpu, last_change = before, time.monotonic()
    samples = [(0.0, before)]
    while time.monotonic() - began < LIMIT:
        now_cpu = cpu(tree(pid))
        now = time.monotonic()
        samples.append((now - began, now_cpu))
        if now_cpu != last_cpu:
            last_cpu, last_change = now_cpu, now
        elif now - last_change >= SETTLE:
            break
        time.sleep(0.02)
    return ready(samples), (last_cpu - before) / TICK


def run(label, binary, doc, scratch):
    for d in ("config", "data", "cache", "state"):
        os.makedirs(os.path.join(scratch, d), exist_ok=True)
    pidfile = os.path.join(scratch, "pid")
    # CACHE keeps one cache folder across runs, so that the GPU driver's
    # shader cache is warm, as it is for anyone who has run pdvim before.
    cache = os.environ.get("CACHE") or f"{scratch}/cache"
    os.makedirs(cache, exist_ok=True)
    env = (f"XDG_CONFIG_HOME={scratch}/config XDG_DATA_HOME={scratch}/data "
           f"XDG_CACHE_HOME={cache} XDG_STATE_HOME={scratch}/state")
    inner = f"echo $$ > {pidfile}; exec env {env} {binary} {doc} >{scratch}/log 2>&1"
    start = time.monotonic()
    subprocess.run(["hyprctl", "dispatch",
                    f'hl.dsp.exec_cmd("sh -c \'{inner}\'", '
                    '{ float = true, size = "' + SIZE + '", workspace = "1 silent" })'],
                   check=True, capture_output=True)
    while not os.path.exists(pidfile) or not open(pidfile).read().strip():
        time.sleep(0.002)
    pid = int(open(pidfile).read())
    shown = None
    last_cpu, last_change = -1, time.monotonic()
    samples = []
    while time.monotonic() - start < LIMIT:
        pids = tree(pid)
        if shown is None and window(pids):
            shown = time.monotonic() - start
        now_cpu = cpu(pids)
        now = time.monotonic()
        samples.append((now - start, now_cpu))
        if now_cpu != last_cpu:
            last_cpu, last_change = now_cpu, now
        elif shown is not None and now - last_change >= SETTLE:
            break
        time.sleep(0.02)
    pids = tree(pid)
    found = window(pids)
    result = {
        "label": label, "doc": os.path.basename(doc),
        "window_s": shown, "ready_s": ready(samples),
        "open_cpu_s": last_cpu / TICK, "open_pss_mb": pss_kb(pids) / 1024,
        "threads": threads(pids), "title": found["title"] if found else None,
    }
    if found:
        address = found["address"]
        before = cpu(tree(pid))
        time.sleep(IDLE)
        result["idle_cpu_s"] = (cpu(tree(pid)) - before) / TICK

        before, began = cpu(tree(pid)), time.monotonic()
        for _ in range(SCROLL):
            press(address, "", "j")
        result["scroll_s"], result["scroll_cpu_s"] = settle(pid, began, before)

        before, began = cpu(tree(pid)), time.monotonic()
        for _ in range(PAGES):
            press(address, "CTRL", "f")
        result["page_s"], result["page_cpu_s"] = settle(pid, began, before)

        before, began = cpu(tree(pid)), time.monotonic()
        press(address, "SHIFT", "g")
        result["jump_s"], result["jump_cpu_s"] = settle(pid, began, before)
        result["end_pss_mb"] = pss_kb(tree(pid)) / 1024
        result["end_title"] = (window(tree(pid)) or {}).get("title")
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
    time.sleep(0.5)
    return result


if __name__ == "__main__":
    label, binary, doc, runs = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[3]), int(sys.argv[4])
    base = os.environ.get("SCRATCH", "/tmp")
    for i in range(runs):
        scratch = os.path.join(base, f"{label}-{os.path.basename(doc)}-{i}")
        shutil.rmtree(scratch, ignore_errors=True)
        print(json.dumps(run(label, binary, doc, scratch)), flush=True)
