//! Autosave and crash recovery. While kimo runs, the project is saved in the background (every minute after an edit, or
//! after 20 edits) to `<project>.kimo.autosave` beside the user's own file (so relative media paths stay valid), or to
//! `<data>/autosave/untitled.kimo` for a project never saved. `recovery.json` records the session: which autosave holds
//! the newest work, the user's own project path, and whether kimo exited cleanly. A crash (PC restart, power cut, kill)
//! leaves `clean: false`; on the next start kimo offers to restore the autosave when it is newer than the last save.
//! Undo survives too: each autosave also writes `<autosave>.history` (the engine's `save_project with_history`: the newest
//! undo steps and every named checkpoint), and Restore loads it back (`load_project with_history restore`), so Ctrl+Z after
//! a crash undoes the edits made before it.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct State {
    /// The autosave file with the newest work.
    pub autosave: Option<String>,
    /// The user's own project file, when the project was opened or saved (restore points Save back at it).
    pub project_path: Option<String>,
    /// Unix ms of the newest autosave.
    pub autosaved_at: u64,
    /// Edits in the autosave vs at the user's last save/open: work to recover only when autosaved > saved.
    pub autosaved_rev: u64,
    pub saved_rev: u64,
    /// The session ended normally (window closed). false after a crash.
    pub clean: bool,
    pub pid: u32,
}

pub fn state_path(data_dir: &Path) -> PathBuf { data_dir.join("recovery.json") }

