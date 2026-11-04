#![no_main]
// TEMPORARY (branch diag/fuzz-fork): the allocation-failure hook needs
// nightly, which is already the toolchain of this folder only.
#![feature(alloc_error_hook)]
//! Records with correct checksums and arbitrary payloads, so the fuzzer
//! reaches the document library behind the container.

use libfuzzer_sys::fuzz_target;

#[path = "oom_hook.rs"]
mod oom_hook;

fuzz_target!(
    init: {
        oom_hook::install();
    },
    |data: &[u8]| {
        bivar_poc_verificador::fuzz_records(data);
    }
);
