#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(b) = odid::parse_beacon(data) {
        odid_fuzz::touch_pack(b.pack);
    }
});
