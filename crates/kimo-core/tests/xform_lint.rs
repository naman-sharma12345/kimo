//! A5: TEXTUAL lint (only the listed call patterns are caught: prop(c|&c, "<name>"..) and prop_or(c, ..), not props.get("x") style reads). The renderer reads a clip's x/y/scale/rotation/opacity through `resolve_xform` only, so parenting composes in one place.
use std::fs;
fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) { for e in fs::read_dir(dir).unwrap().flatten() { let p = e.path(); if p.is_dir() { rs_files(&p, out); } else if p.extension().map_or(false, |x| x == "rs") { out.push(p); } } }
#[test] fn no_direct_transform_prop_reads_outside_resolve_xform() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"); let mut files = vec![]; rs_files(&src, &mut files); let mut bad = vec![];
    for f in files { let name = f.file_name().unwrap().to_string_lossy().to_string(); if name == "curves.rs" || name == "subject_preview.rs" || name == "tools_0.rs" { continue; } // non-render: sampling, preview diff, clip setup
        for (i, l) in fs::read_to_string(&f).unwrap().lines().enumerate() {
            if l.contains("xform-lint: ok") { continue; }
            for k in ["x", "y", "scale", "rotation", "opacity"] { for pat in [format!("props.get(\"{k}\")"), format!("props[\"{k}\"]")] { if l.contains(&pat) { bad.push(format!("{}:{}: {}", name, i + 1, l.trim().chars().take(100).collect::<String>())); } } for pat in [format!("prop(c, \"{k}\""), format!("prop(&c, \"{k}\""), format!("prop_or(c, \"{k}\"")] { if l.contains(&pat) && !l.contains(&format!("xprop(p, {}", &pat[5..])) && !l.contains(&format!("xprop(p, &{}", &pat[5..])) { bad.push(format!("{}:{}: {}", name, i + 1, l.trim().chars().take(100).collect::<String>())); } } } } }
    assert!(bad.is_empty(), "direct transform prop reads (use resolve_xform):\n{}", bad.join("\n"));
}
