//! Bundled, openly licensed asset library: search_assets, add_asset, export_asset_svg.
//!
//! The library is compiled into the engine (assets/library/library.json, built by tools/build_asset_library.py), so it
//! works offline on every OS. Contents and licences (also in THIRD_PARTY.md, licence texts in assets/library/licenses):
//!   icon/*       curated Lucide icons (ISC; the Feather-derived ones MIT), stroke style; draw_on_svg_layer draws them on
//!   map/world    world map, one layer per country (Natural Earth 1:110m, public domain, via world-atlas, ISC)
//!   character/*  two flat figures (ava, ravi) in seven poses, layers head/torso/arm_left/arm_right/legs (MIT, made for kimo)
//!   prop/*       flat props: desk, chair, plant, laptop, coffee mug, books, speech/thought bubbles, money bag, house (MIT, made for kimo)
use super::kit::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Asset { pub id: String, pub category: String, pub name: String, pub tags: Vec<String>, pub license: String, pub svg: String }
#[derive(Deserialize)]
struct Lib { assets: Vec<Asset> }

const LIBRARY: &str = include_str!("../../assets/library/library.json");
pub fn library() -> &'static [Asset] { static L: std::sync::OnceLock<Vec<Asset>> = std::sync::OnceLock::new(); L.get_or_init(|| serde_json::from_str::<Lib>(LIBRARY).map(|l| l.assets).unwrap_or_default()) }
pub fn get(id: &str) -> Result<&'static Asset, String> {
    library().iter().find(|a| a.id == id || a.id.split_once('/').map_or(false, |(_, n)| n == id)).ok_or_else(|| format!("no asset '{id}'; find ids with search_assets"))
}

/// Relevance of an asset to a query: every word must match the id, name or a tag (prefix match); exact hits rank higher.
fn score(a: &Asset, words: &[String]) -> Option<u32> {
    let mut s = 0; let id = a.id.to_lowercase(); let name = a.name.to_lowercase();
    for w in words { let mut best = 0;
        if name == *w || id.ends_with(&format!("/{w}")) { best = 10; } else if name.split(|c: char| !c.is_alphanumeric()).any(|p| p == w) { best = 8; } else if name.contains(w.as_str()) || id.contains(w.as_str()) { best = 5; }
        if best < 4 { if a.tags.iter().any(|t| t.to_lowercase() == *w) { best = 4; } else if a.tags.iter().any(|t| t.to_lowercase().starts_with(w.as_str())) { best = 2; } }
        if best == 0 { return None; } s += best; }
    Some(s)
}
/// Matching map layers (country names) for a query, so "india" finds the world map and names the layer to split out.
fn map_hits(a: &Asset, words: &[String]) -> Vec<String> { if a.category != "map" { return vec![]; } a.tags.iter().filter(|t| words.iter().any(|w| t.to_lowercase().contains(w.as_str()))).take(10).cloned().collect() }

fn search(a: &Value) -> Result<Value, String> {
    let q = a.get("query").and_then(|v| v.as_str()).unwrap_or("").to_lowercase(); let words: Vec<String> = q.split_whitespace().map(String::from).collect();
    let cat = a.get("category").and_then(|v| v.as_str()); if let Some(c) = cat { if !["icon", "character", "prop", "map"].contains(&c) { return Err(format!("category must be icon|character|prop|map, got '{c}'")); } }
    let limit = opt_num(a, "limit", 20.0).clamp(1.0, 200.0) as usize;
    let mut hits: Vec<(u32, &Asset)> = library().iter().filter(|x| cat.map_or(true, |c| x.category == c)).filter_map(|x| if words.is_empty() { Some((0, x)) } else { score(x, &words).map(|s| (s, x)) }).collect();
    hits.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.id.cmp(&y.1.id))); let total = hits.len();
    let res: Vec<Value> = hits.into_iter().take(limit).map(|(_, x)| { let layers: Vec<String> = crate::svgimport::parse(&x.svg).map(|d| d.layers.iter().map(|l| l.name.clone()).collect()).unwrap_or_default();
        let mut o = json!({"id": x.id, "category": x.category, "name": x.name, "license": x.license});
        if x.category == "map" { o["layers"] = json!(layers.len()); o["matching_layers"] = json!(map_hits(x, &words)); o["tip"] = json!("split: [country names] gives those countries their own clips; the rest share one"); }
        else { o["layers"] = json!(layers); o["tags"] = json!(x.tags.iter().take(12).collect::<Vec<_>>()); }
        o }).collect();
    Ok(json!({"results": res, "total": total, "library_size": library().len(), "categories": {"icon": count("icon"), "character": count("character"), "prop": count("prop"), "map": count("map")}}))
}
fn count(c: &str) -> usize { library().iter().filter(|a| a.category == c).count() }

