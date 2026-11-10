//! Code clips (Source::Code, codeclip.rs): a web project folder whose window.renderFrame(t, params) draws each frame is a clip
//! like footage. These tests drive real headless Chromium through the sidecar (python3 + playwright + Pillow), with canvas-2D
//! pages whose pixels are a known function of (t, params), so exact values prove the time mapping, params, keyframes, split,
//! compositing, sandbox and cache. The three.js example (examples/codeclip_orbit) proves WebGL and writes proof frames to
//! KIMO_PROOF_DIR when it is set.
use kimo_core::codeclip;
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// A scratch folder under the temp dir (point TMPDIR somewhere disposable), deleted when dropped, panics included.
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Scratch { let d = std::env::temp_dir().join(format!("kimo_cc_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); Scratch(d) }
    /// A canvas-2D code clip page in `<scratch>/clip` with `body` as its script, plus an optional kimo.json.
    fn page(&self, body: &str, manifest: Option<&str>) -> PathBuf {
        let d = self.0.join("clip"); std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("index.html"), format!(r#"<!doctype html><html><head><meta charset="utf-8"><style>html,body{{margin:0;background:transparent;overflow:hidden}}canvas{{display:block}}</style></head>
<body><canvas id="c"></canvas><script>
const c = document.getElementById('c'); c.width = innerWidth; c.height = innerHeight; const g = c.getContext('2d');
{body}
</script></body></html>"#)).unwrap();
        if let Some(m) = manifest { std::fs::write(d.join("kimo.json"), m).unwrap(); }
        d
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

/// Left half filled with rgb(round(t*50) mod 256, round(g*255), 77), right half transparent.
const HALF: &str = "window.renderFrame = (t, p) => { g.clearRect(0, 0, c.width, c.height); g.fillStyle = 'rgb(' + Math.round((t * 50) % 256) + ',' + Math.round(p.g * 255) + ',77)'; g.fillRect(0, 0, c.width / 2, c.height); }; window.__ready = true;";
const HALF_JSON: &str = r#"{"params": {"g": {"default": 0.5, "min": 0, "max": 1}}}"#;

/// 64x36 @ 10 fps, blue solid under everything, the code clip from `dir` on the track above (start 0.5 s, 4 s long). The project
/// lives in the scratch folder, so the frame cache (<project>/.kimo_cache/codeclip) is deleted with it.
fn scene(sc: &Scratch, dir: &Path) -> (Session, String) {
    let mut s = Session::new(); s.project.base_dir = Some(sc.0.clone());
    c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [0, 0, 1], "start": 0, "duration": 6}));
    let r = c(&mut s, "add_code_clip", json!({"track_id": t2, "path": dir.to_str().unwrap(), "start": 0.5, "duration": 4}));
    (s, r["clip_id"].as_str().unwrap().to_string())
}

/// Composited canvas pixel (x, y) at timeline time t, as 0..255 rgb.
fn px(s: &mut Session, t: f64, x: usize, y: usize) -> [i32; 3] {
    kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(t)).unwrap_or_else(|e| panic!("prepare at {t}: {e}"));
    let f = kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap();
    let i = (y * f.w + x) * 4; [0, 1, 2].map(|j| (f.d[i + j] * 255.0).round() as i32)
}
fn near(got: [i32; 3], want: [i32; 3], tol: i32, what: &str) { assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() <= tol), "{what}: got {got:?}, want {want:?} (+-{tol})"); }

