//! SVG import as animatable layers: import_svg, inspect_svg, recolor_svg_layer, draw_on_svg_layer.
//!
//! Built on the existing Bezier path shape (`shape: "path"` + a `shape_path` effect, see path_tools.rs) and the existing
//! shape modifiers (Trim Paths for stroke reveal, Gradient Fill/Stroke), nothing new in the rasterizer except a bounding-box
//! limit for path shapes (render.rs). The parser lives in crate::svgimport.
//!
//! Layout: each top-level SVG group becomes ONE precomp clip (a "layer", named after the group's id / Inkscape label) on its
//! own track, so the ordinary tools move, scale, rotate, fade and keyframe it as a unit. Inside, every drawable is one path
//! shape clip, centred on its own bounding box and offset with x/y (so it scales and rotates about itself, and gradients
//! span the shape). recolor_svg_layer and draw_on_svg_layer reach into a layer's shapes.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::model::*;
use crate::svgimport::{Doc, Paint, PathData};
use std::collections::BTreeMap;

pub fn new_clip(ss: &mut Session, source: Source, start: f64, dur: f64) -> Clip { Clip { id: ss.project.gen_id("clip"), source, start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, speed_blend: Default::default(), props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None } }
pub fn set_prop(c: &mut Clip, p: &str, v: f64) { c.props.entry(p.to_string()).or_insert_with(|| Prop::new(v)).base = v; }
pub fn key(c: &mut Clip, p: &str, t: f64, v: f64, e: Ease) { c.props.entry(p.to_string()).or_insert_with(|| Prop::new(v)).set_key(t, v, e); }
pub fn new_track(ss: &mut Session, name: &str, clips: Vec<Clip>) -> Track { Track { id: ss.project.gen_id("track"), clips: clips.into_iter().map(std::sync::Arc::new).collect(), locked: false, muted: false, solo: false, name: name.to_string() } }
/// A precomp ("group") clip holding `tracks` on its own clock from 0.
pub fn precomp(ss: &mut Session, tracks: Vec<Track>, start: f64, dur: f64, name: &str) -> Clip { let mut c = new_clip(ss, Source::Precomp { tracks }, start, dur); c.name = Some(name.to_string()); c }
/// A path-shape clip for `d` (pixels, relative to the canvas centre). `fill` None = no fill.
pub fn path_clip(ss: &mut Session, d: &str, evenodd: bool, size: [f64; 2], fill: Option<[f64; 3]>, stroke_width: f64, stroke: [f64; 3], start: f64, dur: f64) -> Clip {
    let mut c = new_clip(ss, Source::Shape { shape: "path".into(), width: size[0].max(1.0), height: size[1].max(1.0), fill: fill.unwrap_or([-1.0, 0.0, 0.0]), stroke_width, stroke_color: stroke, sides: 4, corner_radius: 0.0 }, start, dur);
    let mut strings = BTreeMap::new(); strings.insert("d".into(), d.to_string()); strings.insert("fill_rule".into(), if evenodd { "evenodd" } else { "nonzero" }.into());
    c.effects.push(Effect { id: "shape_path".into(), kind: "shape_path".into(), params: BTreeMap::new(), strings }); c
}
pub fn effect(kind: &str, params: &[(&str, f64)]) -> Effect { Effect { id: kind.into(), kind: kind.into(), params: params.iter().map(|(k, v)| (k.to_string(), Prop::new(*v))).collect(), strings: BTreeMap::new() } }

pub fn bool_opt(a: &Value, k: &str) -> Result<Option<bool>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) } }
pub fn num_opt(a: &Value, k: &str) -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "number")) } }
pub fn rgb_opt(a: &Value, k: &str) -> Result<Option<[f64; 3]>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => crate::svgimport::parse_color(s, [0.0; 3]).map(|c| c.map(|c| c.0)).map_err(|e| format!("{k}: {e}")),
        Some(Value::Array(v)) if v.len() >= 3 && v.iter().take(3).all(|x| x.as_f64().map_or(false, |f| (0.0..=1.0).contains(&f))) => Ok(Some([v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()])),
        Some(v) => Err(format!("{k} must be [r,g,b] 0..1 or a CSS colour like \"#ff8800\", got {v}")) }
}
pub fn str_list(a: &Value, k: &str) -> Result<Option<Vec<String>>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::Array(v)) => v.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| format!("{k} must be a list of strings"))).collect::<Result<Vec<_>, _>>().map(Some), Some(_) => Err(format!("{k} must be a list of strings")) } }

