//! Read/import exact working RGBA floats; hash-addressed lossless native assets.
use super::kit::*;
use crate::{effects::Frame, pixels};
fn integer(a: &Value, k: &str) -> Result<usize, String> {
    let v = a
        .get(k)
        .and_then(Value::as_u64)
        .ok_or_else(|| crate::errors::arg_err(a, k, "nonnegative integer"))?;
    usize::try_from(v).map_err(|_| format!("{k} too large"))
}
fn load_import(a: &Value) -> Result<Frame, String> {
    let (w, h) = (integer(a, "w")?, integer(a, "h")?);
    let n = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(16))
        .ok_or("pixel allocation overflow")?;
    if w == 0 || h == 0 || w > u32::MAX as usize || h > u32::MAX as usize {
        return Err("w/h must be positive u32 dimensions".into());
    }
    crate::resource_limits::reserve(n)?;
    let format = str_arg(a, "format")?;
    let raw = match (a.get("data_base64"), a.get("path")) {
        (Some(v), None) => pixels::b64_decode(v.as_str().ok_or("data_base64 must be a string")?)?,
        (None, Some(v)) => {
            let path = v.as_str().ok_or("path must be a string")?;
            let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
            if m.len() > n as u64 + 16 {
                return Err("pixel input byte count exceeds requested dimensions".into());
            }
            std::fs::read(path).map_err(|e| e.to_string())?
        }
        _ => return Err("provide exactly one data_base64 or path".into()),
    };
    match format {
        "f32_rgba_le" => {
            if raw.len() != n {
                return Err(format!("expected {n} f32_rgba_le bytes, got {}", raw.len()));
            }
            let mut bytes = b"KIMOPX01".to_vec();
            bytes.extend((w as u32).to_le_bytes());
            bytes.extend((h as u32).to_le_bytes());
            bytes.extend(raw);
            pixels::decode(&bytes)
        }
        "kimopx" => {
            let f = pixels::decode(&raw)?;
            if f.w != w || f.h != h {
                return Err("pixel file dimensions differ from w/h".into());
            }
            Ok(f)
        }
        _ => Err("format must be f32_rgba_le|kimopx".into()),
    }
}
fn import(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let mut f = load_import(a)?;
    if let Some(r) = a.get("region") {
        let (x, y, w, h) = (
            integer(r, "x")?,
            integer(r, "y")?,
            integer(r, "w")?,
            integer(r, "h")?,
        );
        if w == 0
            || h == 0
            || x.checked_add(w).map_or(true, |n| n > f.w)
            || y.checked_add(h).map_or(true, |n| n > f.h)
        {
            return Err("import region must be positive and inside w/h".into());
        }
        crate::resource_limits::reserve(
            w.checked_mul(h)
                .and_then(|n| n.checked_mul(16))
                .ok_or("ROI overflow")?,
        )?;
        let mut d = Vec::with_capacity(w * h * 4);
        for yy in y..y + h {
            crate::guard::check();
            d.extend_from_slice(&f.d[(yy * f.w + x) * 4..(yy * f.w + x + w) * 4]);
        }
        f = Frame { w, h, d };
    }
    let bytes = pixels::encode(&f)?;
    let sha = crate::deps::sha256_hex(&bytes);
    let dir = crate::assets::store_dir(&ss.project,"pixels")?;
    let path = dir.join(format!("{sha}.kimopx"));
    if path.exists() {
        if std::fs::read(&path).map_err(|e| e.to_string())? != bytes {
            return Err("hash-addressed pixel asset changed unexpectedly".into());
        }
    } else {
        let tmp = dir.join(format!("{sha}-{}.tmp", std::process::id()));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    let id = format!("pixels_{sha}");
    let path = path.to_string_lossy().to_string();
    ss.project.assets.insert(
        id.clone(),
        crate::model::Asset {
 origin:None,hash:None,kind:None,
            id: id.clone(),
            path: path.clone(),
            width: f.w as u32,
            height: f.h as u32,
            duration: 0.,
            fps: 25.,
            bin: None,
        },
    );
    Ok(
        json!({"asset_id":id,"sha256":sha,"path":path,"w":f.w,"h":f.h,"format":"kimopx","encoding":"working_linear_rgba_f32","name":a.get("name").and_then(Value::as_str).unwrap_or("pixels")}),
    )
}
fn read(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = num_arg(a, "time")?;
    if !t.is_finite() {
        return Err("time must be finite".into());
    }
    let source = a.get("source").cloned().unwrap_or(json!("mix"));
    crate::resource_limits::reserve((ss.project.width as usize).checked_mul(ss.project.height as usize).and_then(|n|n.checked_mul(32)).ok_or("read_pixels frame allocation overflow")?)?;
    let f = if let Some(id) = source.get("asset_id").and_then(Value::as_str) {
        let asset = ss
            .project
            .assets
            .get(id)
            .ok_or_else(|| format!("no asset '{id}'"))?;
        if !asset.path.ends_with(".kimopx") {
            return Err("asset source requires lossless pixel asset".into());
        }
        pixels::read(&ss.project.resolve(&asset.path))?
    } else {
        let clip = source
            .get("clip_id")
            .and_then(Value::as_str)
            .or_else(|| source.as_str().filter(|s| *s != "mix"));
        if source != "mix" && clip.is_none() {
            return Err("source must be mix, clip id, {clip_id} or {asset_id}".into());
        }
        crate::colortools::any_frame(&ss.project, clip, t)?
    };
    let cap = a
        .get("max_pixels")
        .map(|_| integer(a, "max_pixels"))
        .transpose()?
        .unwrap_or(ss.resource_limits.max_pixels as usize);
    if cap == 0 {
        return Err("max_pixels must be positive".into());
    }
    let cap=cap.min(ss.resource_limits.max_pixels as usize);let mut coords = vec![];
    let (w, h) = if let Some(points) = a.get("points") {
        if a.get("region").is_some() {
            return Err("provide region or points, not both".into());
        }
        let points = points
            .as_array()
            .ok_or("points must be [[x,y],...] pixel coordinates")?;
        if points.len() > cap {
            return Err(format!("max_pixels cap {cap} exceeded; set_limits max_pixels"));
        }
        for p in points {
            let a = p
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or("point must have two integer coordinates")?;
            let (x, y) = (
                a[0].as_u64().ok_or("point x must be nonnegative integer")? as usize,
                a[1].as_u64().ok_or("point y must be nonnegative integer")? as usize,
            );
            if x >= f.w || y >= f.h {
                return Err("point outside source dimensions".into());
            }
            coords.push((x, y));
        }
        (coords.len(), 1)
    } else {
        let region = a
            .get("region")
            .cloned()
            .unwrap_or(json!({"x":0,"y":0,"w":f.w,"h":f.h}));
        let (x, y, w, h) = (
            integer(&region, "x")?,
            integer(&region, "y")?,
            integer(&region, "w")?,
            integer(&region, "h")?,
        );
        let n = w.checked_mul(h).ok_or("region size overflow")?;
        if w == 0
            || h == 0
            || x.checked_add(w).map_or(true, |v| v > f.w)
            || y.checked_add(h).map_or(true, |v| v > f.h)
        {
            return Err("region must be positive and inside source".into());
        }
        if n > cap {
            return Err(format!("max_pixels cap {cap} exceeded: requested {n}; set_limits max_pixels"));
        }
        crate::resource_limits::reserve(n.checked_mul(32).ok_or("pixel allocation overflow")?)?;
        for yy in y..y + h {
            for xx in x..x + w {
                coords.push((xx, yy));
            }
        }
        (w, h)
    };
    let mut d = Vec::with_capacity(coords.len() * 4);
    for (x, y) in &coords {
        crate::guard::check();
        let i = (y * f.w + x) * 4;
        d.extend_from_slice(&f.d[i..i + 4]);
    }
    let format = a
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("f32_json");
    let mut out = json!({"w":w,"h":h,"source_w":f.w,"source_h":f.h,"count":coords.len(),"time":t,"source":source,"encoding":"working_linear_rgba_f32","output_transform_applied":false,"format":format});
    match format {
        "f32_json" => out["data"] = json!(d),
        "file" => {
            if w == 0 {
                return Err("file needs at least one pixel".into());
            }
            let bytes = pixels::encode(&Frame { w, h, d })?;
            let sha = crate::deps::sha256_hex(&bytes);
            let path = std::env::temp_dir().join(format!("kimo-read-{sha}.kimopx"));
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            out["path"] = json!(path);
            out["sha256"] = json!(sha);
            out["file_format"] = json!("kimopx");
        }
        _ => return Err("format must be f32_json|file".into()),
    }
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"read_pixels",mutates:false,description:"Read exact working-buffer RGBA f32 pixels before display/output transform, no quantization. source mix (default), clip id/{clip_id}, or imported {asset_id}; time seconds. region {x,y,w,h} or points [[x,y],...] in integer source pixels. max_pixels hard result cap (default session max_pixels), not a creative clamp. format f32_json returns flat interleaved RGBA; file writes lossless KIMOPX01 (header + little endian f32) for import_pixels. Signed HDR samples preserved.",schema:obj(vec![("time",n("timeline seconds")),("source",json!({"description":"mix, clip id, {clip_id} or {asset_id}"})),("region",json!({"type":"object","description":"integer x,y,w,h pixel rectangle"})),("points",json!({"type":"array","items":{"type":"array","items":{"type":"integer"}}})),("format",json!({"type":"string","enum":["f32_json","file"]})),("max_pixels",n("result cap default session max_pixels"))],&["time"]),run:read},Command{name:"import_pixels",mutates:true,description:"Import lossless working-linear RGBA f32 pixels as a SHA256-addressed native .kimopx asset usable by add_clip and clip mattes. w,h positive dimensions; format f32_rgba_le (raw interleaved little-endian) or kimopx; exactly one data_base64 or path. Signed HDR RGB and finite alpha preserved without clipping. name labels the result. Does not change the project color pipeline.",schema:obj(vec![("w",n("width")),("h",n("height")),("format",json!({"type":"string","enum":["f32_rgba_le","kimopx"]})),("data_base64",s("base64 encoded bytes")),("path",s("caller input file")),("name",s("asset label")),("region",json!({"type":"object","description":"optional input crop x,y,w,h; data dimensions are original w/h"}))],&["w","h","format"]),run:import}]
}
