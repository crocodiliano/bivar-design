#![no_main]
#![feature(alloc_error_hook)]
//! TEMPORARY (branch diag/fuzz-fork, never merged): proves what fork mode
//! does with each kind of failure. An input starting with `!` triggers the
//! failure named by `DIAG_PROBE`; every other input does nothing.
//!
//! - `oom-real`: the real input of run 41876124671, through the same
//!   function as the bivar_records target (must be ignored and counted);
//! - `panic`, `abort`, `stack-overflow`, `timeout`: artificial failures
//!   (each must stop the run and turn the job red).

use libfuzzer_sys::fuzz_target;

#[path = "oom_hook.rs"]
mod oom_hook;

static REAL: &[u8] = include_bytes!("../diag/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61");

fn deep(n: u64) -> u64 {
    let pad = std::hint::black_box([n; 64]);
    if n == 0 { 0 } else { deep(n - 1) + pad[3] }
}

fuzz_target!(
    init: {
        oom_hook::install();
    },
    |data: &[u8]| {
        if data.first() != Some(&b'!') {
            return;
        }
        match std::env::var("DIAG_PROBE").as_deref() {
            Ok("oom-real") => bivar_poc_verificador::fuzz_records(REAL),
            Ok("panic") => panic!("artificial panic (diagnosis)"),
            Ok("abort") => std::process::abort(),
            Ok("stack-overflow") => {
                std::hint::black_box(deep(std::hint::black_box(u64::MAX >> 8)));
            }
            Ok("timeout") => std::thread::sleep(std::time::Duration::from_secs(120)),
            _ => {}
        }
    }
);
