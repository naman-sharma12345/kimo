//! v7 GUI (FEATURES.md L562, built LAST as instructed): a minimal local web UI over the same tool
//! registry/dispatch the CLI and MCP server use -- no second dispatch path (`run_tool`, shared with main.rs).
//! `kimo gui` serves it. Requirements this meets: a timeline view of the project (timeline_view_model,
//! served at GET /api/timeline), a preview frame rendered by the actual engine (GET /preview.png?t=..,
//! through render::render_png -- the same function render_frame/export use), and a tool-invocation panel
//! (POST /api/call -> run_tool, the same path the CLI's `call` subcommand and the MCP server use).
//!
//! `handle_request` is a pure function of (Session, method, path, query, body) with no socket anywhere in
//! it, so every route is unit-tested headless (tests/gui_tests.rs) without ever binding a port; only
//! `serve`'s outer accept loop (untested here, it is a thin and unremarkable std::net::TcpListener read/
//! write loop) touches a real socket.
use kimo_core::commands::{registry_ref, Session};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn source_kind(c: &kimo_core::model::Clip) -> String {
    serde_json::to_value(&c.source).ok().and_then(|v| v.get("type").and_then(|x| x.as_str().map(String::from))).unwrap_or_default()
}

/// The timeline view-model: tracks and their clips, with the fields a timeline UI actually needs to lay
/// out bars on a ruler (start/duration/end in project seconds) and label them (name/role/source kind).
pub fn timeline_view_model(ss: &Session) -> Value {
    let p = &ss.project;
    let tracks: Vec<Value> = p.tracks.iter().map(|t| {
        let clips: Vec<Value> = t.clips.iter().map(|c| json!({
            "id": c.id, "name": c.name, "role": c.role, "source_kind": source_kind(c),
            "start": c.start, "duration": c.duration, "end": c.start + c.duration,
        })).collect();
        json!({"id": t.id, "name": t.name, "clip_count": clips.len(), "clips": clips})
    }).collect();
    // v7.3: markers, sorted by time (same order list_markers already returns), for the timeline's marker
    // ticks -- the GUI's own display, not a second source of truth (add_marker/list_markers/remove_marker
    // are untouched; this just surfaces what they already hold).
    let mut markers: Vec<Value> = p.markers.iter().map(|m| json!({"id": m.id, "time": m.time, "duration": m.duration, "name": m.name, "color": m.color})).collect();
    markers.sort_by(|a, b| a["time"].as_f64().unwrap_or(0.0).partial_cmp(&b["time"].as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    json!({"width": p.width, "height": p.height, "fps": p.fps, "duration": p.duration(), "track_count": tracks.len(), "tracks": tracks, "markers": markers})
}

/// Full detail for one clip -- the effect-params panel's data: id/name/role/timing, every property's base
/// value (not its keyframes; this panel edits base values the same way set_property does without a time),
/// and every effect with its id, kind and params (again base values) so the panel can list and edit them
/// through the same set_effect_param the tool-invocation panel already exposes generically.
pub fn clip_detail(ss: &Session, clip_id: &str) -> Option<Value> {
    let c = ss.project.find_clip(clip_id)?;
    let props: Value = json!(c.props.iter().map(|(k, p)| (k.clone(), json!(p.base))).collect::<serde_json::Map<_, _>>());
    let effects: Vec<Value> = c.effects.iter().map(|e| json!({
        "id": e.id, "kind": e.kind,
        "params": e.params.iter().map(|(k, p)| (k.clone(), json!(p.base))).collect::<serde_json::Map<_, _>>(),
        "strings": e.strings,
    })).collect();
    Some(json!({"id": c.id, "name": c.name, "role": c.role, "start": c.start, "duration": c.duration, "end": c.start + c.duration, "source_kind": source_kind(c), "props": props, "effects": effects}))
}

/// Every registered tool, name + first line of its description + mutates, for the tool-invocation panel's
/// picker (the full schema is available per-tool at GET /api/tool/<name> so the picker stays small).
pub fn registry_summary() -> Value {
    json!(registry_ref().iter().map(|c| { let first = c.description.split(". ").next().unwrap_or(&c.description); json!({"name": c.name, "summary": first, "mutates": c.mutates}) }).collect::<Vec<_>>())
}

pub struct Response { pub status: u16, pub content_type: &'static str, pub body: Vec<u8> }
fn json_response(status: u16, v: Value) -> Response { Response { status, content_type: "application/json", body: serde_json::to_vec(&v).unwrap_or_default() } }
fn err_response(status: u16, msg: impl Into<String>) -> Response { json_response(status, json!({"error": msg.into()})) }

const INDEX_HTML: &str = include_str!("gui_index.html");
/// v7.3: the keyboard-shortcut resolver (space/J/K/L/Delete/S/undo/redo), served as a real static file so
/// gui_index.html's <script src="/gui_keymap.js"> and tests/js/keymap_test.js (Node, headless) run the
/// exact same bytes -- one mapping, not two to keep in sync.
const KEYMAP_JS: &str = include_str!("gui_keymap.js");
/// v7.3: the timeline-snapping helper (nearest clip edge / marker / 0, within tolerance), served the same
/// way and for the same reason -- gui_index.html's drag handlers and tests/js/snap_test.js (Node) run the
/// exact same bytes.
const SNAP_JS: &str = include_str!("gui_snap.js");

/// Parses `?t=1.5` (or any single-key query) out of a raw query string; used for /preview.png's `t`.
fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| { let (k, v) = kv.split_once('=')?; if k == key { Some(v) } else { None } })
}

/// The whole routing table. No socket, no I/O beyond what a route explicitly needs (render_png writes a
/// temp file and reads it back; everything else is pure JSON over the in-memory Session). This is what
/// tests/gui_tests.rs calls directly.
pub fn handle_request(ss: &Arc<Mutex<Session>>, method: &str, path: &str, query: &str, body: &[u8]) -> Response {
    match (method, path) {
        ("GET", "/") => Response { status: 200, content_type: "text/html; charset=utf-8", body: INDEX_HTML.as_bytes().to_vec() },
        ("GET", "/gui_keymap.js") => Response { status: 200, content_type: "application/javascript; charset=utf-8", body: KEYMAP_JS.as_bytes().to_vec() },
        ("GET", "/gui_snap.js") => Response { status: 200, content_type: "application/javascript; charset=utf-8", body: SNAP_JS.as_bytes().to_vec() },
        ("GET", "/api/timeline") => { let ss = ss.lock().unwrap(); json_response(200, timeline_view_model(&ss)) }
        ("GET", "/api/registry") => json_response(200, registry_summary()),
        ("GET", "/api/clip") => { // ?id=<clip_id>
            let Some(id) = query_param(query, "id") else { return err_response(400, "missing_arg: id") };
            let ss = ss.lock().unwrap();
            match clip_detail(&ss, id) { Some(v) => json_response(200, v), None => err_response(404, format!("unknown_clip: no clip '{id}'")) }
        }
        ("GET", "/api/tool") => { // ?name=<tool>
            let Some(name) = query_param(query, "name") else { return err_response(400, "missing_arg: name") };
            match registry_ref().iter().find(|c| c.name == name) {
                Some(c) => json_response(200, json!({"name": c.name, "description": c.description, "schema": c.schema, "mutates": c.mutates})),
                None => err_response(404, format!("unknown_tool: no tool named '{name}'")),
            }
        }
        ("GET", "/preview.png") => {
            let t: f64 = query_param(query, "t").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let ss = ss.lock().unwrap();
            let dir = std::env::temp_dir().join("kimo_gui_preview");
            if std::fs::create_dir_all(&dir).is_err() { return err_response(500, "could not create preview temp dir"); }
            let path = dir.join(format!("preview_{}.png", std::process::id()));
            let path_str = match path.to_str() { Some(s) => s, None => return err_response(500, "non-utf8 temp path") };
            let mut cache = kimo_core::render::Cache::new();
            match kimo_core::render::render_png(&ss.project, t, path_str, &mut cache) {
                Ok(()) => match std::fs::read(&path) { Ok(bytes) => Response { status: 200, content_type: "image/png", body: bytes }, Err(e) => err_response(500, format!("rendered but could not read back: {e}")) },
                Err(e) => err_response(500, format!("render failed: {e}")),
            }
        }
        ("POST", "/api/call") => {
            let v: Value = match serde_json::from_slice(body) { Ok(v) => v, Err(e) => return err_response(400, format!("body is not valid JSON: {e}")) };
            let Some(tool) = v.get("tool").and_then(|v| v.as_str()) else { return err_response(400, "missing_arg: tool") };
            let args = v.get("args").cloned().unwrap_or(json!({}));
            match crate::run_tool(ss, tool, &args) {
                Ok(result) => json_response(200, json!({"ok": true, "result": result})),
                Err(e) => json_response(200, json!({"ok": false, "error": e})),
            }
        }
        (_, "/") | (_, "/gui_keymap.js") | (_, "/gui_snap.js") | (_, "/api/timeline") | (_, "/api/registry") | (_, "/api/tool") | (_, "/api/clip") | (_, "/preview.png") | (_, "/api/call") => err_response(405, format!("method {method} not allowed on {path}")),
        _ => err_response(404, format!("no route for {path}")),
    }
}

/// Reads one HTTP/1.1 request off `stream` (request line, headers for Content-Length, then the body) --
/// minimal by design (no chunked transfer, no keep-alive): this server is for one person's browser on
/// localhost, not a public endpoint. Returns (method, path, query, body).
fn read_request(stream: &mut std::net::TcpStream) -> std::io::Result<(String, String, String, Vec<u8>)> {
    use std::io::{BufRead, BufReader, Read};
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new(); reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = target.split_once('?').map(|(p, q)| (p.to_string(), q.to_string())).unwrap_or((target, String::new()));
    let mut content_length = 0usize;
    loop { let mut h = String::new(); if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" { break; }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") { content_length = v.trim().parse().unwrap_or(0); } }
    let mut body = vec![0u8; content_length]; if content_length > 0 { reader.read_exact(&mut body)?; }
    Ok((method, path, query, body))
}

