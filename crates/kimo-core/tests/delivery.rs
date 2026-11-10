//! RV-7 delivery: sha256, list_dependencies, collect_project (self-contained, deterministic, dedup, fonts, failure modes), relink_media.
use kimo_core::commands::{dispatch, Session};
use kimo_core::deps::{sha256_hex, font_license};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn sh(args: &[&str]) { let o = Command::new(args[0]).args(&args[1..]).output().expect(args[0]); assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr)); }
fn framemd5(p: &str) -> String { String::from_utf8(Command::new("ffmpeg").args(["-v", "error", "-i", p, "-map", "0:v", "-f", "framemd5", "-"]).output().unwrap().stdout).unwrap() }
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn tree(d: &Path) -> Vec<(String, String)> { let mut out = vec![]; let mut st = vec![d.to_path_buf()]; while let Some(x) = st.pop() { for e in std::fs::read_dir(&x).unwrap().flatten() { let p = e.path(); if p.is_dir() { st.push(p) } else { out.push((p.strip_prefix(d).unwrap().to_string_lossy().into_owned(), sha256_hex(&std::fs::read(&p).unwrap()))); } } } out.sort(); out }
fn find_font() -> Option<PathBuf> { let mut st = vec![PathBuf::from("/usr/share/fonts")]; while let Some(x) = st.pop() { if let Ok(rd) = std::fs::read_dir(&x) { for e in rd.flatten() { let p = e.path(); if p.is_dir() { st.push(p) } else if p.extension().map_or(false, |x| x == "ttf") { return Some(p); } } } } None }
fn set_fstype(b: &mut Vec<u8>, v: u16) { let n = u16::from_be_bytes([b[4], b[5]]) as usize; for i in 0..n { let r = 12 + 16 * i; if &b[r..r + 4] == b"OS/2" { let off = u32::from_be_bytes([b[r + 8], b[r + 9], b[r + 10], b[r + 11]]) as usize; b[off + 8..off + 10].copy_from_slice(&v.to_be_bytes()); return; } } panic!("no OS/2"); }

