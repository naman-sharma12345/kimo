//! Smallest possible extension tool; shows the pattern and proves auto-registration (tests/ext_registry.rs).
use super::kit::*;
pub fn commands() -> Vec<Command> { vec![
    Command { name: "ext_list", description: "List extension modules compiled into this engine (auto-discovered from src/ext).", mutates: false, schema: obj(vec![], &[]),
        run: |_, _| Ok(json!({"modules": super::MODULES})) },
] }
