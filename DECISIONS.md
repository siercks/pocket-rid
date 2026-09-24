# Decisions

- M0: Toolchains installed 2026-09-24: stable 1.98.1, nightly 1.100.0 (2026-09-23), esp (Xtensa Rust 1.97.0.0) via espup. gh 2.101.0 via winget.
- M0: Locked firmware set resolved to esp-hal 1.1.2, esp-rtos 0.3.0, esp-radio 0.18.0, esp-alloc 0.10.0, esp-backtrace 0.19.0, esp-println 0.17.0, esp-bootloader-esp-idf 0.5.0, embassy-executor 0.10.0, embassy-time 0.5.1, embassy-sync 0.8.0, mipidsi 0.10.0, embedded-graphics 0.8.2 (one embedded-graphics-core, 0.4.1).
- M0: The release link prints a `LOAD segment with RWX permissions` linker warning from esp-hal's linker scripts; it is not a rustc/clippy warning and is left alone.
- M0: CI actions pinned to the majors current on 2026-09-24: actions/checkout@v7, dtolnay/rust-toolchain@stable/@nightly, EmbarkStudios/cargo-deny-action@v2. `host` skips `workflow_dispatch`; `fuzz` runs only on it.
- M0: No board attached (`espflash list-ports` finds none), so the M0 SERIAL CHECK is deferred and batched into the M5 HUMAN CHECK session.
