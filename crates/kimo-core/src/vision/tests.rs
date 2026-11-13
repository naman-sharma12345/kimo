//! Vision tests. Offline by default: the model list, the cache dir rule, the download path against a local HTTP server
//! (verified, resumed, tampered file refused), and the saliency fallback with no models. `online_models_on_test_image`
//! (ignored; run alone with `cargo test -p kimo-core --lib vision::tests::online -- --ignored`) downloads the four real
//! models into a fresh dir and runs them on the bundled test image (NASA portrait of Neil Armstrong, 1969, public domain,
//! via Wikimedia Commons "Neil_Armstrong_pose.jpg", scaled to 176x220).
use super::*;
use std::ffi::OsString;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_vision_{}_{}", name, std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }

/// The bundled test picture as a Frame (binary PPM, display-referred).
pub fn test_image() -> Frame {
    let b = include_bytes!("testdata/astronaut_176x220.ppm");
    let mut parts = b.splitn(4, |c| *c == b'\n'); let magic = parts.next().unwrap(); assert_eq!(magic, b"P6");
    let dims = std::str::from_utf8(parts.next().unwrap()).unwrap(); let mut it = dims.split_whitespace().map(|v| v.parse::<usize>().unwrap()); let (w, h) = (it.next().unwrap(), it.next().unwrap());
    parts.next().unwrap(); let px = parts.next().unwrap(); assert_eq!(px.len(), w * h * 3);
    let mut d = vec![0.0f32; w * h * 4]; for i in 0..w * h { for c in 0..3 { d[i * 4 + c] = px[i * 3 + c] as f32 / 255.0; } d[i * 4 + 3] = 1.0; }
    Frame { w, h, d }
}

/// A small white disc (r 8 at 115, 35) on lightly grained grey (as in the saliency_map test).
fn square_frame() -> Frame {
    let (w, h) = (160usize, 90usize); let mut d = vec![0.0f32; w * h * 4];
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; let on = (x as f32 - 115.0).powi(2) + (y as f32 - 35.0).powi(2) <= 64.0;
        let g = ((((x * 73856093) ^ (y * 19349663)) % 9) as f32 - 4.0) / 255.0; let v = if on { 1.0 } else { 0.5 + g }; d[i] = v; d[i + 1] = v; d[i + 2] = v; d[i + 3] = 1.0; } }
    Frame { w, h, d }
}

#[test] fn model_list_is_pinned() {
    let r = models::registry();
    let ids: Vec<&str> = r.iter().map(|m| m.id.as_str()).collect(); assert_eq!(ids, ["yunet", "yolox_nano", "u2netp", "modnet"]);
    for m in r { assert_eq!(m.sha512.len(), 128); assert!(m.size > 100_000 && m.size < 10_000_000, "{} is small", m.id); assert!(m.url.starts_with("https://")); assert!(["MIT", "Apache-2.0"].contains(&m.license.as_str()), "{} licence {}", m.id, m.license); }
    assert!(models::parse_models("[[model]]\nid = \"x\"\n").is_err());
}

#[test] fn cache_dir_uses_userprofile_when_home_is_unset() {
    let p = models::cache_dir_from(None, None, Some(OsString::from("C:\\Users\\me")));
    assert_eq!(p, PathBuf::from("C:\\Users\\me").join(".cache").join("kimo_models").join("vision"));
    assert_eq!(models::cache_dir_from(None, Some(OsString::from("/home/me")), Some(OsString::from("x"))), PathBuf::from("/home/me/.cache/kimo_models/vision"));
    assert_eq!(models::cache_dir_from(Some(OsString::from("/o")), Some(OsString::from("/home/me")), None), PathBuf::from("/o"));
    assert_eq!(models::cache_dir_from(None, Some(OsString::new()), Some(OsString::from("/up"))), PathBuf::from("/up/.cache/kimo_models/vision"));
}