/// Read SVG text from `svg` (inline) or `path` (file).
fn svg_text(a: &Value) -> Result<String, String> {
    match (a.get("svg").and_then(|v| v.as_str()), a.get("path").and_then(|v| v.as_str())) {
        (Some(_), Some(_)) => Err("give either 'svg' (inline SVG text) or 'path' (an .svg file), not both".into()),
        (Some(s), None) => Ok(s.to_string()),
        (None, Some(p)) => { let md = std::fs::metadata(p).map_err(|e| format!("cannot read '{p}': {e}"))?; if md.len() > 16 << 20 { return Err(format!("'{p}' is larger than 16 MB")); } std::fs::read_to_string(p).map_err(|e| format!("cannot read '{p}': {e}")) }
        (None, None) => Err("give 'svg' (inline SVG text) or 'path' (an .svg file)".into()),
    }
}

/// Fill `t0..t0+dur` on the timeline: `start`/`duration` args, default = 0 and the project length (min 5 s).
pub fn timing(ss: &Session, a: &Value) -> Result<(f64, f64), String> {
    let start = num_opt(a, "start")?.unwrap_or(0.0); if start < 0.0 { return Err("start must be >= 0".into()); }
    let dur = num_opt(a, "duration")?.unwrap_or_else(|| (ss.project.duration() - start).max(5.0)); if dur <= 0.0 { return Err("duration must be > 0".into()); } Ok((start, dur))
}

pub struct Placed { pub layers: Vec<Value>, pub track_ids: Vec<String>, pub scale: f64 }

