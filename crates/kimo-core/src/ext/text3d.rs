//! add_text3d: extruded 3D text with PBR materials and lights, seen through the project camera (a Scene3D layer). Each
//! letter is its own object (its outline triangulated for the front and back faces, side walls along every contour),
//! pivoting on its own centre, so per-character 3D animation (flip in, rise, spin) works on top of the whole-word
//! transform.
use super::kit::*;

type P = [f64; 2];
fn area(c: &[P]) -> f64 { let n = c.len(); (0..n).map(|i| { let (a, b) = (c[i], c[(i + 1) % n]); a[0] * b[1] - b[0] * a[1] }).sum::<f64>() / 2.0 }
fn inside(p: P, c: &[P]) -> bool { let mut r = false; let n = c.len(); let mut j = n - 1; for i in 0..n { let (a, b) = (c[i], c[j]); if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] { r = !r; } j = i; } r }
fn cross(o: P, a: P, b: P) -> f64 { (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]) }

/// Joins each hole into its outer polygon through a bridge from the hole's rightmost point to a visible outer vertex,
/// giving one simple polygon (outer counter-clockwise, holes clockwise in the math sense).
fn bridge(mut outer: Vec<P>, mut holes: Vec<Vec<P>>) -> Vec<P> {
    if area(&outer) < 0.0 { outer.reverse(); }
    for h in holes.iter_mut() { if area(h) > 0.0 { h.reverse(); } }
    holes.sort_by(|a, b| { let mx = |c: &Vec<P>| c.iter().map(|p| p[0]).fold(f64::MIN, f64::max); mx(b).partial_cmp(&mx(a)).unwrap_or(std::cmp::Ordering::Equal) });
    for h in holes {
        let hi = (0..h.len()).max_by(|&a, &b| h[a][0].partial_cmp(&h[b][0]).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
        let m = h[hi];
        // the nearest outer vertex to the right whose segment to m crosses no outer edge
        let seg_ok = |v: P| { let n = outer.len(); (0..n).all(|i| { let (a, b) = (outer[i], outer[(i + 1) % n]); if a == v || b == v { return true; }
            let d1 = cross(m, v, a); let d2 = cross(m, v, b); let d3 = cross(a, b, m); let d4 = cross(a, b, v); !((d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)) }) };
        let mut cand: Vec<usize> = (0..outer.len()).collect();
        cand.sort_by(|&a, &b| { let da = (outer[a][0] - m[0]).powi(2) + (outer[a][1] - m[1]).powi(2) + if outer[a][0] < m[0] { 1e12 } else { 0.0 }; let db = (outer[b][0] - m[0]).powi(2) + (outer[b][1] - m[1]).powi(2) + if outer[b][0] < m[0] { 1e12 } else { 0.0 }; da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal) });
        let oi = cand.into_iter().find(|&i| seg_ok(outer[i])).unwrap_or(0);
        let mut merged: Vec<P> = outer[..=oi].to_vec();
        for k in 0..=h.len() { merged.push(h[(hi + k) % h.len()]); }
        merged.push(outer[oi]); merged.extend_from_slice(&outer[oi + 1..]);
        outer = merged;
    }
    outer
}
/// Ear-clipping triangulation of a simple counter-clockwise polygon (bridged holes allowed).
fn earclip(poly: &[P]) -> Vec<[P; 3]> {
    let mut idx: Vec<usize> = (0..poly.len()).collect(); let mut out = vec![]; let mut guard = 0;
    while idx.len() > 3 && guard < poly.len() * poly.len() + 10 {
        guard += 1; let n = idx.len(); let mut clipped = false;
        for k in 0..n {
            let (a, b, c) = (poly[idx[(k + n - 1) % n]], poly[idx[k]], poly[idx[(k + 1) % n]]);
            if cross(a, b, c) <= 1e-12 { continue; }
            let tri = [a, b, c];
            if idx.iter().any(|&j| { let p = poly[j]; p != a && p != b && p != c && cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0 }) { continue; }
            out.push(tri); idx.remove(k); clipped = true; break;
        }
        if !clipped { // degenerate leftovers (collinear runs): drop a vertex and go on
            idx.remove(0);
        }
    }
    if idx.len() == 3 { let (a, b, c) = (poly[idx[0]], poly[idx[1]], poly[idx[2]]); if cross(a, b, c).abs() > 1e-12 { out.push(if cross(a, b, c) > 0.0 { [a, b, c] } else { [a, c, b] }); } }
    out
}
/// Front-face triangles of a glyph's contours (outers with their holes).
pub fn triangulate(contours: &[Vec<P>]) -> Vec<[P; 3]> {
    // repeated and collinear points (curve segments meeting, a straight run split in two) stall ear clipping: drop them
    let clean = |c: &Vec<P>| -> Vec<P> { let mut o: Vec<P> = vec![]; for &p in c { if o.last().map_or(true, |q: &P| (q[0] - p[0]).abs() > 1e-7 || (q[1] - p[1]).abs() > 1e-7) { o.push(p); } }
        while o.len() > 1 && (o[0][0] - o[o.len() - 1][0]).abs() < 1e-7 && (o[0][1] - o[o.len() - 1][1]).abs() < 1e-7 { o.pop(); }
        let mut changed = true; while changed && o.len() > 3 { changed = false; let n = o.len(); for i in 0..n { let (a, b, c) = (o[(i + n - 1) % n], o[i], o[(i + 1) % n]); if cross(a, b, c).abs() < 1e-9 { o.remove(i); changed = true; break; } } } o };
    let contours: Vec<Vec<P>> = contours.iter().map(clean).filter(|c| c.len() >= 3 && area(c).abs() > 1e-6).collect();
    let contours = &contours[..];
    let mut outers: Vec<(usize, f64)> = vec![]; let mut holes: Vec<(usize, usize)> = vec![];
    // a contour inside an odd number of others is a hole of the smallest one containing it
    for (i, c) in contours.iter().enumerate() {
        let p = c[0]; let containing: Vec<usize> = (0..contours.len()).filter(|&j| j != i && inside(p, &contours[j])).collect();
        if containing.len() % 2 == 1 { let parent = *containing.iter().min_by(|&&x, &&y| area(&contours[x]).abs().partial_cmp(&area(&contours[y]).abs()).unwrap_or(std::cmp::Ordering::Equal)).unwrap(); holes.push((i, parent)); } else { outers.push((i, area(c).abs())); }
    }
    let mut tris = vec![];
    for (oi, _) in outers { let hs: Vec<Vec<P>> = holes.iter().filter(|h| h.1 == oi).map(|h| contours[h.0].clone()).collect(); tris.extend(earclip(&bridge(contours[oi].clone(), hs))); }
    tris
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let text = str_arg(a, "text")?.replace("\\n", "\n");
    let font = a.get("font").and_then(|v| v.as_str()).unwrap_or("Inter").to_string();
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let size = opt_num(a, "size", h.min(w) * 0.16).clamp(8.0, 2000.0); let depth = opt_num(a, "depth", size * 0.22).clamp(0.0, size * 4.0);
    let weight = opt_num(a, "weight", 800.0);
    let mut fx = crate::model::TextFx::default(); fx.weight = weight;
    let glyphs = crate::text::glyph_contours(&text, &font, size, opt_num(a, "letter_spacing", 0.0), weight, "center", w, h, Some(&fx))?;
    if glyphs.is_empty() { return Err("the text has no visible glyphs".into()); }
    let col = |k: &str, d: [f64; 3]| -> [f64; 3] { a.get(k).and_then(|v| v.as_array()).filter(|v| v.len() == 3).map(|v| [v[0].as_f64().unwrap_or(d[0]), v[1].as_f64().unwrap_or(d[1]), v[2].as_f64().unwrap_or(d[2])]).unwrap_or(d) };
    let (color, side) = (col("color", [0.95, 0.95, 0.95]), col("side_color", [0.0; 3]));
    let side = if a.get("side_color").is_some() { Some(side) } else { None };
    let (rough, metal) = (opt_num(a, "rough", 0.35), opt_num(a, "metal", 0.0));
    let (start, dur) = (opt_num(a, "start", 0.0), opt_num(a, "duration", 4.0));
    let anim = a.get("per_char").and_then(|v| v.as_str()).unwrap_or("none");
    if !["none", "flip", "rise", "spin", "drop"].contains(&anim) { return Err("per_char must be none|flip|rise|spin|drop".into()); }
    let stagger = opt_num(a, "stagger", 0.05).max(0.0); let anim_len = opt_num(a, "anim_duration", 0.7).max(0.05);
    // object 0: a null that carries the whole word's transform; each letter is parented to it
    let mut objects = vec![json!({"kind": "null", "pos": a.get("pos").cloned().unwrap_or(json!([0, 0, 0])), "rot": a.get("rot").cloned().unwrap_or(json!([0, 0, 0])), "keys": a.get("keys").cloned().unwrap_or(json!([]))})];
    let mut ntri = 0;
    for (k, (ch, contours)) in glyphs.iter().enumerate() {
        let pts: Vec<P> = contours.iter().flatten().copied().collect(); if pts.is_empty() { continue; }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in &pts { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
        let (cx, cy, cz) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, depth / 2.0);
        let mut tris: Vec<[f64; 9]> = vec![];
        let v3 = |p: P, z: f64| [p[0] - cx, p[1] - cy, z - cz];
        // screen y points down, so a counter-clockwise 2D triangle (math sense) faces the camera when reversed here
        for t in triangulate(contours) {
            let (a2, b2, c2) = (v3(t[0], 0.0), v3(t[1], 0.0), v3(t[2], 0.0)); tris.push([a2[0], a2[1], a2[2], c2[0], c2[1], c2[2], b2[0], b2[1], b2[2]]);
            if depth > 0.0 { let (a3, b3, c3) = (v3(t[0], depth), v3(t[1], depth), v3(t[2], depth)); tris.push([a3[0], a3[1], a3[2], b3[0], b3[1], b3[2], c3[0], c3[1], c3[2]]); }
        }
        let mut walls: Vec<[f64; 9]> = vec![];
        if depth > 0.0 { for c in contours { let s = if area(c) > 0.0 { 1.0 } else { -1.0 }; let n = c.len(); for i in 0..n { let (p, q) = (c[i], c[(i + 1) % n]);
            let (a0, b0, a1, b1) = (v3(p, 0.0), v3(q, 0.0), v3(p, depth), v3(q, depth));
            if s > 0.0 { walls.push([a0[0], a0[1], a0[2], b0[0], b0[1], b0[2], b1[0], b1[1], b1[2]]); walls.push([a0[0], a0[1], a0[2], b1[0], b1[1], b1[2], a1[0], a1[1], a1[2]]); }
            else { walls.push([a0[0], a0[1], a0[2], b1[0], b1[1], b1[2], b0[0], b0[1], b0[2]]); walls.push([a0[0], a0[1], a0[2], a1[0], a1[1], a1[2], b1[0], b1[1], b1[2]]); } } } }
        let t0 = k as f64 * stagger;
        let keys = match anim {
            "flip" => json!([{"t": t0, "rot": [-90, 0, 0]}, {"t": t0 + anim_len, "rot": [0, 0, 0]}]),
            "spin" => json!([{"t": t0, "rot": [0, 180, 0]}, {"t": t0 + anim_len, "rot": [0, 0, 0]}]),
            "rise" => json!([{"t": t0, "pos": [cx, cy + size * 0.6, 0.0]}, {"t": t0 + anim_len, "pos": [cx, cy, 0.0]}]),
            "drop" => json!([{"t": t0, "pos": [cx, cy - size * 1.2, -size]}, {"t": t0 + anim_len, "pos": [cx, cy, 0.0]}]),
            _ => json!([]),
        };
        ntri += tris.len() + walls.len();
        let base = json!({"kind": "mesh", "parent": 0, "pos": [cx, cy, 0.0], "color": color, "rough": rough, "metal": metal, "flat": false, "keys": keys});
        match side {
            Some(sc) => { let mut f = base.clone(); f["tris"] = json!(tris); objects.push(f); let mut wv = base.clone(); wv["tris"] = json!(walls); wv["color"] = json!(sc); objects.push(wv); }
            None => { let mut f = base.clone(); let mut all = tris; all.extend(walls); f["tris"] = json!(all); objects.push(f); }
        }
        let _ = ch;
    }
    let lights = a.get("lights").cloned().unwrap_or(json!([{"kind": "directional", "dir": [-0.4, 0.5, 1.0], "color": [1.0, 0.97, 0.92], "intensity": 1.1}, {"kind": "directional", "dir": [0.6, -0.2, 0.8], "color": [0.6, 0.7, 0.9], "intensity": 0.45}]));
    let config = json!({"objects": objects, "lights": lights, "ambient": a.get("ambient").cloned().unwrap_or(json!([0.12, 0.12, 0.14])), "shadows": a.get("shadows").cloned().unwrap_or(json!(false))});
    let tid = match a.get("track_id").and_then(|v| v.as_str()) { Some(t) => t.to_string(), None => crate::commands::dispatch_nested(ss, "add_track", &json!({"name": "3D text"}))?["track_id"].as_str().unwrap_or("").to_string() };
    let mut r = crate::commands::dispatch_nested(ss, "add_scene3d", &json!({"track_id": tid, "start": start, "duration": dur, "config": config}))?;
    r["letters"] = json!(glyphs.len()); r["triangles"] = json!(ntri); r["track_id"] = json!(tid);
    Ok(r)
}

