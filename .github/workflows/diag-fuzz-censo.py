"""TEMPORARY (branch diag/fuzz-censo, never merged).

Run from fuzz/ after diag-fuzz-censo.sh. Replays every input the census kept
(crash-, oom-, timeout-), one process each, groups them by the stack that
failed, keeps one input per family in census-out/families/ and writes the
annotations (readable without logging in) to census-out/ann-*.txt.
Standard library only.
"""

import base64
import collections
import concurrent.futures
import glob
import hashlib
import os
import random
import re
import shutil
import subprocess

OUT = "census-out"
ART = "artifacts/bivar_records"
BIN = "target/x86_64-unknown-linux-gnu/release/bivar_records"
REAL_SHA1 = "c6869029ad0329fc11298f77b7f2ea0fe497bf61"
REAL_BYTES = "1747498019256"
MAX_OOM_REPLAYS = 1500
MAX_TIMEOUT_REPLAYS = 6
MAX_B64_INPUT = 9000

SKIP = (
    "std::", "core::", "alloc::", "<std::", "<core::", "<alloc::", "__rust",
    "rust_", "__rg_", "__rdl_", "__sanitizer", "__asan", "__interceptor",
    "__lsan", "malloc", "calloc", "realloc", "free", "operator new",
    "fuzzer::", "<fuzzer::", "LLVMFuzzer", "libfuzzer_sys", "<libfuzzer_sys",
    "__libc", "_start", "main", "bivar_records::oom_hook",
    "<bivar_records::oom_hook", "backtrace::", "rust_begin_unwind",
    "panic_unwind", "__GI_", "abort", "raise", "pthread_", "start_thread",
    "clone", "bivar_records::_::", "bivar_records::__",
)


def strip_hash(name):
    name = re.sub(r"::h[0-9a-f]{16}$", "", name.strip())
    return name


def short_path(path):
    m = re.search(r"registry/src/[^/]+/(.+)$", path)
    if m:
        return m.group(1)
    m = re.search(r"/library/(.+)$", path)
    if m:
        return "rust/" + m.group(1)
    m = re.search(r"bivar-design/(.+)$", path)
    if m:
        return m.group(1)
    return path


def rust_frames(text):
    """Frames of a Rust backtrace (std::backtrace or the panic hook)."""
    frames = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = re.match(r"^\s*\d+: (.+)$", line)
        if not m:
            continue
        loc = ""
        if i + 1 < len(lines):
            m2 = re.match(r"^\s+at (.+)$", lines[i + 1])
            if m2:
                loc = short_path(m2.group(1))
        frames.append((strip_hash(m.group(1)), loc))
    return frames


def sanitizer_frames(text, after=None):
    """Frames of the first sanitizer stack (after the line containing
    `after`, when given)."""
    start = 0
    if after is not None:
        idx = text.find(after)
        if idx >= 0:
            start = idx
    frames = []
    seen_any = False
    for line in text[start:].splitlines():
        m = re.match(r"^\s*#(\d+) 0x[0-9a-f]+ in (.+?)(?: (/\S+|\(\S+\)))?\s*$", line)
        if m:
            if m.group(1) == "0" and seen_any:
                break  # second stack
            seen_any = True
            frames.append((strip_hash(m.group(2)), short_path(m.group(3) or "")))
        elif seen_any and frames and line.strip() == "":
            break
    return frames


def useful(frames):
    return [f for f in frames if not f[0].startswith(SKIP)]


def crate_of(func, loc):
    name = func.lstrip("<")
    m = re.match(r"([A-Za-z0-9_]+)::", name)
    if m:
        return m.group(1)
    if loc:
        return loc.split("/")[0]
    return "?"


def classify(text, rc):
    """(kind, signature, origin crate, Bivar frame, detail)."""
    m = re.search(r"panicked at (\S+?):(\d+):\d+:\n(.*)", text)
    if m:
        loc = short_path(m.group(1)) + ":" + m.group(2)
        msg = re.sub(r"\d+", "N", m.group(3).strip())[:100]
        frames = useful(rust_frames(text))
        via = next((f[0] for f in frames if f[0].startswith(("bivar_poc", "<bivar_poc"))), "")
        origin = loc.split("/")[0]
        return "panic", f"panic {loc} | {msg}", origin, via, ""
    m = re.search(r"BIVAR-FUZZ: allocation of (\d+) bytes failed", text)
    if m:
        bt = text.split("BIVAR-FUZZ-BACKTRACE-BEGIN", 1)
        frames = useful(rust_frames(bt[1])) if len(bt) == 2 else []
        if not frames:
            frames = useful(sanitizer_frames(text, "out-of-memory (malloc("))
        top = frames[:2]
        via = next((f[0] for f in frames if f[0].startswith(("bivar_poc", "<bivar_poc"))), "")
        origin = crate_of(*top[0]) if top else "?"
        sig = "alloc-fail " + " <- ".join(f"{f[0]} [{f[1]}]" for f in top)
        return "alloc-fail", sig, origin, via, m.group(1)
    for marker, kind in (
        ("out-of-memory (malloc(", "oom-malloc"),
        ("out-of-memory (used:", "oom-rss"),
        ("ERROR: libFuzzer: timeout", "timeout"),
        ("ERROR: AddressSanitizer:", "asan"),
        ("ERROR: LeakSanitizer", "leak"),
        ("ERROR: libFuzzer: deadly signal", "signal"),
    ):
        if marker in text:
            frames = useful(sanitizer_frames(text, marker))
            if kind == "asan":
                m = re.search(r"ERROR: AddressSanitizer: ([A-Za-z0-9_-]+)", text)
                kind = "asan-" + (m.group(1) if m else "?")
            top = frames[:2]
            via = next((f[0] for f in frames if f[0].startswith(("bivar_poc", "<bivar_poc"))), "")
            origin = crate_of(*top[0]) if top else "?"
            sig = kind + " " + " <- ".join(f"{f[0]} [{f[1]}]" for f in top)
            return kind, sig, origin, via, ""
    return "no-repro", f"no-repro rc={rc}", "-", "", ""