/// Lay `doc` out on the canvas and add its layers. Shared by import_svg and add_asset.
pub fn place(ss: &mut Session, doc: &Doc, a: &Value) -> Result<Placed, String> {
    let (start, dur) = timing(ss, a)?;
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let only = str_list(a, "only")?; let split = str_list(a, "split")?;
    if let Some(o) = &only { for n in o { if !doc.layers.iter().any(|l| &l.name == n) { return Err(format!("only: no layer '{n}'. Layers: {}", names(doc))); } } }
    if let Some(o) = &split { for n in o { if !doc.layers.iter().any(|l| &l.name == n) { return Err(format!("split: no layer '{n}'. Layers: {}", names(doc))); } } }
    let layers: Vec<&crate::svgimport::Layer> = doc.layers.iter().filter(|l| only.as_ref().map_or(true, |o| o.contains(&l.name))).collect();
    // what is framed: the document's viewBox, or (with `only`) just the chosen layers
    let frame = if only.is_some() { let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN]; for l in &layers { for s in &l.shapes { if let Some(x) = s.path.bbox() { b = [b[0].min(x[0]), b[1].min(x[1]), b[2].max(x[2]), b[3].max(x[3])]; } } } [b[0], b[1], (b[2] - b[0]).max(1e-6), (b[3] - b[1]).max(1e-6)] } else { doc.view };
    let k = match (num_opt(a, "width")?, num_opt(a, "height")?) {
        (Some(w), _) if w > 0.0 => w / frame[2], (None, Some(h)) if h > 0.0 => h / frame[3], (Some(_), _) | (None, Some(_)) => return Err("width/height must be > 0".into()),
        _ => (0.7 * cw / frame[2]).min(0.7 * ch / frame[3]),
    };
    let (ox, oy) = (num_opt(a, "x")?.unwrap_or(0.0), num_opt(a, "y")?.unwrap_or(0.0));
    let (fcx, fcy) = (frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0);
    let color = rgb_opt(a, "color")?;
    let to_canvas = |p: crate::svgimport::Pt| [(p[0] - fcx) * k + ox, (p[1] - fcy) * k + oy];
    // each split layer is its own precomp; the rest share one ("rest") precomp. Default: split everything up to 24 layers.
    let split_all = split.is_none() && layers.len() <= 24;
    let mut groups: Vec<(String, Vec<&crate::svgimport::Layer>)> = vec![]; let mut rest: Vec<&crate::svgimport::Layer> = vec![];
    for l in &layers { if split_all || split.as_ref().map_or(false, |s| s.contains(&l.name)) { groups.push((l.name.clone(), vec![*l])); } else { rest.push(*l); } }
    if !rest.is_empty() { groups.insert(0, (if groups.is_empty() { a.get("name").and_then(|v| v.as_str()).unwrap_or("svg").to_string() } else { "rest".to_string() }, rest)); }
    let mut out = vec![]; let mut tids = vec![]; let mut new_tracks = vec![];
    for (gname, ls) in groups {
        // shapes in canvas pixels
        let mut shapes: Vec<(String, crate::paths::Path, PathData, &crate::svgimport::Shape)> = vec![];
        for l in &ls { for (i, s) in l.shapes.iter().enumerate() {
            let pd = s.path.map(to_canvas); let d = pd.to_d(); let p = crate::paths::Path::parse(&d, s.evenodd).map_err(|e| format!("layer '{}' shape {} ({}): {e}", l.name, i + 1, s.tag))?;
            shapes.push((if s.id.is_empty() { format!("{}_{}", s.tag, i + 1) } else { s.id.clone() }, p, pd, s)); } }
        if shapes.is_empty() { continue; }
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN]; for (_, p, _, _) in &shapes { b = [b[0].min(p.bbox[0]), b[1].min(p.bbox[1]), b[2].max(p.bbox[2]), b[3].max(p.bbox[3])]; }
        let lc = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let mut clips = vec![];
        for (name, p, pd, s) in &shapes {
            let sc = [(p.bbox[0] + p.bbox[2]) / 2.0, (p.bbox[1] + p.bbox[3]) / 2.0]; let local = pd.map(|q| [q[0] - sc[0], q[1] - sc[1]]).to_d();
            let size = [p.bbox[2] - p.bbox[0], p.bbox[3] - p.bbox[1]];
            let fill = match (&s.fill, color) { (Paint::None, _) => None, (_, Some(c)) => Some(c), (Paint::Color(c), None) => Some(*c), (Paint::Grad(g), None) => Some(g.c1) };
            let (sw, scol) = match (&s.stroke, color) { (Paint::None, _) => (0.0, [0.0; 3]), (_, Some(c)) => ((s.stroke_width * k).max(1.0), c), (Paint::Color(c), None) => ((s.stroke_width * k).max(1.0), *c), (Paint::Grad(g), None) => ((s.stroke_width * k).max(1.0), g.c1) };
            let mut c = path_clip(ss, &local, s.evenodd, size, fill, sw, scol, 0.0, dur);
            if color.is_none() { if let Paint::Grad(g) = &s.fill { c.effects.push(effect("shape_gradient_fill", &[("r1", g.c1[0]), ("g1", g.c1[1]), ("b1", g.c1[2]), ("r2", g.c2[0]), ("g2", g.c2[1]), ("b2", g.c2[2]), ("angle", g.angle)])); }
                if let Paint::Grad(g) = &s.stroke { c.effects.push(effect("shape_gradient_stroke", &[("r1", g.c1[0]), ("g1", g.c1[1]), ("b1", g.c1[2]), ("r2", g.c2[0]), ("g2", g.c2[1]), ("b2", g.c2[2]), ("angle", g.angle)])); } }
            set_prop(&mut c, "x", sc[0] - lc[0]); set_prop(&mut c, "y", sc[1] - lc[1]);
            if s.opacity < 1.0 { set_prop(&mut c, "opacity", s.opacity); } if s.fill_opacity < 1.0 { set_prop(&mut c, "fill_alpha", s.fill_opacity); } if s.stroke_opacity < 1.0 && sw > 0.0 { set_prop(&mut c, "stroke_alpha", s.stroke_opacity); }
            c.name = Some(name.clone()); c.tags = vec!["svg_shape".into()]; clips.push(c);
        }
        let n = clips.len(); let inner = new_track(ss, &gname, clips);
        let mut g = precomp(ss, vec![inner], start, dur, &gname); set_prop(&mut g, "x", lc[0]); set_prop(&mut g, "y", lc[1]); g.tags = vec!["svg_layer".into()];
        let gid = g.id.clone(); let t = new_track(ss, &gname, vec![g]); tids.push(t.id.clone());
        out.push(json!({"name": gname, "clip_id": gid, "track_id": t.id, "shapes": n, "center": {"x": lc[0], "y": lc[1]}, "size": {"w": b[2] - b[0], "h": b[3] - b[1]}, "layers_inside": ls.len()}));
        new_tracks.push(t);
    }
    if out.is_empty() { return Err("nothing to import (every chosen layer is empty)".into()); }
    ss.project.tracks.extend(new_tracks);
    Ok(Placed { layers: out, track_ids: tids, scale: k })
}
fn names(doc: &Doc) -> String { let v: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect(); if v.len() > 40 { format!("{} ... ({} total)", v[..40].join(", "), v.len()) } else { v.join(", ") } }