#[test]
fn code_clip_draws_its_frames_on_the_timeline_with_params_keys_and_split() {
    let sc = Scratch::new("half"); let d = sc.page(HALF, Some(HALF_JSON)); let (mut s, id) = scene(&sc, &d);
    // timeline 1.75 -> clip-local 1.25 -> frame slot 12 -> t = 1.2: red 60; g declared default 0.5 -> 128.
    near(px(&mut s, 1.75, 5, 18), [60, 128, 77], 1, "left half at 1.75 s");
    near(px(&mut s, 1.75, 50, 18), [0, 0, 255], 0, "right half is transparent: the solid shows through");
    near(px(&mut s, 0.2, 5, 18), [0, 0, 255], 0, "before the clip starts");
    // g keyframed on the code clock: 0 at t=0 -> 1 at t=2 (linear); t = 1.2 -> 0.6 -> 153.
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "g", "value": 0.0, "time": 0.0}));
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "g", "value": 1.0, "time": 2.0}));
    near(px(&mut s, 1.75, 5, 18), [60, 153, 77], 1, "keyed g at t=1.2");
    // Split at 2.5: the right half continues the same code clock (t = in_point 2.0 + 0.5 = 2.5 at timeline 3.05), keys included.
    let r = c(&mut s, "split_clip", json!({"clip_id": id, "at": 2.5}))["right_clip_id"].as_str().unwrap().to_string();
    near(px(&mut s, 2.45, 5, 18), [95, 242, 77], 1, "left of the split, t=1.9");
    near(px(&mut s, 3.05, 5, 18), [125, 255, 77], 1, "right of the split, t=2.5");
    // Constant again: no time clears the keys.
    c(&mut s, "set_code_clip_param", json!({"clip_id": r, "name": "g", "value": 0.2}));
    near(px(&mut s, 3.05, 5, 18), [125, 51, 77], 1, "constant g on the right clip");
    // Speed 2 on the right clip: clip-local 0.5 -> t = 2.0 + 1.0 = 3.0 -> red 150.
    c(&mut s, "set_speed", json!({"clip_id": r, "speed": 2.0}));
    near(px(&mut s, 3.05, 5, 18), [150, 51, 77], 1, "speed 2");
}

#[test]
fn code_clip_composites_like_footage() {
    let sc = Scratch::new("comp"); let d = sc.page(HALF, Some(HALF_JSON)); let (mut s, id) = scene(&sc, &d);
    c(&mut s, "set_property", json!({"clip_id": id, "property": "opacity", "value": 0.5}));
    // straight-alpha normal blend: 0.5 * (60,128,77) + 0.5 * (0,0,255)
    near(px(&mut s, 1.75, 5, 18), [30, 64, 166], 2, "opacity 0.5 over blue");
    c(&mut s, "set_property", json!({"clip_id": id, "property": "opacity", "value": 1.0}));
    c(&mut s, "set_property", json!({"clip_id": id, "property": "x", "value": 32.0}));
    near(px(&mut s, 1.75, 5, 18), [0, 0, 255], 0, "moved right 32 px: the left edge shows the solid");
    near(px(&mut s, 1.75, 40, 18), [60, 128, 77], 1, "moved right 32 px: the drawn half now sits at x 32..63");
}

