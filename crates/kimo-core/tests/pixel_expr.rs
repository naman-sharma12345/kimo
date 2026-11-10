use kimo_core::{
    commands::{dispatch, Session},
    effects::Frame,
    pixel_expr::{apply, compile},
};
use serde_json::json;
// Process-global state (guard::BUDGET, the KIMO_EXPR_MAX_OPS env var, the par
// thread override) is shared by every test in this binary. Tests that write it
// hold GLOBALS exclusively and restore the prior value on drop (panic included);
// every other test holds it shared, so no render or limit read sees a swap.
static GLOBALS: std::sync::RwLock<()> = std::sync::RwLock::new(());
fn shared() -> std::sync::RwLockReadGuard<'static, ()> {
    GLOBALS.read().unwrap_or_else(|e| e.into_inner())
}
fn exclusive() -> std::sync::RwLockWriteGuard<'static, ()> {
    GLOBALS.write().unwrap_or_else(|e| e.into_inner())
}
struct RestoreBudget(usize);
impl RestoreBudget {
    fn swap(v: usize) -> Self {
        RestoreBudget(kimo_core::guard::BUDGET.swap(v, std::sync::atomic::Ordering::Relaxed))
    }
}
impl Drop for RestoreBudget {
    fn drop(&mut self) {
        kimo_core::guard::BUDGET.store(self.0, std::sync::atomic::Ordering::Relaxed);
    }
}
struct RestoreEnv(&'static str, Option<std::ffi::OsString>);
impl RestoreEnv {
    fn set(k: &'static str, v: &str) -> Self {
        let prior = std::env::var_os(k);
        std::env::set_var(k, v);
        RestoreEnv(k, prior)
    }
}
impl Drop for RestoreEnv {
    fn drop(&mut self) {
        match self.1.take() {
            Some(v) => std::env::set_var(self.0, v),
            None => std::env::remove_var(self.0),
        }
    }
}
// par has no getter for its override; every writer here holds GLOBALS
// exclusively and leaves it at its default 0, so 0 is the prior value.
struct RestoreThreads;
impl Drop for RestoreThreads {
    fn drop(&mut self) {
        kimo_core::par::set_threads(0);
    }
}
fn frame() -> Frame {
    Frame {
        w: 32,
        h: 16,
        d: (0..512)
            .flat_map(|i| [i as f32 / 511., 0.25, 0.75, 1.])
            .collect(),
    }
}
#[test]
fn identity_invert_gradient_noise_and_threads() {
    let _globals = exclusive();
    let _threads = RestoreThreads;
    let f = frame();
    let r = apply(&compile("src", 512).unwrap(), &f, None, [0.; 8], 0.).unwrap();
    assert_eq!(r.d, f.d);
    let inv = apply(
        &compile("vec4(1-r,1-g,1-b,a)", 512).unwrap(),
        &f,
        None,
        [0.; 8],
        0.,
    )
    .unwrap();
    for (a, b) in f.d.chunks_exact(4).zip(inv.d.chunks_exact(4)) {
        assert_eq!(b, [1. - a[0], 1. - a[1], 1. - a[2], a[3]])
    }
    let grad = compile("vec4(u,v,0,1)", 512).unwrap();
    let r = apply(&grad, &f, None, [0.; 8], 0.).unwrap();
    assert_eq!(&r.d[..4], &[0., 0., 0., 1.]);
    assert_eq!(&r.d[r.d.len() - 4..], &[1., 1., 0., 1.]);
    let noise = compile("noise(42,x,y)", 512).unwrap();
    kimo_core::par::set_threads(1);
    let a = apply(&noise, &f, None, [0.; 8], 0.).unwrap();
    kimo_core::par::set_threads(4);
    assert_eq!(a.d, apply(&noise, &f, None, [0.; 8], 0.).unwrap().d);
    kimo_core::par::set_threads(0);
}
#[test]
fn shift_matches_rgb_split() {
    let _globals = shared();
    let f = frame();
    let expression =
        "vec4(red(sample(clamp(x+2,0,w-1)/(w-1),v)),g,blue(sample(clamp(x-2,0,w-1)/(w-1),v)),a)";
    let out = apply(&compile(expression, 512).unwrap(), &f, None, [0.; 8], 0.).unwrap();
    let mut reference = frame();
    let mut p = kimo_core::effects::Params::new();
    p.insert("amount".into(), kimo_core::keyframe::Prop::new(2.));
    kimo_core::effects::apply("rgb_split", &p, &Default::default(), &mut reference, 0., 1).unwrap();
    for (a, b) in out.d.iter().zip(reference.d) {
        assert!((a - b).abs() <= 1. / 255.);
    }
}
#[test]
fn limits_roundtrip_named_errors_env_and_memory() {
    let _globals = exclusive();
    let mut s = Session::new();
    let before = serde_json::to_value(&s.project).unwrap();
    let start = s.undo.len();
    let r = dispatch(&mut s, "get_limits", &json!({})).unwrap();
    assert_eq!(r["limits"]["expr_max_ops"], 512);
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":4})).unwrap();
    assert_eq!(s.resource_limits.expr_max_ops, Some(4));
    let e = compile("sin(r)+pow(g,2)+mix(r,b,0.5)", 4).err().unwrap();
    assert!(e.contains("expr_max_ops"));
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":100})).unwrap();
    assert!(compile("sin(r)+pow(g,2)+mix(r,b,0.5)", 100).is_ok());
    assert_eq!(s.undo.len(), start);
    assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
    let ceiling = kimo_core::resource_limits::memory_ceiling();
    assert!(
        dispatch(&mut s, "set_limits", &json!({"memory_bytes":ceiling+1}))
            .unwrap_err()
            .contains("hard ceiling")
    );
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":null})).unwrap();
    assert_eq!(s.resource_limits.expr_max_ops, Some(512));
    let _env = RestoreEnv::set("KIMO_EXPR_MAX_OPS", "19");
    let s = Session::new();
    assert_eq!(s.resource_limits.expr_max_ops, Some(19));
}
#[test]
fn caps_nonfinite_compile_position_and_fuzz() {
    let _globals = shared();
    assert!(compile("sinn(r)", 512)
        .unwrap_err()
        .contains("did you mean 'sin'"));
    assert!(compile("r +", 512).unwrap_err().contains("byte"));
    assert!(apply(
        &compile("sqrt(-1)", 512).unwrap(),
        &frame(),
        None,
        [0.; 8],
        0.
    )
    .is_err());
    let mut l = kimo_core::resource_limits::Limits::defaults();
    l.expr_max_pixel_ops = Some(1);
    let _g = kimo_core::resource_limits::install(l);
    assert!(
        apply(&compile("src", 512).unwrap(), &frame(), None, [0.; 8], 0.)
            .err()
            .unwrap()
            .contains("expr_max_pixel_ops")
    );
    for i in 0..1000 {
        let s = format!("{}sin({}){}", "(".repeat(i % 100), i, "!".repeat(i % 5));
        let _ = compile(&s, 512);
    }
}
#[test]
fn stored_memory_effective_process_cap_and_named_error() {
    let _globals = exclusive();
    let _budget = RestoreBudget::swap(1024);
    let mut s = Session::new();
    let r = dispatch(&mut s, "set_limits", &json!({"memory_bytes":2048})).unwrap();
    assert_eq!(r["memory_bytes"]["requested"], 2048);
    assert_eq!(r["memory_bytes"]["effective"], 1024);
    let _g = kimo_core::resource_limits::install(s.resource_limits);
    let e = kimo_core::resource_limits::reserve(2000).unwrap_err();
    assert!(
        e.contains("memory_bytes")
            && e.contains("KIMO_MEMORY_BUDGET_BYTES")
            && e.contains("process budget")
    );
}
#[test]
fn displace_convolution_grade_primitives() {
    let _globals = shared();
    let mut f = frame();
    let original = f.d.clone();
    let mut strings = std::collections::BTreeMap::new();
    strings.insert("expr".into(), "vec4(0.5,0.5,0,1)".into());
    kimo_core::effects::apply("displace", &Default::default(), &strings, &mut f, 0., 1).unwrap();
    for (a, b) in f.d.iter().zip(&original) {
        assert!((a - b).abs() < 1e-6)
    }
    strings.clear();
    strings.insert("kernel".into(), "[0,0,0,0,1,0,0,0,0]".into());
    kimo_core::effects::apply("convolution", &Default::default(), &strings, &mut f, 0., 1).unwrap();
    for (a, b) in f.d.iter().zip(&original) {
        assert!((a - b).abs() < 1e-6)
    }
    let mut p = kimo_core::effects::Params::new();
    p.insert("gain_r".into(), kimo_core::keyframe::Prop::new(2.));
    p.insert("lift_b".into(), kimo_core::keyframe::Prop::new(-2.));
    kimo_core::effects::apply("grade", &p, &Default::default(), &mut f, 0., 1).unwrap();
    assert!((f.d[2] + 1.25).abs() < 1e-6);
}
#[test]
fn nonfinite_division_and_param_errors() {
    let _globals = shared();
    for s in ["1/0", "0/0", "sqrt(-1)", "pow(1e30,10)"] {
        assert!(
            apply(&compile(s, 512).unwrap(), &frame(), None, [0.; 8], 0.).is_err(),
            "{s}"
        )
    }
    assert!(compile("NaN", 512).is_err());
    assert!(compile("1e999", 512).is_err());
    assert!(apply(
        &compile("p0", 512).unwrap(),
        &frame(),
        None,
        [f32::INFINITY; 8],
        0.
    )
    .is_err());
}
#[test]
fn process_env_override_child() {
    let _globals = shared();
    if std::env::var("KIMO_LIMIT_CHILD").is_ok() {
        kimo_core::guard::init();
        assert_eq!(
            kimo_core::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed),
            128 * 1024 * 1024
        );
        let mut s = Session::new();
        let r = dispatch(&mut s, "get_limits", &json!({})).unwrap();
        assert_eq!(r["memory_bytes"]["source"], "env");
        assert_eq!(r["memory_bytes"]["process_budget"], 128 * 1024 * 1024);
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_env_override_child", "--test-threads=1"])
        .env("KIMO_LIMIT_CHILD", "1")
        .env("KIMO_MEMORY_BUDGET_BYTES", "134217728")
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn renderer_expr_limits_and_named_structured_errors() {
    let _globals = shared();
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":32,"height":16})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let c = dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":t,"source_type":"solid","start":0,"duration":1,"color":[0.3,0.4,0.5]}),
    )
    .unwrap()["clip_id"]
        .clone();
    dispatch(&mut s,"add_effect",&json!({"clip_id":c,"kind":"pixel_expr","strings":{"expr":"vec4(sin(r)+pow(g,2)+mix(r,b,0.5),g,b,a)"}})).unwrap();
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":4})).unwrap();
    let r = kimo_core::commands::dispatch_ex(&mut s, "read_pixels", &json!({"time":0.5}))
        .err()
        .unwrap();
    assert!(r.message.contains("expr_max_ops"), "{r:?}");
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":512})).unwrap();
    assert!(dispatch(&mut s, "read_pixels", &json!({"time":0.5})).is_ok());
}
#[test]
fn expression_goldens() {
    let _globals = shared();
    let f = frame();
    for (name, expr) in [
        ("gradient", "vec4(u,v,0,1)"),
        ("invert", "vec4(1-r,1-g,1-b,a)"),
        ("noise", "noise(42,x,y)"),
    ] {
        let out = apply(&compile(expr, 512).unwrap(), &f, None, [0.; 8], 0.).unwrap();
        let b: Vec<u8> = out.d.iter().flat_map(|v| v.to_le_bytes()).collect();
        let hash = kimo_core::deps::sha256_hex(&b);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
        let path = root.join(format!("expr_{name}.txt"));
        if std::env::var("KIMO_EXPR_RECORD").is_ok() {
            std::fs::write(&path, format!("{hash}\n")).unwrap()
        } else {
            assert_eq!(std::fs::read_to_string(path).unwrap().trim(), hash)
        }
    }
}
#[test]
fn cost_20ops_1080p_report() {
    let _globals = exclusive();
    let _threads = RestoreThreads;
    let f = Frame {
        w: 1920,
        h: 1080,
        d: vec![0.2; 1920 * 1080 * 4],
    };
    let p = compile("sin(r)+pow(g,2)+mix(r,b,0.5)+sin(t)+p0+u*v", 512).unwrap();
    let mut first = None;
    for threads in [1, 4] {
        kimo_core::par::set_threads(threads);
        let t = std::time::Instant::now();
        let out = apply(&p, &f, None, [0.; 8], 0.).unwrap();
        eprintln!(
            "EXPR_PRODUCTION_1080P ops={} threads={} elapsed={}",
            p.ops,
            threads,
            t.elapsed().as_secs_f64()
        );
        if let Some(v) = &first {
            assert_eq!(v, &out.d)
        } else {
            first = Some(out.d)
        }
    }
    kimo_core::par::set_threads(0);
}

