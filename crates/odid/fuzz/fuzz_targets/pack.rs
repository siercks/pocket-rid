#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| odid_fuzz::touch_pack(data));