/// Deep-find a clip by id, also inside precomps.
pub fn find_deep<'a>(tracks: &'a mut [Track], id: &str) -> Option<&'a mut Clip> {
    for t in tracks.iter_mut() { for c in t.clips.iter_mut() {
        if c.id == id { return Some(std::sync::Arc::make_mut(c)); }
        if matches!(c.source, Source::Precomp { .. }) { if let Source::Precomp { tracks } = &mut std::sync::Arc::make_mut(c).source { if let Some(x) = find_deep(tracks, id) { return Some(x); } } } } }
    None
}
/// The path-shape clips of a layer (the clip itself if it is a shape), in draw order.
fn shapes_mut(c: &mut Clip) -> Vec<&mut Clip> {
    let mut v = vec![]; if matches!(c.source, Source::Shape { .. }) { v.push(c); return v; }
    if let Source::Precomp { tracks } = &mut c.source { for t in tracks.iter_mut() { for k in t.clips.iter_mut() { v.extend(shapes_mut(std::sync::Arc::make_mut(k))); } } } v
}
fn layer_mut<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut Clip, String> { let id = str_arg(a, "clip_id")?.to_string(); find_deep(&mut ss.project.tracks, &id).ok_or(format!("no such clip '{id}'")) }

fn draw_on(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t0 = num_opt(a, "start")?.unwrap_or(0.0); let d = num_opt(a, "duration")?.unwrap_or(1.2); if t0 < 0.0 || d <= 0.0 { return Err("start must be >= 0 and duration > 0".into()); }
    let stagger = num_opt(a, "stagger")?.unwrap_or(0.6); if !(0.0..=1.0).contains(&stagger) { return Err("stagger must be 0..1 (0 = all shapes at once, 1 = strictly one after another)".into()); }
    let fill_in = num_opt(a, "fill_in")?.unwrap_or(0.35); if fill_in < 0.0 { return Err("fill_in must be >= 0 seconds".into()); }
    let outline = num_opt(a, "outline_width")?.unwrap_or(3.0); if outline <= 0.0 { return Err("outline_width must be > 0".into()); }
    let erase = bool_opt(a, "erase")?.unwrap_or(false);
    let ez: Ease = serde_json::from_value(a.get("ease").cloned().unwrap_or(json!("in_out"))).map_err(|e| format!("ease: {e}"))?;
    let c = layer_mut(ss, a)?; let mut shapes = shapes_mut(c); let n = shapes.len(); if n == 0 { return Err("clip has no shapes to draw (expects an SVG layer, a chart, or a shape clip)".into()); }
    // shape i draws over [t0 + i*step, t0 + i*step + seg]: stagger 0 -> all together, 1 -> back to back
    let seg = d / (1.0 + stagger * (n as f64 - 1.0)); let step = seg * stagger;
    let mut out = vec![];
    for (i, s) in shapes.iter_mut().enumerate() {
        let (a0, a1) = (t0 + i as f64 * step, t0 + i as f64 * step + seg);
        let (had_stroke, fill_col) = match &mut s.source { Source::Shape { stroke_width, stroke_color, fill, .. } => { let h = *stroke_width > 0.0; if !h { *stroke_width = outline; *stroke_color = if fill[0] >= 0.0 { *fill } else { [1.0; 3] }; } (h, fill[0] >= 0.0) } _ => continue };
        s.effects.retain(|e| e.kind != "shape_trim"); let mut tr = effect("shape_trim", &[("start", 0.0), ("end", 1.0), ("offset", 0.0)]);
        { let e = tr.params.get_mut("end").unwrap(); if erase { e.set_key(a0, 1.0, ez); e.set_key(a1, 0.0, Ease::Hold); } else { e.set_key(a0, 0.0, ez); e.set_key(a1, 1.0, Ease::Hold); } }
        s.effects.push(tr);
        if fill_col && !erase { let fa = s.props.get("fill_alpha").map_or(1.0, |p| p.base); s.props.remove("fill_alpha"); key(s, "fill_alpha", a1 - fill_in.min(seg) * 0.3, 0.0, Ease::Out); key(s, "fill_alpha", a1 + fill_in * 0.7, fa, Ease::Hold); }
        if fill_col && erase { let fa = s.props.get("fill_alpha").map_or(1.0, |p| p.base); s.props.remove("fill_alpha"); key(s, "fill_alpha", a0, fa, Ease::In); key(s, "fill_alpha", a0 + fill_in.min(seg), 0.0, Ease::Hold); }
        // an outline added for the reveal stays, in the fill colour: fading it would leave a see-through ring, since the fill stops at the stroke's inner edge
        let _ = had_stroke;
        out.push(json!({"clip_id": s.id, "from": a0, "to": a1}));
    }
    Ok(json!({"shapes": out.len(), "timing": out, "note": "times are clip-local seconds of the layer"}))
}