#[test] fn masks_measure_in_frame_pixels() {
    let mut data = vec![0.0f32; 10 * 10]; for y in 2..4 { for x in 5..8 { data[y * 10 + x] = 1.0; } }
    let m = Mask { w: 10, h: 10, data, frame_w: 200, frame_h: 100, model: "test", fallback: None };
    assert_eq!(m.bbox(0.5), Some(BBox { x: 100.0, y: 20.0, w: 60.0, h: 20.0 }));
    assert!((m.area_frac(0.5) - 0.06).abs() < 1e-6); assert_eq!(m.at(110.0, 25.0), 1.0); assert_eq!(m.at(10.0, 10.0), 0.0);
    assert!((m.mass_inside(&BBox { x: 0.0, y: 0.0, w: 120.0, h: 100.0 }) - 1.0 / 3.0).abs() < 1e-5);
    let c = m.centroid().unwrap(); assert!((c[0] - 130.0).abs() < 1e-3 && (c[1] - 30.0).abs() < 1e-3, "{c:?}");
}

#[cfg(feature = "vision")]
mod download {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    /// Serve `body` on 127.0.0.1 for `conns` connections, honouring `Range: bytes=N-`. When `drop_first` is set the first
    /// connection announces the full length but closes after half the bytes. Returns the URL and the Range headers seen.
    fn serve(body: Vec<u8>, conns: usize, drop_first: bool) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let url = format!("http://{}/m.onnx", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![])); let s2 = seen.clone();
        std::thread::spawn(move || { for k in 0..conns { let Ok((mut c, _)) = l.accept() else { return };
            let mut r = BufReader::new(c.try_clone().unwrap()); let mut from = 0usize;
            loop { let mut line = String::new(); if r.read_line(&mut line).unwrap_or(0) == 0 { break; } let t = line.trim(); if t.is_empty() { break; }
                if let Some(v) = t.to_ascii_lowercase().strip_prefix("range: bytes=") { s2.lock().unwrap().push(v.to_string()); from = v.trim_end_matches('-').parse().unwrap_or(0); } }
            let part = &body[from.min(body.len())..];
            let head = if from > 0 { format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", part.len(), from, body.len() - 1, body.len()) }
                       else { format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len()) };
            let _ = c.write_all(head.as_bytes());
            let n = if drop_first && k == 0 { part.len() / 2 } else { part.len() };
            let _ = c.write_all(&part[..n]); let _ = c.flush(); drop(c); let mut sink = vec![]; let _ = r.read_to_end(&mut sink);
        } });
        (url, seen)
    }

    fn sha512(b: &[u8]) -> String { use sha2::{Digest, Sha512}; Sha512::digest(b).iter().map(|x| format!("{x:02x}")).collect() }
    fn spec(url: &str, body: &[u8]) -> models::ModelSpec { models::ModelSpec { id: "t".into(), file: "m.onnx".into(), url: url.into(), size: body.len() as u64, sha512: sha512(body), license: "MIT".into(), source: "test".into() } }
    fn body() -> Vec<u8> { (0..200_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect() }

    #[test] fn verified_download_and_resume() {
        let b = body(); let (url, seen) = serve(b.clone(), 3, true); let d = tmp("dl");
        let p = models::ensure_in(&d, &spec(&url, &b), false).expect("download");
        assert_eq!(std::fs::read(&p).unwrap(), b); assert!(!d.join("m.onnx.part").exists());
        assert_eq!(seen.lock().unwrap().as_slice(), [format!("{}-", b.len() / 2)], "second request resumed from the half already on disk");
        // in place and verified: offline is fine now
        assert_eq!(models::ensure_in(&d, &spec(&url, &b), true).unwrap(), p);
    }

    #[test] fn tampered_download_is_refused() {
        let b = body(); let mut bad = b.clone(); bad[1234] ^= 0xff;
        let (url, _) = serve(bad, 1, false); let d = tmp("tamper_dl");
        match models::ensure_in(&d, &spec(&url, &b), false) { Err(models::ModelError::Tampered { got, expected, .. }) => { assert_ne!(got, expected); } r => panic!("expected a refusal, got {r:?}") }
        assert!(!d.join("m.onnx").exists() && !d.join("m.onnx.part").exists(), "nothing tampered is left on disk");
    }

    #[test] fn tampered_file_on_disk_is_refused_and_removed() {
        let b = body(); let d = tmp("tamper_disk"); let mut bad = b.clone(); bad[0] ^= 1; std::fs::write(d.join("m.onnx"), &bad).unwrap();
        let s = spec("http://127.0.0.1:9/never", &b);
        assert!(matches!(models::ensure_in(&d, &s, true), Err(models::ModelError::Tampered { .. })));
        assert!(!d.join("m.onnx").exists(), "the tampered file was removed");
        // the right file is accepted; then swapping its bytes in place (same size) is caught on the next process start
        std::fs::write(d.join("m.onnx"), &b).unwrap(); assert!(models::ensure_in(&d, &s, true).is_ok());
        assert!(matches!(models::ensure_in(&d, &s, true), Ok(_)));
        // a real model file that was truncated is refused too
        std::fs::write(d.join("m.onnx"), &b[..1000]).unwrap(); assert!(matches!(models::ensure_in(&d, &s, true), Err(models::ModelError::Tampered { .. })));
    }

    #[test] fn offline_with_no_file_says_so() {
        let b = body(); let d = tmp("offline_nofile");
        assert!(matches!(models::ensure_in(&d, &spec("http://127.0.0.1:9/x", &b), true), Err(models::ModelError::Offline(_))));
        // unreachable server: an offline error, not a hang
        let e = models::ensure_in(&d, &spec("http://127.0.0.1:9/x", &b), false).unwrap_err(); assert!(matches!(e, models::ModelError::Offline(_)), "{e:?}");
    }

    #[test] fn http_error_is_reported() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let url = format!("http://{}/gone", l.local_addr().unwrap());
        std::thread::spawn(move || { if let Ok((mut c, _)) = l.accept() { let mut buf = [0u8; 1024]; let _ = c.read(&mut buf); let _ = c.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); } });
        let b = body(); let d = tmp("http404");
        assert!(matches!(models::ensure_in(&d, &spec(&url, &b), false), Err(models::ModelError::Http(404, _))));
    }
}

