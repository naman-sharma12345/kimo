//! Unit 6a B: cancellation tokens reach every worker thread, and no code starts threads around the guard helper.
use kimo_core::guard;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) { for e in std::fs::read_dir(d).unwrap() { let p = e.unwrap().path(); if p.is_dir() { walk(&p, out); } else if p.extension().map_or(false, |x| x == "rs") { out.push(p); } } }
#[test] fn no_raw_thread_spawns_outside_guard() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")); let mut files = vec![]; walk(&root.join("src"), &mut files); walk(&root.join("../kimo-mcp/src"), &mut files);
    let pats = ["thread::spawn(", "thread::Builder", " sc.spawn(", " s.spawn(", " scope.spawn("];
    let mut bad = vec![];
    for f in files { if f.file_name().unwrap() == "guard.rs" { continue; } let s = std::fs::read_to_string(&f).unwrap(); for (i, l) in s.lines().enumerate() { if pats.iter().any(|p| l.contains(p)) && !l.contains("spawn_builder_with_token") && !l.contains("spawn_with_token") && !l.contains("scope_spawn") { bad.push(format!("{}:{}", f.display(), i + 1)); } } }
    assert!(bad.is_empty(), "raw thread start outside guard.rs (use guard::spawn_with_token / scope_spawn): {bad:?}");
}
#[test] fn global_cancel_flag_is_gone() {
    let s = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/guard.rs")).unwrap();
    assert!(!s.contains("pub static CANCEL"), "global CANCEL flag must not exist");
}
struct Stopped<'a>(&'a AtomicUsize);
impl Drop for Stopped<'_> { fn drop(&mut self) { self.0.fetch_add(1, SeqCst); } }
#[test] fn cancelling_a_token_stops_every_parallel_worker() {
    let tok = guard::new_token(); let started = AtomicUsize::new(0); let stopped = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let r = std::thread::scope(|sc| {
        let tk = tok.clone(); sc.spawn(move || { std::thread::sleep(std::time::Duration::from_millis(150)); tk.store(true, SeqCst); });
        guard::with_token(&tok, || std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            kimo_core::par::ranges(1000, 1, |_, _| { let _s = Stopped(&stopped); started.fetch_add(1, SeqCst); loop { guard::check(); std::thread::sleep(std::time::Duration::from_millis(2)); } });
        })))
    });
    assert!(r.is_err(), "the cancelled loop must unwind");
    assert!(t0.elapsed() < std::time::Duration::from_secs(5), "workers did not stop promptly: {:?}", t0.elapsed());
    let n = started.load(SeqCst); assert!(n >= 1);
    assert_eq!(stopped.load(SeqCst), n, "every started worker must stop");
}
#[test] fn another_calls_token_does_not_cancel_this_one() {
    let a = guard::new_token(); let b = guard::new_token(); a.store(true, SeqCst);
    assert!(guard::with_token(&b, || std::panic::catch_unwind(|| guard::check())).is_ok(), "token b is not cancelled");
    assert!(guard::with_token(&a, || std::panic::catch_unwind(|| guard::check())).is_err());
    assert!(std::panic::catch_unwind(|| guard::check()).is_ok(), "no token installed means never cancelled");
}

/// V3-2 gate: thread_local! statics are per-render state hiding in thread storage. The current ones are inventoried below (render_state_leak.rs proves they do not
/// leak between renders or change with the worker count). A new one needs either an entry here with a reason, or a `// thread-local-ok: <reason>` comment on the line above.
#[test] fn no_new_thread_locals_outside_the_inventory() {
    // par.rs (1, LOCAL, v5 c2dceb8): a per-thread cap on par's worker count, set once by the spectator's dedicated preview thread so its
    // background renders use at most half the cores while tool calls on other threads stay uncapped (a process-wide value would throttle
    // the calls). It holds no render state: par's outputs are bit-identical for any worker count, and every tool call runs on a fresh
    // guarded thread where the cap is 0, so nothing carries between renders or calls.
    let allowed: &[(&str, usize)] = &[("render.rs", 6), ("frosted.rs", 3), ("shards.rs", 6), ("guard.rs", 1), ("lighting.rs", 1), ("particles.rs", 3), ("par.rs", 1)];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"); let mut files = vec![]; walk(&root, &mut files); let mut bad = vec![];
    for f in files { let text = std::fs::read_to_string(&f).unwrap(); let lines: Vec<&str> = text.lines().collect();
        let n = lines.iter().enumerate().filter(|(i, l)| l.contains("thread_local!") && !(*i > 0 && lines[i - 1].contains("thread-local-ok:"))).count();
        let name = f.file_name().unwrap().to_str().unwrap(); let ok = allowed.iter().find(|a| a.0 == name).map_or(0, |a| a.1);
        if n > ok { bad.push(format!("{}: {n} thread_local! (allowed {ok})", f.display())); } }
    assert!(bad.is_empty(), "new thread_local! statics: {bad:?}");
}
