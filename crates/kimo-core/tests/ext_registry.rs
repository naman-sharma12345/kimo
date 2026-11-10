use kimo_core::commands::registry;
#[test]
fn extension_modules_are_auto_registered() { let names: Vec<&str> = registry().iter().map(|c| c.name).collect(); assert!(names.contains(&"ext_list")); assert!(kimo_core::ext::MODULES.contains(&"example_ping")); }
