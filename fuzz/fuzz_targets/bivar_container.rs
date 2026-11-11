#![no_main]
// The allocation-failure hook (oom_hook.rs) needs nightly, which is the
// toolchain of this folder only.
#![feature(alloc_error_hook)]
//! Any bytes, read as a whole `.bivar` file.

use libfuzzer_sys::fuzz_target;

#[path = "oom_hook.rs"]
mod oom_hook;

fuzz_target!(
    init: {
        oom_hook::install();
    },
    |data: &[u8]| {
        // TEMPORARY (branch diag/fuzz-familias, proof 2 of T3): an artificial
        // request of 2 TiB, a family that is not in the list.
        if data.windows(4).any(|w| w == b"T3!N") {
            let v: Vec<u8> = Vec::with_capacity(std::hint::black_box(2usize << 40));
            std::hint::black_box(&v);
        }
        bivar_poc_verificador::fuzz_container(data);
    }
);