fn recolor(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let fill = rgb_opt(a, "fill")?; let stroke = rgb_opt(a, "stroke")?; let from = rgb_opt(a, "from")?; let to = rgb_opt(a, "to")?; let all = rgb_opt(a, "color")?;
    let tol = num_opt(a, "tolerance")?.unwrap_or(0.08); if from.is_some() != to.is_some() { return Err("give both 'from' and 'to' to swap one colour".into()); }
    if fill.is_none() && stroke.is_none() && from.is_none() && all.is_none() { return Err("give color (fill+stroke), fill, stroke, or from+to".into()); }
    let only: Option<Vec<usize>> = match a.get("shapes") { None | Some(Value::Null) => None, Some(Value::Array(v)) => Some(v.iter().map(|x| x.as_u64().map(|u| u as usize).ok_or("shapes must be a list of 0-based indices")).collect::<Result<_, _>>()?), _ => return Err("shapes must be a list of 0-based indices".into()) };
    let c = layer_mut(ss, a)?; let mut shapes = shapes_mut(c); let mut changed = 0;
    let close = |x: &[f64; 3], y: &[f64; 3]| (0..3).all(|i| (x[i] - y[i]).abs() <= tol);
    for (i, s) in shapes.iter_mut().enumerate() { if only.as_ref().map_or(false, |o| !o.contains(&i)) { continue; }
        let mut hit = false;
        if let Source::Shape { fill: f, stroke_color, stroke_width, .. } = &mut s.source {
            let nf = fill.or(all); let ns = stroke.or(all);
            if let (Some(n), true) = (nf, f[0] >= 0.0) { *f = n; hit = true; } if let (Some(n), true) = (ns, *stroke_width > 0.0) { *stroke_color = n; hit = true; }
            if let (Some(fr), Some(t)) = (from, to) { if f[0] >= 0.0 && close(f, &fr) { *f = t; hit = true; } if *stroke_width > 0.0 && close(stroke_color, &fr) { *stroke_color = t; hit = true; } }
        }
        // a solid recolour replaces gradient paints too
        if fill.or(all).is_some() { s.effects.retain(|e| e.kind != "shape_gradient_fill"); } if stroke.or(all).is_some() { s.effects.retain(|e| e.kind != "shape_gradient_stroke"); }
        if hit { changed += 1; } }
    Ok(json!({"shapes_changed": changed, "shapes_total": shapes.len()}))
}

