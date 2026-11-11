//! Renders example frames for kimo's built-in asset creation (SVG layers, charts, library assets, Indian numbers,
//! Devanagari) so they can be checked by eye:
//!     cargo run --release --example asset_creation_demo -- <out_dir>
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn call(ss: &mut Session, n: &str, a: Value) -> Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> Session {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 1280, "height": 720, "fps": 30}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].clone();
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": [0.07, 0.08, 0.11], "color2": [0.03, 0.035, 0.05], "start": 0.0, "duration": 12.0})); ss
}
fn shot(ss: &mut Session, dir: &str, name: &str, t: f64) { if let Some(f) = std::env::args().nth(2) { if !name.contains(&f) { return; } } let p = format!("{dir}/{name}.png"); call(ss, "render_frame", json!({"time": t, "path": p, "inline": "never"})); println!("{p}"); }
fn txt(ss: &mut Session, text: &str, font: &str, size: f64, y: f64) { let t = call(ss, "add_track", json!({}))["track_id"].clone(); let c = call(ss, "add_text", json!({"track_id": t, "text": text, "start": 0.0, "duration": 12.0, "font": font, "size": size, "color": [0.95, 0.95, 0.95]}))["clip_id"].clone(); call(ss, "set_property", json!({"clip_id": c, "property": "y", "value": y})); }

