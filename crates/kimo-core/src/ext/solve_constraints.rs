//! RV-28 C4: solve_constraints - read-only layout math. Returns coordinates; nothing is placed or moved.
//! Closed form for align / distribute / center / stack / within / anchor / same_size; min_gap uses bounded relaxation.
use super::kit::*;
use crate::commands::Session;

pub const MAX_ITEMS: usize = 1000;
pub const MAX_CONSTRAINTS: usize = 200;
pub const MAX_ITERATIONS: usize = 200;
const EPS: f64 = 1e-6;
#[derive(Clone)]
struct It { id: String, w: f64, h: f64, x: f64, y: f64, pin: bool }
type Bx = [f64; 4];
fn num(v: &Value, what: &str) -> Result<f64, String> { v.as_f64().filter(|x| x.is_finite()).ok_or(format!("out_of_range: {what} must be a finite number")) }
fn pos(it: &mut It, x: Option<f64>, y: Option<f64>) { if !it.pin { if let Some(x) = x { it.x = x; } if let Some(y) = y { it.y = y; } } }
fn ids(its: &[It], c: &Value, i: usize) -> Result<Vec<usize>, String> {
    let l = c.get("items").and_then(|v| v.as_array()).filter(|l| !l.is_empty()).ok_or(format!("invalid_constraint: constraints[{i}].items must be a non-empty array of item ids"))?;
    l.iter().map(|v| { let id = v.as_str().ok_or(format!("invalid_constraint: constraints[{i}].items must be strings"))?; its.iter().position(|t| t.id == id).ok_or(format!("invalid_constraint: constraints[{i}] names unknown item '{id}'")) }).collect()
}
fn rect_arg(c: &Value, k: &str, d: Bx) -> Result<Bx, String> { match c.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => { let a = v.as_array().filter(|a| a.len() == 4).ok_or(format!("out_of_range: {k} must be [x,y,w,h]"))?; let mut r = [0.0; 4]; for j in 0..4 { r[j] = num(&a[j], k)?; } Ok(r) } } }
/// Desired top-left per item for the closed-form kinds: (index, Some(x), Some(y)).
fn desired(its: &[It], bx: Bx, c: &Value, i: usize) -> Result<Vec<(usize, Option<f64>, Option<f64>)>, String> {
    let ty = c.get("type").and_then(|v| v.as_str()).unwrap_or(""); let ix = ids(its, c, i)?; let mut out = vec![];
    match ty {
        "align" => { let edge = c.get("edge").and_then(|v| v.as_str()).unwrap_or(""); let to = c.get("to").and_then(|v| v.as_str()).unwrap_or("box");
            let r = if to == "box" { bx } else { let t = its.iter().find(|t| t.id == to).ok_or(format!("invalid_constraint: constraints[{i}].to names unknown item '{to}'"))?; [t.x, t.y, t.w, t.h] };
            for &k in &ix { let t = &its[k]; let (dx, dy) = match edge { "left" => (Some(r[0]), None), "right" => (Some(r[0] + r[2] - t.w), None), "hcenter" => (Some(r[0] + r[2] / 2.0 - t.w / 2.0), None), "top" => (None, Some(r[1])), "bottom" => (None, Some(r[1] + r[3] - t.h)), "vcenter" => (None, Some(r[1] + r[3] / 2.0 - t.h / 2.0)), _ => return Err(format!("invalid_constraint: constraints[{i}].edge is not allowed. Valid: left, right, top, bottom, hcenter, vcenter")) }; out.push((k, dx, dy)); } }
        "center" => { let axis = c.get("axis").and_then(|v| v.as_str()).unwrap_or("both"); if !["both", "x", "y"].contains(&axis) { return Err(format!("invalid_constraint: constraints[{i}].axis is not allowed. Valid: both, x, y")); }
            for &k in &ix { let t = &its[k]; out.push((k, (axis != "y").then(|| bx[0] + bx[2] / 2.0 - t.w / 2.0), (axis != "x").then(|| bx[1] + bx[3] / 2.0 - t.h / 2.0))); } }
        "anchor" => { let a = c.get("anchor").and_then(|v| v.as_str()).unwrap_or(""); let m = c.get("margin").map(|v| num(v, "margin")).transpose()?.unwrap_or(0.0);
            let (h, v) = match a { "tl" => (0, 0), "tc" => (1, 0), "tr" => (2, 0), "cl" => (0, 1), "c" => (1, 1), "cr" => (2, 1), "bl" => (0, 2), "bc" => (1, 2), "br" => (2, 2), _ => return Err(format!("invalid_constraint: constraints[{i}].anchor is not allowed. Valid: tl, tc, tr, cl, c, cr, bl, bc, br")) };
            for &k in &ix { let t = &its[k]; let x = match h { 0 => bx[0] + m, 1 => bx[0] + bx[2] / 2.0 - t.w / 2.0, _ => bx[0] + bx[2] - t.w - m }; let y = match v { 0 => bx[1] + m, 1 => bx[1] + bx[3] / 2.0 - t.h / 2.0, _ => bx[1] + bx[3] - t.h - m }; out.push((k, Some(x), Some(y))); } }
        "within" => { let r = rect_arg(c, "rect", bx)?; for &k in &ix { let t = &its[k]; out.push((k, Some(if t.w >= r[2] { r[0] } else { t.x.clamp(r[0], r[0] + r[2] - t.w) }), Some(if t.h >= r[3] { r[1] } else { t.y.clamp(r[1], r[1] + r[3] - t.h) }))); } }
        "distribute" | "stack" => { let axis = c.get("axis").and_then(|v| v.as_str()).unwrap_or(""); if !["x", "y"].contains(&axis) { return Err(format!("invalid_constraint: constraints[{i}].axis is not allowed. Valid: x, y")); } let ax = axis == "x";
            let (sz, p0) = (|t: &It| if ax { t.w } else { t.h }, |t: &It| if ax { t.x } else { t.y });
            let mut cur = if ty == "stack" { p0(&its[ix[0]]) } else { if ax { bx[0] } else { bx[1] } };
            let gap = if ty == "stack" { c.get("gap").map(|v| num(v, "gap")).transpose()?.unwrap_or(0.0) } else { let total: f64 = ix.iter().map(|&k| sz(&its[k])).sum(); if ix.len() < 2 { 0.0 } else { ((if ax { bx[2] } else { bx[3] }) - total) / (ix.len() - 1) as f64 } };
            let cross = c.get("align").and_then(|v| v.as_str()); if let Some(a) = cross { if !["start", "center", "end"].contains(&a) { return Err(format!("invalid_constraint: constraints[{i}].align is not allowed. Valid: start, center, end")); } }
            for &k in &ix { let t = &its[k]; let c2 = cross.map(|a| { let (cs, cl, bs, bl) = if ax { (t.h, 0, bx[1], bx[3]) } else { (t.w, 0, bx[0], bx[2]) }; let _ = cl; match a { "start" => bs, "center" => bs + bl / 2.0 - cs / 2.0, _ => bs + bl - cs } });
                if ax { out.push((k, Some(cur), c2)); } else { out.push((k, c2, Some(cur))); } cur += sz(t) + gap; } }
        "same_size" | "min_gap" => {}
        _ => return Err(format!("invalid_constraint: constraints[{i}].type '{ty}' is not allowed. Valid: align, distribute, anchor, min_gap, within, stack, center, same_size")),
    }
    Ok(out)
}
fn shortfall(a: &It, b: &It, gap: f64) -> f64 { let gx = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w)); let gy = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h)); (gap - gx.max(gy)).max(0.0) }
fn residual(its: &[It], bx: Bx, c: &Value, i: usize) -> Result<f64, String> {
    let ty = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "min_gap" => { let gap = c.get("gap").map(|v| num(v, "gap")).transpose()?.unwrap_or(0.0); let ix = ids(its, c, i)?; let mut r = 0f64; for a in 0..ix.len() { for b in a + 1..ix.len() { r = r.max(shortfall(&its[ix[a]], &its[ix[b]], gap)); } } Ok(r) }
        "same_size" => { let ix = ids(its, c, i)?; let f = &its[ix[0]]; Ok(ix.iter().map(|&k| (its[k].w - f.w).abs().max((its[k].h - f.h).abs())).fold(0.0, f64::max)) }
        "within" => { let r = rect_arg(c, "rect", bx)?; let mut m = 0f64; for k in ids(its, c, i)? { let t = &its[k]; m = m.max(r[0] - t.x).max(t.x + t.w - (r[0] + r[2])).max(r[1] - t.y).max(t.y + t.h - (r[1] + r[3])); } Ok(m.max(0.0)) }
        _ => { let mut m = 0f64; for (k, dx, dy) in desired(its, bx, c, i)? { if let Some(x) = dx { m = m.max((its[k].x - x).abs()); } if let Some(y) = dy { m = m.max((its[k].y - y).abs()); } } Ok(m) }
    }
}
fn apply(its: &mut Vec<It>, bx: Bx, c: &Value, i: usize, iters: &mut usize) -> Result<(), String> {
    let ty = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty == "same_size" { let ix = ids(its, c, i)?; let (w, h) = (its[ix[0]].w, its[ix[0]].h); for &k in &ix { its[k].w = w; its[k].h = h; } return Ok(()); }
    if ty == "min_gap" { let gap = c.get("gap").map(|v| num(v, "gap")).transpose()?.unwrap_or(0.0); let ix = ids(its, c, i)?;
        for _ in 0..MAX_ITERATIONS { *iters += 1; let mut moved = false;
            for a in 0..ix.len() { for b in a + 1..ix.len() { let (p, q) = (ix[a], ix[b]); let s = shortfall(&its[p], &its[q], gap); if s <= EPS { continue; } moved = true;
                let (cxp, cyp, cxq, cyq) = (its[p].x + its[p].w / 2.0, its[p].y + its[p].h / 2.0, its[q].x + its[q].w / 2.0, its[q].y + its[q].h / 2.0);
                let gx = (its[q].x - (its[p].x + its[p].w)).max(its[p].x - (its[q].x + its[q].w)); let gy = (its[q].y - (its[p].y + its[p].h)).max(its[p].y - (its[q].y + its[q].h));
                let (dx, dy) = if gx >= gy { (if cxq >= cxp { 1.0 } else { -1.0 }, 0.0) } else { (0.0, if cyq >= cyp { 1.0 } else { -1.0 }) };
                let (mp, mq) = (!its[p].pin, !its[q].pin); let (fp, fq) = match (mp, mq) { (true, true) => (0.5, 0.5), (true, false) => (1.0, 0.0), (false, true) => (0.0, 1.0), _ => (0.0, 0.0) };
                its[p].x -= dx * s * fp; its[p].y -= dy * s * fp; its[q].x += dx * s * fq; its[q].y += dy * s * fq; } }
            if !moved { break; } }
        return Ok(()); }
    for (k, dx, dy) in desired(its, bx, c, i)? { pos(&mut its[k], dx, dy); }
    Ok(())
}
fn solve(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let bx: Bx = match a.get("box") { None => return Err("out_of_range: box is required: [x,y,w,h] or \"canvas\"".into()), Some(Value::String(s)) if s == "canvas" => [0.0, 0.0, ss.project.width as f64, ss.project.height as f64], Some(v) => rect_arg(&json!({"box": v}), "box", [0.0; 4])? };
    if bx[2] < 0.0 || bx[3] < 0.0 { return Err("out_of_range: box width and height must be >= 0".into()); }
    let il = a.get("items").and_then(|v| v.as_array()).ok_or("out_of_range: items must be an array of {id,w,h,pos?,anchor?}")?;
    if il.len() > MAX_ITEMS { return Err(format!("out_of_range: at most {MAX_ITEMS} items")); }
    let mut its: Vec<It> = vec![];
    for (k, v) in il.iter().enumerate() {
        let id = v.get("id").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).ok_or(format!("out_of_range: items[{k}].id must be a string"))?; if its.iter().any(|t| t.id == id) { return Err(format!("out_of_range: duplicate item id '{id}'")); }
        let (w, h) = (num(v.get("w").unwrap_or(&Value::Null), &format!("items[{k}].w"))?, num(v.get("h").unwrap_or(&Value::Null), &format!("items[{k}].h"))?); if w < 0.0 || h < 0.0 { return Err(format!("out_of_range: items[{k}] size must be >= 0")); }
        let (mut x, mut y, pin) = match v.get("pos") { Some(p) if !p.is_null() => { let p = p.as_array().filter(|p| p.len() == 2).ok_or(format!("out_of_range: items[{k}].pos must be [x,y]"))?; (num(&p[0], "pos")?, num(&p[1], "pos")?, true) } _ => (bx[0], bx[1], false) };
        if pin { match v.get("anchor").and_then(|x| x.as_str()).unwrap_or("tl") { "tl" => {} "center" => { x -= w / 2.0; y -= h / 2.0; } o => return Err(format!("out_of_range: items[{k}].anchor '{o}' is not allowed. Valid: tl, center")) } }
        its.push(It { id: id.to_string(), w, h, x, y, pin });
    }
    let cl = a.get("constraints").and_then(|v| v.as_array()).ok_or("out_of_range: constraints must be an array")?; if cl.len() > MAX_CONSTRAINTS { return Err(format!("out_of_range: at most {MAX_CONSTRAINTS} constraints")); }
    let (mut iters, mut satisfied, mut violated): (usize, Vec<usize>, Vec<Value>) = (0, vec![], vec![]);
    for (i, c) in cl.iter().enumerate() {
        let ty = c.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let backup = its.clone(); apply(&mut its, bx, c, i, &mut iters)?;
        let mut broke = None; for &j in &satisfied { if residual(&its, bx, &cl[j], j)? > 1e-3 { broke = Some(j); break; } }
        if let Some(j) = broke { let r = residual(&its, bx, c, i)?; its = backup; violated.push(json!({"idx": i, "type": ty, "reason": format!("conflicts_with_constraint_{j}: applying it would undo an earlier satisfied constraint; left unapplied"), "residual": r})); continue; }
        let r = residual(&its, bx, c, i)?;
        if r <= 1e-3 { satisfied.push(i); } else { let ix = ids(&its, c, i)?; let why = if ix.iter().any(|&k| its[k].pin) { "pinned_item_cannot_move" } else if ty == "min_gap" { "did_not_converge_or_box_too_small" } else { "unsatisfiable_in_box" }; violated.push(json!({"idx": i, "type": ty, "reason": why, "residual": r})); }
    }
    let positions: Vec<Value> = its.iter().map(|t| json!({"id": t.id, "x": t.x, "y": t.y, "w": t.w, "h": t.h, "cx": t.x + t.w / 2.0, "cy": t.y + t.h / 2.0, "pinned": t.pin})).collect();
    Ok(json!({"positions": positions, "satisfied": satisfied, "violated": violated, "iterations": iters, "box": bx, "note": "read-only, nothing is moved. x,y are top-left pixels; apply with batch and set_property or set_keyframes"}))
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "solve_constraints", mutates: false, description: "Read-only, nothing is moved: solve a small layout problem and return coordinates. box is [x,y,w,h] or \"canvas\"; items are {id,w,h,pos?} (pos pins an item, anchor tl|center says which point pos names); constraints apply in order: align (edge left|right|top|bottom|hcenter|vcenter, to box or item id), distribute (axis x|y, equal gaps across the box), anchor (tl..br, margin), min_gap (gap), within (rect default box), stack (axis, gap, align), center (axis), same_size. Unpinned items start at the box origin. Returns positions (top-left x,y plus centre cx,cy), satisfied constraint indexes and violated ones with a reason and residual; a constraint that would undo an earlier satisfied one is reported violated and left unapplied. Max 1000 items, 200 constraints, 200 relaxation iterations for min_gap.",
        schema: obj(vec![("box", json!({"description":"[x,y,w,h] pixels or the string canvas"})), ("items", json!({"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"w":{"type":"number"},"h":{"type":"number"},"pos":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2},"anchor":{"type":"string","enum":["tl","center"]}},"required":["id","w","h"]}})), ("constraints", json!({"type":"array","items":{"type":"object","properties":{"type":{"type":"string","enum":["align","distribute","anchor","min_gap","within","stack","center","same_size"]},"items":{"type":"array","items":{"type":"string"}}},"required":["type","items"]}}))], &["box","items","constraints"]), run: solve },
] }