#[test]
fn slices_expression_reproduces_reference() {
    let _globals = shared();
    let f = frame();
    let expr = "sample(clamp(x+floor(y/4),0,w-1)/(w-1),v)";
    let o = apply(&compile(expr, 512).unwrap(), &f, None, [0.; 8], 0.).unwrap();
    for y in 0..f.h {
        for x in 0..f.w {
            let sx = (x + y / 4).min(f.w - 1);
            for j in 0..4 {
                assert!(
                    (o.d[(y * f.w + x) * 4 + j] - f.d[(y * f.w + sx) * 4 + j]).abs() < 1. / 255.
                )
            }
        }
    }
}
#[test]
fn timeout_and_pixel_cap_edges() {
    let _globals = shared();
    let f = Frame {
        w: 1920,
        h: 1080,
        d: vec![0.1; 1920 * 1080 * 4],
    };
    let p = compile("sin(r)+pow(g,2)+mix(r,b,0.5)+sin(t)+p0+u*v", 512).unwrap();
    let mut l = kimo_core::resource_limits::Limits::defaults();
    l.expr_timeout_ms = Some(1);
    let _g = kimo_core::resource_limits::install(l);
    let e = apply(&p, &f, None, [0.; 8], 0.).err().unwrap();
    assert!(e.contains("expr_timeout_ms"));
    let structured = kimo_core::errors::ToolError::from(e);
    assert_eq!(structured.code, "timeout");
    assert_eq!(structured.param.as_deref(), Some("expr_timeout_ms"));
}
#[test]
fn default_primitives_identity_and_describe() {
    let _globals = shared();
    for kind in ["pixel_expr", "displace", "convolution", "grade"] {
        let mut f = frame();
        let expected = f.d.clone();
        kimo_core::effects::apply(
            kind,
            &Default::default(),
            &Default::default(),
            &mut f,
            0.,
            0,
        )
        .unwrap();
        assert_eq!(f.d, expected, "{kind}");
        let mut s = Session::new();
        let d = dispatch(&mut s, "effect_describe", &json!({"kind":kind})).unwrap();
        assert!(!d["formula"].as_str().unwrap().is_empty());
    }
}
#[test]
fn convolution_cap_and_failure_leave_pixels_unchanged() {
    let _globals = shared();
    let mut f = frame();
    let before = f.d.clone();
    let mut strings = std::collections::BTreeMap::new();
    strings.insert("kernel".into(), "[1,2,3,4]".into());
    assert!(
        kimo_core::effects::apply("convolution", &Default::default(), &strings, &mut f, 0., 1)
            .is_err()
    );
    assert_eq!(f.d, before);
    strings.insert("kernel".into(), "[1]".into());
    let mut p = kimo_core::effects::Params::new();
    p.insert("divisor".into(), kimo_core::keyframe::Prop::new(0.));
    assert!(kimo_core::effects::apply("convolution", &p, &strings, &mut f, 0., 1).is_err());
    assert_eq!(f.d, before);
}
#[test]
fn odd_and_single_pixel_edges_thread_identical() {
    let _globals = exclusive();
    let _threads = RestoreThreads;
    for (w, h) in [(1, 1), (1, 19), (17, 1), (17, 19), (33, 31)] {
        let src = Frame {
            w,
            h,
            d: (0..w * h * 4).map(|i| (i % 17) as f32 / 16.).collect(),
        };
        let p = compile("sample(u,v)+vec4(x/(w+1),y/(h+1),0,0)", 512).unwrap();
        kimo_core::par::set_threads(1);
        let a = apply(&p, &src, None, [0.; 8], 0.).unwrap();
        kimo_core::par::set_threads(4);
        let b = apply(&p, &src, None, [0.; 8], 0.).unwrap();
        assert_eq!(a.d, b.d, "{w}x{h} expression seam");
        for edge in ["clamp", "wrap", "transparent"] {
            let mut p = kimo_core::effects::Params::new();
            p.insert("amount_x".into(), kimo_core::keyframe::Prop::new(0.25));
            p.insert("amount_y".into(), kimo_core::keyframe::Prop::new(-0.25));
            let mut s = std::collections::BTreeMap::new();
            s.insert("edge".into(), edge.into());
            s.insert("expr".into(), "vec4(1,1,0,1)".into());
            let mut a = Frame {
                w,
                h,
                d: src.d.clone(),
            };
            let mut b = Frame {
                w,
                h,
                d: src.d.clone(),
            };
            kimo_core::par::set_threads(1);
            kimo_core::effects::apply("displace", &p, &s, &mut a, 0., 1).unwrap();
            kimo_core::par::set_threads(4);
            kimo_core::effects::apply("displace", &p, &s, &mut b, 0., 1).unwrap();
            assert_eq!(a.d, b.d, "{w}x{h} {edge} seam");
            for y in 0..h {
                for x in 0..w {
                    let expected = kimo_core::pixel_expr::sample(
                        &src,
                        x as f32 / (w - 1).max(1) as f32 + 0.125,
                        y as f32 / (h - 1).max(1) as f32 - 0.125,
                        edge,
                    );
                    for c in 0..4 {
                        assert!((a.d[(y * w + x) * 4 + c] - expected[c]).abs() < 1e-6)
                    }
                }
            }
        }
    }
    kimo_core::par::set_threads(0);
}
