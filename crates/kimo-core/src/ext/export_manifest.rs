//! U3a: dependency-hashed export manifests. Pure functions over the project plus asset file hashes (deps::hash_file, mode full):
//! `clip_dep_hash` covers everything that can change a clip's pixels or audio (canonical JSON of the clip minus its id and any
//! name/role/tags/label, the sha256 of every file it reads, the dep hash of its transform parent chain, the engine identity).
//! `chunk_key` hashes the ordered dep hashes of the clips active in a frame range plus the canvas/fps/colour settings.
//! Tools (read-only): `export_manifest` and `explain_invalidation`. Duration and frame count are never freshness evidence.
use super::kit::*;
use crate::deps;
use crate::model::*;
use std::collections::BTreeMap;

const NON_RENDER_KEYS: [&str; 5] = ["id", "name", "role", "tags", "label"];

fn canon(v: &Value) -> String { serde_json::to_string(v).unwrap_or_default() } // serde_json maps are sorted (no preserve_order); f64 prints shortest round-trip
fn h12(s: &str) -> String { deps::sha256_hex(s.as_bytes())[..16].to_string() }
fn engine() -> Value { crate::buildinfo::identity() }

#[derive(Default)]
pub struct Ctx { files: BTreeMap<String, (String, u64)>, clip_hash: BTreeMap<String, String> }
impl Ctx {
    /// (sha256 hex or "missing", bytes) for a stored path, resolved against the project directory.
    fn file(&mut self, stored: &str) -> (String, u64) {
        let r = deps::resolve_str(stored);
        if let Some(v) = self.files.get(&r) { return v.clone(); }
        let p = std::path::Path::new(&r);
        let v = match deps::hash_file(p, true) { Ok((h, _)) => (h, deps::stat(p).map(|s| s.size).unwrap_or(0)), Err(_) => ("missing".to_string(), 0) };
        self.files.insert(r, v.clone()); v
    }
}

fn nested<'a>(c: &'a Clip, out: &mut Vec<&'a Clip>) { out.push(c); if let Source::Precomp { tracks } = &c.source { for t in tracks { for k in &t.clips { nested(k, out); } } } }

/// Files a clip reads: its media asset(s), font/image/LUT/mask paths owned by it or by clips nested in a precomp. (field, hash, bytes), sorted by field.
fn clip_files(p: &Project, c: &Clip, ctx: &mut Ctx) -> Vec<(String, String, u64)> {
    let mut ks = vec![]; nested(c, &mut ks); let ids: std::collections::BTreeSet<&str> = ks.iter().map(|k| k.id.as_str()).collect();
    let mut out: Vec<(String, String, u64)> = vec![];
    for k in &ks { if let Source::Media { asset } = &k.source { match p.assets.get(asset) { Some(a) => { let (h, n) = ctx.file(&a.path); out.push((format!("asset:{asset}"), h, n)); } None => out.push((format!("asset:{asset}"), "missing".into(), 0)) } } }
    for r in p.external_refs() { if let Some(cid) = &r.owner.clip_id { if ids.contains(cid.as_str()) && !r.family { let (h, n) = ctx.file(&r.path); out.push((r.field.clone(), h, n)); } } }
    out.sort(); out.dedup(); out
}

fn find_clip<'a>(p: &'a Project, id: &str) -> Option<&'a Clip> { p.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| &**c).find(|c| c.id == id) }

/// Per-part hashes (one per top-level key) and the clip dep hash. Parent chain follows `Clip.parent` (cycle-safe, depth 64).
pub fn clip_parts(p: &Project, c: &Clip, ctx: &mut Ctx, depth: u32) -> (String, BTreeMap<String, String>, Vec<(String, String, u64)>) {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null); let mut parent_hash = Value::Null;
    if let Some(o) = v.as_object_mut() {
        for k in NON_RENDER_KEYS { o.remove(k); }
        if let Some(Value::String(pid)) = o.remove("parent") { parent_hash = match find_clip(p, &pid) { Some(pc) if depth < 64 => Value::String(clip_dep_hash_d(p, pc, ctx, depth + 1)), _ => Value::String("unresolved".into()) }; }
    }
    let mut parts = BTreeMap::new(); if let Some(o) = v.as_object() { for (k, x) in o { parts.insert(k.clone(), h12(&canon(x))); } }
    if !parent_hash.is_null() { parts.insert("parent".into(), h12(&canon(&parent_hash))); }
    let files = clip_files(p, c, ctx);
    let key = serde_json::json!({"clip": v, "files": files.iter().map(|f| json!([f.0, f.1])).collect::<Vec<_>>(), "parent": parent_hash, "engine": engine()});
    (deps::sha256_hex(canon(&key).as_bytes()), parts, files)
}
fn clip_dep_hash_d(p: &Project, c: &Clip, ctx: &mut Ctx, depth: u32) -> String {
    if let Some(h) = ctx.clip_hash.get(&c.id) { return h.clone(); }
    let h = clip_parts(p, c, ctx, depth).0; ctx.clip_hash.insert(c.id.clone(), h.clone()); h
}
pub fn clip_dep_hash(p: &Project, c: &Clip, ctx: &mut Ctx) -> String { clip_dep_hash_d(p, c, ctx, 0) }

