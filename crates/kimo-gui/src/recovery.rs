//! Autosave and crash recovery. While kimo runs, the project is saved in the background (every minute after an edit, or
//! after 20 edits) to `<project>.kimo.autosave` beside the user's own file (so relative media paths stay valid), or to
//! `<data>/autosave/untitled.kimo` for a project never saved. `recovery.json` records the session: which autosave holds
//! the newest work, the user's own project path, and whether kimo exited cleanly. A crash (PC restart, power cut, kill)
//! leaves `clean: false`; on the next start kimo offers to restore the autosave when it is newer than the last save.
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
    fn autosave_timing() {
        assert!(!due(5, 5, 999.0, 60.0, 20), "nothing new");
        assert!(!due(6, 5, 10.0, 60.0, 20), "one edit, too soon");
        assert!(due(6, 5, 61.0, 60.0, 20), "an edit and a minute passed");
        assert!(due(25, 5, 1.0, 60.0, 20), "20 edits: save now");
    }
}
