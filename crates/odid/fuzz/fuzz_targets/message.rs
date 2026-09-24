#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(m) = data.first_chunk::<25>() {
        odid_fuzz::touch_message(m);
    }
});