#[test]
fn sandbox_blocks_network_and_files_outside_the_folder() {
    let sc = Scratch::new("sandbox"); let d = sc.page(r#"
let res = 'pending';
const tryFetch = (u) => fetch(u).then(r => r.ok ? 'open' : 'closed').catch(() => 'closed');
window.__ready = false;
Promise.all([tryFetch('https://example.com/x.js'), tryFetch('%2e%2e/kimo_cc_secret.txt'), tryFetch('http://127.0.0.1:9/')]).then(v => { res = v.join(','); window.__ready = true; });
window.renderFrame = () => { g.fillStyle = res === 'closed,closed,closed' ? 'rgb(0,255,0)' : 'rgb(255,0,0)'; g.fillRect(0, 0, c.width, c.height); };
"#, None);
    std::fs::write(d.parent().unwrap().join("kimo_cc_secret.txt"), "secret").unwrap();
    let (mut s, _) = scene(&sc, &d);
    near(px(&mut s, 1.0, 10, 10), [0, 255, 0], 0, "remote, parent-folder and localhost requests all fail inside the page");
}

#[test]
fn disk_cache_serves_repeats_and_a_file_edit_invalidates() {
    let sc = Scratch::new("cache"); let d = sc.page(HALF, Some(HALF_JSON)); let (s, id) = scene(&sc, &d);
    let proj = sc.0.clone();
    let clip = s.project.find_clip(&id).unwrap().clone(); let kimo_core::model::Source::Code(cc) = &clip.source else { panic!() };
    let (a, n1) = codeclip::frames_for(&s.project, &clip, cc, &[3, 4, 5]).unwrap();
    assert_eq!(n1, 3, "first call renders all three frames");
    let (b, n2) = codeclip::frames_for(&s.project, &clip, cc, &[3, 4, 5]).unwrap();
    assert_eq!(n2, 0, "second call is served from {}", proj.join(".kimo_cache/codeclip").display());
    assert_eq!(a, b, "cached frames are the rendered bytes");
    assert_eq!(std::fs::read_dir(proj.join(".kimo_cache/codeclip")).unwrap().count(), 3);
    std::fs::write(d.join("extra.js"), "// any file in the folder is part of the key\n").unwrap();
    let (_, n3) = codeclip::frames_for(&s.project, &clip, cc, &[3]).unwrap();
    assert_eq!(n3, 1, "a new file in the folder changes the key, nothing stale is served");
    // Size cap: least recently used first. Hit frame 3 again (a hit refreshes it), then cap the folder at two frames' bytes.
    let cdir = proj.join(".kimo_cache/codeclip");
    let mut files: Vec<_> = std::fs::read_dir(&cdir).unwrap().filter_map(|e| e.ok()).map(|e| (e.metadata().unwrap().len(), e.path())).collect(); files.sort();
    assert_eq!(files.len(), 4);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let (_, n4) = codeclip::frames_for(&s.project, &clip, cc, &[3]).unwrap(); assert_eq!(n4, 0);
    let two = files.iter().rev().take(2).map(|f| f.0).sum::<u64>();
    let (left, bytes) = codeclip::prune_cache(&cdir, two);
    assert!(left <= 2 && bytes <= two, "pruned to {left} files / {bytes} bytes (cap {two})");
    let (_, n5) = codeclip::frames_for(&s.project, &clip, cc, &[3]).unwrap(); assert_eq!(n5, 0, "the most recently used frame survived the prune");
    assert_eq!(codeclip::cache_max_bytes(), 256 * 1024 * 1024, "default cap");
}

#[test]
fn hung_or_throwing_pages_fail_loud_and_in_bounded_time() {
    let sc = Scratch::new("hang"); let d = sc.page("window.renderFrame = () => new Promise(() => {}); window.__ready = true;", None); let (mut s, id) = scene(&sc, &d);
    std::env::set_var("KIMO_CODECLIP_FRAME_TIMEOUT_MS", "1500");
    let t0 = std::time::Instant::now();
    let e = kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(1.0)).unwrap_err();
    std::env::remove_var("KIMO_CODECLIP_FRAME_TIMEOUT_MS");
    assert!(e.starts_with("renderer_failed:") && e.contains("took longer than 1500 ms"), "{e}");
    assert!(t0.elapsed().as_secs() < 60, "took {:?}", t0.elapsed());
    let _ = id;
    let sc2 = Scratch::new("throw"); let d2 = sc2.page("window.renderFrame = (t) => { throw new Error('boom at ' + t); }; window.__ready = true;", None); let (mut s2, _) = scene(&sc2, &d2);
    let e = kimo_core::render::prepare_at(&s2.project, &mut s2.cache, Some(1.0)).unwrap_err();
    assert!(e.starts_with("renderer_failed:") && e.contains("boom at"), "{e}");
    let sc3 = Scratch::new("noframe"); let d3 = sc3.page("window.__ready = true;", None); let (mut s3, _) = scene(&sc3, &d3);
    let e = kimo_core::render::prepare_at(&s3.project, &mut s3.cache, Some(1.0)).unwrap_err();
    assert!(e.contains("renderFrame is not a function"), "{e}");
}

