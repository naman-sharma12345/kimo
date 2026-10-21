//! Delivery tools (RV-7): list_dependencies, collect_project, relink_media. Built on `deps` (external_refs, set_ref, resolve_path, hashing).
use super::kit::*;
use crate::deps::{self, DepItem, DepRecord, RefKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn flag(a: &Value, k: &str, d: bool) -> bool { a.get(k).and_then(|v| v.as_bool()).unwrap_or(d) }
fn kind_s(k: RefKind) -> String { format!("{k:?}").to_lowercase() }
fn is_dir_ref(it: &DepItem) -> bool { crate::deps::is_dir_field(&it.field) }
fn base(p: &str) -> String { Path::new(&p.replace('\\', "/")).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into()) }
fn sha8(h: &str) -> &str { &h[..8.min(h.len())] }

fn summary(items: &[DepItem]) -> Value {
    let c = |s: &str| items.iter().filter(|i| i.status == s).count();
    json!({"total": items.len(), "ok": c("ok"), "missing": c("missing"), "changed": c("changed")})
}

/// Files of a frame sequence (`%04d` pattern): frame numbers from the first existing one while consecutive.
fn seq_files(pat: &str) -> Vec<PathBuf> {
    let start = if deps::seq_frame(pat, 0).map_or(false, |f| Path::new(&f).is_file()) { 0 } else { 1 };
    let mut v = vec![]; let mut i = start; while let Some(f) = deps::seq_frame(pat, i) { if !Path::new(&f).is_file() { break; } v.push(PathBuf::from(f)); i += 1; if i > 1_000_000 { break; } } v
}
/// Every file under a directory reference, with its path relative to the directory (sorted). A flat matte store lists exactly its
/// files by name; a code clip folder keeps its subfolders (vendored libraries, modules), so the collected copy still loads.
fn dir_files(d: &str) -> Vec<(PathBuf, String)> {
    fn walk(root: &Path, d: &Path, depth: usize, out: &mut Vec<(PathBuf, String)>) {
        if depth > 32 { return; }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() { let p = e.path(); let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() { if e.file_name() != ".kimo_cache" { walk(root, &p, depth + 1, out); } }
            else if ft.is_file() { if let Ok(r) = p.strip_prefix(root) { out.push((p.clone(), r.to_string_lossy().replace('\\', "/"))); } } }
    }
    let mut v = vec![]; walk(Path::new(d), Path::new(d), 0, &mut v); v.sort_by(|a, b| a.1.cmp(&b.1)); v
}

fn free_kb(dir: &Path) -> Option<u64> {
    let o = std::process::Command::new("df").args(["-Pk"]).arg(dir).output().ok()?; let t = String::from_utf8_lossy(&o.stdout);
    t.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()
}
fn copy_file(src: &Path, dst: &Path, link: bool) -> Result<(), String> {
    if let Some(d) = dst.parent() { std::fs::create_dir_all(d).map_err(|e| format!("cannot create '{}': {e}", d.display()))?; }
    if link && std::fs::hard_link(src, dst).is_ok() { return Ok(()); }
    std::fs::copy(src, dst).map(|_| ()).map_err(|e| format!("copy '{}' failed: {e}", src.display()))
}

