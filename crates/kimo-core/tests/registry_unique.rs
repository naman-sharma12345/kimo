use kimo_core::commands::{find_command, registry_ref};
#[test] fn tool_names_are_unique_and_found_by_name() {
    let r = registry_ref(); let mut seen = std::collections::BTreeSet::new();
    for c in r { assert!(seen.insert(c.name), "duplicate tool name {}", c.name); let f = find_command(c.name).expect("find_command"); assert!(std::ptr::eq(f, c), "{} resolves to a different entry", c.name); }
    assert!(find_command("no_such_tool").is_none());
}