#[test]
fn add_code_clip_checks_the_folder_and_declared_params() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let e = dispatch_ex(&mut s, "add_code_clip", &json!({"track_id": t, "path": "/nonexistent/kimo_cc", "duration": 2})).unwrap_err();
    assert_eq!(e.code, "invalid_source", "{}", e.message);
    let se = Scratch::new("empty"); let empty = se.0.clone();
    let e = dispatch_ex(&mut s, "add_code_clip", &json!({"track_id": t, "path": empty.to_str().unwrap(), "duration": 2})).unwrap_err();
    assert!(e.code == "invalid_source" && e.message.contains("no entry page 'index.html'"), "{}", e.message);
    let sc = Scratch::new("decl"); let d = sc.page(HALF, Some(r#"{"params": {"g": {"default": 0.25, "min": 0, "max": 1}, "speed": 2}}"#));
    let r = c(&mut s, "add_code_clip", json!({"track_id": t, "path": d.to_str().unwrap(), "duration": 2, "params": {"speed": 3}}));
    assert_eq!(r["params"], json!({"g": 0.25, "speed": 3.0})); assert_eq!(r["width"], 64); assert_eq!(r["ranges"], json!({"g": [0.0, 1.0]}));
    let id = r["clip_id"].as_str().unwrap().to_string();
    let e = dispatch_ex(&mut s, "add_code_clip", &json!({"track_id": t, "path": d.to_str().unwrap(), "duration": 2, "params": {"hue": 1}})).unwrap_err();
    assert!(e.message.starts_with("unknown param 'hue'") && e.message.contains("Valid: g, speed"), "{}", e.message);
    let e = dispatch_ex(&mut s, "set_code_clip_param", &json!({"clip_id": id, "name": "g", "value": 1.5})).unwrap_err();
    assert_eq!(e.code, "out_of_range", "{}", e.message);
    let e = dispatch_ex(&mut s, "set_code_clip_param", &json!({"clip_id": id, "name": "zoom", "value": 1})).unwrap_err();
    assert!(e.message.contains("Valid: g, speed"), "{}", e.message);
    let sh = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 3, "duration": 1}))["clip_id"].clone();
    let e = dispatch_ex(&mut s, "set_code_clip_param", &json!({"clip_id": sh, "name": "g", "value": 1})).unwrap_err();
    assert!(e.message.contains("is not a code clip"), "{}", e.message);
    let e = dispatch_ex(&mut s, "add_code_clip", &json!({"track_id": t, "path": d.to_str().unwrap(), "duration": 2, "width": 8})).unwrap_err();
    assert!(e.message.contains("between 16 and 4096"), "{}", e.message);
    // Keys on the code clock, read back through the sample the page gets.
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "g", "value": 0.0, "time": 0.0}));
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "g", "value": 1.0, "time": 1.0}));
    let clip = s.project.find_clip(&id).unwrap().clone(); let kimo_core::model::Source::Code(cc) = &clip.source else { panic!() };
    let smp = codeclip::sample(&clip, cc, 5, 10.0); assert_eq!(smp.t, 0.5); assert_eq!(smp.params["g"], 0.5); assert_eq!(smp.params["speed"], 3.0);
    // The project file keeps the code clip.
    let txt = serde_json::to_string(&s.project).unwrap(); assert!(txt.contains(r#""type":"code""#), "serialized as type code");
    let back: kimo_core::model::Project = serde_json::from_str(&txt).unwrap();
    assert!(matches!(back.find_clip(&id).unwrap().source, kimo_core::model::Source::Code(_)));
}

#[test]
fn frame_key_covers_files_params_time_and_size_and_fit_letterboxes() {
    let sc = Scratch::new("key"); let d = sc.page(HALF, Some(HALF_JSON));
    let tree = codeclip::tree_hash(&d).unwrap();
    let cc = codeclip::CodeClip { dir: d.to_str().unwrap().into(), entry: "index.html".into(), width: 64, height: 36, params: Default::default(), ranges: Default::default() };
    let s0 = codeclip::Sample { t: 1.0, params: [("g".to_string(), 0.5)].into() };
    let k = |tr: u128, cc: &codeclip::CodeClip, s: &codeclip::Sample| codeclip::frame_key(tr, cc, s, (64, 36));
    let base = k(tree, &cc, &s0);
    assert_ne!(base, k(tree, &cc, &codeclip::Sample { t: 1.0 + 1e-6, ..s0.clone() }), "t");
    assert_ne!(base, k(tree, &cc, &codeclip::Sample { params: [("g".to_string(), 0.6)].into(), ..s0.clone() }), "params");
    assert_ne!(base, k(tree, &codeclip::CodeClip { width: 32, ..cc.clone() }, &s0), "size");
    assert_eq!(base, k(codeclip::tree_hash(&d).unwrap(), &cc, &s0), "stable");
    std::fs::write(d.join("index.html"), std::fs::read_to_string(d.join("index.html")).unwrap() + "\n<!-- edit -->").unwrap();
    assert_ne!(tree, codeclip::tree_hash(&d).unwrap(), "an edit changes the folder hash");
    // 4x2 opaque red into 8x8: scaled 2x, centred, transparent bars above and below.
    let src: Vec<u8> = (0..8).flat_map(|_| [255u8, 0, 0, 255]).collect();
    let out = codeclip::fit(&src, 4, 2, 8, 8);
    assert_eq!(&out[0..4], &[0, 0, 0, 0], "top bar transparent"); let mid = (4 * 8 + 3) * 4; assert_eq!(&out[mid..mid + 4], &[255, 0, 0, 255], "centre red");
}

#[test]
fn three_js_example_renders_through_webgl_and_takes_kimo_effects() {
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/codeclip_orbit");
    let sc = Scratch::new("orbit"); let mut s = Session::new(); s.project.base_dir = Some(sc.0.clone()); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 30}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "gradient", "color": [0.05, 0.05, 0.12], "color2": [0.25, 0.1, 0.2], "start": 0, "duration": 4}));
    let id = c(&mut s, "add_code_clip", json!({"track_id": t2, "path": ex.to_str().unwrap(), "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "hue", "value": 0.58, "time": 0.0}));
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "hue", "value": 0.95, "time": 3.0}));
    c(&mut s, "add_effect", json!({"clip_id": id, "kind": "glow", "params": {"amount": 0.8, "radius": 10}}));
    let mut frames = vec![];
    for t in [0.0, 1.0, 2.0, 3.0] {
        kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(t)).unwrap();
        let f = kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap();
        // the knot fills the middle of the frame: count pixels in the central box that differ from the gradient on their row
        // (x = 2 is outside the ring of cubes, the gradient is vertical)
        let at = |x: usize, y: usize| { let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] };
        let mut drawn = 0; for y in 50..130 { let bg = at(2, y); for x in 100..220 { let p = at(x, y); if (p[0] - bg[0]).abs() + (p[1] - bg[1]).abs() + (p[2] - bg[2]).abs() > 0.1 { drawn += 1; } } }
        assert!(drawn > 1500, "t={t}: WebGL drew only {drawn} of 9600 pixels in the central box");
        frames.push(f.d.clone());
        if let Ok(dir) = std::env::var("KIMO_PROOF_DIR") { let _ = std::fs::create_dir_all(&dir); c(&mut s, "render_frame", json!({"time": t, "path": format!("{dir}/codeclip_orbit_{t}.png"), "inline": "never"})); }
    }
    assert!(frames.windows(2).all(|w| w[0] != w[1]), "the orbit moves between frames");
}

