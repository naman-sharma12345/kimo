//! Media-pool bins and subclips (FEATURES.md L85, alongside names.rs's clip naming/role/colour-label/tags and
//! find). Bins organise imported *assets* into named folders in the media pool, independent of timeline tracks
//! (an asset's `bin` field names a folder; `create_bin`/`list_bins`/`rename_bin`/`delete_bin` manage the folders
//! themselves, including empty ones). A subclip is a named, saved `[in_point, out_point)` bookmark on an asset --
//! not a new Source kind -- so it can be found again and placed on the timeline with `add_clip_from_subclip`
//! (which is a thin wrapper over the existing `add_clip`, so a subclip-placed clip is byte-identical to one placed
//! by hand with the same asset_id/in_point/duration).
use super::kit::*;
use crate::commands::Session;
use crate::model::Subclip;

fn bin_name_ok(s: &str) -> bool { !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-/ ".contains(c)) }
fn check_bin_name(s: &str) -> Result<(), String> { if bin_name_ok(s) { Ok(()) } else { Err(format!("invalid_name: bin '{s}' must be 1..64 characters of A-Z a-z 0-9 space _ . - /")) } }
/// Every bin name that exists: explicitly created ones (`project.bins`, including empty bins) plus any bin an
/// asset references that predates explicit creation (so an old project's asset.bin values are never orphaned).
fn all_bin_names(p: &crate::model::Project) -> Vec<String> {
    let mut v: Vec<String> = p.bins.clone();
    for a in p.assets.values() { if let Some(b) = &a.bin { if !v.contains(b) { v.push(b.clone()); } } }
    v.sort();
    v
}
fn simple_glob(pat: &str, s: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), s.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) { pi += 1; ti += 1; }
        else if pi < p.len() && p[pi] == '*' { star = pi; mark = ti; pi += 1; }
        else if star != usize::MAX { pi = star + 1; mark += 1; ti = mark; }
        else { return false; }
    }
    while pi < p.len() && p[pi] == '*' { pi += 1; }
    pi == p.len()
}

fn create_bin(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let name = str_arg(a, "name")?.to_string();
    check_bin_name(&name)?;
    if all_bin_names(&ss.project).contains(&name) { return Err(format!("bin_exists: a bin named '{name}' already exists")); }
    ss.project.bins.push(name.clone());
    Ok(json!({"name": name}))
}
fn list_bins(ss: &mut Session, _a: &Value) -> Result<Value, String> {
    let names = all_bin_names(&ss.project);
    let bins: Vec<Value> = names.iter().map(|n| json!({"name": n, "assets": ss.project.assets.values().filter(|a| a.bin.as_deref() == Some(n.as_str())).count()})).collect();
    let unfiled = ss.project.assets.values().filter(|a| a.bin.is_none()).count();
    Ok(json!({"bins": bins, "unfiled_assets": unfiled}))
}
fn rename_bin(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let old = str_arg(a, "name")?.to_string();
    let new = str_arg(a, "new_name")?.to_string();
    check_bin_name(&new)?;
    let existing = all_bin_names(&ss.project);
    if !existing.contains(&old) { return Err(format!("no bin '{old}'")); }
    if old != new && existing.contains(&new) { return Err(format!("bin_exists: a bin named '{new}' already exists")); }
    if let Some(slot) = ss.project.bins.iter_mut().find(|b| **b == old) { *slot = new.clone(); } else { ss.project.bins.push(new.clone()); }
    for asset in ss.project.assets.values_mut() { if asset.bin.as_deref() == Some(old.as_str()) { asset.bin = Some(new.clone()); } }
    Ok(json!({"name": new}))
}
fn delete_bin(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let name = str_arg(a, "name")?.to_string();
    if !all_bin_names(&ss.project).contains(&name) { return Err(format!("no bin '{name}'")); }
    let reassign = match a.get("reassign_to") { None | Some(Value::Null) => None, Some(Value::String(s)) => { check_bin_name(s)?; Some(s.clone()) } _ => return Err("reassign_to must be a string or null".into()) };
    ss.project.bins.retain(|b| *b != name);
    for asset in ss.project.assets.values_mut() { if asset.bin.as_deref() == Some(name.as_str()) { asset.bin = reassign.clone(); } }
    if let Some(r) = &reassign { if !ss.project.bins.contains(r) { ss.project.bins.push(r.clone()); } }
    Ok(json!({"ok": true, "reassigned_to": reassign}))
}
fn move_asset_to_bin(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let asset_id = str_arg(a, "asset_id")?.to_string();
    if !ss.project.assets.contains_key(&asset_id) { return Err(format!("no asset '{asset_id}'")); }
    let bin = match a.get("bin") { None | Some(Value::Null) => None, Some(Value::String(s)) => { check_bin_name(s)?; Some(s.clone()) } _ => return Err("bin must be a string or null (clear, back to unfiled)".into()) };
    if let Some(b) = &bin { if !ss.project.bins.contains(b) { ss.project.bins.push(b.clone()); } }
    ss.project.assets.get_mut(&asset_id).unwrap().bin = bin.clone();
    Ok(json!({"asset_id": asset_id, "bin": bin}))
}
fn find_assets(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let bin = match a.get("bin") { None => None, Some(Value::Null) => Some(None), Some(Value::String(s)) => Some(Some(s.clone())), _ => return Err("bin must be a string or null".into()) };
    let name_glob = match a.get("name_glob") { None | Some(Value::Null) => None, Some(Value::String(s)) => Some(s.clone()), _ => return Err("name_glob must be a string".into()) };
    let limit = match a.get("limit") { None | Some(Value::Null) => 100usize, Some(v) => v.as_u64().filter(|n| (1..=500).contains(n)).ok_or("limit must be an integer 1..500")? as usize };
    let (mut out, mut total) = (vec![], 0usize);
    for (id, asset) in &ss.project.assets {
        if let Some(want) = &bin { if asset.bin != *want { continue; } }
        if let Some(g) = &name_glob {
            let fname = std::path::Path::new(&asset.path).file_name().and_then(|x| x.to_str()).unwrap_or(&asset.path);
            if !simple_glob(g, fname) { continue; }
        }
        total += 1;
        if out.len() < limit { out.push(json!({"asset_id": id, "path": asset.path, "bin": asset.bin, "width": asset.width, "height": asset.height, "duration": asset.duration, "fps": asset.fps})); }
    }
    Ok(json!({"matches": out, "count": out.len(), "total": total, "truncated": total > out.len()}))
}

