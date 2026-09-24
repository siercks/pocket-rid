# Decisions

- M0: Toolchains installed 2026-09-24: stable 1.98.1, nightly 1.100.0 (2026-09-23), esp (Xtensa Rust 1.97.0.0) via espup. gh 2.101.0 via winget.
- M0: Locked firmware set resolved to esp-hal 1.1.2, esp-rtos 0.3.0, esp-radio 0.18.0, esp-alloc 0.10.0, esp-backtrace 0.19.0, esp-println 0.17.0, esp-bootloader-esp-idf 0.5.0, embassy-executor 0.10.0, embassy-time 0.5.1, embassy-sync 0.8.0, mipidsi 0.10.0, embedded-graphics 0.8.2 (one embedded-graphics-core, 0.4.1).
- M0: The release link prints a `LOAD segment with RWX permissions` linker warning from esp-hal's linker scripts; it is not a rustc/clippy warning and is left alone.
- M0: CI actions pinned to the majors current on 2026-09-24: actions/checkout@v7, dtolnay/rust-toolchain@stable/@nightly, EmbarkStudios/cargo-deny-action@v2. `host` skips `workflow_dispatch`; `fuzz` runs only on it.
- M0: No board attached (`espflash list-ports` finds none), so the M0 SERIAL CHECK is deferred and batched into the M5 HUMAN CHECK session.
- M1: `parse_beacon` returns at the first matching RID element; elements after it are not walked, so trailing garbage cannot reject a valid pack.
- M1: `as_str` is a method on each struct with one text field (`BasicId`, `SelfId`, `OperatorId`). Invalid UTF-8 is cut at the last valid char.
- M1: `AuthPage` stores bytes 2–24 raw in `data`; `last_page_index`, `length`, `timestamp_unix_s` return `None` unless `data_page == 0`.
- M1: `System::area_radius_m()` returns `f32` (no invalid raw value); `System::timestamp_unix_s()` returns `u64`.
- M1: `build_beacon` emits only the fixed 36-byte header (SA = BSSID = mac, DA broadcast) and the RID element. `build_pack` writes header `0xF2` (protocol version 2).
- M1: `odid` enables `encode` for its own tests through a self dev-dependency.
- M2: `cargo fuzz init` is Unix-only, so the fuzz crate was written by hand in its layout. Shared accessor calls live in `fuzz/src/lib.rs` (three targets use them). `fuzz/Cargo.lock` is not committed (targets and seeds only); seeds are `corpus/<target>/seed_*`.
- M3: `rid-proto` API: `Frame { seq, t_dev_us, payload: Payload }`, `encode_raw`, `decode_raw`, `FrameBuf::encode` (COBS + `0x00`), `cobs_encode`/`cobs_decode`, `crc32`. `FrameBuf` lives here because both firmware and tests build it.
- M3: `decode_raw` returns `BadLength` when a known type's payload size disagrees with its layout; `rid-host` counts that as `bad_frames`.
