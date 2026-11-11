"""Tests of fork-families.py (treatment of the audit 2026-11-10, T2).
Standard library only. From the repository or from fuzz/:

    python3 fuzz/test_fork_families.py

The grouping runs on synthetic texts of the target's runs: `replay` is
replaced, everything after it (classes, files, exit code) is the real code.
The last test reads the prototype's source and checks before-verifier.txt.
"""

import importlib.util
import os
import re
import shutil
import sys
import tempfile
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PROTO = os.path.join(ROOT, "poc", "crdt-armazenamento", "prototipo")

spec = importlib.util.spec_from_file_location("fork_families", os.path.join(HERE, "fork-families.py"))
ff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ff)

CI_REPO = "/home/runner/work/bivar-design/bivar-design"
REG = "/home/runner/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f"


def frames(repo, *ours_or_libs):
    """A sanitizer stack: libFuzzer and allocator frames, then the given
    (function, path:line) frames, then the fuzz target."""
    lines = [
        "    #0 0x55d1 in __sanitizer_print_stack_trace /rustc/llvm/compiler-rt/lib/asan/asan_stack.cpp:87:3",
        f"    #1 0x55d2 in fuzzer::PrintStackTrace() {REG}/libfuzzer-sys-0.4.10/libfuzzer/FuzzerUtil.cpp:210:5",
        "    #2 0x55d3 in alloc::raw_vec::finish_grow /rustc/1a2b/library/alloc/src/raw_vec.rs:600:13",
    ]
    for i, (func, loc) in enumerate(ours_or_libs, 3):
        lines.append(f"    #{i} 0x55e{i} in {func} {loc}:7")
    n = 3 + len(ours_or_libs)
    lines.append(f"    #{n} 0x55f0 in bivar_records::_::__libfuzzer_sys_run {repo}/fuzz/fuzz_targets/bivar_records.rs:20:9")
    return "\n".join(lines) + "\n\n"


def oom_d1b(repo, *extra):
    return (
        "==1== ERROR: libFuzzer: out-of-memory (malloc(12884901884))\n"
        + frames(
            repo,
            *extra,
            ("loro_internal::oplog::change_store::block_meta_encode::decode_changes_header",
             f"{REG}/loro-internal-1.17.1/src/oplog/change_store/block_meta_encode.rs:132"),
            ("loro_internal::oplog::change_store::ChangesBlockBytes::parse",
             f"{REG}/loro-internal-1.17.1/src/oplog/change_store.rs:1900"),
            ("bivar_poc_arquivo::check_bytes::<bivar_poc_loja_loro::LoroStore>",
             f"{repo}/poc/crdt-armazenamento/prototipo/arquivo/src/lib.rs:609"),
        )
    )


def panic_text(repo, loc, message, *stack):
    return (
        f"thread '<unnamed>' panicked at {loc}:9:\n{message}\n"
        "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n"
        "==1== ERROR: libFuzzer: deadly signal\n" + frames(repo, *stack)
    )


def panic_d2(repo):
    return panic_text(
        repo, f"{REG}/loro-kv-store-1.17.1/src/block.rs:305",
        "range end index 9 out of range for slice of length 4",
        ("loro_kv_store::block::Block::decode", f"{REG}/loro-kv-store-1.17.1/src/block.rs:305"),
    )


SCAN = ("bivar_poc_arquivo::scan::<std::io::cursor::Cursor<&[u8]>>",
        f"{CI_REPO}/poc/crdt-armazenamento/prototipo/arquivo/src/lib.rs:470")
NO_REPRO = "INFO: Running with entropic power schedule\nExecuted crash-x in 3 ms\n"


