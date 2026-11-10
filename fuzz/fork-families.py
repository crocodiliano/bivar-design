"""Groups the inputs a fuzzing run left behind by the stack that failed, and
keeps one input per family (ADR-0024, section 5). Standard library only.

    python3 fork-families.py <target> <artifacts dir> <kept dir> <log dir>

Every input (oom-, crash-, timeout-, leak-...) is run again alone by the
target, with function names, and its family is the place it failed: the
panic's location, or the first two frames of our code or of a library (not
of Rust's standard library, of libFuzzer or of the out-of-memory hook). The
smallest input of each family goes to <kept dir>/<NN>-<kind>/ with the text
of its run; the others are discarded. The families, with how many inputs
each had, are printed (the job log) and written to <kept dir>/families.tsv.

Runs again at most MAX_REPLAYS inputs and stops after REPLAY_SECONDS, so the
job keeps to its time limit; the inputs not run again are counted.
Exit code 0 when the grouping worked, 2 when it could not run.
"""

import collections
import concurrent.futures
import glob
import hashlib
import os
import random
import re
import shutil
import subprocess
import sys
import time

MAX_REPLAYS = 300
REPLAY_SECONDS = 150
ONE_REPLAY_SECONDS = 90

SKIP_FUNCTIONS = (
    "std::", "core::", "alloc::", "<std::", "<core::", "<alloc::", "__rust",
    "rust_", "__rg_", "__rdl_", "__sanitizer", "__asan", "__interceptor",
    "__lsan", "fuzzer::", "<fuzzer::", "LLVMFuzzer", "__libc", "_start",
)


def short_path(path):
    for pattern, prefix in (
        (r"registry/src/[^/]+/(.+)$", ""),
        (r"/library/(.+)$", "rust/"),
        (r"/checkouts/[^/]+/[^/]+/(.+)$", ""),
        (r"bivar-design/(.+)$", ""),
    ):
        m = re.search(pattern, path)
        if m:
            return prefix + m.group(1)
    return path


def is_ours_or_a_library(func, path):
    """A frame that says where the failure is: not the standard library,
    not libFuzzer, not the hook that turns a failed allocation into an
    out-of-memory."""
    if func.startswith(SKIP_FUNCTIONS):
        return False
    if not path or "/library/" in path or "/rustc/" in path:
        return False
    if "libfuzzer" in path or "oom_hook.rs" in path or "fuzz_targets/" in path:
        return False
    return "registry/src/" in path or "bivar-design/" in path or "/checkouts/" in path


def stack_after(text, marker):
    """The first sanitizer stack after the line holding `marker`: a list of
    (function, path:line)."""
    idx = text.find(marker)
    if idx < 0:
        return []
    frames = []
    for line in text[idx:].splitlines()[1:]:
        m = re.match(r"^\s*#(\d+) 0x[0-9a-f]+ in (.+?) (/\S+)\s*$", line)
        if m:
            if m.group(1) == "0" and frames:
                break  # a second stack
            # path:line:column -> path:line
            loc = re.sub(r"^(.+?:\d+):\d+$", r"\1", m.group(3))
            frames.append((re.sub(r"::h[0-9a-f]{16}$", "", m.group(2)), loc))
        elif re.match(r"^\s*#\d+ 0x", line):
            if line.strip().startswith("#0") and frames:
                break
            frames.append(("?", ""))
        elif frames and not line.strip():
            break
    return frames


def where(frames):
    kept = [f for f in frames if is_ours_or_a_library(*f)]
    return " <- ".join(short_path(f[1]) for f in kept[:2]) or "no frame of ours or of a library"


def classify(text, prefix):
    """(kind, family)."""
    m = re.search(r"panicked at (\S+?):(\d+):\d+:\n(.*)", text)
    if m:
        message = re.sub(r"\d+", "N", m.group(3).strip())[:100]
        return "panic", f"panic at {short_path(m.group(1))}:{m.group(2)}: {message}"
    for marker, kind in (
        ("ERROR: libFuzzer: out-of-memory (malloc(", "oom"),
        ("ERROR: libFuzzer: out-of-memory (used:", "oom-rss"),
        ("ERROR: libFuzzer: timeout", "timeout"),
        ("ERROR: AddressSanitizer:", "asan"),
        ("ERROR: LeakSanitizer", "leak"),
        ("ERROR: libFuzzer: deadly signal", "signal"),
    ):
        if marker in text:
            if kind == "asan":
                m = re.search(r"ERROR: AddressSanitizer: ([A-Za-z0-9_-]+)", text)
                kind = "asan-" + (m.group(1) if m else "?")
            if kind == "oom" and "BIVAR-FUZZ: allocation of" in text:
                kind = "oom-request-failed"
            return kind, f"{kind} in {where(stack_after(text, marker))}"
    # The run did not fail again: kept as its own family, by its file kind.
    return "no-repro", f"no-repro ({prefix}): the input did not fail when run again"


