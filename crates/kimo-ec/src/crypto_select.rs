// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Editing a Cryptomatte effect's Selection text from a pick in the viewer (#473): replace it
//! with the picked ID, add it, or remove it, in the grammar `selection_hashes` reads.

use crate::channel3d::{selection_hashes, selection_names};

/// How a pick changes the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    Replace,
    Add,
    Remove,
}

/// A name as a selection token: quoted (with `\` escapes) when it has a comma, a quote or a
/// backslash, which the selection parser would otherwise split on or read as syntax.
fn quote(name: &str) -> String {
    if !name.contains([',', '"', '\'', '\\']) {
        return name.to_string();
    }
    let mut out = String::from("\"");
    for c in name.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The selection token for the ID `hash` named `name` in `manifest`: the (quoted) name when it
/// reads back as exactly that ID, else `<hex>`. That covers unnamed IDs, names with `*` (which
/// would select by wildcard), names that look like `<hex>`, names with surrounding spaces (the
/// parser trims them) and names shared by several IDs.
pub fn token_for(name: Option<&str>, hash: u32, manifest: Option<&[(String, u32)]>) -> String {
    match name {
        Some(n) if !n.is_empty() && !n.contains('*') && selection_hashes(&quote(n), manifest) == [hash] => quote(n),
        _ => format!("<{hash:08x}>"),
    }
}

/// `sel` edited by a pick of the ID `hash` whose token is `token` (see [`token_for`]). The text
/// stays as typed when the edit wouldn't change what it selects: Replace when it selects just
/// that ID, Add when it already selects it, Remove when no name or `<hex>` token selects it (a
/// wildcard that matches it stays). Other tokens (wildcards included) keep their text,
/// re-quoted where needed, separated by `, `.
pub fn edit_selection(sel: &str, token: &str, hash: u32, mode: PickMode, manifest: Option<&[(String, u32)]>) -> String {
    let tokens: Vec<String> = selection_names(sel).into_iter().filter(|t| !t.is_empty()).collect();
    let selects = |t: &String| !t.contains('*') && selection_hashes(&quote(t), manifest).contains(&hash);
    let kept: Vec<String> = match mode {
        PickMode::Replace if selection_hashes(sel, manifest) == [hash] => return sel.to_string(),
        PickMode::Replace => return token.to_string(),
        PickMode::Add if selection_hashes(sel, manifest).contains(&hash) => return sel.to_string(),
        PickMode::Add => tokens.iter().map(|t| quote(t)).chain([token.to_string()]).collect(),
        PickMode::Remove if !tokens.iter().any(selects) => return sel.to_string(),
        PickMode::Remove => tokens.iter().filter(|t| !selects(t)).map(|t| quote(t)).collect(),
    };
    kept.join(", ")
}

