#![no_main]
//! Any bytes, read as a whole `.bivar` file.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    bivar_poc_verificador::fuzz_container(data);
});