class Grouping(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir)
        self.art = os.path.join(self.dir, "artifacts")
        self.kept = os.path.join(self.dir, "kept")
        self.log = os.path.join(self.dir, "log")
        for d in (self.art, self.log):
            os.makedirs(d)
        binary = os.path.join(self.dir, "target/x86_64-unknown-linux-gnu/release/bivar_records")
        os.makedirs(os.path.dirname(binary))
        open(binary, "w").close()
        self.known = os.path.join(self.dir, "known.txt")
        self.know()
        self.texts = {}

    def know(self, *keys):
        with open(self.known, "w", encoding="utf-8") as f:
            f.write("# test list\n")
            for k in keys:
                f.write(f"{k}\toom\tD1b\t2026-11-11\n")

    def input(self, name, text, size=10):
        with open(os.path.join(self.art, name), "wb") as f:
            f.write(b"x" * size)
        self.texts[name] = text

    def run_grouping(self, repo=CI_REPO, replay_seconds=ff.REPLAY_SECONDS):
        def fake_replay(binary, path, env):
            return path, self.texts[os.path.basename(path)]

        argv = ["fork-families.py", "bivar_records", self.art, self.kept, self.log, repo]
        cwd = os.getcwd()
        os.chdir(self.dir)
        try:
            with mock.patch.object(ff, "replay", fake_replay), \
                 mock.patch.object(ff, "KNOWN_FILE", self.known), \
                 mock.patch.object(sys, "argv", argv), \
                 mock.patch.object(ff, "REPLAY_SECONDS", replay_seconds), \
                 mock.patch("sys.stdout"):
                code = ff.main()
        finally:
            os.chdir(cwd)
        tsv = os.path.join(self.kept, "families.tsv")
        if not os.path.exists(tsv):
            return code, None
        with open(tsv, encoding="utf-8") as f:
            rows = [r.split("\t") for r in f.read().splitlines()[1:]]
        return code, rows

    def key(self, text, repo=CI_REPO):
        return ff.classify(text, "oom", repo)[1]

    def test_known_oom_family_is_known_and_green(self):
        self.know(self.key(oom_d1b(CI_REPO)))
        self.input("oom-a", oom_d1b(CI_REPO))
        self.input("oom-b", oom_d1b(CI_REPO), size=5)
        code, rows = self.run_grouping()
        self.assertEqual(code, 0)
        self.assertEqual([(r[2], r[3], r[7]) for r in rows], [("2", "oom-b", "known")])
        # The key, as it goes in known-families.txt, with Loro's version (D-2).
        self.assertEqual(
            rows[0][6],
            "oom in loro-internal-1.17.1/src/oplog/change_store/block_meta_encode.rs:132"
            " <- loro-internal-1.17.1/src/oplog/change_store.rs:1900",
        )

    def test_new_oom_family_is_new_and_red(self):
        self.know(self.key(oom_d1b(CI_REPO)))
        self.input("oom-a", oom_d1b(CI_REPO))
        other = oom_d1b(CI_REPO).replace("block_meta_encode.rs:132", "block_meta_encode.rs:140")
        self.input("oom-c", other)
        code, rows = self.run_grouping()
        self.assertEqual(code, 1)
        self.assertEqual(sorted(r[7] for r in rows), ["known", "new"])

    def test_known_d2_panic_is_known_and_green(self):
        key = ff.classify(panic_d2(CI_REPO), "crash", CI_REPO)[1]
        self.assertTrue(key.startswith("panic at loro-kv-store-1.17.1/src/block.rs:305: "), key)
        self.know(key)
        self.input("crash-d2", panic_d2(CI_REPO))
        code, rows = self.run_grouping()
        self.assertEqual(code, 0)
        self.assertEqual([(r[1], r[7]) for r in rows], [("panic", "known")])

    def test_new_panic_is_red(self):
        self.know(ff.classify(panic_d2(CI_REPO), "crash", CI_REPO)[1])
        self.input("crash-new", panic_text(
            CI_REPO, f"{REG}/loro-internal-1.17.1/src/state.rs:88", "called `Option::unwrap()` on a `None` value"))
        code, rows = self.run_grouping()
        self.assertEqual(code, 1)
        self.assertEqual([r[7] for r in rows], ["new"])

    def test_any_failure_through_scan_is_red_even_when_known(self):
        # The real before-verifier.txt, not a test copy.
        for name, text in (
            ("oom-scan", oom_d1b(CI_REPO, SCAN)),
            ("crash-scan", panic_text(
                CI_REPO, f"{CI_REPO}/poc/crdt-armazenamento/prototipo/arquivo/src/lib.rs:470",
                "attempt to add with overflow", SCAN)),
        ):
            with self.subTest(name):
                self.setUp()
                self.know(ff.classify(text, name.split("-")[0], CI_REPO)[1])
                self.input(name, text)
                code, rows = self.run_grouping()
                self.assertEqual(code, 1)
                self.assertEqual([r[7] for r in rows], ["outside-verifier"])

    def test_one_input_through_scan_makes_a_known_family_red(self):
        self.know(self.key(oom_d1b(CI_REPO)))
        self.input("oom-a", oom_d1b(CI_REPO))
        # Same first two frames (same family), with scan further down.
        self.input("oom-b", oom_d1b(CI_REPO).replace(
            "    #6 ", f"    #6 0x1 in {SCAN[0]} {SCAN[1]}:1\n    #6 ", 1))
        code, rows = self.run_grouping()
        self.assertEqual(code, 1)
        self.assertEqual([(r[2], r[7]) for r in rows], [("2", "outside-verifier")])

    def test_same_stack_in_a_folder_of_another_name_is_the_same_family(self):
        ours = ("bivar_poc_verificador::fuzz_records",
                "{repo}/poc/crdt-armazenamento/prototipo/verificador/src/lib.rs:432")

        def text(repo):
            return "==1== ERROR: libFuzzer: out-of-memory (malloc(99))\n" + frames(
                repo, (ours[0], ours[1].format(repo=repo)))

        other = "/tmp/x/checkout-named-otherwise"
        self.assertEqual(self.key(text(CI_REPO)), self.key(text(other), other))
        self.assertEqual(self.key(text(other), other),
                         "oom in poc/crdt-armazenamento/prototipo/verificador/src/lib.rs:432"
                         " <- fuzz/fuzz_targets/bivar_records.rs:20")
        self.know(self.key(text(CI_REPO)))
        self.input("oom-a", text(other))
        code, rows = self.run_grouping(repo=other + "/")
        self.assertEqual((code, rows[0][7]), (0, "known"))

    def test_crash_not_run_again_is_unclassified_and_red(self):
        self.input("crash-a", panic_d2(CI_REPO))
        self.input("timeout-b", panic_d2(CI_REPO))
        code, rows = self.run_grouping(replay_seconds=-1)
        self.assertEqual(code, 1)
        self.assertEqual([(r[1], r[2], r[7]) for r in rows], [("not-run-again", "2", "unclassified")])

    def test_oom_not_run_again_is_only_counted(self):
        self.input("oom-a", oom_d1b(CI_REPO))
        code, rows = self.run_grouping(replay_seconds=-1)
        self.assertEqual(code, 0)
        self.assertEqual([(r[1], r[2], r[7]) for r in rows], [("not-run-again", "1", "counted")])

    def test_crash_that_does_not_fail_again_is_unclassified_and_red(self):
        self.input("crash-a", NO_REPRO)
        code, rows = self.run_grouping()
        self.assertEqual(code, 1)
        self.assertEqual([(r[1], r[7]) for r in rows], [("no-repro", "unclassified")])

    def test_no_input_is_green(self):
        code, rows = self.run_grouping()
        self.assertEqual((code, rows), (0, []))

    def test_malformed_known_list_stops_the_grouping(self):
        with open(self.known, "w", encoding="utf-8") as f:
            f.write("a key without its other fields\n")
        self.input("oom-a", oom_d1b(CI_REPO))
        self.assertEqual(self.run_grouping(), (2, None))