/// With no models (offline, empty cache dir) the detectors say so and the subject falls back to saliency, which still
/// finds the bright square. This test sets KIMO_OFFLINE and KIMO_VISION_DIR for the test process.
#[test] fn offline_falls_back_to_saliency() {
    let d = tmp("offline_fallback"); std::env::set_var("KIMO_VISION_DIR", &d); std::env::set_var("KIMO_OFFLINE", "1");
    #[cfg(feature = "vision")] infer::forget_failures();
    clear_cache();
    let f = square_frame();
    let fs = detect_faces(&f, None); assert!(fs.value.is_empty()); assert!(fs.fallback.as_deref().unwrap_or("").contains("unavailable"), "{:?}", fs.fallback);
    let ob = detect_objects(&f, None); assert!(ob.value.is_empty() && ob.fallback.is_some());
    let s = detect_subject(&f, None);
    assert_eq!(s.model, "saliency"); assert!(s.fallback.as_deref().unwrap().contains("saliency"), "{:?}", s.fallback);
    let b = s.value.bbox(0.5).expect("saliency finds the disc");
    let near = BBox { x: 80.0, y: 0.0, w: 70.0, h: 70.0 };
    assert!(b.frac_inside(&near) > 0.8, "{b:?}");
    let c = s.value.centroid().unwrap(); assert!((c[0] - 115.0).abs() < 20.0 && (c[1] - 35.0).abs() < 20.0, "{c:?}");
    assert!(subject_mask(&f).fallback.is_some()); assert!(faces(&f).is_empty());
    // no model file was written
    assert_eq!(std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0), 0);

    // and through the tool: fallback named, subject box on the shape, annotated png written
    let mut ss = crate::commands::Session::new();
    let c = |ss: &mut crate::commands::Session, n: &str, a: serde_json::Value| crate::commands::dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"));
    c(&mut ss, "project_set", serde_json::json!({"width": 320, "height": 180, "fps": 25}));
    let t0 = c(&mut ss, "add_track", serde_json::json!({}))["track_id"].clone();
    c(&mut ss, "add_clip", serde_json::json!({"track_id": t0, "source_type": "solid", "color": [0.05, 0.05, 0.06], "start": 0.0, "duration": 2.0}));
    let t1 = c(&mut ss, "add_track", serde_json::json!({}))["track_id"].clone();
    c(&mut ss, "add_shape", serde_json::json!({"track_id": t1, "shape": "rect", "width": 60, "height": 60, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 2.0}));
    let png = d.join("detect.png");
    let r = c(&mut ss, "detect", serde_json::json!({"time": 0.5, "what": ["faces", "subject"], "path": png.to_str().unwrap()}));
    eprintln!("detect offline: {r}");
    assert_eq!(r["canvas"], serde_json::json!([320, 180])); assert_eq!(r["faces"], serde_json::json!([])); assert_eq!(r["models"]["subject"], "saliency");
    assert!(r["fallback"].as_array().map(|a| a.len() == 2).unwrap_or(false), "{r}");
    let bx: Vec<f64> = r["subject"]["box"].as_array().expect("a subject box").iter().map(|v| v.as_f64().unwrap()).collect();
    let (cx, cy) = (bx[0] + bx[2] / 2.0, bx[1] + bx[3] / 2.0); assert!((cx - 160.0).abs() < 30.0 && (cy - 90.0).abs() < 30.0, "{r}");
    assert!(std::fs::metadata(&png).map(|m| m.len() > 0).unwrap_or(false), "annotated png");
    assert!(crate::commands::dispatch(&mut ss, "detect", &serde_json::json!({"time": 0.5, "what": ["cats"]})).is_err());
    std::env::remove_var("KIMO_OFFLINE"); std::env::remove_var("KIMO_VISION_DIR");
}

