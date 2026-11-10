//! TEMPORARY (branch diag/fuzz-fork, never merged): an allocation that
//! fails is reported to libFuzzer as out-of-memory, not as a crash.
//!
//! In the program, a file that asks for more memory than the verifier's cap
//! is refused by the verifier (ADR-0024), so in fuzzing it is the same
//! family as libFuzzer's own out-of-memory, which fork mode can ignore
//! (`-ignore_ooms=1`) while still counting it and keeping the input.
//!
//! How: with `ASAN_OPTIONS=allocator_may_return_null=1` a request the
//! sanitizer cannot serve (above 1 TiB) returns null, and Rust calls this
//! hook. The hook writes the size, then asks for a block above libFuzzer's
//! malloc limit (`-malloc_limit_mb`, the same as `-rss_limit_mb`): libFuzzer
//! then reports out-of-memory, keeps the input as `oom-…` and exits with its
//! out-of-memory code. If that does not happen, the hook returns and Rust
//! aborts the process, which libFuzzer reports as a crash: never silent.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

/// Above `-malloc_limit_mb` (4096 MiB in the workflow), below the
/// sanitizer's 1 TiB maximum.
const TRIPWIRE: usize = 8 << 30;

static IN_HOOK: AtomicBool = AtomicBool::new(false);

pub fn install() {
    std::alloc::set_alloc_error_hook(|layout| {
        // A failure inside the hook itself goes to the default: abort.
        if IN_HOOK.swap(true, Ordering::SeqCst) {
            return;
        }
        eprintln!(
            "BIVAR-FUZZ: allocation of {} bytes failed; reported to libFuzzer as out-of-memory",
            layout.size()
        );
        if let Some(path) = std::env::var_os("BIVAR_FUZZ_OOM_LOG")
            && let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        {
            let _ = writeln!(f, "allocation_failed bytes={}", layout.size());
        }
        // Census replay only (branch diag/fuzz-censo): the stack that asked,
        // so the inputs can be grouped by family.
        if std::env::var_os("BIVAR_FUZZ_OOM_BT").is_some() {
            eprintln!(
                "BIVAR-FUZZ-BACKTRACE-BEGIN\n{}\nBIVAR-FUZZ-BACKTRACE-END",
                std::backtrace::Backtrace::force_capture()
            );
        }
        let tripwire: Vec<u8> = Vec::with_capacity(TRIPWIRE);
        std::hint::black_box(&tripwire);
    });
}
