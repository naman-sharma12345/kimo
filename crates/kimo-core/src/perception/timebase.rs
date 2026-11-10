//! Exact PCM indices plus explicitly snapped video-frame cuts. All ends are exclusive.
use serde_json::{json, Value};
pub const SR: u64 = 48000;
pub fn sample(t: f64) -> u64 {
    (t.max(0.) * SR as f64).round() as u64
}
pub fn frame_sample(frame: u64, fps: f64) -> u64 {
    (frame as f64 * SR as f64 / fps).round() as u64
}
pub fn annotate(v: &mut Value, fps: f64, inward: bool) {
    let a = sample(v["start"].as_f64().unwrap_or(0.));
    let b = sample(v["end"].as_f64().unwrap_or(0.)).max(a);
    v["start"] = json!(a as f64 / SR as f64);
    v["end"] = json!(b as f64 / SR as f64);
    if v.get("duration").is_some() {
        v["duration"] = json!((b - a) as f64 / SR as f64)
    }
    if v.get("remove_s").is_some() {
        v["remove_s"] = json!((b - a) as f64 / SR as f64)
    }
    v["start_sample"] = json!(a);
    v["end_sample"] = json!(b);
    v["sample_count"] = json!(b - a);
    v["start_ms"] = json!(a as f64 * 1000. / SR as f64);
    v["end_ms"] = json!(b as f64 * 1000. / SR as f64);
    if fps.is_finite() && fps > 0. {
        let mut fa = (a as f64 * fps / SR as f64).floor() as u64;
        let mut fb = (b as f64 * fps / SR as f64).ceil() as u64;
        if inward {
            while frame_sample(fa, fps) < a {
                fa += 1
            }
            while fb > 0 && frame_sample(fb, fps) > b {
                fb -= 1
            }
        } else {
            while fa > 0 && frame_sample(fa, fps) > a {
                fa -= 1
            }
            while frame_sample(fb, fps) < b {
                fb += 1
            }
        }
        if fb >= fa {
            let sa = frame_sample(fa, fps);
            let sb = frame_sample(fb, fps);
            v["frame_aligned"] = json!({"start_frame":fa,"end_frame":fb,"frame_count":fb-fa,"start_sample":sa,"end_sample":sb,"sample_count":sb-sa,"fps":fps,"snap":if inward{"inward"}else{"outward"},"usable":fb>fa})
        } else {
            v["frame_aligned"] = json!({"usable":false,"snap":"inward","fps":fps})
        }
    }
}
pub fn meta(fps: f64) -> Value {
    json!({"sample_rate":SR,"sample_unit":"per_channel_pcm_frame","origin":"target_start","end":"exclusive","fps":fps,"frame_sample_rule":"round(frame_index*48000/fps)","cut_snap":"inward; protects speech edges"})
}