def symbolizer():
    for c in [shutil.which("llvm-symbolizer")] + sorted(glob.glob("/usr/lib/llvm-*/bin/llvm-symbolizer"), reverse=True):
        if c and os.path.exists(c):
            return c
    return None


def replay(path, env):
    try:
        p = subprocess.run(
            [BIN, "-rss_limit_mb=4096", "-timeout=60", path],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=150,
        )
        return path, p.stdout.decode("utf-8", "replace"), p.returncode
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b"").decode("utf-8", "replace")
        return path, out + "\nREPLAY-WALL-TIMEOUT", -1


def parse_parent_log():
    stats = []
    seeds = None
    try:
        log = open(f"{OUT}/fuzz.log", encoding="utf-8", errors="replace").read()
    except OSError:
        return stats, seeds, ""
    for m in re.finditer(
        r"^#(\d+): cov: (\d+) ft: (\d+) corp: (\d+) exec/s: (\d+) oom/timeout/crash: (\d+)/(\d+)/(\d+) time: (\d+)s job: (\d+)",
        log, re.M,
    ):
        stats.append(tuple(int(x) for x in m.groups()))
    m = re.search(r"INFO: -fork=\d+: (\d+) seed inputs", log)
    if m:
        seeds = int(m.group(1))
    wrap = "wrapping up soon" in log
    exit_m = re.findall(r"INFO: exiting: (\d+)", log)
    return stats, seeds, f"wrapped={wrap} exiting={exit_m[-1] if exit_m else 'none'}"


def esc(s):
    return s.replace("%", "%25").replace("\r", "").replace("\n", " ")