/// The folder is a project dependency: list_dependencies reports it as a directory, collect_project copies it with its
/// subfolders and points the copy at it, and the collected project renders the same pixels with the original folder gone.
#[test]
fn collect_project_carries_the_code_clip_folder() {
    let sc = Scratch::new("collect");
    let d = sc.0.join("clip"); std::fs::create_dir_all(d.join("lib/deep")).unwrap();
    std::fs::write(d.join("index.html"), r#"<!doctype html><html><head><style>html,body{margin:0;background:transparent;overflow:hidden}</style></head><body><canvas id="c"></canvas><script src="lib/deep/draw.js"></script></body></html>"#).unwrap();
    std::fs::write(d.join("lib/deep/draw.js"), "const c = document.getElementById('c'); c.width = innerWidth; c.height = innerHeight; const g = c.getContext('2d');\nwindow.renderFrame = (t) => { g.fillStyle = 'rgb(10,200,30)'; g.fillRect(0, 0, c.width, c.height); };").unwrap();
    let mut s = Session::new(); s.project.base_dir = Some(sc.0.clone());
    c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let tr = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_code_clip", json!({"track_id": tr, "path": d.to_str().unwrap(), "duration": 2}));
    let l = c(&mut s, "list_dependencies", json!({}));
    assert_eq!(l["summary"]["missing"], 0, "{l}");
    assert!(l["items"].as_array().unwrap().iter().any(|i| i["field"].as_str().unwrap_or("").ends_with("source.dir")), "code clip folder not listed: {l}");
    let dest = sc.0.join("collected"); let r = c(&mut s, "collect_project", json!({"dest_dir": dest.to_str().unwrap()}));
    assert_eq!(r["missing"], 0, "{r}");
    std::fs::remove_dir_all(&d).unwrap();
    let mut b = Session::new(); c(&mut b, "load_project", json!({"path": dest.join("project.kimo").to_str().unwrap()}));
    let l = c(&mut b, "list_dependencies", json!({})); assert_eq!(l["summary"]["missing"], 0, "collected copy is missing files: {l}");
    near(px(&mut b, 0.5, 10, 10), [10, 200, 30], 1, "collected code clip (script in a subfolder)");
}