/// Canvas, fps and every project-level render setting (lights, camera, colour pipeline, grade scopes, editorial), everything except tracks/assets/markers/name/deps.
pub fn settings_value(p: &Project) -> Value {
    let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() { for k in ["tracks", "assets", "markers", "name", "deps", "kimo", "templates"] { o.remove(k); } }
    v
}

pub fn project_end(p: &Project) -> f64 { p.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.start + c.duration).fold(0.0, f64::max) }

/// Manifest for [f_start, f_end) frames in chunks of `chunk_frames`. Pure given the project and the files it reads.
pub fn build(p: &Project, f_start: u64, f_end: u64, chunk_frames: u64, include_audio: bool) -> Value {
    let mut ctx = Ctx::default(); let fps = if p.fps > 0.0 { p.fps } else { 30.0 };
    let mut clips: BTreeMap<String, Value> = BTreeMap::new(); let mut order: Vec<(usize, usize, String, f64, f64)> = vec![]; let mut asset_hashes: BTreeMap<String, (String, u64)> = BTreeMap::new();
    for (ti, t) in p.tracks.iter().enumerate() { for (ci, c) in t.clips.iter().enumerate() {
        let (dep, parts, files) = clip_parts(p, c, &mut ctx, 0); ctx.clip_hash.insert(c.id.clone(), dep.clone());
        for f in &files { asset_hashes.insert(f.0.clone(), (f.1.clone(), f.2)); }
        clips.insert(c.id.clone(), json!({"dep": dep, "parts": parts, "files": files.iter().map(|f| json!({"ref": f.0, "hash": f.1})).collect::<Vec<_>>(), "track": t.id, "muted": t.muted, "effective_muted": crate::model::effective_mute(&p.tracks, t)}));
        order.push((ti, ci, c.id.clone(), c.start, c.start + c.duration));
    } }
    let settings = settings_value(p); let sh = deps::sha256_hex(canon(&json!([settings, p.width, p.height, p.fps, include_audio])).as_bytes());
    let mut chunks = vec![]; let mut f = f_start; let mut idx = 0u64;
    while f < f_end { let e = (f + chunk_frames).min(f_end); let (t0, t1) = (f as f64 / fps, e as f64 / fps);
        let act: Vec<&(usize, usize, String, f64, f64)> = order.iter().filter(|o| o.3 < t1 && o.4 > t0).collect(); // already in z-order (track, then position)
        let ids: Vec<&str> = act.iter().map(|o| o.2.as_str()).collect(); let deps_v: Vec<&str> = act.iter().map(|o| clips[&o.2]["dep"].as_str().unwrap_or("")).collect();
        let tracks_of: Vec<usize> = act.iter().map(|o| o.0).collect();
        let key = deps::sha256_hex(canon(&json!({"clips": deps_v, "z": tracks_of, "settings": sh, "audio": include_audio, "engine": engine()})).as_bytes());
        let mut fh: Vec<String> = act.iter().flat_map(|o| clips[&o.2]["files"].as_array().cloned().unwrap_or_default()).filter_map(|x| x["hash"].as_str().map(String::from)).collect(); fh.sort(); fh.dedup();
        chunks.push(json!({"index": idx, "range": [f, e], "key": key, "clips": ids, "deps": fh})); f = e; idx += 1; }
    let assets: Vec<Value> = asset_hashes.iter().map(|(k, v)| json!({"ref": k, "hash": v.0, "bytes": v.1})).collect();
    let project_hash = deps::sha256_hex(canon(&json!([clips.values().map(|c| c["dep"].clone()).collect::<Vec<_>>(), sh])).as_bytes());
    let mut m = json!({"project_hash": project_hash, "engine": engine(), "settings": {"width": p.width, "height": p.height, "fps": p.fps, "hash": sh, "include_audio": include_audio, "chunk_frames": chunk_frames}, "assets": assets, "clips": clips, "chunks": chunks});
    let mh = deps::sha256_hex(canon(&m).as_bytes()); m["manifest_hash"] = Value::String(mh); m
}

