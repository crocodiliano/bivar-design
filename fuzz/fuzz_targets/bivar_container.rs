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
        // TEMPORARY (branch diag/fuzz-familias, proof 4 of T3): an artificial
        // panic, a family that is not in the list.
        if data.windows(4).any(|w| w == b"T3!C") {
            panic!("artificial panic in the target (proof 4 of T3)");
        }
        bivar_poc_verificador::fuzz_container(data);
    }
);