def rust_code(path):
    """The source without comments and string literals."""
    with open(path, encoding="utf-8") as f:
        src = f.read()
    return re.sub(r'//[^\n]*|"(?:\\.|[^"\\])*"',
                  lambda m: '""' if m.group(0).startswith('"') else "", src)


def body(code, name):
    m = re.search(r"\bfn " + re.escape(name) + r"\b[^{;]*\{", code)
    if not m:
        return None
    i, depth = m.end(), 1
    while depth:
        depth += {"{": 1, "}": -1}.get(code[i], 0)
        i += 1
    return code[m.end():i - 1]


KEYWORDS = {"if", "match", "while", "for", "in", "return", "let", "loop", "as", "move", "else"}


def free_calls(code):
    """Names called as free functions, also through a module path
    (bivar_poc_arquivo::f): not methods (.f), not associated functions
    (Type::f), not macros (f!), not tuple variants (Ok, Some)."""
    calls = re.findall(r"(?<![.:\w])(?:[a-z_][a-z0-9_]*::)*([a-z_][a-z0-9_]*)\s*\(", code)
    return sorted(set(calls) - KEYWORDS)


class BeforeVerifier(unittest.TestCase):
    def test_list_matches_the_code_before_the_verifier(self):
        """D-1: the functions called in verify_measured before the verifier
        starts, and the free functions they call in the reader."""
        crates = {
            "bivar_poc_arquivo": rust_code(os.path.join(PROTO, "arquivo", "src", "lib.rs")),
            "bivar_poc_verificador": rust_code(os.path.join(PROTO, "verificador", "src", "lib.rs")),
        }

        def qualified(name):
            found = [c for c, code in crates.items() if body(code, name) is not None]
            return f"{found[0]}::{name}" if len(found) == 1 else f"?::{name}"

        measured = body(crates["bivar_poc_verificador"], "verify_measured")
        self.assertIsNotNone(measured, "verify_measured not found")
        start = measured.find("run_verifier(")
        self.assertGreater(start, 0, "verify_measured no longer calls run_verifier")
        found = set()
        for name in free_calls(measured[:start]):
            found.add(qualified(name))
            inner = body(crates["bivar_poc_arquivo"], name)
            if inner is not None:
                found.update(qualified(n) for n in free_calls(inner))
        listed = {r[0] for r in ff.read_list(ff.BEFORE_FILE, 1)}
        self.assertEqual(
            sorted(found), sorted(listed),
            "the code run before the verifier changed: update fuzz/before-verifier.txt "
            "with the owner (decision D-1)",
        )

    def test_extraction_finds_a_new_call(self):
        """The extraction itself: a call added before the verifier shows up."""
        code = ("fn verify_measured(p: &Path) -> R { let x = peek(p)?; let c = io::Cursor::new(x); "
                "c.len(); bivar_poc_arquivo::fingerprint_file(p, l)?; "
                "let (_, n) = file_summary(p, l)?; run_verifier(p, n) }")
        m = body(code, "verify_measured")
        self.assertEqual(free_calls(m[:m.find("run_verifier(")]),
                         ["file_summary", "fingerprint_file", "peek"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