fn parse_args(ss: &Session, a: &Value) -> Result<(u64, u64, u64, bool), String> {
    let fps = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 };
    let (s, e) = match a.get("range") { None | Some(Value::Null) => (0.0, project_end(&ss.project)), Some(r) => { let v: Vec<f64> = r.as_array().filter(|x| x.len() == 2).map(|x| x.iter().filter_map(|y| y.as_f64()).collect()).filter(|x: &Vec<f64>| x.len() == 2).ok_or("range must be [start_s, end_s]")?; (v[0], v[1]) } };
    if !s.is_finite() || !e.is_finite() || s < 0.0 || e < s { return Err("range must be [start_s, end_s] with 0 <= start <= end".into()); }
    let cs = opt_num(a, "chunk_s", 4.0); if !cs.is_finite() || cs <= 0.0 { return Err("chunk_s must be a positive number of seconds".into()); }
    let (f0, f1) = ((s * fps).round() as u64, (e * fps).ceil() as u64); let cf = ((cs * fps).round() as u64).max(1);
    Ok((f0, f1, cf, a.get("include_audio").and_then(|v| v.as_bool()).unwrap_or(true)))
}

fn load_prev(a: &Value) -> Result<Value, String> {
    if let Some(m) = a.get("previous").filter(|v| v.is_object()) { return Ok(m.clone()); }
    let p = a.get("previous_path").and_then(|v| v.as_str()).ok_or("give previous (a manifest object from export_manifest) or previous_path (a saved manifest .json)")?;
    let t = std::fs::read_to_string(deps::resolve_str(p)).map_err(|e| format!("cannot read previous manifest '{p}': {e}"))?; serde_json::from_str(&t).map_err(|e| format!("previous manifest '{p}' is not valid JSON: {e}"))
}