fn collect(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let dest = PathBuf::from(str_arg(a, "dest_dir")?); let mode = a.get("copy").and_then(|v| v.as_str()).unwrap_or("copy");
    if !matches!(mode, "copy" | "link") { return Err(format!("invalid_argument: copy must be copy|link, got '{mode}'")); }
    let (link, rewrite, overwrite, dry) = (mode == "link", flag(a, "rewrite", true), flag(a, "overwrite", false), flag(a, "dry_run", false));
    let (inc_fonts, inc_luts) = (flag(a, "include_fonts", true), flag(a, "include_luts", true));
    let name = a.get("project_name").and_then(|v| v.as_str()).unwrap_or("project.kimo").to_string();
    if name.contains('/') || name.contains('\\') || name.is_empty() { return Err("invalid_argument: project_name must be a plain file name".into()); }
    let tmp = dest.join(".collect-tmp");
    let had_content = dest.is_dir() && std::fs::read_dir(&dest).map(|r| r.flatten().any(|e| e.file_name() != ".collect-tmp")).unwrap_or(false);
    if had_content && !overwrite { return Err(format!("dest_exists: '{}' is not empty; pass overwrite:true to replace a previous collect", dest.display())); }
    let items = deps::list_dependencies(&ss.project, true);
    // ---- plan: every readable item gets a destination (relative to dest); same content shares one file.
    struct Plan { item: usize, rel: String, files: Vec<(PathBuf, String)>, #[allow(dead_code)] bytes: u64 }
    let mut skipped: Vec<Value> = vec![]; let mut plans: Vec<Plan> = vec![]; let mut fonts_out: Vec<Value> = vec![];
    // smallest basename wins within a sha group
    let mut best_name: BTreeMap<String, String> = BTreeMap::new();
    for it in &items { if it.status == "missing" || it.family && false { continue; } if let Some(h) = &it.sha256 { let b = base(&it.resolved); best_name.entry(h.clone()).and_modify(|x| if b < *x { *x = b.clone() }).or_insert(b); } }
    let mut shared: BTreeMap<String, String> = BTreeMap::new(); // sha -> rel
    let mut fonts_seen: BTreeMap<String, bool> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        if it.status == "missing" { skipped.push(json!({"field": it.field, "path": it.path, "reason": "missing"})); continue; }
        let src = PathBuf::from(&it.resolved);
        if it.kind == RefKind::Font {
            if !inc_fonts { skipped.push(json!({"field": it.field, "path": it.path, "reason": "include_fonts=false"})); continue; }
            let bytes = std::fs::read(&src).map_err(|e| format!("cannot read font '{}': {e}", src.display()))?; let lic = deps::font_license(&bytes);
            let sha = it.sha256.clone().unwrap_or_default(); let ok = lic.as_ref().map_or(true, |l| l.embeddable);
            let rel = format!("fonts/{}", if fonts_seen.contains_key(&sha) { best_name[&sha].clone() } else { base(&it.resolved) });
            let mut row = json!({"path": it.path, "file": base(&it.resolved), "sha256": sha, "copied": ok, "fs_type": lic.as_ref().map(|l| l.fs_type), "license": lic.as_ref().map(|l| l.license.clone())});
            if !ok { row["reason"] = json!("font_license: embedding restricted (OS/2 fsType); not copied - install the font on the target machine"); }
            fonts_out.push(row);
            if !ok { continue; }
            fonts_seen.insert(sha.clone(), true); plans.push(Plan { item: i, rel: rel.clone(), files: vec![(src, rel)], bytes: bytes.len() as u64 }); continue;
        }
        if it.kind == RefKind::Lut && !inc_luts { skipped.push(json!({"field": it.field, "path": it.path, "reason": "include_luts=false"})); continue; }
        if is_dir_ref(it) {
            let fs = dir_files(&it.resolved); let mut h = deps::Sha256::new(); for (f, r) in &fs { h.update(r.as_bytes()); if let Ok((x, _)) = deps::hash_file(f, true) { h.update(x.as_bytes()); } }
            let hh = h.finish_hex(); let rel = format!("dirs/{}_{}", sha8(&hh), base(&it.resolved)); let bytes = fs.iter().filter_map(|(f, _)| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
            plans.push(Plan { item: i, files: fs.iter().map(|(f, r)| (f.clone(), format!("{rel}/{r}"))).collect(), rel, bytes }); continue;
        }
        if deps::seq_frame(&it.resolved, 0).is_some() {
            let fs = seq_files(&it.resolved); let mut h = deps::Sha256::new(); for f in &fs { if let Ok((x, _)) = deps::hash_file(f, true) { h.update(x.as_bytes()); } }
            let hh = h.finish_hex(); let dir = format!("sequences/{}", sha8(&hh)); let b = base(&it.path); let bytes = fs.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
            plans.push(Plan { item: i, rel: format!("{dir}/{b}"), files: fs.iter().map(|f| (f.clone(), format!("{dir}/{}", base(&f.to_string_lossy())))).collect(), bytes }); continue;
        }
        let sha = it.sha256.clone().unwrap_or_default(); let sub = if it.kind == RefKind::Lut { "luts" } else { "media" };
        let rel = shared.entry(sha.clone()).or_insert_with(|| format!("{sub}/{}_{}", sha8(&sha), best_name.get(&sha).cloned().unwrap_or_else(|| base(&it.resolved)))).clone();
        plans.push(Plan { item: i, files: vec![(src, rel.clone())], rel, bytes: it.size.unwrap_or(0) });
    }
    // unique files to write (dedup by destination)
    let mut uniq: BTreeMap<String, PathBuf> = BTreeMap::new(); for p in &plans { for (s, d) in &p.files { uniq.entry(d.clone()).or_insert_with(|| s.clone()); } }
    let total: u64 = uniq.values().filter_map(|s| std::fs::metadata(s).ok()).map(|m| m.len()).sum();
    let missing = items.iter().filter(|i| i.status == "missing").count();
    let plan_json = json!({"files": uniq.len(), "bytes": total, "missing": missing, "dest": dest.to_string_lossy()});
    if dry { return Ok(json!({"dry_run": true, "plan": plan_json, "skipped": skipped, "fonts": fonts_out})); }
    // ---- disk space (copies need the bytes; hard links need almost nothing)
    let probe = if dest.exists() { dest.clone() } else { dest.ancestors().find(|p| p.exists()).map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")) };
    let need = if link { 1 << 20 } else { total + total / 20 + (1 << 20) };
    if let Some(kb) = free_kb(&probe) { if kb.saturating_mul(1024) < need { return Err(format!("disk_space_low: collect needs {need} bytes but only {} are free on '{}'", kb * 1024, probe.display())); } }
    // ---- stage everything under .collect-tmp, then move into place; the project and manifest go last
    let _ = std::fs::remove_dir_all(&tmp); std::fs::create_dir_all(&tmp).map_err(|e| format!("cannot create '{}': {e}", tmp.display()))?;
    let res = (|| -> Result<Value, String> {
        for (d, s) in &uniq { crate::guard::check(); copy_file(s, &tmp.join(d), link)?; }
        // copy's project
        let mut proj = ss.project.clone(); let mut table: BTreeMap<String, DepRecord> = BTreeMap::new();
        for p in &plans { let it = &items[p.item]; if rewrite { proj.set_ref(&it.field, &p.rel)?; }
            table.insert(if rewrite { p.rel.clone() } else { it.path.clone() }, DepRecord { kind: kind_s(it.kind), rel_path: p.rel.clone(), abs_hint: it.resolved.clone(), size: it.size.unwrap_or(0), sha256: it.sha256.clone().unwrap_or_default(), hash_mode: it.hash_mode.unwrap_or("full").into(), mtime: 0, status: "ok".into() }); }
        proj.deps = Some(table);
        let src_sha = deps::sha256_hex(&serde_json::to_vec(&ss.project).map_err(|e| e.to_string())?);
        let mut t = Session::new(); t.project = proj; let keep = deps::base_dir();
        let r = crate::commands::dispatch(&mut t, "save_project", &json!({"path": tmp.join(&name).to_string_lossy()})); deps::set_base_dir(keep.map(|p| (*p).clone()));
        r?;
        let mut item_rows: Vec<Value> = plans.iter().map(|p| { let it = &items[p.item]; json!({"kind": kind_s(it.kind), "field": it.field, "path": it.path, "dest": p.rel, "size": it.size, "sha256": it.sha256}) }).collect();
        item_rows.extend(skipped.iter().cloned()); item_rows.sort_by(|x, y| (x["field"].as_str(), x["path"].as_str()).cmp(&(y["field"].as_str(), y["path"].as_str())));
        let manifest = json!({"manifest_version": 1, "build": crate::buildinfo::stamp(json!({})), "source_project_sha": src_sha, "project": name, "rewritten": rewrite, "items": item_rows, "fonts": fonts_out,
            "totals": {"files": uniq.len(), "bytes": total, "missing": missing, "skipped": skipped.len()}});
        std::fs::write(tmp.join("collect_manifest.json"), serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(manifest)
    })();
    let manifest = match res { Ok(m) => m, Err(e) => { let _ = std::fs::remove_dir_all(&tmp); return Err(e); } };
    // overwrite: drop only what a previous collect wrote
    if had_content { for d in ["media", "fonts", "luts", "sequences", "dirs"] { let _ = std::fs::remove_dir_all(dest.join(d)); } let _ = std::fs::remove_file(dest.join(&name)); let _ = std::fs::remove_file(dest.join("collect_manifest.json")); }
    let mut tops: Vec<String> = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(); tops.sort();
    tops.sort_by_key(|n| (n == &name, n == "collect_manifest.json")); // project, then manifest, move last
    for n in tops { std::fs::rename(tmp.join(&n), dest.join(&n)).map_err(|e| format!("cannot move '{n}' into '{}': {e}", dest.display()))?; }
    let _ = std::fs::remove_dir_all(&tmp);
    let warnings: Vec<String> = fonts_out.iter().filter(|f| f["copied"] == json!(false)).map(|f| format!("font_license: '{}' not copied ({})", f["file"].as_str().unwrap_or(""), f["license"].as_str().unwrap_or("restricted"))).collect();
    Ok(json!({"dest_dir": dest.to_string_lossy(), "project": dest.join(&name).to_string_lossy(), "manifest": dest.join("collect_manifest.json").to_string_lossy(), "files": uniq.len(), "bytes": total, "missing": missing, "skipped": skipped, "fonts": fonts_out, "warnings": warnings, "source_project_sha": manifest["source_project_sha"]}))
}

// ------------------------------------------------------------------ relink
fn walk(dir: &Path, recursive: bool, depth: usize, out: &mut Vec<PathBuf>) { if depth > 12 { return; } if let Ok(rd) = std::fs::read_dir(dir) { let mut es: Vec<_> = rd.flatten().collect(); es.sort_by_key(|e| e.file_name()); for e in es { let p = e.path(); if p.is_dir() { if recursive && !e.file_type().map(|t| t.is_symlink()).unwrap_or(false) { walk(&p, true, depth + 1, out); } } else if p.is_file() { out.push(p); } } } }

fn relink(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let dirs: Vec<String> = a.get("search_dirs").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "search_dirs", "array of directories"))?.iter().filter_map(|v| v.as_str().map(String::from)).collect();
    if dirs.is_empty() { return Err("invalid_argument: search_dirs needs at least one directory".into()); }
    let m = a.get("match").and_then(|v| v.as_str()).unwrap_or("name_size_hash"); if !matches!(m, "name_size_hash" | "name" | "hash") { return Err(format!("invalid_argument: match must be name_size_hash|name|hash, got '{m}'")); }
    let (recursive, apply, only_missing) = (flag(a, "recursive", true), flag(a, "apply", false), flag(a, "only_missing", true));
    let mut files = vec![]; for d in &dirs { if !Path::new(d).is_dir() { return Err(format!("invalid_argument: search dir '{d}' is not a directory")); } walk(Path::new(d), recursive, 0, &mut files); }
    let items = deps::list_dependencies(&ss.project, false); let mut rows = vec![]; let mut applied = vec![]; let (mut n_amb, mut n_none) = (0, 0);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for it in &items {
        if it.family || is_dir_ref(it) || it.kind == RefKind::Font && false { continue; }
        if only_missing && it.status != "missing" { continue; }
        if !seen.insert(it.field.clone()) { continue; }
        let rec = ss.project.deps.as_ref().and_then(|d| d.get(&it.path)).cloned(); let want = base(&it.path);
        let (size, sha) = (rec.as_ref().map(|r| r.size).filter(|s| *s > 0).or(it.size), rec.as_ref().map(|r| r.sha256.clone()).filter(|s| !s.is_empty()).or(it.sha256.clone()));
        let mut cands: Vec<(u8, PathBuf, &'static str)> = vec![];
        for f in &files {
            let same_name = base(&f.to_string_lossy()) == want; let fsz = std::fs::metadata(f).map(|m| m.len()).ok();
            let size_ok = size.is_some() && fsz == size;
            let hash_ok = |f: &Path| -> bool { match (&sha, size_ok) { (Some(s), true) => deps::hash_file(f, false).map(|(h, _)| &h == s).unwrap_or(false), _ => false } };
            let c = match m {
                "name" => if same_name { Some((1, "name")) } else { None },
                "hash" => if size_ok && hash_ok(f) { Some((3, "sha256")) } else { None },
                _ => if size_ok && hash_ok(f) { Some((3, "sha256")) } else if same_name && size_ok { Some((2, "name+size")) } else if same_name && size.is_none() { Some((1, "name")) } else { None },
            };
            if let Some((sc, why)) = c { cands.push((sc, f.clone(), why)); }
        }
        let top = cands.iter().map(|c| c.0).max();
        let best: Vec<&(u8, PathBuf, &str)> = cands.iter().filter(|c| Some(c.0) == top).collect();
        let cj: Vec<Value> = cands.iter().map(|c| json!({"path": c.1.to_string_lossy(), "score": c.0, "match": c.2})).collect();
        let mut row = json!({"field": it.field, "path": it.path, "status": it.status, "candidates": cj});
        match best.len() { 0 => { n_none += 1; row["result"] = json!("not_found"); }
            1 => { let p = best[0].1.clone(); row["result"] = json!("match"); row["new_path"] = json!(p.to_string_lossy()); applied.push((it.field.clone(), p)); }
            _ => { n_amb += 1; row["result"] = json!("ambiguous"); row["error"] = json!("ambiguous_match: several equally good candidates; not applied - narrow search_dirs"); } }
        rows.push(row);
    }
    if apply { for (f, p) in &applied { let store = match deps::base_dir() { Some(b) => p.strip_prefix(&*b).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string_lossy().into_owned()), None => p.to_string_lossy().into_owned() }; ss.project.set_ref(f, &store)?; } }
    let mut out = json!({"applied": apply, "relinked": if apply { applied.len() } else { 0 }, "would_relink": applied.len(), "ambiguous": n_amb, "not_found": n_none, "items": rows});
    if !apply { out["no_history"] = json!(true); } // a dry run changes nothing: dispatch skips the undo snapshot
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "list_dependencies", description: "List every external file the project reads (media, images, audio, LUTs, fonts, occlusion/matte inputs, OCIO configs): kind, owner, field, stored path, resolved path, status ok|missing|changed, size, sha256 (check_hash), used source range. Read-only.", mutates: false,
        schema: obj(vec![("check_hash", json!({"type":"boolean","description":"hash every present file (sha256; head+tail above 256 MB) and compare with the recorded hash"}))], &[]),
        run: |ss, a| { let items = deps::list_dependencies(&ss.project, flag(a, "check_hash", false)); Ok(json!({"summary": summary(&items), "items": items})) } },
    Command { name: "collect_project", description: "Copy the project and everything it reads into dest_dir (media/, fonts/, luts/, collect_manifest.json) and write a copy of the .kimo with relative paths. The original project file is never modified. Same content is stored once. Fonts with a restricted embedding license (OS/2 fsType) are not copied and are listed as warnings. Missing files are listed; the collect still succeeds. A killed collect leaves no partial project.", mutates: false,
        schema: obj(vec![("dest_dir", s("destination directory")), ("copy", json!({"type":"string","enum":["copy","link"],"description":"copy (default) or hard-link where possible"})), ("project_name", s("file name of the collected project, default project.kimo")), ("include_fonts", json!({"type":"boolean"})), ("include_luts", json!({"type":"boolean"})), ("rewrite", json!({"type":"boolean","description":"rewrite references to relative paths in the copy (default true)"})), ("overwrite", json!({"type":"boolean","description":"replace a previous collect in dest_dir"})), ("dry_run", json!({"type":"boolean","description":"report the plan and byte total only"}))], &["dest_dir"]),
        run: collect },
    Command { name: "relink_media", description: "Find moved or renamed files for missing references by searching search_dirs. Score: same sha256 > same name+size > same name. Several equal best candidates are reported as ambiguous and never applied. apply:false (default) only reports; apply:true rewrites the references as one undoable step.", mutates: true,
        schema: obj(vec![("search_dirs", json!({"type":"array","items":{"type":"string"}})), ("match", json!({"type":"string","enum":["name_size_hash","name","hash"]})), ("recursive", json!({"type":"boolean"})), ("apply", json!({"type":"boolean"})), ("only_missing", json!({"type":"boolean","description":"default true"}))], &["search_dirs"]),
        run: relink },
] }
