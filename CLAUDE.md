# rid-pocket — agent guide

`docs/DESIGN.md` is the spec and wins every conflict. This file says how to move through it with the least drag.

## Each session
1. `git log --oneline -1` gives the last finished milestone (`M<n>: …`). `DECISIONS.md` holds choices and check results.
2. Read DESIGN §0, then the next milestone in §12, then only the sections that milestone cites.
3. Implement, run the §12 gates, commit `M<n>: <title>`, and continue. Don't pause between milestones that need no hardware.
4. SERIAL CHECK: run it yourself if the board is attached. HUMAN CHECK: stop, print the exact commands and pass criteria, and wait.

## Lean code (binding)
- Write the least code that meets the spec. Add no trait, generic, builder, module or helper unless the spec names it or two call sites need it.
- Add no options, features or dependencies beyond DESIGN §4.4 and §4.5 (plus `cc` for M10 and `libfuzzer-sys` for fuzz).
- Tests prove the spec's vectors and criteria. Build no extra test scaffolding.
- Comments say why, never what.
- Delete dead code; don't comment it out.

## Environment
- Native Windows dev host, per DESIGN §4.1. Run gates in Git Bash. No WSL2 for firmware.
- Repo root: stable toolchain. Firmware: `cd firmware`, always build `--release`; espup's environment is already injected.
- Fuzzing runs only in CI (`fuzz` workflow). Push after every milestone commit.
- Never run `cargo update` after M0. Never add anything that transmits. `unsafe` only in `firmware/src/radio.rs`.

## Stop and ask
Only for the DESIGN §0 stop conditions. Otherwise pick the simplest option, log one line in `DECISIONS.md`, and keep going.