pub fn load(data_dir: &Path) -> State { std::fs::read(state_path(data_dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }

pub fn store(data_dir: &Path, s: &State) {
    let p = state_path(data_dir);
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(s).unwrap_or_default()).is_ok() { let _ = std::fs::rename(&tmp, &p); }
}

/// Where to autosave this project. A never-saved project gets one file per session (`untitled-<pid>.kimo`), so a new
/// session's autosave can never overwrite the work a crashed session left to restore.
pub fn autosave_path(data_dir: &Path, project_path: Option<&str>, session: u32) -> PathBuf {
    match project_path {
        Some(p) if !p.is_empty() => PathBuf::from(format!("{p}.autosave")),
        _ => data_dir.join("autosave").join(format!("untitled-{session}.kimo")),
    }
}

/// Undo steps kept with each autosave (every step is a whole project, so this is bounded; checkpoints are always kept).
pub const HISTORY_STEPS: u32 = 30;

/// The engine call that writes one autosave: the project plus its undo history beside it.
pub fn autosave_call(path: &Path) -> serde_json::Value {
    serde_json::json!({"action": "save_project", "args": {"path": path.display().to_string(), "with_history": true, "history_steps": HISTORY_STEPS}})
}

/// The engine call that restores an autosave with its undo history. `restore`: the load is not an undo step of its
/// own, so the first Undo reverts the last edit made before the crash (not back to an empty project).
pub fn restore_call(autosave: &str) -> serde_json::Value {
    serde_json::json!({"action": "load_project", "args": {"path": autosave, "with_history": true, "restore": true}})
}

/// The history file the engine writes beside a project saved `with_history`.
pub fn history_path(autosave: &str) -> PathBuf { PathBuf::from(format!("{autosave}.history")) }

/// Delete an autosave the user discarded, and its history.
pub fn remove_autosave(autosave: &str) {
    let _ = std::fs::remove_file(autosave);
    let _ = std::fs::remove_file(history_path(autosave));
}

/// What the previous session left behind for the user to restore, if anything.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Offer { pub autosave: String, pub project_path: Option<String>, pub autosaved_at: u64 }

pub fn offer(prev: &State) -> Option<Offer> {
    if prev.clean || prev.autosaved_rev <= prev.saved_rev { return None; }
    let a = prev.autosave.as_ref()?;
    if !Path::new(a).is_file() { return None; }
    Some(Offer { autosave: a.clone(), project_path: prev.project_path.clone(), autosaved_at: prev.autosaved_at })
}

/// Should the autosave thread write now? After any edit once `min_s` passed since the last autosave, or at once after
/// `max_edits` unsaved edits.
pub fn due(rev: u64, last_rev: u64, secs_since: f64, min_s: f64, max_edits: u64) -> bool {
    rev > last_rev && (secs_since >= min_s || rev - last_rev >= max_edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(n: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("kimo-rec-{n}-{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }

    #[test]
    fn a_crash_with_newer_work_is_offered_a_clean_exit_is_not() {
        let d = tmp("offer");
        let a = d.join("p.kimo.autosave");
        std::fs::write(&a, b"{}").unwrap();
        let crashed = State { autosave: Some(a.display().to_string()), project_path: Some("p.kimo".into()), autosaved_at: 5, autosaved_rev: 7, saved_rev: 3, clean: false, pid: 1 };
        store(&d, &crashed);
        let o = offer(&load(&d)).expect("unsaved work after a crash is offered");
        assert_eq!(o.project_path.as_deref(), Some("p.kimo"));
        assert!(offer(&State { clean: true, ..crashed.clone() }).is_none(), "a clean exit offers nothing");
        assert!(offer(&State { saved_rev: 7, ..crashed.clone() }).is_none(), "saved after the last autosave: nothing to recover");
        std::fs::remove_file(&a).unwrap();
        assert!(offer(&crashed).is_none(), "the autosave file is gone");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn autosave_sits_beside_the_project_or_in_the_data_dir() {
        let d = Path::new("/data");
        assert_eq!(autosave_path(d, Some("/home/a/cut.kimo"), 7), PathBuf::from("/home/a/cut.kimo.autosave"));
        assert_eq!(autosave_path(d, None, 7), d.join("autosave").join("untitled-7.kimo"));
        assert_ne!(autosave_path(d, None, 7), autosave_path(d, None, 8), "a new session never overwrites a crashed one's work");
    }

    #[test]
    fn autosave_keeps_history_and_restore_loads_it_without_an_extra_step() {
        let a = autosave_call(Path::new("/home/a/cut.kimo.autosave"));
        assert_eq!(a["action"], "save_project");
        assert_eq!(a["args"]["with_history"], true);
        assert_eq!(a["args"]["history_steps"], HISTORY_STEPS);
        let r = restore_call("/home/a/cut.kimo.autosave");
        assert_eq!(r["action"], "load_project");
        assert_eq!((&r["args"]["with_history"], &r["args"]["restore"]), (&serde_json::json!(true), &serde_json::json!(true)));
        assert_eq!(history_path("/x/u.kimo"), PathBuf::from("/x/u.kimo.history"));
    }

    #[test]
    fn discarding_removes_the_autosave_and_its_history() {
        let d = tmp("discard");
        let a = d.join("untitled-1.kimo");
        std::fs::write(&a, b"{}").unwrap();
        std::fs::write(history_path(&a.display().to_string()), b"{}").unwrap();
        remove_autosave(&a.display().to_string());
        assert!(!a.exists() && !history_path(&a.display().to_string()).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn autosave_timing() {
        assert!(!due(5, 5, 999.0, 60.0, 20), "nothing new");
        assert!(!due(6, 5, 10.0, 60.0, 20), "one edit, too soon");
        assert!(due(6, 5, 61.0, 60.0, 20), "an edit and a minute passed");
        assert!(due(25, 5, 1.0, 60.0, 20), "20 edits: save now");
    }
}

/// Against the real engine (KIMO_MCP_BIN): autosave with history, a "crash" (the engine is stopped), a fresh engine
/// restores, and Undo walks back through the edits made before the crash. Skipped when unset.
#[cfg(test)]
mod real_engine {
    use super::*;
    use crate::engine::{is_error, result_text, Supervisor};
    use serde_json::{json, Value};

    fn tracks(sup: &Supervisor, dir: &Path) -> usize {
        let p = dir.join("probe.kimo");
        let r = sup.call("project", &json!({"action": "save_project", "args": {"path": p.display().to_string()}})).unwrap();
        assert!(!is_error(&r), "{}", result_text(&r));
        let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        v["tracks"].as_array().map(|a| a.len()).unwrap_or(0)
    }

    #[test]
    fn undo_works_after_a_crash_restore() {
        let Some(bin) = std::env::var_os("KIMO_MCP_BIN").map(PathBuf::from).filter(|p| p.is_file()) else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        let d = std::env::temp_dir().join(format!("kimo-rec-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap();
        let env = vec![("KIMO_SPECTATOR".to_string(), "off".to_string())];
        let a = autosave_path(&d, None, 1);
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();

        let before = Supervisor::new(Some(bin.clone()), env.clone(), None);
        before.start();
        for _ in 0..3 { before.call("timeline", &json!({"action": "add_track", "args": {}})).unwrap(); }
        let r = before.call("project", &autosave_call(&a)).unwrap();
        assert!(!is_error(&r), "{}", result_text(&r));
        assert!(history_path(&a.display().to_string()).is_file(), "the autosave carries its undo history");
        before.stop(); // the crash

        let after = Supervisor::new(Some(bin), env, None);
        after.start();
        let r = after.call("project", &restore_call(&a.display().to_string())).unwrap();
        assert!(!is_error(&r), "{}", result_text(&r));
        assert_eq!(crate::agent::history(&after).unwrap().0, 3, "the undo steps came back, with no extra load step");
        assert_eq!(tracks(&after, &d), 3);
        after.call("project", &json!({"action": "undo"})).unwrap();
        assert_eq!(tracks(&after, &d), 2, "the first undo reverts the last edit made before the crash");
        after.call("project", &json!({"action": "undo", "args": {"steps": 2}})).unwrap();
        assert_eq!(tracks(&after, &d), 0);
        after.stop();
        remove_autosave(&a.display().to_string());
        let _ = std::fs::remove_dir_all(&d);
    }
}