const LANDSCAPE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 450">
  <defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1d3b6e"/><stop offset="1" stop-color="#f29e6d"/></linearGradient></defs>
  <style>.leaf{fill:#2f8a41;stroke:#1d5a29;stroke-width:3}</style>
  <g id="sky"><rect width="800" height="450" rx="24" fill="url(#sky)"/></g>
  <g id="sun"><circle cx="600" cy="150" r="70" fill="#ffd166"/></g>
  <g id="mountains"><path d="M0 380 L180 170 L300 300 L430 120 L620 380 Z" fill="#3d4a6b"/><path d="M380 380 L560 210 L800 380 Z" fill="#2b3550"/></g>
  <g id="ground"><path d="M0 360 Q200 330 400 360 T800 350 L800 450 L0 450 Z" fill="#1f6b3a"/></g>
  <g id="tree" transform="translate(150 250)"><rect x="-8" y="40" width="16" height="70" fill="#6b4329"/><circle class="leaf" cx="0" cy="20" r="45"/><circle class="leaf" cx="-30" cy="45" r="30"/><circle class="leaf" cx="30" cy="45" r="30"/></g>
  <g id="birds" fill="none" stroke="#14213d" stroke-width="4" stroke-linecap="round"><path d="M300 90 q15 -15 30 0 q15 -15 30 0"/><path d="M380 60 q10 -10 20 0 q10 -10 20 0"/></g>
</svg>"##;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into()); std::fs::create_dir_all(&dir).unwrap();
    // 1. SVG import, drawn on layer by layer
    let mut ss = scene(); let r = call(&mut ss, "import_svg", json!({"svg": LANDSCAPE, "width": 1000.0}));
    for (i, l) in r["layers"].as_array().unwrap().iter().enumerate() { call(&mut ss, "draw_on_svg_layer", json!({"clip_id": l["clip_id"], "start": i as f64 * 0.35, "duration": 1.0})); }
    let tree = r["layers"].as_array().unwrap().iter().find(|l| l["name"] == "tree").unwrap()["clip_id"].clone();
    call(&mut ss, "set_keyframe", json!({"clip_id": tree, "property": "x", "time": 2.4, "value": -330.0, "ease": "in_out"})); call(&mut ss, "set_keyframe", json!({"clip_id": tree, "property": "x", "time": 3.4, "value": 330.0}));
    shot(&mut ss, &dir, "01_svg_draw_on_mid", 1.1); shot(&mut ss, &dir, "02_svg_final_tree_moved", 3.6);
    let sun = r["layers"].as_array().unwrap().iter().find(|l| l["name"] == "sun").unwrap()["clip_id"].clone(); call(&mut ss, "recolor_svg_layer", json!({"clip_id": sun, "fill": "#ff5d5d"})); shot(&mut ss, &dir, "03_svg_sun_recoloured", 3.6);
    // 2. library: icons drawn on, characters, props
    let mut ss = scene(); let icons = ["rocket", "globe", "lightbulb", "chart-line", "piggy-bank", "graduation-cap", "heart-pulse", "laptop"];
    for (i, n) in icons.iter().enumerate() { call(&mut ss, "add_asset", json!({"asset_id": format!("icon/{n}"), "height": 120.0, "x": -525.0 + i as f64 * 150.0, "y": -230.0, "draw_on": true, "color": chart_col(i)})); }
    for (i, n) in ["ava_waving", "ravi_pointing", "ava_thinking", "ravi_celebrating", "ava_walking"].iter().enumerate() { call(&mut ss, "add_asset", json!({"asset_id": format!("character/{n}"), "height": 300.0, "x": -480.0 + i as f64 * 160.0, "y": 110.0})); }
    for (i, n) in ["plant", "laptop", "coffee_mug", "speech_bubble"].iter().enumerate() { call(&mut ss, "add_asset", json!({"asset_id": format!("prop/{n}"), "height": 110.0, "x": 380.0 + (i % 2) as f64 * 150.0, "y": 30.0 + (i / 2) as f64 * 160.0})); }
    shot(&mut ss, &dir, "04_library_icons_drawing", 0.6); shot(&mut ss, &dir, "05_library_final", 2.5);
    // 3. world map with India split out and highlighted
    let mut ss = scene(); let r = call(&mut ss, "add_asset", json!({"asset_id": "map/world", "width": 1180.0, "split": ["India"], "color": "#3a4256"}));
    let india = r["layers"][1]["clip_id"].clone(); call(&mut ss, "recolor_svg_layer", json!({"clip_id": india, "fill": "#ff9933", "stroke": "#ffffff"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": india, "property": "scale", "time": 0.0, "value": 1.0, "ease": "out_back"})); call(&mut ss, "set_keyframe", json!({"clip_id": india, "property": "scale", "time": 0.8, "value": 1.6}));
    shot(&mut ss, &dir, "06_world_map_india", 1.5);
    // 4. charts
    let charts = [
        ("07_line_mid", 1.6, json!({"type": "line", "title": "Monthly UPI payments (₹ crore)", "grouping": "indian", "csv": "month,Payments\nJan,1230000\nFeb,1180000\nMar,1400000\nApr,1390000\nMay,1490000\nJun,1460000\nJul,1610000\nAug,1580000\nSep,1720000\nOct,1820000"})),
        ("08_line_two_series_end", 4.0, json!({"type": "line", "title": "Revenue vs cost", "smooth": true, "prefix": "$", "grouping": "compact", "decimals": 1, "csv": "q,Revenue,Cost\nQ1,1200000,900000\nQ2,1500000,1000000\nQ3,1400000,1150000\nQ4,2100000,1300000\nQ5,2600000,1500000"})),
        ("09_bar_race_mid", 3.3, json!({"type": "bar_race", "title": "Most populous countries", "grouping": "compact", "decimals": 1, "top_n": 6, "duration": 8.0, "csv": "year,India,China,USA,Indonesia,Pakistan,Nigeria,Brazil\n1960,445954579,667070000,180671000,88296070,45954226,45138458,72179226\n1980,696828385,981235000,227225000,147490365,80624057,73423633,121159746\n2000,1056575549,1262645000,282162411,211513823,154369924,122283850,174790340\n2020,1396387127,1411100000,331501080,271857970,227196741,208327405,213196304\n2023,1428627663,1410710000,334914895,277534122,240485658,223804632,216422446"})),
        ("10_donut", 3.5, json!({"type": "donut", "title": "Where the money goes", "center_label": "per month", "prefix": "₹", "grouping": "indian", "csv": "label,value\nRent,45000\nFood,22000\nTravel,8000\nSavings,25000\nOther,10000"})),
        ("11_treemap", 3.5, json!({"type": "treemap", "title": "Union budget by ministry", "suffix": " cr", "grouping": "indian", "csv": "label,value\nDefence,621541\nRoad transport,278000\nRailways,255393\nConsumer affairs,213323\nHome affairs,202868\nRural development,177566\nChemicals,168500\nCommunications,137294\nAgriculture,127470\nEducation,120628"})),
        ("12_waffle", 3.5, json!({"type": "waffle", "title": "How people commute", "csv": "label,value\nWalk,38\nBus,24\nTwo-wheeler,21\nCar,9\nTrain,8"})),
    ];
    for (name, t, a) in charts { let mut ss = scene(); call(&mut ss, "add_chart", a); shot(&mut ss, &dir, name, t); }
    // 5. Indian numbers and Devanagari
    let mut ss = scene(); let tr = call(&mut ss, "add_track", json!({}))["track_id"].clone();
    call(&mut ss, "add_counter", json!({"track_id": tr, "start": 0.0, "duration": 2.0, "hold": 3.0, "from": 0, "to": 98765432.0, "prefix": "₹", "grouping": "indian", "size": 110.0, "y": -170.0}));
    txt(&mut ss, "{num:0:indian:out:0=0,2=10000000}  ·  {num:1:compact_indian:out:0=0,2=32000000}", "Inter", 48.0, -40.0);
    txt(&mut ss, "भारत की जनसंख्या", "Nirmala UI", 72.0, 70.0);
    txt(&mut ss, "{num:0:indian:out:0=0,2=1428627663:deva}", "Nirmala UI", 64.0, 170.0);
    txt(&mut ss, "क्षत्रिय  श्री  प्रधानमंत्री  हिंदी", "Nirmala UI", 54.0, 262.0);
    shot(&mut ss, &dir, "13_indian_numbers_devanagari", 3.0);
}
fn chart_col(i: usize) -> Value { let c = [[0.31, 0.55, 1.0], [1.0, 0.54, 0.24], [0.18, 0.77, 0.65], [0.95, 0.76, 0.31], [0.90, 0.34, 0.43], [0.61, 0.48, 1.0], [0.36, 0.78, 1.0], [0.64, 0.84, 0.36]][i % 8]; json!(c) }