pub fn commands() -> Vec<Command> {
    vec![Command { name: "add_text3d", description: "Extruded 3D text: real geometry with depth, PBR material (color, rough, metal, optional side_color for the walls) and lights, seen through the project camera. Each letter is its own object, so per_char animates them in 3D (flip, rise, spin, drop; staggered); pos/rot/keys move the whole word (Scene3D keys {t, pos, rot, scale}).", mutates: true,
        schema: obj(vec![("text", s("the text")), ("track_id", s("track (default: a new one)")), ("start", n("seconds")), ("duration", n("seconds, default 4")), ("font", s("default Inter")), ("weight", n("variable-font weight, default 800")),
            ("size", n("type size px (default 16% of the short side)")), ("depth", n("extrusion depth px (default 22% of size)")), ("color", json!({"type":"array","items":{"type":"number"}})), ("side_color", json!({"type":"array","items":{"type":"number"},"description":"walls in another colour"})),
            ("rough", n("0..1, default 0.35")), ("metal", n("0..1, default 0")), ("per_char", json!({"type":"string","enum":["none","flip","rise","spin","drop"]})), ("stagger", n("seconds between letters, default 0.05")), ("anim_duration", n("seconds per letter, default 0.7")),
            ("pos", json!({"type":"array","items":{"type":"number"}})), ("rot", json!({"type":"array","items":{"type":"number"},"description":"degrees [x, y, z]"})), ("keys", json!({"type":"array","items":{"type":"object"},"description":"whole-word keys [{t, pos?, rot?, scale?}]"})),
            ("lights", json!({"type":"array","items":{"type":"object"}})), ("shadows", json!({"type":"boolean"})), ("letter_spacing", n("px"))], &["text"]),
        run }]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn a_square_with_a_hole_triangulates_to_the_ring_area() {
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]; let hole = vec![[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0]];
        let t = triangulate(&[outer, hole]); let a: f64 = t.iter().map(|q| cross(q[0], q[1], q[2]).abs() / 2.0).sum();
        assert!((a - 84.0).abs() < 1e-6, "area {a} (100 - 16)");
    }
    #[test] fn real_glyphs_triangulate_to_their_outline_area() {
        for txt in ["H", "o", "B", "g", "%"] {
            let mut fx = crate::model::TextFx::default(); fx.weight = 800.0;
            let gl = crate::text::glyph_contours(txt, "Inter", 200.0, 0.0, 800.0, "center", 1000.0, 600.0, Some(&fx)).unwrap();
            for (_, cs) in gl { let want: f64 = { let mut v: Vec<f64> = cs.iter().map(|c| area(c)).collect(); let sign = if v.iter().cloned().fold(0.0, |a: f64, b| if b.abs() > a.abs() { b } else { a }) < 0.0 { -1.0 } else { 1.0 }; v.iter_mut().for_each(|x| *x *= sign); v.iter().sum::<f64>().abs() };
                let got: f64 = triangulate(&cs).iter().map(|q| cross(q[0], q[1], q[2]).abs() / 2.0).sum();
                assert!((got - want).abs() < want * 0.01, "{txt}: triangles cover {got:.0} of {want:.0} px^2"); }
        }
    }
    #[test] fn a_plain_polygon_triangulates_fully() {
        let l = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 1.0], [1.0, 1.0], [1.0, 5.0], [0.0, 5.0]];
        let t = triangulate(&[l]); let a: f64 = t.iter().map(|q| cross(q[0], q[1], q[2]).abs() / 2.0).sum(); assert!((a - 8.0).abs() < 1e-6, "{a}"); assert_eq!(t.len(), 4);
    }
}