/// Per-chunk staleness of `cur` against `prev`. Reasons: engine | settings | clip_changed (with changed parts) | asset_changed | clip_added | clip_removed | grid.
pub fn explain(prev: &Value, cur: &Value) -> Value {
    let (pe, ce) = (&prev["engine"], &cur["engine"]); let engine_changed = pe != ce; let settings_changed = prev["settings"]["hash"] != cur["settings"]["hash"];
    let grid_same = prev["settings"]["chunk_frames"] == cur["settings"]["chunk_frames"];
    let pc = prev["chunks"].as_array().cloned().unwrap_or_default(); let mut out = vec![]; let (mut stale_n, mut fresh_n) = (0, 0);
    for c in cur["chunks"].as_array().cloned().unwrap_or_default() {
        let range = c["range"].clone(); let old = if grid_same { pc.iter().find(|o| o["range"] == range) } else { None };
        let (stale, reasons): (bool, Vec<Value>) = if engine_changed { (true, vec![json!({"kind": "engine", "old": pe, "new": ce})]) }
            else if settings_changed { (true, vec![json!({"kind": "settings", "old_hash": prev["settings"]["hash"], "new_hash": cur["settings"]["hash"]})]) }
            else if let Some(o) = old { if o["key"] == c["key"] { (false, vec![]) } else {
                let mut rs = vec![]; let oc: Vec<&str> = o["clips"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default(); let nc: Vec<&str> = c["clips"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                for id in &nc { if !oc.contains(id) { rs.push(json!({"kind": "clip_added", "clip": id})); } }
                for id in &oc { if !nc.contains(id) { rs.push(json!({"kind": "clip_removed", "clip": id})); } }
                for id in nc.iter().filter(|i| oc.contains(i)) {
                    let (a, b) = (&prev["clips"][*id], &cur["clips"][*id]); if a["dep"] == b["dep"] { continue; }
                    let (fa, fb) = (a["files"].as_array().cloned().unwrap_or_default(), b["files"].as_array().cloned().unwrap_or_default()); let mut file_reason = false;
                    for x in &fb { let old_h = fa.iter().find(|y| y["ref"] == x["ref"]).map(|y| y["hash"].clone()); if old_h.as_ref() != Some(&x["hash"]) { file_reason = true; rs.push(json!({"kind": "asset_changed", "clip": id, "asset": x["ref"], "old_hash": old_h.unwrap_or(Value::Null), "new_hash": x["hash"]})); } }
                    let (pa, pb) = (a["parts"].as_object().cloned().unwrap_or_default(), b["parts"].as_object().cloned().unwrap_or_default());
                    let mut ch: Vec<&String> = pb.keys().filter(|k| pa.get(*k) != pb.get(*k)).collect(); for k in pa.keys() { if !pb.contains_key(k) { ch.push(k); } }
                    if !ch.is_empty() || !file_reason { rs.push(json!({"kind": "clip_changed", "clip": id, "property": ch, "old_hash": a["dep"], "new_hash": b["dep"]})); }
                }
                if rs.is_empty() { rs.push(json!({"kind": "order_changed", "detail": "same clips and hashes, different z-order or timing window"})); }
                (true, rs) } }
            else { (true, vec![json!({"kind": "grid", "detail": "no previous chunk covers this range"})]) };
        if stale { stale_n += 1 } else { fresh_n += 1 }
        out.push(json!({"index": c["index"], "range": range, "stale": stale, "reason": reasons}));
    }
    json!({"chunks": out, "stale": stale_n, "unchanged": fresh_n, "project_hash": {"previous": prev["project_hash"], "current": cur["project_hash"]}})
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_manifest", mutates: false,
        description: "Dependency-hashed manifest of what an export would render, in fixed-length chunks. Dependency-hashed manifest of what an export would render, in fixed-length chunks: per clip a dep hash (clip content minus names/ids, the sha256 of every file it reads, its transform-parent chain, the engine version) plus per-part hashes, per chunk a key over the clips active in it and the canvas/fps/render settings. Save the result and later pass it to explain_invalidation to learn exactly which chunks went stale and why, instead of trusting duration or frame count. Read-only; hashes asset files in full (cached by path, size, mtime).",
        schema: obj(vec![("range", json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"[start_s, end_s]; default the whole timeline"})), ("chunk_s", n("chunk length in seconds, default 4 (rounded to whole frames, at least 1)")), ("include_audio", json!({"type":"boolean","description":"chunk keys cover the audio dependencies too (default true)"}))], &[]),
        run: |ss, a| { let (f0, f1, cf, au) = parse_args(ss, a)?; Ok(build(&ss.project, f0, f1, cf, au)) } },
    Command { name: "explain_invalidation", mutates: false,
        description: "Compare the project with an earlier export_manifest and say, per chunk, whether it is stale and why. Compare the current project with an earlier export_manifest and say, per chunk, whether it is stale and why: reason kinds are engine, settings, clip_changed (names the changed parts such as props or effects, with old/new hashes), asset_changed (the file whose bytes changed), clip_added, clip_removed, order_changed, grid. Unrelated edits (markers, track locks, names) leave every chunk fresh. Uses the previous manifest's chunk length and range unless you override them. Read-only.",
        schema: obj(vec![("previous", json!({"type":"object","description":"a manifest returned by export_manifest"})), ("previous_path", s("path of a saved manifest .json file (used when previous is not given)")), ("range", json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2})), ("chunk_s", n("chunk length in seconds; default the previous manifest's"))], &[]),
        run: |ss, a| {
            let prev = load_prev(a)?; let mut a2 = a.clone();
            if a2.get("chunk_s").is_none() { if let (Some(cf), fps) = (prev["settings"]["chunk_frames"].as_u64(), ss.project.fps.max(1e-9)) { a2["chunk_s"] = json!(cf as f64 / fps); } }
            if a2.get("range").is_none() { if let Some(ch) = prev["chunks"].as_array().filter(|c| !c.is_empty()) { let fps = ss.project.fps.max(1e-9); let (f0, f1) = (ch[0]["range"][0].as_f64().unwrap_or(0.0), ch[ch.len() - 1]["range"][1].as_f64().unwrap_or(0.0)); a2["range"] = json!([f0 / fps, f1 / fps]); } }
            let au = prev["settings"]["include_audio"].as_bool(); if let Some(b) = au { a2["include_audio"] = json!(b); }
            let (f0, f1, cf, au) = parse_args(ss, &a2)?; let cur = build(&ss.project, f0, f1, cf, au); Ok(explain(&prev, &cur)) } },
] }