fn write_response(stream: &mut std::net::TcpStream, r: &Response) -> std::io::Result<()> {
    use std::io::Write;
    let status_text = match r.status { 200 => "OK", 400 => "Bad Request", 404 => "Not Found", 405 => "Method Not Allowed", 500 => "Internal Server Error", _ => "Unknown" };
    write!(stream, "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", r.status, status_text, r.content_type, r.body.len())?;
    stream.write_all(&r.body)
}

/// Blocks forever accepting one connection at a time (a local single-user tool; no concurrency needed).
pub fn serve(ss: Arc<Mutex<Session>>, bind: &str, port: u16) -> std::io::Result<()> {
    let listener = std::net::TcpListener::bind((bind, port))?;
    eprintln!("kimo gui listening on http://{bind}:{port}");
    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let resp = match read_request(&mut stream) {
            Ok((method, path, query, body)) => handle_request(&ss, &method, &path, &query, &body),
            Err(e) => err_response(400, format!("malformed request: {e}")),
        };
        let _ = write_response(&mut stream, &resp);
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use kimo_core::commands::dispatch;

    fn fixture() -> Arc<Mutex<Session>> {
        let mut ss = Session::new();
        dispatch(&mut ss, "project_set", &json!({"width": 64, "height": 36, "fps": 25})).unwrap();
        let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
        dispatch(&mut ss, "add_shape", &json!({"track_id": tr, "shape": "rect", "start": 0.0, "duration": 2.0, "color": [1.0, 0.0, 0.0]})).unwrap();
        Arc::new(Mutex::new(ss))
    }

    #[test]
    fn timeline_view_model_lists_tracks_and_clips_with_timing() {
        let ss = fixture();
        let v = timeline_view_model(&ss.lock().unwrap());
        assert_eq!(v["width"], 64);
        assert_eq!(v["track_count"], 1);
        let clips = v["tracks"][0]["clips"].as_array().unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0]["start"], 0.0);
        assert_eq!(clips[0]["duration"], 2.0);
        assert_eq!(clips[0]["end"], 2.0);
        assert_eq!(clips[0]["source_kind"], "shape");
    }

    #[test]
    fn registry_summary_lists_every_tool_with_a_short_summary() {
        let v = registry_summary();
        let arr = v.as_array().unwrap();
        assert!(arr.len() > 100, "the whole registry should be listed, got {}", arr.len());
        let add_track = arr.iter().find(|c| c["name"] == "add_track").expect("add_track should be in the registry");
        assert!(!add_track["summary"].as_str().unwrap().is_empty());
        assert_eq!(add_track["mutates"], true);
    }

    #[test]
    fn query_param_reads_a_key_from_a_raw_query_string() {
        assert_eq!(query_param("t=1.5&x=2", "t"), Some("1.5"));
        assert_eq!(query_param("t=1.5&x=2", "x"), Some("2"));
        assert_eq!(query_param("t=1.5", "missing"), None);
        assert_eq!(query_param("", "t"), None);
    }

    #[test]
    fn handle_request_serves_the_index_page() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/html; charset=utf-8");
        assert!(String::from_utf8_lossy(&r.body).contains("kimo gui"));
    }

    #[test]
    fn handle_request_serves_the_timeline_json() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/api/timeline", "", &[]);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["track_count"], 1);
    }

    #[test]
    fn handle_request_serves_registry_and_one_tool_schema() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/api/registry", "", &[]);
        assert_eq!(r.status, 200);
        let r2 = handle_request(&ss, "GET", "/api/tool", "name=add_track", &[]);
        assert_eq!(r2.status, 200);
        let v: Value = serde_json::from_slice(&r2.body).unwrap();
        assert_eq!(v["name"], "add_track");
        assert!(v["schema"].is_object());
        let r3 = handle_request(&ss, "GET", "/api/tool", "name=not_a_real_tool_xyz", &[]);
        assert_eq!(r3.status, 404);
        let r4 = handle_request(&ss, "GET", "/api/tool", "", &[]);
        assert_eq!(r4.status, 400);
    }

    #[test]
    fn handle_request_renders_a_real_preview_frame_png() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/preview.png", "t=0.5", &[]);
        assert_eq!(r.status, 200, "{:?}", String::from_utf8_lossy(&r.body));
        assert_eq!(r.content_type, "image/png");
        assert!(r.body.len() > 8, "a real PNG should have real bytes");
        assert_eq!(&r.body[1..4], b"PNG", "the engine should have actually rendered a PNG, not a placeholder");
    }

    #[test]
    fn handle_request_dispatches_a_tool_call_and_it_changes_the_project() {
        let ss = fixture();
        let before = timeline_view_model(&ss.lock().unwrap())["track_count"].as_u64().unwrap();
        let body = serde_json::to_vec(&json!({"tool": "add_track", "args": {}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["ok"], true);
        let after = timeline_view_model(&ss.lock().unwrap())["track_count"].as_u64().unwrap();
        assert_eq!(after, before + 1, "the tool-invocation panel should actually run real tools through the same dispatch path");
    }

    #[test]
    fn handle_request_reports_a_tool_error_without_an_http_error() {
        // A bad tool call is a normal (200) response with ok:false -- the HTTP layer isn't the place to
        // encode tool-level failure, the same way the MCP server and the CLI's `call` both return it as data.
        let ss = fixture();
        let body = serde_json::to_vec(&json!({"tool": "delete_clip", "args": {"clip_id": "not_a_real_clip"}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].is_object() || v["error"].is_string(), "error should carry something useful: {v}");
        assert!(v["error"]["message"].as_str().map(|s| !s.is_empty()).unwrap_or(false), "{v}");
    }

    #[test]
    fn handle_request_rejects_malformed_call_bodies() {
        let ss = fixture();
        let r = handle_request(&ss, "POST", "/api/call", "", b"not json");
        assert_eq!(r.status, 400);
        let r2 = handle_request(&ss, "POST", "/api/call", "", b"{}");
        assert_eq!(r2.status, 400, "missing tool name should be a 400, not a dispatch attempt");
    }

    #[test]
    fn handle_request_404s_an_unknown_route_and_405s_a_wrong_method() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/nope", "", &[]);
        assert_eq!(r.status, 404);
        let r2 = handle_request(&ss, "POST", "/api/timeline", "", &[]);
        assert_eq!(r2.status, 405);
    }

    #[test]
    fn clip_detail_lists_props_and_effects_with_base_values() {
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        dispatch(&mut ss.lock().unwrap(), "add_effect", &json!({"clip_id": clip_id, "kind": "glow", "params": {"amount": 0.6}})).unwrap();
        let v = { let s = ss.lock().unwrap(); clip_detail(&s, &clip_id).expect("clip should exist") };
        assert_eq!(v["id"], clip_id); assert_eq!(v["duration"], 2.0); assert_eq!(v["end"], 2.0); assert_eq!(v["source_kind"], "shape");
        assert!(v["props"]["opacity"].as_f64().is_some(), "{v}");
        let effects = v["effects"].as_array().unwrap(); assert_eq!(effects.len(), 1);
        assert_eq!(effects[0]["kind"], "glow"); assert_eq!(effects[0]["params"]["amount"], 0.6);
        let s = ss.lock().unwrap(); assert!(clip_detail(&s, "not_a_real_clip").is_none());
    }

    #[test]
    fn handle_request_serves_clip_detail_and_404s_an_unknown_id() {
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        let r = handle_request(&ss, "GET", "/api/clip", &format!("id={clip_id}"), &[]);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["id"], clip_id);
        let r2 = handle_request(&ss, "GET", "/api/clip", "id=nope", &[]);
        assert_eq!(r2.status, 404);
        let r3 = handle_request(&ss, "GET", "/api/clip", "", &[]);
        assert_eq!(r3.status, 400);
        let r4 = handle_request(&ss, "POST", "/api/clip", "", &[]);
        assert_eq!(r4.status, 405);
    }

    #[test]
    fn undo_redo_and_history_list_round_trip_through_the_same_api_call_path() {
        // The History panel's own sequence: history_list, undo, history_list again, redo, history_list again.
        let ss = fixture();
        let baseline = { let mut s = ss.lock().unwrap(); dispatch(&mut s, "history_list", &json!({})).unwrap() };
        let n0 = baseline["undo"].as_array().unwrap().len(); // fixture() itself did project_set/add_track/add_shape

        let add_track_body = serde_json::to_vec(&json!({"tool": "add_track", "args": {}})).unwrap();
        handle_request(&ss, "POST", "/api/call", "", &add_track_body);
        let after_add = { let mut s = ss.lock().unwrap(); dispatch(&mut s, "history_list", &json!({})).unwrap() };
        let undo_steps = after_add["undo"].as_array().unwrap();
        assert_eq!(undo_steps.len(), n0 + 1); assert_eq!(undo_steps.last().unwrap()["tool"], "add_track", "the most recent step is the one undo reverts next");
        assert_eq!(after_add["redo"].as_array().unwrap().len(), 0);

        let track_count_before = timeline_view_model(&ss.lock().unwrap())["track_count"].as_u64().unwrap();
        let undo_body = serde_json::to_vec(&json!({"tool": "undo", "args": {}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &undo_body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        assert_eq!(timeline_view_model(&ss.lock().unwrap())["track_count"].as_u64().unwrap(), track_count_before - 1, "undo actually reverted the add_track");
        let after_undo = { let mut s = ss.lock().unwrap(); dispatch(&mut s, "history_list", &json!({})).unwrap() };
        assert_eq!(after_undo["undo"].as_array().unwrap().len(), n0);
        assert_eq!(after_undo["redo"].as_array().unwrap(), &vec![json!("add_track")], "redo now holds the undone step");

        let redo_body = serde_json::to_vec(&json!({"tool": "redo", "args": {}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &redo_body);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], true, "{v2}");
        assert_eq!(timeline_view_model(&ss.lock().unwrap())["track_count"].as_u64().unwrap(), track_count_before, "redo brought the track back");

        // undo with nothing to undo is a tool-level error (not an HTTP error), same shape the panel's alert() reads
        let r3 = handle_request(&ss, "POST", "/api/call", "", &serde_json::to_vec(&json!({"tool": "undo", "args": {"steps": 99}})).unwrap());
        let v3: Value = serde_json::from_slice(&r3.body).unwrap();
        assert_eq!(v3["ok"], true, "steps clamps to what's actually available rather than failing"); // one real undo step existed (the redo above)
    }

    #[test]
    fn the_index_page_carries_the_scrub_and_play_controls() {
        // Scrubbing/play is JS driving the existing /preview.png endpoint repeatedly (no new backend route,
        // same honest shape as the rest of this GUI); what's checkable without a browser is that the markup
        // and functions it depends on actually shipped in the page this server sends.
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        let body = String::from_utf8_lossy(&r.body);
        for needle in ["id=\"play-btn\"", "function togglePlay", "function scrubTo", "ph.id = 'playhead'", "id=\"undo-btn\"", "function doUndo", "function loadHistory"] {
            assert!(body.contains(needle), "index page should carry {needle:?}");
        }
    }

    #[test]
    fn the_editable_timeline_moves_and_trims_a_clip_through_the_same_api_call_path() {
        // The drag itself is mouse events in gui_index.html's JS (no DOM here); what's tested is the exact
        // sequence a move-drag and a trim-drag each end in: one /api/call, then a /api/timeline refetch.
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        let before = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(before["tracks"][0]["clips"][0]["start"], 0.0);

        // move-drag: drops the clip at a new start time
        let move_body = serde_json::to_vec(&json!({"tool": "move_clip", "args": {"clip_id": clip_id, "start": 3.5}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &move_body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        let after_move = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(after_move["tracks"][0]["clips"][0]["start"], 3.5);
        assert_eq!(after_move["tracks"][0]["clips"][0]["duration"], 2.0, "trim wasn't touched yet");

        // trim-drag: drags the right edge, changing only duration
        let trim_body = serde_json::to_vec(&json!({"tool": "trim_clip", "args": {"clip_id": clip_id, "duration": 1.2}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &trim_body);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], true, "{v2}");
        let after_trim = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(after_trim["tracks"][0]["clips"][0]["start"], 3.5, "move's position survives the trim");
        assert_eq!(after_trim["tracks"][0]["clips"][0]["duration"], 1.2);
        assert_eq!(after_trim["tracks"][0]["clips"][0]["end"], 4.7);

        // a too-short drag (<=3px in the JS, so no call at all) is simply never sent -- nothing to test at
        // this layer; a clip_id that no longer exists is the one real failure mode worth covering here:
        let bad = serde_json::to_vec(&json!({"tool": "move_clip", "args": {"clip_id": "not_a_real_clip", "start": 1.0}})).unwrap();
        let r3 = handle_request(&ss, "POST", "/api/call", "", &bad);
        let v3: Value = serde_json::from_slice(&r3.body).unwrap(); assert_eq!(v3["ok"], false, "a stale clip id reports a tool error, not an HTTP error");
    }

    #[test]
    fn the_effect_params_panel_round_trips_through_the_same_api_call_path() {
        // The panel itself is HTML/JS (no DOM in this test binary); what's tested here is the exact
        // sequence it drives: fetch clip detail, edit a param through /api/call, refetch and see it moved.
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        let eid = { let mut s = ss.lock().unwrap(); dispatch(&mut s, "add_effect", &json!({"clip_id": clip_id, "kind": "blur", "params": {"radius": 2.0}})).unwrap()["effect_id"].as_str().unwrap().to_string() };
        let before = { let s = ss.lock().unwrap(); clip_detail(&s, &clip_id).unwrap() };
        assert_eq!(before["effects"][0]["params"]["radius"], 2.0);
        let body = serde_json::to_vec(&json!({"tool": "set_effect_param", "args": {"clip_id": clip_id, "effect_id": eid, "param": "radius", "value": 9.0}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        assert_eq!(r.status, 200); let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true);
        let after = { let s = ss.lock().unwrap(); clip_detail(&s, &clip_id).unwrap() };
        assert_eq!(after["effects"][0]["params"]["radius"], 9.0, "the panel's own round trip actually moved the param: {after}");
    }

    // ---- v7.3: keyboard shortcuts ----

    #[test]
    fn gui_keymap_js_is_served_as_a_real_static_file() {
        // Served so the browser's <script src="/gui_keymap.js"> and tests/js/keymap_test.js (Node,
        // headless) run the exact same bytes -- one mapping, not two to keep in sync.
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/gui_keymap.js", "", &[]);
        assert_eq!(r.status, 200);
        assert!(r.content_type.contains("javascript"), "{}", r.content_type);
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("resolveKeyAction"), "should serve the real keymap module");
        assert!(body.contains("module.exports"), "must stay requireable from Node for the headless test");
    }

    #[test]
    fn gui_keymap_js_rejects_non_get() {
        let ss = fixture();
        let r = handle_request(&ss, "POST", "/gui_keymap.js", "", &[]);
        assert_eq!(r.status, 405);
    }

    #[test]
    fn the_index_page_wires_up_the_keyboard_shortcuts() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        let body = String::from_utf8(r.body).unwrap();
        for needle in ["/gui_keymap.js", "addEventListener('keydown'", "kimoKeymap.resolveKeyAction", "deleteSelectedClip", "splitAtPlayhead", "stepFrame"] {
            assert!(body.contains(needle), "index page should carry {needle:?}");
        }
    }

    #[test]
    fn delete_shortcut_removes_the_selected_clip_through_the_same_api_call_path() {
        // The keypress itself is DOM (tested in tests/js/keymap_test.js + the wiring check above); what's
        // tested here is the exact call deleteSelectedClip() makes and its effect on the project.
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        let before = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(before["tracks"][0]["clips"].as_array().unwrap().len(), 1);

        let body = serde_json::to_vec(&json!({"tool": "delete_clip", "args": {"clip_id": clip_id}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        let after = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(after["tracks"][0]["clips"].as_array().unwrap().len(), 0, "the clip should actually be gone: {after}");

        // a clip that's already gone (double-press, or stale selection) reports a tool error, not a crash
        let bad = serde_json::to_vec(&json!({"tool": "delete_clip", "args": {"clip_id": clip_id}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &bad);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], false);
    }

    #[test]
    fn split_shortcut_splits_the_selected_clip_at_the_playhead() {
        let ss = fixture(); // a 2s shape clip, start 0
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };

        let body = serde_json::to_vec(&json!({"tool": "split_clip", "args": {"clip_id": clip_id, "at": 1.25}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        let after = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        let clips = after["tracks"][0]["clips"].as_array().unwrap();
        assert_eq!(clips.len(), 2, "splitting should leave two clips: {after}");
        assert_eq!(clips[0]["duration"], 1.25);
        assert_eq!(clips[1]["start"], 1.25);
        assert_eq!(clips[1]["end"], 2.0);

        // a playhead outside the (new, shorter) clip is a plain tool error, not a crash -- this is the
        // path splitAtPlayhead()'s alert() surfaces to the person
        let outside = serde_json::to_vec(&json!({"tool": "split_clip", "args": {"clip_id": clips[0]["id"], "at": 5.0}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &outside);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], false);
    }

    #[test]
    fn undo_redo_shortcuts_use_the_same_history_tools_the_buttons_already_use() {
        // Ctrl+Z / Ctrl+Shift+Z call the same doUndo()/doRedo() the existing Undo/Redo buttons do --
        // nothing new to dispatch, just confirming the mapping lands on tools that actually exist and work.
        let ss = fixture();
        let clip_id = { let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].id.clone() };
        let move_body = serde_json::to_vec(&json!({"tool": "move_clip", "args": {"clip_id": clip_id, "start": 1.0}})).unwrap();
        handle_request(&ss, "POST", "/api/call", "", &move_body);
        assert_eq!({ let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].start }, 1.0);

        let undo_body = serde_json::to_vec(&json!({"tool": "undo", "args": {}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &undo_body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        assert_eq!({ let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].start }, 0.0, "ctrl+z's own tool should undo the move");

        let redo_body = serde_json::to_vec(&json!({"tool": "redo", "args": {}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &redo_body);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], true, "{v2}");
        assert_eq!({ let s = ss.lock().unwrap(); s.project.tracks[0].clips[0].start }, 1.0, "ctrl+shift+z's own tool should redo it");
    }

    // ---- v7.3: add/remove tracks ----

    #[test]
    fn the_index_page_wires_up_track_add_and_remove() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        let body = String::from_utf8(r.body).unwrap();
        for needle in ["+ Track", "onclick=\"addTrack()\"", "function addTrack", "function removeTrack", "delete_track"] {
            assert!(body.contains(needle), "index page should carry {needle:?}");
        }
    }

    #[test]
    fn add_track_button_adds_an_empty_track_through_the_same_api_call_path() {
        let ss = fixture();
        let before = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(before["track_count"], 1);

        let body = serde_json::to_vec(&json!({"tool": "add_track", "args": {}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");

        let after = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(after["track_count"], 2, "{after}");
        assert_eq!(after["tracks"][1]["clips"].as_array().unwrap().len(), 0, "a freshly added track starts empty");
    }

    #[test]
    fn remove_track_button_deletes_the_track_and_its_clips_through_the_same_api_call_path() {
        let ss = fixture();
        let (track2, clip2) = {
            let mut s = ss.lock().unwrap();
            let t2 = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
            let c2 = dispatch(&mut s, "add_shape", &json!({"track_id": t2, "shape": "rect", "start": 0.0, "duration": 1.0, "color": [0.0, 0.0, 1.0]})).unwrap()["clip_id"].as_str().unwrap().to_string();
            (t2, c2)
        };
        let before = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(before["track_count"], 2);
        let _ = clip2;

        let body = serde_json::to_vec(&json!({"tool": "delete_track", "args": {"track_id": track2}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");

        let after = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        assert_eq!(after["track_count"], 1, "the track (and its clip with it) should actually be gone: {after}");

        // a stale track id (double-click, already removed) reports a tool error, not a crash
        let bad = serde_json::to_vec(&json!({"tool": "delete_track", "args": {"track_id": track2}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &bad);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], false);
    }

    // ---- v7.3: marker display + add ----

    #[test]
    fn timeline_view_model_lists_markers_sorted_by_time() {
        let ss = fixture();
        { let mut s = ss.lock().unwrap();
          dispatch(&mut s, "add_marker", &json!({"time": 1.5, "name": "Second"})).unwrap();
          dispatch(&mut s, "add_marker", &json!({"time": 0.25, "name": "First"})).unwrap(); }
        let v = timeline_view_model(&ss.lock().unwrap());
        let markers = v["markers"].as_array().unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0]["name"], "First", "{v}");
        assert_eq!(markers[0]["time"], 0.25);
        assert_eq!(markers[1]["name"], "Second");
    }

    #[test]
    fn timeline_view_model_markers_is_empty_not_missing_when_there_are_none() {
        let ss = fixture();
        let v = timeline_view_model(&ss.lock().unwrap());
        assert_eq!(v["markers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn the_index_page_wires_up_marker_display_and_add() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        let body = String::from_utf8(r.body).unwrap();
        for needle in ["+ Marker", "marker-tick", "function addMarkerAtPlayhead", "function removeMarkerAt", "tl.markers", "add_marker", "remove_marker"] {
            assert!(body.contains(needle), "index page should carry {needle:?}");
        }
    }

    #[test]
    fn add_marker_button_adds_a_marker_at_the_given_time_through_the_same_api_call_path() {
        let ss = fixture();
        let body = serde_json::to_vec(&json!({"tool": "add_marker", "args": {"time": 0.9, "name": "beat"}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        let tl = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        let markers = tl["markers"].as_array().unwrap();
        assert_eq!(markers.len(), 1, "{tl}");
        assert_eq!(markers[0]["name"], "beat");
        assert_eq!(markers[0]["time"], 0.9);
    }

    #[test]
    fn remove_marker_tick_deletes_it_through_the_same_api_call_path() {
        let ss = fixture();
        let marker_id = { let mut s = ss.lock().unwrap(); dispatch(&mut s, "add_marker", &json!({"time": 0.5, "name": "x"})).unwrap()["marker_id"].as_str().unwrap().to_string() };
        assert_eq!({ let s = ss.lock().unwrap(); timeline_view_model(&s)["markers"].as_array().unwrap().len() }, 1);

        let body = serde_json::to_vec(&json!({"tool": "remove_marker", "args": {"marker_id": marker_id}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        assert_eq!({ let s = ss.lock().unwrap(); timeline_view_model(&s)["markers"].as_array().unwrap().len() }, 0, "the marker should actually be gone");

        // a stale marker id (double-click after it's already gone) reports a tool error, not a crash
        let bad = serde_json::to_vec(&json!({"tool": "remove_marker", "args": {"marker_id": marker_id}})).unwrap();
        let r2 = handle_request(&ss, "POST", "/api/call", "", &bad);
        let v2: Value = serde_json::from_slice(&r2.body).unwrap(); assert_eq!(v2["ok"], false);
    }

    // ---- v7.3: timeline snapping ----

    #[test]
    fn gui_snap_js_is_served_as_a_real_static_file() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/gui_snap.js", "", &[]);
        assert_eq!(r.status, 200);
        assert!(r.content_type.contains("javascript"), "{}", r.content_type);
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("snapTime"), "should serve the real snap module");
        assert!(body.contains("candidatesFromTimeline"), "{body}");
        assert!(body.contains("module.exports"), "must stay requireable from Node for the headless test");
    }

    #[test]
    fn gui_snap_js_rejects_non_get() {
        let ss = fixture();
        let r = handle_request(&ss, "POST", "/gui_snap.js", "", &[]);
        assert_eq!(r.status, 405);
    }

    #[test]
    fn the_index_page_wires_up_snapping() {
        let ss = fixture();
        let r = handle_request(&ss, "GET", "/", "", &[]);
        let body = String::from_utf8(r.body).unwrap();
        for needle in ["/gui_snap.js", "snap-enabled", "kimoSnap.candidatesFromTimeline", "kimoSnap.snapTime", "function snapProposed", "lastTimeline"] {
            assert!(body.contains(needle), "index page should carry {needle:?}");
        }
    }

    #[test]
    fn a_snapped_move_lands_the_dragged_clip_exactly_flush_against_its_neighbour() {
        // The pure snapping math itself lives in gui_snap.js (tested headless in tests/js/snap_test.js);
        // what's tested here is what a snapped drag actually produces once move_clip is called with the
        // already-snapped value -- the second clip's start should land EXACTLY on the first clip's end,
        // not a near-miss a few milliseconds off (which is the whole point of snapping).
        let ss = fixture(); // one 2s clip, start 0, on track1
        let (track_id, second_id) = {
            let mut s = ss.lock().unwrap();
            let t = s.project.tracks[0].id.clone();
            let c2 = dispatch(&mut s, "add_shape", &json!({"track_id": t, "shape": "rect", "start": 2.1, "duration": 1.5, "color": [0.0, 1.0, 0.0]})).unwrap()["clip_id"].as_str().unwrap().to_string();
            (t, c2)
        };
        let _ = track_id;
        // the drag proposed 2.05s (a near miss); gui_snap.js's own candidatesFromTimeline+snapTime would
        // resolve this to 2.0 (the first clip's end) within the default 8px tolerance at any sane zoom --
        // this call is what onUp() sends once that snap has already happened client-side.
        let body = serde_json::to_vec(&json!({"tool": "move_clip", "args": {"clip_id": second_id, "start": 2.0}})).unwrap();
        let r = handle_request(&ss, "POST", "/api/call", "", &body);
        let v: Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(v["ok"], true, "{v}");
        let tl = { let s = ss.lock().unwrap(); timeline_view_model(&s) };
        let clips = tl["tracks"][0]["clips"].as_array().unwrap();
        assert_eq!(clips[0]["end"], 2.0); assert_eq!(clips[1]["start"], 2.0, "snapped clips must be exactly flush, not a near-miss: {tl}");
    }
}