def main():
    os.makedirs(f"{OUT}/families", exist_ok=True)
    files = sorted(glob.glob(f"{ART}/*"))
    by_prefix = collections.Counter(os.path.basename(f).split("-")[0] for f in files)
    crash = [f for f in files if os.path.basename(f).startswith("crash-")]
    oom = [f for f in files if os.path.basename(f).startswith("oom-")]
    tmo = sorted((f for f in files if os.path.basename(f).startswith("timeout-")), key=os.path.getsize)
    rnd = random.Random(20261110)
    oom_sel = oom if len(oom) <= MAX_OOM_REPLAYS else rnd.sample(oom, MAX_OOM_REPLAYS)
    # The real input first, when it was kept.
    real = [f for f in oom + crash if REAL_SHA1 in f]
    todo = list(dict.fromkeys(real + crash + oom_sel + tmo[:MAX_TIMEOUT_REPLAYS]))

    env = dict(os.environ)
    env.pop("BIVAR_FUZZ_OOM_LOG", None)
    env["RUST_BACKTRACE"] = "1"
    env["BIVAR_FUZZ_OOM_BT"] = "1"
    sym = symbolizer()
    asan = "allocator_may_return_null=1:symbolize=1"
    if sym:
        env["ASAN_SYMBOLIZER_PATH"] = sym
        asan += f":external_symbolizer_path={sym}"
    env["ASAN_OPTIONS"] = asan

    fams = collections.OrderedDict()
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as ex:
        for path, text, rc in ex.map(lambda p: replay(p, env), todo):
            kind, sig, origin, via, detail = classify(text, rc)
            prefix = os.path.basename(path).split("-")[0]
            fam = fams.get(sig)
            if fam is None:
                fam = fams[sig] = {
                    "kind": kind, "origin": origin, "via": via, "rep": path,
                    "text": text, "count": collections.Counter(), "sizes": set(),
                }
            elif os.path.getsize(path) < os.path.getsize(fam["rep"]):
                fam["rep"], fam["text"] = path, text  # keep the smallest
            fam["count"][prefix] += 1
            if detail:
                fam["sizes"].add(int(detail))

    rows = []
    for i, (sig, fam) in enumerate(fams.items(), 1):
        d = f"{OUT}/families/{i:02d}-{fam['kind']}"
        os.makedirs(d, exist_ok=True)
        shutil.copy(fam["rep"], d)
        with open(f"{d}/replay.txt", "w", encoding="utf-8") as f:
            f.write(fam["text"])
        data = open(fam["rep"], "rb").read()
        sizes = sorted(fam["sizes"])
        rows.append({
            "id": i, "kind": fam["kind"], "origin": fam["origin"], "via": fam["via"],
            "count": dict(fam["count"]), "sig": sig, "rep": os.path.basename(fam["rep"]),
            "len": len(data), "sha1": hashlib.sha1(data).hexdigest(), "data": data,
            "alloc": f"{sizes[0]}..{sizes[-1]}" if sizes else "",
            "real": REAL_SHA1 in fam["rep"] or any(REAL_SHA1 in p for p in [fam["rep"]]),
        })

    with open(f"{OUT}/families.tsv", "w", encoding="utf-8") as f:
        f.write("id\tkind\torigin\tcount\tinput\tlen\tsha1\talloc_bytes\tvia\tsignature\n")
        for r in rows:
            f.write(f"{r['id']}\t{r['kind']}\t{r['origin']}\t{r['count']}\t{r['rep']}\t{r['len']}\t{r['sha1']}\t{r['alloc']}\t{r['via']}\t{r['sig']}\n")

    stats, seeds, endinfo = parse_parent_log()
    hook = []
    try:
        hook = open(f"{OUT}/oom-hook.txt").read().split()
    except OSError:
        pass
    hook_total = sum(1 for h in hook if h.startswith("bytes="))
    hook_real = sum(1 for h in hook if h == f"bytes={REAL_BYTES}")
    real_kept = [os.path.basename(f) for f in files if REAL_SHA1 in f]
    timeinfo = open(f"{OUT}/time.txt").read().strip() if os.path.exists(f"{OUT}/time.txt") else "?"

    samples = []
    for t in (120, 231, 600, 1200, 1800):
        s = next((x for x in stats if x[8] >= t), None)
        if s:
            samples.append(f"t={s[8]}s runs={s[0]} cov={s[1]} ft={s[2]} corp={s[3]} exec/s={s[4]} o/t/c={s[5]}/{s[6]}/{s[7]} job={s[9]}")
    last = stats[-1] if stats else None
    lastinfo = (
        f"last: t={last[8]}s runs={last[0]} cov={last[1]} ft={last[2]} corp={last[3]} exec/s(job)={last[4]} oom/timeout/crash={last[5]}/{last[6]}/{last[7]} jobs={last[9]}"
        if last else "last: none"
    )
    summary = (
        f"CENSUS {timeinfo} {endinfo} seeds_after_merge={seeds} | {lastinfo} | "
        f"files_by_prefix={dict(by_prefix)} replayed={len(todo)} (oom sampled {len(oom_sel)}/{len(oom)}, timeouts {min(len(tmo), MAX_TIMEOUT_REPLAYS)}/{len(tmo)}) "
        f"families={len(rows)} symbolizer={sym} | hook_lines={hook_total} hook_lines_1.6TiB={hook_real} real_input_kept_as={real_kept}"
    )
    with open(f"{OUT}/summary.txt", "w", encoding="utf-8") as f:
        f.write(summary + "\n" + "\n".join(samples) + "\n")
    print(summary)
    print("\n".join(samples))
    for r in rows:
        print(r["id"], r["kind"], r["origin"], r["count"], r["len"], r["sha1"], r["alloc"], r["via"], r["sig"], sep=" | ")

    # Annotations: at most 10 per step.
    ann0 = [f"::notice title=census summary::{esc(summary)}", f"::notice title=census samples::{esc(' || '.join(samples))}"]
    chunk = ""
    n = 0
    for r in rows:
        line = f"F{r['id']} {r['kind']} origin={r['origin']} count={r['count']} len={r['len']} sha1={r['sha1']} alloc={r['alloc']} via={r['via']} sig={r['sig']} || "
        if len(chunk) + len(line) > 3800:
            ann0.append(f"::notice title=census families {n}::{esc(chunk)}")
            n += 1
            chunk = ""
        chunk += line
    if chunk:
        ann0.append(f"::notice title=census families {n}::{esc(chunk)}")
    open(f"{OUT}/ann-0.txt", "w").write("\n".join(ann0[:10]) + "\n")
    for k, part in ((1, rows[:10]), (2, rows[10:20])):
        lines = []
        for r in part:
            if r["len"] <= MAX_B64_INPUT:
                lines.append(f"::notice title=input F{r['id']} sha1={r['sha1']} len={r['len']}::{base64.b64encode(r['data']).decode()}")
            else:
                lines.append(f"::notice title=input F{r['id']} sha1={r['sha1']} len={r['len']}::too large; in the artifact")
        open(f"{OUT}/ann-{k}.txt", "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