fn create_subclip(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let asset_id = str_arg(a, "asset_id")?.to_string();
    let asset = ss.project.assets.get(&asset_id).ok_or_else(|| format!("no asset '{asset_id}'"))?;
    let in_point = num_arg(a, "in_point")?;
    let out_point = num_arg(a, "out_point")?;
    if !in_point.is_finite() || in_point < 0.0 { return Err("in_point must be a finite number >= 0".into()); }
    if !out_point.is_finite() || out_point <= in_point { return Err("out_point must be a finite number greater than in_point".into()); }
    if asset.duration > 0.0 && out_point > asset.duration + 1e-6 { return Err(format!("out_point {out_point} exceeds asset '{asset_id}' duration {}", asset.duration)); }
    let name = match a.get("name") { None | Some(Value::Null) => None, Some(Value::String(s)) => { if s.is_empty() || s.chars().count() > 64 { return Err("name must be 1..64 characters".into()); } Some(s.clone()) } _ => return Err("name must be a string or null".into()) };
    let id = ss.project.gen_id("subclip");
    ss.project.subclips.insert(id.clone(), Subclip { id: id.clone(), asset_id: asset_id.clone(), name: name.clone(), in_point, out_point });
    Ok(json!({"subclip_id": id, "asset_id": asset_id, "name": name, "in_point": in_point, "out_point": out_point}))
}
fn list_subclips(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let asset_id = match a.get("asset_id") { None | Some(Value::Null) => None, Some(Value::String(s)) => Some(s.clone()), _ => return Err("asset_id must be a string".into()) };
    let out: Vec<Value> = ss.project.subclips.values().filter(|sc| asset_id.as_deref().map_or(true, |want| want == sc.asset_id))
        .map(|sc| json!({"subclip_id": sc.id, "asset_id": sc.asset_id, "name": sc.name, "in_point": sc.in_point, "out_point": sc.out_point}))
        .collect();
    Ok(json!({"subclips": out}))
}
fn delete_subclip(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "subclip_id")?;
    if ss.project.subclips.remove(id).is_none() { return Err(format!("no subclip '{id}'")); }
    Ok(json!({"ok": true}))
}
fn add_clip_from_subclip(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let subclip_id = str_arg(a, "subclip_id")?.to_string();
    let track_id = str_arg(a, "track_id")?.to_string();
    let start = num_arg(a, "start")?;
    let sc = ss.project.subclips.get(&subclip_id).ok_or_else(|| format!("no subclip '{subclip_id}'"))?.clone();
    let full_span = sc.out_point - sc.in_point;
    let duration = match a.get("duration") {
        None | Some(Value::Null) => full_span,
        Some(v) => {
            let d = v.as_f64().filter(|d| d.is_finite() && *d > 0.0).ok_or("duration must be a finite number > 0")?;
            if d > full_span + 1e-9 { return Err(format!("duration {d} exceeds the subclip's own span {full_span}")); }
            d
        }
    };
    if !ss.project.assets.contains_key(&sc.asset_id) { return Err(format!("subclip's asset '{}' no longer exists", sc.asset_id)); }
    // Delegates to add_clip so a subclip-placed clip is produced by the exact same code path (and stays
    // byte-identical in output) as a hand-placed one with the same asset_id/in_point/duration -- a subclip is a
    // saved bookmark of those three numbers, not a new kind of timeline clip.
    let r = crate::commands::dispatch(ss, "add_clip", &json!({"track_id": track_id, "source_type": "media", "asset_id": sc.asset_id, "start": start, "duration": duration, "in_point": sc.in_point}))?;
    Ok(json!({"clip_id": r["clip_id"], "subclip_id": subclip_id, "asset_id": sc.asset_id, "in_point": sc.in_point, "duration": duration}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "create_bin", description: "Create a named bin in the media pool (asset organisation, independent of timeline tracks). Errors: bin_exists, invalid_name.", mutates: true,
        schema: obj(vec![("name", s("1..64 characters of A-Z a-z 0-9 space _ . - /"))], &["name"]), run: create_bin },
    Command { name: "list_bins", description: "List every bin in the media pool with its asset count, plus how many assets are unfiled (no bin). Nothing is changed.", mutates: false,
        schema: obj(vec![], &[]), run: list_bins },
    Command { name: "rename_bin", description: "Rename a bin; every asset filed in it moves with it. Errors: no such bin, bin_exists, invalid_name.", mutates: true,
        schema: obj(vec![("name", s("current bin name")), ("new_name", s("new name"))], &["name", "new_name"]), run: rename_bin },
    Command { name: "delete_bin", description: "Delete a bin. Its assets become unfiled, or move to reassign_to (created if it doesn't exist yet) when given. Errors: no such bin.", mutates: true,
        schema: obj(vec![("name", s("bin to delete")), ("reassign_to", json!({"type":["string","null"],"description":"bin for its assets to move to; omitted/null = unfiled"}))], &["name"]), run: delete_bin },
    Command { name: "move_asset_to_bin", description: "File an asset into a bin (creating the bin if it doesn't exist yet), or back to unfiled with bin: null.", mutates: true,
        schema: obj(vec![("asset_id", s("asset")), ("bin", json!({"type":["string","null"]}))], &["asset_id", "bin"]), run: move_asset_to_bin },
    Command { name: "find_assets", description: "Read-only search of imported media-pool assets by bin (omit = any, null = unfiled only, a name = that bin) and name_glob (matched against the file name, * and ?). Returns up to limit matches (default 100, max 500) with asset_id, path, bin, width, height, duration, fps; total and truncated say whether more matched.", mutates: false,
        schema: obj(vec![("bin", json!({"type":["string","null"]})), ("name_glob", s("file-name pattern with * and ?")), ("limit", json!({"type":"integer","minimum":1,"maximum":500}))], &[]), run: find_assets },
    Command { name: "create_subclip", description: "Save a named [in_point, out_point) source-seconds bookmark on a media asset (a subclip), so it can be found again (list_subclips) and placed on the timeline (add_clip_from_subclip) without re-remembering the exact numbers. Does not itself appear on any track. Errors: invalid in_point/out_point, out_point beyond the asset's known duration.", mutates: true,
        schema: obj(vec![("asset_id", s("media asset")), ("in_point", n("source seconds, >= 0")), ("out_point", n("source seconds, > in_point")), ("name", json!({"type":["string","null"],"description":"optional label, 1..64 characters"}))], &["asset_id", "in_point", "out_point"]), run: create_subclip },
    Command { name: "list_subclips", description: "List saved subclips, optionally filtered to one asset. Nothing is changed.", mutates: false,
        schema: obj(vec![("asset_id", s("optional: only this asset's subclips"))], &[]), run: list_subclips },
    Command { name: "delete_subclip", description: "Delete a saved subclip bookmark. Any clip already placed from it on the timeline is unaffected (it is a plain media clip by then).", mutates: true,
        schema: obj(vec![("subclip_id", s("subclip"))], &["subclip_id"]), run: delete_subclip },
    Command { name: "add_clip_from_subclip", description: "Place a saved subclip on the timeline: a thin wrapper over add_clip using the subclip's asset_id/in_point and its full span as the duration (or a shorter duration, trimmed from its in_point). Byte-identical to calling add_clip by hand with those exact numbers.", mutates: true,
        schema: obj(vec![("subclip_id", s("subclip")), ("track_id", s("target track")), ("start", n("timeline start seconds")), ("duration", json!({"type":["number","null"],"description":"optional: trim shorter than the subclip's full span; omitted/null uses the full span"}))], &["subclip_id", "track_id", "start"]), run: add_clip_from_subclip },
] }