fn add(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let asset = get(str_arg(a, "asset_id")?)?; let doc = crate::svgimport::parse(&asset.svg)?;
    let mut args = a.clone(); if args.get("name").is_none() { args["name"] = json!(asset.name); }
    // icons are line art: default to a white stroke on a dark video unless a colour is given; maps default to one clip
    if asset.category == "icon" && args.get("color").is_none() { args["color"] = json!([1.0, 1.0, 1.0]); }
    if asset.category == "icon" && args.get("width").is_none() && args.get("height").is_none() { args["height"] = json!(ss.project.height as f64 * 0.3); }
    if matches!(asset.category.as_str(), "map" | "icon") && args.get("split").is_none() { args["split"] = json!([]); }
    let p = super::svg_tools::place(ss, &doc, &args)?;
    let mut out = json!({"asset_id": asset.id, "license": asset.license, "layers": p.layers, "track_ids": p.track_ids, "scale": p.scale});
    if a.get("draw_on").and_then(|v| v.as_bool()).unwrap_or(false) { let d = super::svg_tools::num_opt(a, "draw_duration")?.unwrap_or(1.2); for l in p.layers.iter() { crate::commands::dispatch_nested(ss, "draw_on_svg_layer", &json!({"clip_id": l["clip_id"], "start": 0.0, "duration": d}))?; } out["drawn_on"] = json!(true); }
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "search_assets", description: "Search kimo's bundled, openly licensed graphics: icons, characters, props and a world map. 176 icons (Lucide, ISC), flat characters ava and ravi in poses standing/waving/pointing/thinking/presenting/walking/celebrating (MIT), props (desk, chair, plant, laptop, coffee mug, books, speech and thought bubbles, money bag, house; MIT) and a world map with one layer per country (Natural Earth, public domain). query matches names and tags (all words must match); category icon|character|prop|map. Returns ids, layer names and licence. Place one with add_asset.", mutates: false,
        schema: obj(vec![("query", s("words, e.g. 'rocket', 'person pointing', 'india'")), ("category", json!({"type":"string","enum":["icon","character","prop","map"]})), ("limit", n("max results, default 20"))], &[]),
        run: |_, a| search(a) },
    Command { name: "add_asset", description: "Place a bundled library asset (id from search_assets, e.g. icon/rocket, character/ava_pointing, prop/laptop, map/world) as animatable SVG layers, exactly like import_svg: each layer is a group clip on its own track (characters: legs, torso, arm_left, arm_right, head; map: split: [\"India\"] gives countries their own clips). Icons are one clip (white line art, 30% of frame height, by default). draw_on: true adds a stroke reveal. Returns the clip ids and the licence.", mutates: true,
        schema: { let mut v = vec![("asset_id", s("library id, e.g. icon/rocket")), ("draw_on", json!({"type":"boolean"})), ("draw_duration", n("seconds, default 1.2"))]; v.extend(super::svg_tools::placement_schema()); obj(v, &["asset_id"]) },
        run: add },
    Command { name: "export_asset_svg", description: "Write a library asset's SVG to a file (for a code clip, another tool, or editing), with its licence in an XML comment.", mutates: false,
        schema: obj(vec![("asset_id", s("library id")), ("path", s("output .svg path"))], &["asset_id", "path"]),
        run: |_, a| { let x = get(str_arg(a, "asset_id")?)?; let p = str_arg(a, "path")?; let body = format!("<!-- kimo library asset {} | licence: {} -->\n{}\n", x.id, x.license, x.svg); std::fs::write(p, &body).map_err(|e| format!("cannot write '{p}': {e}"))?; Ok(json!({"path": p, "bytes": body.len(), "license": x.license})) } },
] }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_loads_and_every_asset_parses() {
        let l = library(); assert!(l.len() > 150, "{}", l.len());
        for c in ["icon", "character", "prop", "map"] { assert!(count(c) > 0, "{c}"); }
        for a in l { let d = crate::svgimport::parse(&a.svg).unwrap_or_else(|e| panic!("{}: {e}", a.id)); assert!(!a.license.is_empty(), "{}", a.id); assert!(d.skipped.is_empty(), "{}: {:?}", a.id, d.skipped); }
        let ch = crate::svgimport::parse(&get("character/ava_waving").unwrap().svg).unwrap(); assert_eq!(ch.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["legs", "torso", "arm_left", "arm_right", "head"]);
        assert!(crate::svgimport::parse(&get("map/world").unwrap().svg).unwrap().layers.iter().any(|l| l.name == "India"));
    }
    #[test]
    fn search_ranks_and_filters() {
        let r = search(&json!({"query": "rocket"})).unwrap(); assert_eq!(r["results"][0]["id"], "icon/rocket");
        let r = search(&json!({"query": "pointing", "category": "character"})).unwrap(); assert!(r["results"].as_array().unwrap().iter().all(|x| x["id"].as_str().unwrap().ends_with("_pointing")));
        let r = search(&json!({"query": "india"})).unwrap(); assert_eq!(r["results"][0]["id"], "map/world"); assert_eq!(r["results"][0]["matching_layers"][0], "India");
        assert!(search(&json!({"category": "font"})).is_err()); assert_eq!(search(&json!({"query": "zzzzqq"})).unwrap()["total"], 0);
    }
    #[test]
    fn add_asset_places_layers() {
        let mut ss = Session::new(); ss.project.width = 640; ss.project.height = 360;
        let r = crate::commands::dispatch(&mut ss, "add_asset", &json!({"asset_id": "character/ravi_pointing", "height": 300.0})).unwrap(); assert_eq!(r["layers"].as_array().unwrap().len(), 5);
        let r = crate::commands::dispatch(&mut ss, "add_asset", &json!({"asset_id": "map/world", "split": ["India"]})).unwrap(); let l = r["layers"].as_array().unwrap(); assert_eq!(l.len(), 2); assert_eq!(l[1]["name"], "India");
        let r = crate::commands::dispatch(&mut ss, "add_asset", &json!({"asset_id": "rocket", "draw_on": true})).unwrap(); assert_eq!(r["drawn_on"], true);
        assert!(crate::commands::dispatch(&mut ss, "add_asset", &json!({"asset_id": "nope"})).is_err());
    }
}
