# Adding tools without touching existing files

Create `crates/kimo-core/src/ext/<name>.rs` with `pub fn commands() -> Vec<Command>` (use `use super::kit::*;` for helpers).
`build.rs` discovers every file in `src/ext/` at build time, compiles it as `ext::<name>` and adds its commands to the registry.
`ext_list` returns the modules compiled in. Add tests under `crates/kimo-core/tests/`. Run `python3 tests/tool_fuzz.py` before publishing.
