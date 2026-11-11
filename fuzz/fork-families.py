"""Groups the inputs a fuzzing run left behind by the stack that failed, and
keeps one input per family (ADR-0024, section 5). Standard library only.

    python3 fork-families.py <target> <artifacts dir> <kept dir> <log dir> <repository dir>

Every input (oom-, crash-, timeout-, leak-...) is run again alone by the
target, with function names, and its family is the place it failed: the
panic's location, or the first two frames of our code (any file under
<repository dir>) or of a library (not of Rust's standard library, of
libFuzzer or of the out-of-memory hook). The smallest input of each family
goes to <kept dir>/<NN>-<kind>/ with the text of its run; the others are
discarded. The families, with how many inputs each had and their class, are
printed (the job log) and written to <kept dir>/families.tsv.

Each family gets a class (owner, 2026-11-11; audit 2026-11-10, I1):
- outside-verifier: a stack of its run passes through a function listed in
  before-verifier.txt, code the program runs itself before the verifier.
  Always red, even if the family is in known-families.txt;
- known: its key is in known-families.txt;
- new: any other family;
- unclassified: the input did not fail when run again.
A crash or timeout input not run again is unclassified too; out-of-memory
inputs not run again are only counted.

The reserve (<kept dir>/reserve/, owner, 2026-11-11; audit 2026-11-10, M6)
keeps every input of a new, outside-verifier or unclassified family, every
input not run again, and every input of a known family, except out-of-memory
ones: at most KNOWN_OOM_KEPT of those per family, the rest counted on the
"reserve:" line.

Runs again at most MAX_REPLAYS inputs and stops after REPLAY_SECONDS, so the
job keeps to its time limit; the inputs not run again are counted.
Exit code 0 when every family is known (or there is none), 3 when there is a
new, outside-verifier or unclassified one, with a last line "verdict: red:"
giving each reason; any other code means the grouping could not run (2 for
a bad argument or list, 1 for an error of Python itself).
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
KNOWN_OOM_KEPT = 5

HERE = os.path.dirname(os.path.abspath(__file__))
KNOWN_FILE = os.path.join(HERE, "known-families.txt")
BEFORE_FILE = os.path.join(HERE, "before-verifier.txt")
RED_CLASSES = ("new", "outside-verifier", "unclassified")
# Not 1: Python itself exits with 1 on an uncaught error, which must read as
# "the grouping failed", never as a verdict.
RED = 3

SKIP_FUNCTIONS = (
    "std::", "core::", "alloc::", "<std::", "<core::", "<alloc::", "__rust",
    "rust_", "__rg_", "__rdl_", "__sanitizer", "__asan", "__interceptor",
    "__lsan", "fuzzer::", "<fuzzer::", "LLVMFuzzer", "__libc", "_start",
)


def read_list(path, fields):
    """The lines of a list file that are not comments, each split by tabs
    into exactly `fields` fields. A malformed line stops the grouping."""
    rows = []
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.rstrip("\r\n")
            if not line.strip() or line.startswith("#"):
                continue
            row = line.split("\t")
            if len(row) != fields or not all(c.strip() for c in row):
                raise ValueError(f"{path}:{n}: expected {fields} tab-separated fields")
            rows.append(row)
    return rows


def in_repo(path, repo):
    return path.startswith(repo + "/")


def short_path(path, repo):
    if in_repo(path, repo):
        return path[len(repo) + 1:]
    for pattern, prefix in (
        (r"registry/src/[^/]+/(.+)$", ""),
        (r"/library/(.+)$", "rust/"),
        (r"/checkouts/[^/]+/[^/]+/(.+)$", ""),
    ):
        m = re.search(pattern, path)
        if m:
            return prefix + m.group(1)
    return path


def is_ours_or_a_library(func, path, repo):
    """A frame that says where the failure is: not the standard library,
    not libFuzzer, not the hook that turns a failed allocation into an
    out-of-memory."""
    if func.startswith(SKIP_FUNCTIONS):
        return False
    if not path or "/library/" in path or "/rustc/" in path:
        return False
    if "libfuzzer" in path or "oom_hook.rs" in path:
        return False
    return "registry/src/" in path or in_repo(path, repo) or "/checkouts/" in path


def passes_through(text, functions):
    """Whether a frame of any stack in `text` is in one of `functions`
    (crate::name; generics, closures, hashes and crate disambiguators of
    the frame are ignored)."""
    for m in re.finditer(r"^\s*#\d+ 0x[0-9a-f]+ in (\S+)", text, re.M):
        func = re.sub(r"\[[0-9a-f]+\]", "", m.group(1))
        for name in functions:
            if func == name or (func.startswith(name) and func[len(name)] in ":<"):
                return True
    return False


def stack_after(text, marker):
    """The first sanitizer stack after the line holding `marker`: a list of
    (function, path:line). Frames without a name are kept as ("?", "")."""
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


def where(frames, repo):
    kept = [f for f in frames if is_ours_or_a_library(*f, repo)]
    return " <- ".join(short_path(f[1], repo) for f in kept[:2]) or "no frame of ours or of a library"


def classify(text, prefix, repo):
    """(kind, family)."""
    m = re.search(r"panicked at (\S+?):(\d+):\d+:\n(.*)", text)
    if m:
        message = re.sub(r"\d+", "N", m.group(3).strip())[:100]
        return "panic", f"panic at {short_path(m.group(1), repo)}:{m.group(2)}: {message}"
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
            return kind, f"{kind} in {where(stack_after(text, marker), repo)}"
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
    if len(sys.argv) != 6:
        print(__doc__)
        return 2
    target, art, kept, log_dir, repo = sys.argv[1:]
    repo = repo.rstrip("/")
    try:
        known = {row[0] for row in read_list(KNOWN_FILE, 4)}
        before = [row[0] for row in read_list(BEFORE_FILE, 1)]
    except (OSError, ValueError) as e:
        print(f"families: {e}")
        return 2
    if not before:
        print(f"families: {BEFORE_FILE} lists no function")
        return 2
    binary = f"target/x86_64-unknown-linux-gnu/release/{target}"
    files = sorted(f for f in glob.glob(f"{art}/*") if os.path.isfile(f))
    files = [f for f in files if not os.path.basename(f).startswith("slow-unit-")]
    os.makedirs(kept, exist_ok=True)
    header = "family\tkind\tinputs\tkept\tlen\tsha1\twhere\tclass\n"
    if not files:
        print(f"families: no input to group for {target}")
        open(f"{kept}/families.tsv", "w").write(header)
        print("verdict: green")
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
    replayed = set()
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
                replayed.add(path)
                prefix = os.path.basename(path).split("-")[0]
                kind, family = classify(text, prefix, repo)
                fam = families.get(family)
                if fam is None:
                    if passes_through(text, before):
                        cls = "outside-verifier"
                    elif kind == "no-repro":
                        cls = "unclassified"
                    else:
                        cls = "known" if family in known else "new"
                    fam = families[family] = {
                        "kind": kind, "class": cls, "rep": path, "text": text, "count": 0,
                        "rep_prefix": prefix, "members": [],
                    }
                else:
                    # One input of the family through the code before the
                    # verifier makes the whole family red.
                    if passes_through(text, before):
                        fam["class"] = "outside-verifier"
                    if os.path.getsize(path) < os.path.getsize(fam["rep"]):
                        fam["rep"], fam["text"] = path, text
                fam["count"] += 1
                fam["members"].append(path)
    failures_not_replayed = len([f for f in failures if f not in replayed])
    ooms_not_replayed = len(files) - len(replayed) - failures_not_replayed

    rows = [header.rstrip("\n")]
    print(f"families of {target}: {len(families)} from {len(replayed)} inputs run again "
          f"({len(files) - len(replayed)} not run again; symbolizer: {sym})")
    for i, (family, fam) in enumerate(families.items(), 1):
        d = os.path.join(kept, f"{i:02d}-{fam['kind']}")
        os.makedirs(d, exist_ok=True)
        name = os.path.basename(fam["rep"])
        shutil.copy(fam["rep"], os.path.join(d, name))
        with open(os.path.join(d, "replay.txt"), "w", encoding="utf-8") as f:
            f.write(fam["text"])
        data = open(fam["rep"], "rb").read()
        rows.append(f"{i}\t{fam['kind']}\t{fam['count']}\t{name}\t{len(data)}\t"
                    f"{hashlib.sha1(data).hexdigest()}\t{family}\t{fam['class']}")
        print(f"  family {i} [{fam['class']}]: {fam['count']} input(s), "
              f"kept {name} ({len(data)} bytes): {family}")
    if failures_not_replayed:
        rows.append(f"-\tnot-run-again\t{failures_not_replayed}\t-\t-\t-\t"
                    f"crash or timeout inputs over the limit of time\tunclassified")
    if ooms_not_replayed:
        rows.append(f"-\tnot-run-again\t{ooms_not_replayed}\t-\t-\t-\t"
                    f"out-of-memory inputs over the limit of runs or of time\tcounted")
    with open(os.path.join(kept, "families.tsv"), "w", encoding="utf-8") as f:
        f.write("\n".join(rows) + "\n")
    shutil.copy(os.path.join(kept, "families.tsv"), os.path.join(log_dir, "families.tsv"))

    reserve = os.path.join(kept, "reserve")
    os.makedirs(reserve, exist_ok=True)
    to_keep = [f for f in files if f not in replayed]
    left_out = 0
    for fam in families.values():
        members = sorted(fam["members"], key=os.path.getsize)
        if fam["class"] == "known" and fam["kind"].startswith("oom"):
            left_out += max(0, len(members) - KNOWN_OOM_KEPT)
            members = members[:KNOWN_OOM_KEPT]
        to_keep += members
    for f in to_keep:
        shutil.copy(f, reserve)
    print(f"reserve: kept {len(to_keep)} input(s); left out {left_out} out-of-memory "
          f"input(s) of known families (over {KNOWN_OOM_KEPT} per family)")

    count = collections.Counter(fam["class"] for fam in families.values())
    count["unclassified"] += failures_not_replayed
    print("classes: " + " ".join(
        f"{c}={count[c]}" for c in ("known",) + RED_CLASSES
    ) + f" out-of-memory-not-run-again={ooms_not_replayed}")
    reasons = red_reasons(families, failures_not_replayed, known)
    if reasons:
        print("verdict: red: " + "; ".join(reasons))
        return RED
    print("verdict: green")
    return 0


def red_reasons(families, failures_not_replayed, known):
    """Why the run is red, one sentence per case (owner, 2026-11-11), so the
    job says the right reason."""
    def inputs(cls, kinds=None):
        return sum(f["count"] for f in families.values()
                   if f["class"] == cls and (kinds is None or f["rep_prefix"] in kinds))

    reasons = []
    n = sum(1 for f in families.values() if f["class"] == "outside-verifier")
    if n:
        reasons.append(f"{n} family(ies) failed in code the program runs before the "
                       "verifier (fuzz/before-verifier.txt): always red")
    n = sum(1 for f in families.values() if f["class"] == "new")
    if n:
        reasons.append(f"{n} new family(ies): the failure happened again but is not in "
                       "fuzz/known-families.txt"
                       + (" (the list is still empty)" if not known else ""))
    n = inputs("unclassified", ("oom",))
    if n:
        reasons.append(f"{n} out-of-memory input(s) did not run out of memory when run "
                       "again: family unknown")
    n = inputs("unclassified") - n
    if n:
        reasons.append(f"{n} crash or timeout input(s) did not fail when run again: "
                       "family unknown")
    if failures_not_replayed:
        reasons.append(f"{failures_not_replayed} crash or timeout input(s) were not run "
                       "again (time limit): family unknown")
    return reasons


if __name__ == "__main__":
    sys.exit(main())
