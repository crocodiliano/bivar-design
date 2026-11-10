//! An allocation that fails is reported to libFuzzer as out-of-memory, not
//! as a crash (ADR-0024, section 5).
//!
//! In the program, a file that asks for more memory than the verifier's cap
//! is refused by the verifier, so in fuzzing it is the same family as
//! libFuzzer's own out-of-memory, which fork mode ignores (`-ignore_ooms=1`)
//! while still counting it and keeping the input.
//!
//! How: with `ASAN_OPTIONS=allocator_may_return_null=1` a request the
//! sanitizer cannot serve (above 1 TiB) returns null, and Rust calls this
//! hook. The hook writes the size, then asks for a block above libFuzzer's
//! malloc limit (`-malloc_limit_mb`, the same as `-rss_limit_mb`): libFuzzer
//! then reports out-of-memory, keeps the input as `oom-…` and exits with its
//! out-of-memory code. If that does not happen, the hook returns and Rust
//! aborts the process, which libFuzzer reports as a crash: never silent.
//!
//! The hook is a nightly feature, used only in this folder.

use std::sync::atomic::{AtomicBool, Ordering};

/// Above `-malloc_limit_mb` (4096 MiB in `run-fork.sh`), below the
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
        let tripwire: Vec<u8> = Vec::with_capacity(TRIPWIRE);
        std::hint::black_box(&tripwire);
    });
}