/// Downloads the four real models (about 15 MB) into a fresh dir and runs them on the bundled portrait.
#[cfg(feature = "vision")]
#[test] #[ignore] fn online_models_on_test_image() {
    let d = tmp("online"); std::env::set_var("KIMO_VISION_DIR", &d); std::env::remove_var("KIMO_OFFLINE");
    infer::forget_failures(); clear_cache();
    // every model downloads and verifies
    for m in models::registry() { let p = models::ensure(m).unwrap_or_else(|e| panic!("{}: {e}", m.id)); assert_eq!(std::fs::metadata(&p).unwrap().len(), m.size); assert_eq!(models::sha512_file(&p).unwrap(), m.sha512); }
    let f = test_image();
    let t0 = std::time::Instant::now(); let fs = detect_faces(&f, None); eprintln!("faces {:?} in {:?}", fs.value, t0.elapsed());
    assert!(fs.fallback.is_none(), "{:?}", fs.fallback); assert!(!fs.value.is_empty(), "a face in the portrait");
    let face = &fs.value[0]; let c = face.bbox.center();
    // the face is in the upper half, centred-ish; eyes above the mouth
    assert!(c[1] < f.h as f32 * 0.5 && (c[0] - f.w as f32 / 2.0).abs() < f.w as f32 * 0.3, "{face:?}");
    assert!(face.landmarks[0][1] < face.landmarks[3][1] && face.landmarks[1][1] < face.landmarks[4][1], "{face:?}");
    assert!(detect_faces(&f, None).cached);
    let t0 = std::time::Instant::now(); let ob = detect_objects(&f, None); eprintln!("objects {:?} in {:?}", ob.value, t0.elapsed());
    let ppl = people(&f); assert!(!ppl.is_empty(), "a person"); assert!(face.bbox.frac_inside(&ppl[0].bbox) > 0.8, "face inside the person box");
    let t0 = std::time::Instant::now(); let s = detect_subject(&f, None); eprintln!("subject {:?} {:?} in {:?}", s.value.bbox(0.5), s.value.area_frac(0.5), t0.elapsed());
    assert_eq!(s.model, "u2netp"); assert!(s.fallback.is_none());
    assert!(s.value.at(c[0], c[1]) > 0.5, "the face is part of the subject"); assert!(s.value.area_frac(0.5) > 0.1);
    let t0 = std::time::Instant::now(); let m = detect_matte(&f, None); eprintln!("matte {:?} {:?} in {:?}", m.value.bbox(0.5), m.value.area_frac(0.5), t0.elapsed());
    assert_eq!(m.model, "modnet"); assert!(m.value.at(c[0], c[1]) > 0.5, "the face is inside the person matte");
    std::env::remove_var("KIMO_VISION_DIR");
}
