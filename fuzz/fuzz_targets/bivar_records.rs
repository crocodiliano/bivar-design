#![no_main]
//! Records with correct checksums and arbitrary payloads, so the fuzzer
//! reaches the document library behind the container.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bivar_poc_verificador::fuzz_records(data);
});
