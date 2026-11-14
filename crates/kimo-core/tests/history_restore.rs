//! Crash recovery keeps Undo: the desktop app autosaves with `save_project with_history`, and after a crash restores with
//! `load_project with_history restore`. The restored session must undo exactly like the one that crashed: no extra
//! "load" step on top, every saved step reachable, and redo of what was undone.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn widths(s: &mut Session) -> (u32, usize) { (s.project.width, c(s, "history_list", json!({}))["undo"].as_array().unwrap().len()) }

#[test]
fn restore_brings_back_the_exact_undo_stack() {
    let p = std::env::temp_dir().join(format!("kimo_hr_{}.kimo.autosave", std::process::id()));
    let ps = p.to_str().unwrap();
    // the session that will crash: three edits
    let mut a = Session::new();
    for w in [320, 640, 1280] { c(&mut a, "project_set", json!({"width": w, "height": 180, "fps": 24})); }
    assert_eq!(widths(&mut a), (1280, 3));
    let r = c(&mut a, "save_project", json!({"path": ps, "with_history": true, "history_steps": 50}));
    assert_eq!(r["undo_steps"], 3, "{r}");
    drop(a); // the crash

    // the next launch: a fresh engine restores the autosave
    let mut b = Session::new();
    let r = c(&mut b, "load_project", json!({"path": ps, "with_history": true, "restore": true}));
    assert!(r["history_ignored"].is_null(), "{r}");
    assert_eq!(widths(&mut b), (1280, 3), "restore: the load is not a step of its own");
    c(&mut b, "undo", json!({}));
    assert_eq!(b.project.width, 640, "the first undo reverts the last edit made before the crash");
    c(&mut b, "undo", json!({"steps": 2}));
    assert_eq!(widths(&mut b).1, 0);
    c(&mut b, "redo", json!({}));
    assert_eq!(b.project.width, 320);

    // without `restore` the documented behaviour stays: the load is one more step on top
    let mut d = Session::new();
    c(&mut d, "load_project", json!({"path": ps, "with_history": true}));
    assert_eq!(widths(&mut d), (1280, 4));

    // `restore` without a matching history is an ordinary load (still undoable)
    let _ = std::fs::remove_file(format!("{ps}.history"));
    let mut e = Session::new();
    let r = c(&mut e, "load_project", json!({"path": ps, "with_history": true, "restore": true}));
    assert!(r["history_ignored"].is_string(), "{r}");
    assert_eq!(widths(&mut e), (1280, 1));
    let _ = std::fs::remove_file(&p);
}