fn inspect(doc: &Doc) -> Value {
    json!({"view_box": doc.view, "layers": doc.layers.iter().map(|l| json!({"name": l.name, "shapes": l.shapes.len(), "kinds": l.shapes.iter().map(|s| s.tag.as_str()).collect::<std::collections::BTreeSet<_>>()})).collect::<Vec<_>>(), "skipped": doc.skipped})
}

pub fn placement_schema() -> Vec<(&'static str, Value)> { vec![
    ("start", n("timeline seconds, default 0")), ("duration", n("seconds, default to the end of the project (min 5)")),
    ("x", n("px offset of the drawing's centre from the canvas centre")), ("y", n("px offset (down is +)")),
    ("width", n("px width of the drawing (keeps aspect); default fits 70% of the frame")), ("height", n("px height instead of width")),
    ("color", json!({"description":"recolour every fill and stroke: [r,g,b] 0..1 or a CSS colour like \"#ffcc00\""})),
    ("only", json!({"type":"array","items":{"type":"string"},"description":"import only these layers (names from inspect_svg); the view is framed on them"})),
    ("split", json!({"type":"array","items":{"type":"string"},"description":"layers that get their own movable clip; the others share one 'rest' clip. Default: every layer is its own clip when there are at most 24"})),
    ("name", s("name of the clip when everything is one layer"))] }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "import_svg", description: "Import an SVG illustration as animatable vector layers. Each top-level group (<g>, Inkscape layer, Illustrator group; a single wrapper group is unwrapped) becomes one group clip on its own new track, named after the group id, so set_property / set_keyframe move, scale, rotate and fade it; inside, every shape is a Bezier path shape with the SVG's fill, stroke, opacity and gradient. Then recolor_svg_layer and draw_on_svg_layer (stroke reveal). Paths (all commands incl. arcs), rect, circle, ellipse, line, polyline, polygon, use, transforms, CSS classes and gradients are imported; text and images are reported in 'skipped'. Give svg (inline text) or path (.svg file).", mutates: true,
        schema: { let mut v = vec![("svg", s("inline SVG document text")), ("path", s("path to an .svg file"))]; v.extend(placement_schema()); obj(v, &[]) },
        run: |ss, a| { let doc = crate::svgimport::parse(&svg_text(a)?)?; let p = place(ss, &doc, a)?; Ok(json!({"layers": p.layers, "track_ids": p.track_ids, "scale": p.scale, "skipped": doc.skipped})) } },
    Command { name: "inspect_svg", description: "List an SVG's layers with shape counts, its viewBox and what import would skip, without changing the project. Layers are the top-level groups import_svg would create.", mutates: false,
        schema: obj(vec![("svg", s("inline SVG text")), ("path", s("path to an .svg file"))], &[]),
        run: |_, a| Ok(inspect(&crate::svgimport::parse(&svg_text(a)?)?)) },
    Command { name: "recolor_svg_layer", description: "Recolour the shapes of an imported SVG layer, chart or asset. Takes a group clip from import_svg / add_asset / add_chart, or one shape clip. color sets fill and stroke, or fill / stroke separately, or swap one colour with from + to (within tolerance, default 0.08 per channel). Colours are [r,g,b] 0..1 or CSS (\"#e63946\"). shapes limits it to 0-based shape indices. Solid colours replace gradients.", mutates: true,
        schema: obj(vec![("clip_id", s("layer / group clip or shape clip")), ("color", json!({"description":"fill and stroke"})), ("fill", json!({"description":"fill colour"})), ("stroke", json!({"description":"stroke colour"})), ("from", json!({"description":"colour to replace"})), ("to", json!({"description":"replacement colour"})), ("tolerance", n("per-channel match tolerance 0..1, default 0.08")), ("shapes", json!({"type":"array","items":{"type":"integer"}}))], &["clip_id"]),
        run: recolor },
    Command { name: "draw_on_svg_layer", description: "Stroke reveal: the shapes of an SVG layer, asset or chart draw themselves on along their outline, then their fill fades in. Works on one shape clip too, via Trim Paths, one shape after another. Shapes without a stroke get an outline in their fill colour to draw (outline_width px; it stays, so the shape grows by half of it). start/duration in the layer's clip-local seconds; stagger 0 = all at once .. 1 = strictly one after another (default 0.6); erase reverses it (draw-off).", mutates: true,
        schema: obj(vec![("clip_id", s("layer / group clip or shape clip")), ("start", n("clip-local seconds, default 0")), ("duration", n("seconds for the whole reveal, default 1.2")), ("stagger", n("0..1, default 0.6")), ("fill_in", n("seconds for the fill to fade in after a shape's outline completes, default 0.35")), ("outline_width", n("px outline for fill-only shapes, default 3")), ("ease", json!({"type":"string","enum":crate::keyframe::EASE_NAMES})), ("erase", json!({"type":"boolean"}))], &["clip_id"]),
        run: draw_on },
] }

