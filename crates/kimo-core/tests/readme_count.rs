//! The v3 README quotes the tool count; it must equal the registry (keeps docs from drifting).
#[test] fn readme_v3_tool_count_matches_the_registry() {
    let r = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md")).unwrap();
    let line = r.lines().find(|l| l.contains("registered tools")).expect("README must state the registered tool count");
    let n: usize = line.trim_start().split(' ').next().unwrap().parse().expect("tool count");
    assert_eq!(n, kimo_core::commands::registry_ref().len(), "README says {n} tools");
}
