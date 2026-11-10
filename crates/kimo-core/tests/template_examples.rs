//! docs/examples/templates/*.json reproduce the Rust recipes: same project bytes as recipe_apply (core params, no optional text lines).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn lib(n: &str) -> String { format!("{}/../../docs/examples/templates/{n}.json", env!("CARGO_MANIFEST_DIR")) }
fn same(name: &str, recipe_params: Value, args: Value, start: f64) {
    let (mut a, mut b) = (Session::new(), Session::new());
    c(&mut a, "recipe_apply", json!({"name": name, "start": start, "params": recipe_params}));
    c(&mut b, "template_define", json!({"library_path": lib(name)}));
    let mut ar = args; ar["start"] = json!(start); c(&mut b, "template_apply", json!({"name": name, "args": ar}));
    let mut pb: Value = serde_json::from_str(&pj(&b)).unwrap(); pb.as_object_mut().unwrap().remove("templates");
    assert_eq!(serde_json::from_str::<Value>(&pj(&a)).unwrap(), pb, "{name}: template example == recipe output");
}
#[test] fn title_card_example_reproduces_the_recipe() {
    same("title_card", json!({"title": "Hello"}), json!({"title": "Hello"}), 2.0);
    same("title_card", json!({"title": "Hi", "duration": 5, "fade": 0.8, "title_size": 120}), json!({"title": "Hi", "duration": 5, "fade": 0.8, "title_size": 120}), 0.0);
}
#[test] fn stat_counter_example_reproduces_the_recipe() {
    same("stat_counter", json!({"value": "42"}), json!({"value": "42"}), 2.0);
    same("stat_counter", json!({"value": "1.2M", "y": 100, "pop": 0.8, "duration": 4}), json!({"value": "1.2M", "y": 100, "pop": 0.8, "duration": 4}), 1.0);
}
