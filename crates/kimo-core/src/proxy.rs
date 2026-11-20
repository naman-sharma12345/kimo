//! Optional, disposable preview media. Asset paths always refer to the originals.
use crate::model::Project;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proxies {
    pub enabled: bool,
    pub entries: BTreeMap<String, Proxy>,
}
impl Proxies {
    pub fn is_empty(&self) -> bool {
        !self.enabled && self.entries.is_empty()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub bytes: u64,
    pub modified_ns: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    pub stamp: Stamp,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub path: String,
    pub source: Fingerprint,
    pub proxy: Fingerprint,
    pub source_width: u32,
    pub source_height: u32,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frames: usize,
}
pub fn stamp(path: &Path) -> Result<Stamp, String> {
    let m = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let t = m
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(Stamp {
        bytes: m.len(),
        modified_ns: t.as_nanos().to_string(),
    })
}
/// Deliberately uncached: explicit deep validation also detects edits preserving size and mtime.
pub fn fingerprint(path: &Path) -> Result<Fingerprint, String> {
    use std::io::Read;
    let before = stamp(path)?;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = crate::deps::Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        crate::guard::check();
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if stamp(path)? != before {
        return Err("media changed while hashing; retry".into());
    }
    Ok(Fingerprint {
        stamp: before,
        sha256: hash.finish_hex(),
    })
}
pub fn status(p: &Project, id: &str, deep: bool) -> Value {
    let Some(a) = p.assets.get(id) else {
        return json!({"asset_id":id,"status":"asset_missing"});
    };
    let Some(q) = p.proxies.entries.get(id) else {
        return json!({"asset_id":id,"status":"unregistered"});
    };
    let check = |path: &Path, f: &Fingerprint, missing: &str, changed: &str| -> Option<String> {
        if deep {
            match fingerprint(path) {
                Ok(now) if now.sha256 == f.sha256 => None,
                Ok(_) => Some(changed.into()),
                Err(_) if !path.is_file() => Some(missing.into()),
                Err(e) => Some(format!("validation_failed: {e}")),
            }
        } else {
            match stamp(path) {
                Ok(s) if s == f.stamp => None,
                Ok(_) => Some(changed.into()),
                Err(_) => Some(missing.into()),
            }
        }
    };
    let state = check(
        Path::new(&p.resolve(&a.path)),
        &q.source,
        "source_missing",
        "source_changed",
    )
    .or_else(|| {
        check(
            Path::new(&p.resolve(&q.path)),
            &q.proxy,
            "proxy_missing",
            "proxy_changed",
        )
    })
    .unwrap_or_else(|| "ready".into());
    let fingerprints_current = stamp(Path::new(&p.resolve(&a.path))).ok().as_ref()
        == Some(&q.source.stamp)
        && stamp(Path::new(&p.resolve(&q.path))).ok().as_ref() == Some(&q.proxy.stamp);
    let eligible = fingerprints_current
        && state == "ready"
        && !p.color_pipeline.enabled
        && a.width == q.source_width
        && a.height == q.source_height;
    json!({"asset_id":id,"path":q.path,"status":state,"deep":deep,"eligible":eligible,
        "fingerprints_current":fingerprints_current,"requires_relink":state == "ready" && !fingerprints_current,
        "preview_active":eligible && p.proxies.enabled,"managed_color_bypass":p.color_pipeline.enabled,
        "width":q.width,"height":q.height,"frames":q.frames,"fps_num":q.fps_num,"fps_den":q.fps_den,
        "source_bytes":q.source.stamp.bytes,"proxy_bytes":q.proxy.stamp.bytes})
}
/// Fast preview checks use stat fingerprints. Export and analysis never call this selector.
/// An enabled, current, registered proxy wins; otherwise a ready automatic proxy (auto_proxy.rs) is used.
pub fn preview_path(p: &Project, id: &str) -> Option<String> {
    registered_preview_path(p, id).or_else(|| crate::auto_proxy::preview_path(p, id))
}
fn registered_preview_path(p: &Project, id: &str) -> Option<String> {
    if !p.proxies.enabled || p.color_pipeline.enabled {
        return None;
    }
    let a = p.assets.get(id)?;
    let q = p.proxies.entries.get(id)?;
    if a.width != q.source_width || a.height != q.source_height {
        return None;
    }
    if stamp(Path::new(&p.resolve(&a.path))).ok()? != q.source.stamp
        || stamp(Path::new(&p.resolve(&q.path))).ok()? != q.proxy.stamp
    {
        return None;
    }
    Some(p.resolve(&q.path))
}

#[derive(Debug)]
pub struct Video {
    pub width: u32,
    pub height: u32,
    pub num: u32,
    pub den: u32,
    pub frames: usize,
    /// avg_frame_rate, reduced. Equals num/den for CFR sources; phone footage is often VFR and this
    /// is the honest average (a big-numerator rational like 25110000/835459 is a timebase artifact,
    /// not a rate problem -- the VALUE is what matters, see inspect).
    pub avg_num: u32,
    pub avg_den: u32,
    /// True when r_frame_rate and avg_frame_rate disagree (variable frame rate). VFR sources get
    /// passthrough proxies that keep every frame ordinal; callers choose the build path with this.
    pub vfr: bool,
    /// First video stream's codec name, lowercase ("" when unknown).
    pub codec: String,
    /// Some((range, matrix, primaries, transfer)) for HLG/PQ camera HDR (10-bit Pixel/iPhone
    /// "HDR" footage): the proxy is encoded 8-bit but re-declares these exact color tags, so any
    /// decode that honors stream metadata renders the proxy like the original (quantization
    /// aside). None for SDR sources, whose argv stays byte-identical to before.
    pub color_tags: Option<(String, String, String, String)>,
}
pub fn avg_fps(v: &Video) -> f64 { v.avg_num as f64 / v.avg_den as f64 }
fn number(v: &Value, key: &str) -> Result<f64, String> {
    v[key]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| v[key].as_f64())
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("missing video {key}"))
}
/// Inspect every timestamp rather than guessing CFR from container rate tags.
///
/// Rate tags are validated by VALUE (1..240 fps): a VFR phone file reports avg_frame_rate as a raw
/// timebase rational (e.g. 25110000/835459 = 30.05 fps), and the old numerator cap rejected exactly
/// that footage. VFR itself is no longer a rejection here -- it is reported in `vfr` and the proxy
/// builders respond with a passthrough proxy that preserves the frame sequence.
///
/// The scan is two-stage because a decoded-frame scan of 4K HEVC takes minutes on one core, which
/// froze every proxy build behind it. Stage one is header-only (milliseconds) and picks the scan:
/// streams without B-frames -- every camera/phone file, and every proxy this module writes (intra,
/// bf 0) -- have packet pts in presentation order already, so stage two demuxes without decoding
/// (measured 0.17 s on a 42 Mbps 4K HEVC clip). B-frame streams reorder, so for those (and any
/// stream that does not say) stage two scans decoded frames, multithreaded: ffprobe decodes on one
/// thread unless told otherwise.
pub fn inspect(path: &Path, timeout: Duration) -> Result<Video, String> {
    let probe = |entries: &str, threads: bool| -> Result<Value, String> {
        let mut cmd = Command::new("ffprobe");
        cmd.args(["-v", "error", "-select_streams", "v:0"]);
        if threads { cmd.args(["-threads", "0"]); }
        cmd.args(["-show_entries", entries, "-of", "json"]).arg(path);
        let bytes = crate::color::process::decode_capture(&mut cmd, 32 << 20, timeout)?;
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    };
    let s = probe("stream=codec_name,width,height,r_frame_rate,avg_frame_rate,start_time,pix_fmt,sample_aspect_ratio,color_transfer,has_b_frames:stream_tags=rotate:stream_side_data=rotation:stream_disposition=attached_pic", false)?["streams"]
        .as_array()
        .and_then(|a| a.first())
        .cloned()
        .ok_or("proxy requires a video stream")?;
    let rate = |key: &str| -> Result<(u64, u64), String> {
        let (n, d) = s[key]
            .as_str()
            .and_then(|x| x.split_once('/'))
            .ok_or("missing rational video rate")?;
        let (n, d) = (
            n.parse::<u64>().map_err(|e| e.to_string())?,
            d.parse::<u64>().map_err(|e| e.to_string())?,
        );
        if n == 0 || d == 0 || !(1.0..=240.0).contains(&(n as f64 / d as f64)) {
            return Err("unsupported video rate (requires 1..240 fps)".into());
        }
        Ok((n, d))
    };
    let (rnum, rden) = rate("r_frame_rate")?;
    let (anum, aden) = rate("avg_frame_rate")?;
    let vfr = (rnum as f64 / rden as f64 - anum as f64 / aden as f64).abs() > 1e-6;
    if number(&s, "start_time")?.abs() > 0.0001 {
        return Err("proxy requires a video stream starting at zero".into());
    }
    if s["sample_aspect_ratio"]
        .as_str()
        .is_some_and(|x| x != "1:1" && x != "N/A")
    {
        return Err("proxy requires square pixels".into());
    }
    if s["tags"]["rotate"]
        .as_str()
        .is_some_and(|x| x.parse::<f64>().unwrap_or(1.0).abs() > 0.0001)
        || s["side_data_list"].as_array().is_some_and(|a| {
            a.iter()
                .any(|v| v["rotation"].as_f64().unwrap_or(0.0).abs() > 0.0001)
        })
        || s["disposition"]["attached_pic"].as_u64().unwrap_or(0) != 0
    {
        return Err("proxy does not support rotated video or attached pictures".into());
    }
    // Camera HDR (10-bit HLG/PQ -- every Pixel "HDR" clip) is proxied to 8-bit with the source's
    // color tags re-declared, so previews still match the original's decode; everything else
    // outside the historical 8-bit SDR set stays on originals, exactly as before. The proxy itself
    // is 8-bit + HLG tags, so re-inspecting it passes this same rule.
    let pix = s["pix_fmt"].as_str().unwrap_or("");
    let trc = s["color_transfer"].as_str().unwrap_or("unknown");
    let hdr = trc == "arib-std-b67" || trc == "smpte2084";
    let pix_ok = [
        "yuv420p", "yuvj420p", "yuv422p", "yuvj422p", "yuv444p", "yuvj444p", "rgb24", "bgr24",
        "gbrp", "nv12",
    ]
    .contains(&pix)
        || (hdr && pix == "yuv420p10le");
    let trc_ok = hdr
        || [
            "unknown",
            "bt709",
            "iec61966-2-1",
            "smpte170m",
            "gamma22",
            "gamma28",
            "bt470m",
            "bt470bg",
        ]
        .contains(&trc);
    if !pix_ok || !trc_ok {
        return Err(
            "proxy supports opaque 8-bit SDR and 10-bit HLG/PQ camera video only; alpha/float media stays on originals"
                .into(),
        );
    }
    let color_tags = hdr.then(|| {
        let g = |k: &str| s[k].as_str().unwrap_or("unknown").to_string();
        (g("color_range"), g("color_space"), g("color_primaries"), g("color_transfer"))
    });
    // B-frame streams reorder packets, so only decoded frames give the presentation order there;
    // a stream that does not report has_b_frames also takes the safe decoded path. Note the ffprobe
    // quirk: -show_entries names sections in the singular (packet=/frame=) while the JSON output
    // section is plural (packets/frames).
    let (spec, section, key, threads) = match s["has_b_frames"].as_u64() {
        Some(0) => ("packet", "packets", "pts_time", false),
        _ => ("frame", "frames", "best_effort_timestamp_time", true),
    };
    let scan = probe(&format!("{spec}={key}"), threads)?[section]
        .as_array()
        .cloned()
        .ok_or("video timestamps missing")?;
    if scan.is_empty() || scan.len() > 216_000 {
        return Err("proxy requires 1..216000 video frames".into());
    }
    let mut ts = Vec::with_capacity(scan.len());
    for f in &scan {
        crate::guard::check();
        ts.push(number(f, key)?);
    }
    if vfr {
        // VFR source: the renderer indexes frames by ordinal (media_index = time*fps -> decode
        // position), so the proxy must preserve the frame SEQUENCE, which a passthrough encode
        // does. What must still hold: timestamps strictly increase with no long stall (a sane,
        // decodable stream) -- the ordinal contract's VFR counterpart of the CFR grid check.
        let mut prev = -1.0f64;
        for t in &ts {
            if *t <= prev {
                return Err("nonmonotonic video timestamps: this stream cannot be proxied".into());
            }
            if *t - prev > 1.0 {
                return Err("video timestamps stall for over a second: this stream cannot be proxied".into());
            }
            prev = *t;
        }
    } else {
        let dt = rden as f64 / rnum as f64;
        for (i, t) in ts.iter().enumerate() {
            if (t - i as f64 * dt).abs() > dt * 0.02 + 0.00001 {
                return Err("nonuniform video timestamps: conform variable frame rate before generating proxies".into());
            }
        }
    }
    let width = s["width"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or("invalid video width")?;
    let height = s["height"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or("invalid video height")?;
    let (num, den) = fit_rate(rnum, rden);
    let (avg_num, avg_den) = fit_rate(anum, aden);
    Ok(Video {
        width,
        height,
        num,
        den,
        frames: scan.len(),
        avg_num,
        avg_den,
        vfr,
        codec: s["codec_name"].as_str().unwrap_or("").to_string(),
        color_tags,
    })
}
/// Reduce a rate rational by gcd so metadata rates fit u32 without changing the value.
fn fit_rate(num: u64, den: u64) -> (u32, u32) {
    let (mut a, mut b) = (num, den);
    while b != 0 { (a, b) = (b, a % b); }
    match (u32::try_from(num / a), u32::try_from(den / a)) {
        (Ok(n), Ok(d)) => (n, d),
        // A pathological timebase could still overflow u32 after reduction; keep the nearest
        // representable value instead of failing the whole inspect over metadata.
        _ => { let k = (num as f64 / den as f64).clamp(1.0, 240.0); (k.round() as u32, 1) }
    }
}
pub fn dimensions(w: u32, h: u32, max_side: u32) -> Result<(u32, u32), String> {
    if w == 0 || h == 0 {
        return Err("invalid source dimensions".into());
    }
    let (mut a, mut b) = (w, h);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let (rw, rh) = (w / a, h / a);
    let mut k = (max_side / rw.max(rh)).min(a);
    if rw % 2 != 0 || rh % 2 != 0 {
        k -= k % 2;
    }
    if k == 0 {
        return Err(
            "cannot preserve the exact source aspect ratio at this max_side; choose a larger bound"
                .into(),
        );
    }
    Ok((rw * k, rh * k))
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Re-declare a 10-bit HDR source's color tags on its 8-bit proxy (range, matrix, primaries,
/// transfer), skipping anything the source did not declare. Encoder-agnostic: NVENC and libx264
/// both write these into the H.264 VUI, which ffprobe reads back, so the preview decode applies
/// the same color interpretation as the original.
pub(crate) fn color_args(cmd: &mut Command, t: &(String, String, String, String)) {
    if t.0 != "unknown" { cmd.args(["-color_range", &t.0]); }
    if t.1 != "unknown" { cmd.args(["-colorspace", &t.1]); }
    if t.2 != "unknown" { cmd.args(["-color_primaries", &t.2]); }
    if t.3 != "unknown" { cmd.args(["-color_trc", &t.3]); }
}
pub fn generate(
    p: &Project,
    id: &str,
    destination: &str,
    max_side: u32,
    timeout: Duration,
) -> Result<Proxy, String> {
    if !(64..=4096).contains(&max_side) {
        return Err("max_side must be an integer in 64..4096".into());
    }
    let a = p.assets.get(id).ok_or("no such asset")?;
    if p.color_pipeline.enabled {
        return Err(
            "proxy generation is limited to unmanaged SDR projects; managed previews use originals"
                .into(),
        );
    }
    let source_path = PathBuf::from(p.resolve(&a.path));
    let dest = PathBuf::from(p.resolve(destination));
    if !dest
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("mp4"))
    {
        return Err("proxy path must end in .mp4".into());
    }
    if dest.exists() {
        return Err(
            "proxy destination already exists; choose a new path (files are never overwritten)"
                .into(),
        );
    }
    let parent = dest
        .parent()
        .filter(|x| !x.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("proxy parent directory must exist: {e}"))?;
    let dest = parent.join(dest.file_name().ok_or("invalid proxy path")?);
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .ok_or("invalid proxy timeout")?;
    let remaining = || deadline.saturating_duration_since(std::time::Instant::now());
    let source = fingerprint(&source_path)?;
    let v = inspect(&source_path, remaining())?;
    if v.width != a.width
        || v.height != a.height
        || (v.num as f64 / v.den as f64 - a.fps).abs() > 0.01
    {
        return Err(
            "source metadata differs from the imported asset; reimport before generating a proxy"
                .into(),
        );
    }
    let (width, height) = dimensions(v.width, v.height, max_side)?;
    let tmp_path = parent.join(format!(
        ".kimo-proxy-{}-{}.mp4",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|e| e.to_string())?;
    let tmp = Temp(tmp_path);
    // Hardware decode/encode when the GPU answers (hw.rs probes once per process). The software
    // argv below is byte-identical to the pre-hw path, so with KIMO_HWACCEL=off, without an NVIDIA
    // GPU, or whenever the hardware attempt fails, everything behaves exactly as before.
    let hw_decode = crate::hw::nvdec_available() && (v.codec == "h264" || v.codec == "hevc");
    let nvenc = crate::hw::nvenc_available();
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y"]);
    if hw_decode {
        cmd.args(["-hwaccel", "cuda"]);
    }
    cmd.args(["-noautorotate", "-i"])
        .arg(&source_path)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-vf",
            &format!("scale={width}:{height}:flags=fast_bilinear,setsar=1"),
        ]);
    if nvenc {
        // NVENC all-intra: gopLength 0 means every frame is an intra keyframe (SDK semantics).
        // `-g 1` is rejected outright by SDK 12.2+ ("Gop Length should be greater than number of
        // B frames + 1"), so the x264 `-g 1` literal cannot carry over.
        cmd.args(["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "23", "-b:v", "0", "-g", "0", "-bf", "0"]);
    } else {
        cmd.args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-g", "1", "-bf", "0"]);
    }
    cmd.arg("-pix_fmt").arg("yuv420p");
    if let Some(t) = &v.color_tags { color_args(&mut cmd, t); }
    if v.vfr {
        // VFR source: keep every frame and its real timestamp (passthrough). The renderer maps
        // time to frame ordinals, so the proxy must hold the same frame sequence -- a CFR
        // resample would duplicate/drop frames and shift every later frame.
        cmd.args(["-fps_mode", "passthrough", "-video_track_timescale", "90000"]);
    } else {
        cmd.args([
            "-r",
            &format!("{}/{}", v.num, v.den),
            "-fps_mode",
            "cfr",
            "-video_track_timescale",
            &v.num.to_string(),
        ]);
    }
    cmd.args(["-f", "mp4"]).arg(&tmp.0);
    let attempt = crate::color::process::wait_command(&mut cmd, remaining());
    if attempt.is_err() && (hw_decode || nvenc) {
        // The hardware attempt failed (GPU busy, NVENC session limit, driver state): retry once,
        // all-software -- also when only the encoder was on the GPU. The temp file may hold a partial
        // encode; -y rewrites it.
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-nostdin", "-y", "-noautorotate", "-i"])
            .arg(&source_path)
            .args([
                "-map",
                "0:v:0",
                "-an",
                "-sn",
                "-dn",
                "-map_metadata",
                "-1",
                "-vf",
                &format!("scale={width}:{height}:flags=fast_bilinear,setsar=1"),
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "23",
                "-g",
                "1",
                "-bf",
                "0",
                "-pix_fmt",
                "yuv420p",
            ]);
        if let Some(t) = &v.color_tags { color_args(&mut cmd, t); }
        if v.vfr {
            cmd.args(["-fps_mode", "passthrough", "-video_track_timescale", "90000"]);
        } else {
            cmd.args([
                "-r",
                &format!("{}/{}", v.num, v.den),
                "-fps_mode",
                "cfr",
                "-video_track_timescale",
                &v.num.to_string(),
            ]);
        }
        cmd.args(["-f", "mp4"]).arg(&tmp.0);
        crate::color::process::wait_command(&mut cmd, remaining())?;
    } else {
        attempt?;
    }
    let encoded = inspect(&tmp.0, remaining())?;
    let rate_ok = if v.vfr {
        (avg_fps(&encoded) - avg_fps(&v)).abs() <= avg_fps(&v) * 0.01 + 0.01
    } else {
        (encoded.num as f64 / encoded.den as f64 - v.num as f64 / v.den as f64).abs() <= 1e-6
    };
    if encoded.frames != v.frames
        || encoded.width != width
        || encoded.height != height
        || !rate_ok
    {
        return Err("proxy failed frame-count/rate/dimension validation".into());
    }
    let proxy = fingerprint(&tmp.0)?;
    let source_after = fingerprint(&source_path)?;
    if source_after.stamp != source.stamp || source_after.sha256 != source.sha256 {
        return Err("source changed during proxy generation; retry".into());
    }
    crate::guard::check();
    // Atomic, no-clobber publication. Neither errors nor cancellation attach a partial proxy.
    std::fs::hard_link(&tmp.0, &dest)
        .map_err(|e| format!("cannot publish proxy without overwriting: {e}"))?;
    // Metadata rate: the honest average for VFR passthrough proxies, the CFR grid rate otherwise.
    let (fps_num, fps_den) = if v.vfr { (v.avg_num, v.avg_den) } else { (v.num, v.den) };
    Ok(Proxy {
        path: dest.to_string_lossy().into_owned(),
        source,
        proxy,
        source_width: v.width,
        source_height: v.height,
        width,
        height,
        fps_num,
        fps_den,
        frames: v.frames,
    })
}