#[cfg(test)]
mod tests {
    use super::*;
    fn ss() -> Session { let mut s = Session::new(); s.project.width = 1280; s.project.height = 720; s }
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><g id="sun"><circle cx="50" cy="50" r="30" fill="#ffcc00"/></g><g id="hill" stroke="#225522" stroke-width="4" fill="none"><path d="M100 90 Q150 20 200 90"/></g></svg>"##;
    fn run(ss: &mut Session, n: &str, a: Value) -> Result<Value, String> { crate::commands::dispatch(ss, n, &a) }

    #[test]
    fn import_makes_one_group_clip_per_layer_centred_and_scaled() {
        let mut s = ss(); let r = run(&mut s, "import_svg", json!({"svg": SVG, "width": 400.0, "x": 100.0})).unwrap();
        let l = r["layers"].as_array().unwrap(); assert_eq!(l.len(), 2); assert_eq!(l[0]["name"], "sun"); assert_eq!(l[1]["name"], "hill");
        assert_eq!(r["scale"].as_f64().unwrap(), 2.0);
        // sun centre (50,50) in a 200x100 view -> canvas (-50*2+100, 0)
        let c = s.project.find_clip(l[0]["clip_id"].as_str().unwrap()).unwrap();
        assert!((c.props["x"].base - 0.0).abs() < 1.0, "{}", c.props["x"].base); assert!(c.props["y"].base.abs() < 1.0); // xform-lint: ok (test reads stored keyframes, not a render)
        let Source::Precomp { tracks } = &c.source else { panic!() }; let sh = &tracks[0].clips[0];
        match &sh.source { Source::Shape { shape, fill, stroke_width, width, .. } => { assert_eq!(shape, "path"); assert_eq!(*fill, [1.0, 0.8, 0.0]); assert_eq!(*stroke_width, 0.0); assert!((width - 120.0).abs() < 1.0); } _ => panic!() }
        let h = s.project.find_clip(l[1]["clip_id"].as_str().unwrap()).unwrap(); let Source::Precomp { tracks } = &h.source else { panic!() };
        match &tracks[0].clips[0].source { Source::Shape { fill, stroke_width, .. } => { assert!(fill[0] < 0.0, "no fill"); assert_eq!(*stroke_width, 8.0, "stroke scales with the drawing"); } _ => panic!() }
        assert_eq!(s.project.tracks.len(), 2, "one new track per layer");
    }
    #[test]
    fn only_split_and_errors() {
        let mut s = ss(); let r = run(&mut s, "import_svg", json!({"svg": SVG, "only": ["hill"]})).unwrap(); assert_eq!(r["layers"].as_array().unwrap().len(), 1);
        let mut s = ss(); let r = run(&mut s, "import_svg", json!({"svg": SVG, "split": ["sun"]})).unwrap(); let l = r["layers"].as_array().unwrap(); assert_eq!(l.len(), 2); assert_eq!(l[0]["name"], "rest"); assert_eq!(l[1]["name"], "sun");
        let mut s = ss(); assert!(run(&mut s, "import_svg", json!({"svg": SVG, "only": ["moon"]})).unwrap_err().contains("no layer 'moon'"));
        assert!(run(&mut s, "import_svg", json!({})).is_err()); assert!(s.project.tracks.is_empty(), "failed import changes nothing");
        let i = run(&mut s, "inspect_svg", json!({"svg": SVG})).unwrap(); assert_eq!(i["layers"][0]["name"], "sun");
    }
    #[test]
    fn draw_on_keys_trim_and_fill() {
        let mut s = ss(); let r = run(&mut s, "import_svg", json!({"svg": SVG})).unwrap(); let id = r["layers"][0]["clip_id"].as_str().unwrap().to_string();
        let d = run(&mut s, "draw_on_svg_layer", json!({"clip_id": id, "duration": 1.0})).unwrap(); assert_eq!(d["shapes"], 1);
        let c = s.project.find_clip(&id).unwrap(); let Source::Precomp { tracks } = &c.source else { panic!() }; let sh = &tracks[0].clips[0];
        let tr = sh.effects.iter().find(|e| e.kind == "shape_trim").unwrap(); assert_eq!(tr.params["end"].at(0.0), 0.0); assert_eq!(tr.params["end"].at(1.0), 1.0); assert!(tr.params["end"].at(0.5) > 0.2 && tr.params["end"].at(0.5) < 0.8);
        assert_eq!(sh.props["fill_alpha"].at(0.5), 0.0); assert!((sh.props["fill_alpha"].at(2.0) - 1.0).abs() < 1e-9);
        match &sh.source { Source::Shape { stroke_width, stroke_color, fill, .. } => { assert_eq!(*stroke_width, 3.0, "outline to draw"); assert_eq!(stroke_color, fill, "in the fill colour"); } _ => panic!() }
    }
    #[test]
    fn recolor_swaps_and_sets() {
        let mut s = ss(); let r = run(&mut s, "import_svg", json!({"svg": SVG})).unwrap(); let hill = r["layers"][1]["clip_id"].as_str().unwrap().to_string(); let sun = r["layers"][0]["clip_id"].as_str().unwrap().to_string();
        let x = run(&mut s, "recolor_svg_layer", json!({"clip_id": hill, "from": "#225522", "to": "#ffffff"})).unwrap(); assert_eq!(x["shapes_changed"], 1);
        run(&mut s, "recolor_svg_layer", json!({"clip_id": sun, "fill": [0.0, 0.0, 1.0]})).unwrap();
        let col = |s: &Session, id: &str| { let c = s.project.find_clip(id).unwrap(); let Source::Precomp { tracks } = &c.source else { panic!() }; match &tracks[0].clips[0].source { Source::Shape { fill, stroke_color, .. } => (*fill, *stroke_color), _ => panic!() } };
        assert_eq!(col(&s, &hill).1, [1.0; 3]); assert_eq!(col(&s, &sun).0, [0.0, 0.0, 1.0]);
        assert!(run(&mut s, "recolor_svg_layer", json!({"clip_id": sun})).is_err());
    }
    #[test]
    fn imported_layer_renders_pixels_where_the_svg_draws() {
        let mut s = ss(); s.project.width = 320; s.project.height = 180; let r = run(&mut s, "import_svg", json!({"svg": SVG, "width": 200.0})).unwrap(); let _ = r;
        let f = crate::render::render_frame(&s.project, 0.5, &crate::render::Cache::new()).unwrap();
        let px = |x: usize, y: usize| { let i = (y * 320 + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] };
        // sun centre: view (50,50) -> canvas centre + ((50-100)*1, 0) = (110, 90)
        let p = px(110, 90); assert!(p[0] > 0.9 && p[1] > 0.7 && p[2] < 0.1, "{p:?}");
        assert!(px(5, 5)[0] < 0.05, "background stays empty");
    }
}