def symbolizer():
    found = shutil.which("llvm-symbolizer")
    if found:
        return found
    for c in sorted(glob.glob("/usr/lib/llvm-*/bin/llvm-symbolizer"), reverse=True):
        if os.path.exists(c):
            return c
    return None


def replay(binary, path, env):
    try:
        p = subprocess.run(
            [binary, "-rss_limit_mb=4096", "-timeout=60", path],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            timeout=ONE_REPLAY_SECONDS,
        )
        return path, p.stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired as e:
        return path, (e.stdout or b"").decode("utf-8", "replace") + "\nREPLAY-WALL-TIMEOUT"


def main():
    if len(sys.argv) != 5:
        print(__doc__)
        return 2
    target, art, kept, log_dir = sys.argv[1:]
    binary = f"target/x86_64-unknown-linux-gnu/release/{target}"
    files = sorted(f for f in glob.glob(f"{art}/*") if os.path.isfile(f))
    files = [f for f in files if not os.path.basename(f).startswith("slow-unit-")]
    os.makedirs(kept, exist_ok=True)
    if not files:
        print(f"families: no input to group for {target}")
        open(f"{kept}/families.tsv", "w").write("family\tkind\tinputs\tkept\tlen\tsha1\twhere\n")
        return 0
    if not os.path.exists(binary):
        print(f"families: {binary} is missing")
        return 2

    # Crashes and timeouts first (each stopped the run), then a sample of
    # the out-of-memory inputs, smallest first.
    failures = [f for f in files if not os.path.basename(f).startswith("oom-")]
    ooms = [f for f in files if os.path.basename(f).startswith("oom-")]
    if len(ooms) > MAX_REPLAYS:
        ooms = random.Random(len(ooms)).sample(ooms, MAX_REPLAYS)
    todo = failures + sorted(ooms, key=os.path.getsize)

    env = dict(os.environ)
    env["RUST_BACKTRACE"] = "0"
    asan = "allocator_may_return_null=1:symbolize=1"
    sym = symbolizer()
    if sym:
        env["ASAN_SYMBOLIZER_PATH"] = sym
        asan += f":external_symbolizer_path={sym}"
    env["ASAN_OPTIONS"] = asan

    families = collections.OrderedDict()
    deadline = time.monotonic() + REPLAY_SECONDS
    done = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        pending = {}
        queue = list(todo)
        while queue or pending:
            while queue and len(pending) < 2 and time.monotonic() < deadline:
                path = queue.pop(0)
                pending[pool.submit(replay, binary, path, env)] = path
            if not pending:
                break
            finished, _ = concurrent.futures.wait(
                pending, return_when=concurrent.futures.FIRST_COMPLETED
            )
            for fut in finished:
                path = pending.pop(fut)
                _, text = fut.result()
                done += 1
                prefix = os.path.basename(path).split("-")[0]
                kind, family = classify(text, prefix)
                fam = families.get(family)
                if fam is None:
                    fam = families[family] = {"kind": kind, "rep": path, "text": text, "count": 0}
                elif os.path.getsize(path) < os.path.getsize(fam["rep"]):
                    fam["rep"], fam["text"] = path, text
                fam["count"] += 1
    not_replayed = len(files) - done

    rows = ["family\tkind\tinputs\tkept\tlen\tsha1\twhere"]
    print(f"families of {target}: {len(families)} from {done} inputs run again "
          f"({not_replayed} not run again; symbolizer: {sym})")
    for i, (family, fam) in enumerate(families.items(), 1):
        d = os.path.join(kept, f"{i:02d}-{fam['kind']}")
        os.makedirs(d, exist_ok=True)
        name = os.path.basename(fam["rep"])
        shutil.copy(fam["rep"], os.path.join(d, name))
        with open(os.path.join(d, "replay.txt"), "w", encoding="utf-8") as f:
            f.write(fam["text"])
        data = open(fam["rep"], "rb").read()
        rows.append(f"{i}\t{fam['kind']}\t{fam['count']}\t{name}\t{len(data)}\t"
                    f"{hashlib.sha1(data).hexdigest()}\t{family}")
        print(f"  family {i}: {fam['count']} input(s), kept {name} ({len(data)} bytes): {family}")
    if not_replayed:
        rows.append(f"-\tnot-run-again\t{not_replayed}\t-\t-\t-\tover the limit of runs or of time")
    with open(os.path.join(kept, "families.tsv"), "w", encoding="utf-8") as f:
        f.write("\n".join(rows) + "\n")
    shutil.copy(os.path.join(kept, "families.tsv"), os.path.join(log_dir, "families.tsv"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