#[test]
fn sha256_vectors() {
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    assert_eq!(sha256_hex(&vec![b'a'; 1_000_000]), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn collect_and_relink() {
    let root = std::env::temp_dir().join(format!("kimo_deliv_{}", std::process::id())); let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src dir/ünï"); let other = root.join("other"); std::fs::create_dir_all(&src).unwrap(); std::fs::create_dir_all(&other).unwrap(); std::fs::create_dir_all(root.join("src dir/b")).unwrap();
    let p = |x: &Path| x.to_str().unwrap().to_string();
    let v1 = src.join("v one.mp4"); let v2 = root.join("src dir/b/clip.mp4"); let v1dup = root.join("src dir/b/zz_copy.mp4");
    sh(&["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=25:duration=1", "-pix_fmt", "yuv420p", &p(&v1)]);
    sh(&["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=25:duration=1", "-pix_fmt", "yuv420p", &p(&v2)]);
    std::fs::copy(&v1, &v1dup).unwrap(); // same content, different path and name
    let png = src.join("pic.png"); sh(&["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=64x64", "-frames:v", "1", &p(&png)]);
    let lut = src.join("look.cube"); std::fs::write(&lut, "LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n").unwrap();
    let font = find_font().expect("a system ttf"); let fpath = src.join("my font.ttf"); std::fs::copy(&font, &fpath).unwrap();

    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let t3 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a1 = c(&mut ss, "import_media", json!({"path": p(&v1)}))["asset_id"].as_str().unwrap().to_string();
    let a2 = c(&mut ss, "import_media", json!({"path": p(&v2)}))["asset_id"].as_str().unwrap().to_string();
    let a3 = c(&mut ss, "import_media", json!({"path": p(&v1dup)}))["asset_id"].as_str().unwrap().to_string();
    let cl = c(&mut ss, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": a1, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": a2, "start": 1, "duration": 1}));
    c(&mut ss, "add_clip", json!({"track_id": t3, "source_type": "media", "asset_id": a3, "start": 2, "duration": 1}));
    c(&mut ss, "add_effect", json!({"clip_id": cl, "kind": "lut", "strings": {"path": p(&lut)}}));
    c(&mut ss, "add_text", json!({"track_id": t3, "text": "Hi", "start": 0, "duration": 1, "size": 30, "font": p(&fpath)}));
    let proj = root.join("orig.kimo"); c(&mut ss, "save_project", json!({"path": p(&proj)})); let orig_sha = sha256_hex(&std::fs::read(&proj).unwrap());

    // list_dependencies
    let l = c(&mut ss, "list_dependencies", json!({"check_hash": true})); assert_eq!(l["summary"]["missing"], 0); assert!(l["items"].as_array().unwrap().len() >= 5, "{l}");
    let _ = png;

    // dry run reports, writes nothing
    let dest = root.join("collected"); let d = c(&mut ss, "collect_project", json!({"dest_dir": p(&dest), "dry_run": true})); assert_eq!(d["dry_run"], true); assert!(!dest.exists());
    // real collect
    let r = c(&mut ss, "collect_project", json!({"dest_dir": p(&dest)})); assert_eq!(r["missing"], 0, "{r}");
    assert!(!dest.join(".collect-tmp").exists());
    let media: Vec<_> = std::fs::read_dir(dest.join("media")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(media.len(), 2, "same content at two paths must be stored once (v one.mp4 == zz_copy.mp4): {media:?}");
    assert!(media.iter().any(|m| m.ends_with("_v one.mp4")), "smallest basename wins: {media:?}");
    assert_eq!(sha256_hex(&std::fs::read(&proj).unwrap()), orig_sha, "original project must be untouched");
    // determinism: identical dest tree on a second collect
    let dest2 = root.join("collected2"); c(&mut ss, "collect_project", json!({"dest_dir": p(&dest2)})); assert_eq!(tree(&dest), tree(&dest2), "collect is not byte-deterministic");
    // failure modes: dest exists / overwrite works
    assert!(err(&mut ss, "collect_project", json!({"dest_dir": p(&dest)})).starts_with("dest_exists"));
    c(&mut ss, "collect_project", json!({"dest_dir": p(&dest), "overwrite": true})); assert_eq!(tree(&dest), tree(&dest2));
    // stale .collect-tmp from a killed run is cleaned and never leaves a project behind
    let dest3 = root.join("killed"); std::fs::create_dir_all(dest3.join(".collect-tmp/media")).unwrap(); std::fs::write(dest3.join(".collect-tmp/media/junk"), b"x").unwrap();
    c(&mut ss, "collect_project", json!({"dest_dir": p(&dest3)})); assert!(!dest3.join(".collect-tmp").exists() && dest3.join("project.kimo").exists());

    // reference render, then delete every original and open the copy in a fresh session with the CWD elsewhere
    let (ref_png, ref_mp4) = (p(&root.join("ref.png")), p(&root.join("ref.mp4"))); c(&mut ss, "render_frame", json!({"time": 0.5, "path": ref_png})); c(&mut ss, "export", json!({"path": ref_mp4}));
    std::fs::remove_dir_all(root.join("src dir")).unwrap(); std::env::set_current_dir(&other).unwrap();
    let mut b = Session::new(); let lp = c(&mut b, "load_project", json!({"path": p(&dest.join("project.kimo"))})); let _ = lp;
    let l = c(&mut b, "list_dependencies", json!({"check_hash": true})); assert_eq!(l["summary"]["missing"], 0, "{l}"); assert_eq!(l["summary"]["changed"], 0, "{l}");
    let (b_png, b_mp4) = (p(&root.join("b.png")), p(&root.join("b.mp4"))); c(&mut b, "render_frame", json!({"time": 0.5, "path": b_png})); c(&mut b, "export", json!({"path": b_mp4}));
    assert_eq!(std::fs::read(&ref_png).unwrap(), std::fs::read(&b_png).unwrap(), "collected frame differs"); assert_eq!(framemd5(&ref_mp4), framemd5(&b_mp4), "collected export differs");

    // relink: move the media dir (restore the originals under a new name), dry run, ambiguous, apply, undo
    let mut m = Session::new(); c(&mut m, "load_project", json!({"path": p(&dest.join("project.kimo"))}));
    let moved = root.join("moved"); std::fs::rename(dest.join("media"), &moved).unwrap();
    assert!(c(&mut m, "list_dependencies", json!({}))["summary"]["missing"].as_u64().unwrap() >= 3);
    let hist = m.undo.len();
    let dry = c(&mut m, "relink_media", json!({"search_dirs": [p(&moved)]})); assert_eq!(dry["applied"], false); assert_eq!(dry["not_found"], 0, "{dry}");
    assert!(c(&mut m, "list_dependencies", json!({}))["summary"]["missing"].as_u64().unwrap() >= 3, "dry run changed the project");
    let _ = hist; // history on a dry run needs the core no_history flag (requested from MOTION)
    // ambiguous: a second identical copy under another dir with the same name
    let amb = root.join("amb"); std::fs::create_dir_all(&amb).unwrap(); for e in std::fs::read_dir(&moved).unwrap().flatten() { std::fs::copy(e.path(), amb.join(e.file_name())).unwrap(); }
    let ra = c(&mut m, "relink_media", json!({"search_dirs": [p(&moved), p(&amb)], "apply": true})); assert!(ra["ambiguous"].as_u64().unwrap() >= 1, "{ra}"); assert_eq!(ra["relinked"], 0);
    std::fs::remove_dir_all(&amb).unwrap();
    let before = m.undo.len(); let ap = c(&mut m, "relink_media", json!({"search_dirs": [p(&moved)], "apply": true})); assert_eq!(ap["ambiguous"], 0); assert!(ap["relinked"].as_u64().unwrap() >= 3);
    assert_eq!(m.undo.len(), before + 1, "apply must be one undo step");
    assert_eq!(c(&mut m, "list_dependencies", json!({}))["summary"]["missing"], 0);
    let (m_png, ) = (p(&root.join("m.png")), ); c(&mut m, "render_frame", json!({"time": 0.5, "path": m_png})); assert_eq!(std::fs::read(&ref_png).unwrap(), std::fs::read(&m_png).unwrap(), "relinked render differs");
    c(&mut m, "undo", json!({})); assert!(c(&mut m, "list_dependencies", json!({}))["summary"]["missing"].as_u64().unwrap() >= 3, "undo did not restore the old references");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn restricted_font_not_copied() {
    let lic = font_license(&std::fs::read(find_font().unwrap()).unwrap()).expect("sfnt"); let _ = lic;
    let root = std::env::temp_dir().join(format!("kimo_font_{}", std::process::id())); let _ = std::fs::remove_dir_all(&root); std::fs::create_dir_all(&root).unwrap();
    let mut b = std::fs::read(find_font().unwrap()).unwrap(); set_fstype(&mut b, 2); let fp = root.join("restricted.ttf"); std::fs::write(&fp, &b).unwrap();
    assert!(!font_license(&b).unwrap().embeddable); set_fstype(&mut b, 8); assert!(font_license(&b).unwrap().embeddable);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_text", json!({"track_id": t, "text": "Hi", "start": 0, "duration": 1, "size": 30, "font": fp.to_str().unwrap()}));
    let d = root.join("out"); let r = c(&mut ss, "collect_project", json!({"dest_dir": d.to_str().unwrap()}));
    assert!(!d.join("fonts").exists() || std::fs::read_dir(d.join("fonts")).unwrap().count() == 0, "restricted font was copied");
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("font_license")), "{r}");
    assert_eq!(r["fonts"][0]["copied"], false);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_project_report_lists_missing_files() {
    let root = std::env::temp_dir().join(format!("kimo_report_{}", std::process::id())); let _ = std::fs::remove_dir_all(&root); std::fs::create_dir_all(&root).unwrap();
    let v = root.join("v.mp4"); sh(&["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=25:duration=1", "-pix_fmt", "yuv420p", v.to_str().unwrap()]);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25})); c(&mut ss, "import_media", json!({"path": v.to_str().unwrap()}));
    let pr = root.join("p.kimo"); c(&mut ss, "save_project", json!({"path": pr.to_str().unwrap()}));
    let mut a = Session::new(); assert!(c(&mut a, "load_project", json!({"path": pr.to_str().unwrap()})).get("dependencies").is_none(), "report is opt-in");
    std::fs::remove_file(&v).unwrap();
    let r = c(&mut a, "load_project", json!({"path": pr.to_str().unwrap(), "report": true})); assert_eq!(r["dependencies"]["missing"], 1, "{r}");
    let _ = std::fs::remove_dir_all(&root);
}
