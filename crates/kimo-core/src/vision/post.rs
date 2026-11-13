//! Pre- and post-processing shared by the detectors: resizing into the model's input tensor and decoding the raw
//! outputs into boxes. Pure functions (no model runtime), so they are unit tested without any download.
use super::{BBox, Detection, FaceBox};

/// An RGBA8 picture (display-referred, as `render::to_rgba8` makes it).
pub struct Rgba<'a> { pub w: usize, pub h: usize, pub px: &'a [u8] }

/// Bilinear sample of channel `c` at continuous pixel position (x, y), edges clamped.
fn sample(im: &Rgba, x: f32, y: f32, c: usize) -> f32 {
    let x = x.clamp(0.0, (im.w - 1) as f32); let y = y.clamp(0.0, (im.h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize); let (x1, y1) = ((x0 + 1).min(im.w - 1), (y0 + 1).min(im.h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |xx: usize, yy: usize| im.px[(yy * im.w + xx) * 4 + c] as f32;
    (p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx) * (1.0 - fy) + (p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx) * fy
}

/// Area-ish resample of `im` into a `tw` x `th` NCHW tensor. `fit`: Some(pad) keeps the aspect ratio (scaled to fit,
/// placed top-left, the rest filled with `pad`) and returns the scale; None stretches to fill. `chan` picks the source
/// channel for each output plane (e.g. [2,1,0] for BGR), `norm` maps 0..255 to the model's range per plane.
pub fn to_tensor(im: &Rgba, tw: usize, th: usize, fit: Option<f32>, chan: [usize; 3], norm: &dyn Fn(usize, f32) -> f32) -> (Vec<f32>, f32, f32) {
    let (sx, sy) = match fit { Some(_) => { let s = (tw as f32 / im.w as f32).min(th as f32 / im.h as f32); (s, s) } None => (tw as f32 / im.w as f32, th as f32 / im.h as f32) };
    let (cw, ch) = (((im.w as f32 * sx).round() as usize).min(tw), ((im.h as f32 * sy).round() as usize).min(th));
    // pre-blur by averaging a few taps when shrinking a lot, so thin detail does not alias
    let taps = ((1.0 / sx.min(sy)).ceil() as usize).clamp(1, 4);
    let mut out = vec![0.0f32; 3 * tw * th];
    for y in 0..th { for x in 0..tw {
        for (k, &c) in chan.iter().enumerate() {
            let v = if x < cw && y < ch {
                let mut s = 0.0; for ty in 0..taps { for tx in 0..taps {
                    let px = (x as f32 + (tx as f32 + 0.5) / taps as f32) / sx - 0.5; let py = (y as f32 + (ty as f32 + 0.5) / taps as f32) / sy - 0.5;
                    s += sample(im, px, py, c); } }
                norm(k, s / (taps * taps) as f32)
            } else { norm(k, fit.unwrap_or(0.0)) };
            out[k * tw * th + y * tw + x] = v;
        } } }
    (out, sx, sy)
}

pub fn iou(a: &BBox, b: &BBox) -> f32 {
    let ix = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0); let iy = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let i = ix * iy; let u = a.w * a.h + b.w * b.h - i; if u > 0.0 { i / u } else { 0.0 }
}

/// Greedy non-maximum suppression: indices kept, best score first.
pub fn nms(boxes: &[BBox], scores: &[f32], thr: f32, keep_max: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..boxes.len()).collect(); order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]).then(a.cmp(b)));
    let mut keep: Vec<usize> = vec![];
    for i in order { if keep.len() >= keep_max { break; } if keep.iter().all(|&k| iou(&boxes[k], &boxes[i]) <= thr) { keep.push(i); } }
    keep
}

fn clip(b: BBox, w: usize, h: usize) -> BBox {
    let (x0, y0) = (b.x.clamp(0.0, w as f32), b.y.clamp(0.0, h as f32)); let (x1, y1) = ((b.x + b.w).clamp(0.0, w as f32), (b.y + b.h).clamp(0.0, h as f32));
    BBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

pub const YUNET_SIZE: usize = 640;
pub const YUNET_STRIDES: [usize; 3] = [8, 16, 32];

/// Decode YuNet (2023mar) outputs: `outs` in model order cls_8,16,32, obj_8,16,32, bbox_8,16,32, kps_8,16,32, for a
/// `size` x `size` input made with `scale` (frame px * scale = input px). Boxes and landmarks in frame px.
pub fn decode_yunet(outs: &[&[f32]], size: usize, scale: f32, fw: usize, fh: usize, thr: f32) -> Vec<FaceBox> {
    let (mut boxes, mut scores, mut lms) = (vec![], vec![], vec![]);
    for (si, &s) in YUNET_STRIDES.iter().enumerate() {
        let (cls, obj, bb, kp) = (outs[si], outs[3 + si], outs[6 + si], outs[9 + si]);
        let cols = size / s; let n = cols * (size / s);
        for i in 0..n.min(cls.len()) {
            let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
            if score < thr { continue; }
            let (r, c) = ((i / cols) as f32, (i % cols) as f32); let sf = s as f32;
            let (cx, cy) = ((c + bb[i * 4]) * sf, (r + bb[i * 4 + 1]) * sf); let (w, h) = (bb[i * 4 + 2].exp() * sf, bb[i * 4 + 3].exp() * sf);
            boxes.push(BBox { x: (cx - w / 2.0) / scale, y: (cy - h / 2.0) / scale, w: w / scale, h: h / scale }); scores.push(score);
            let mut l = [[0.0f32; 2]; 5];
            for (k, p) in l.iter_mut().enumerate() { *p = [((kp[i * 10 + 2 * k] + c) * sf / scale).clamp(0.0, fw as f32), ((kp[i * 10 + 2 * k + 1] + r) * sf / scale).clamp(0.0, fh as f32)]; }
            lms.push(l);
        }
    }
    nms(&boxes, &scores, 0.3, 200).into_iter().map(|i| FaceBox { bbox: clip(boxes[i], fw, fh), score: scores[i], landmarks: lms[i] }).filter(|f| f.bbox.w > 1.0 && f.bbox.h > 1.0).collect()
}

pub const YOLOX_SIZE: usize = 416;
pub const COCO: [&str; 80] = ["person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat", "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle", "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange", "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone", "microwave", "oven", "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush"];

/// Decode YOLOX (no in-graph decode) output [N, 85] for a `size` input letterboxed with `scale`: per anchor
/// (dx, dy, log w, log h) on grids of strides 8/16/32, objectness and 80 class scores (already sigmoid).
/// Class-wise NMS at IoU 0.45. Boxes in frame px.
pub fn decode_yolox(out: &[f32], size: usize, scale: f32, fw: usize, fh: usize, thr: f32) -> Vec<Detection> {
    let (mut boxes, mut scores, mut cls) = (vec![], vec![], vec![]);
    let mut i = 0usize;
    for s in [8usize, 16, 32] { let g = size / s; for gy in 0..g { for gx in 0..g {
        let row = &out[i * 85..(i + 1) * 85]; i += 1;
        let (mut best, mut bc) = (0.0f32, 0usize); for (c, v) in row[5..].iter().enumerate() { if *v > best { best = *v; bc = c; } }
        let score = row[4] * best; if score < thr { continue; }
        let sf = s as f32; let (cx, cy) = ((row[0] + gx as f32) * sf, (row[1] + gy as f32) * sf); let (w, h) = (row[2].exp() * sf, row[3].exp() * sf);
        boxes.push(BBox { x: (cx - w / 2.0) / scale, y: (cy - h / 2.0) / scale, w: w / scale, h: h / scale }); scores.push(score); cls.push(bc);
    } } }
    let mut dets = vec![];
    for c in 0..80 { let idx: Vec<usize> = (0..boxes.len()).filter(|&k| cls[k] == c).collect(); if idx.is_empty() { continue; }
        let bx: Vec<BBox> = idx.iter().map(|&k| boxes[k]).collect(); let sc: Vec<f32> = idx.iter().map(|&k| scores[k]).collect();
        for k in nms(&bx, &sc, 0.45, 100) { dets.push(Detection { bbox: clip(bx[k], fw, fh), class: COCO[c], class_id: c as u16, score: sc[k] }); } }
    dets.retain(|d| d.bbox.w > 1.0 && d.bbox.h > 1.0);
    dets.sort_by(|a, b| b.score.total_cmp(&a.score)); dets
}

/// Min-max scale to 0..1 in place (constant -> zeros).
pub fn minmax(v: &mut [f32]) { let (mn, mx) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x))); if mx > mn { for x in v.iter_mut() { *x = (*x - mn) / (mx - mn); } } else { v.iter_mut().for_each(|x| *x = 0.0); } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn nms_keeps_best_of_overlaps() {
        let b = [BBox { x: 0.0, y: 0.0, w: 10.0, h: 10.0 }, BBox { x: 1.0, y: 1.0, w: 10.0, h: 10.0 }, BBox { x: 50.0, y: 50.0, w: 10.0, h: 10.0 }];
        assert_eq!(nms(&b, &[0.5, 0.9, 0.7], 0.3, 10), vec![1, 2]);
    }
    #[test] fn letterbox_keeps_aspect_and_pads() {
        // a 4x2 red picture into an 8x8 tensor: scale 2, content rows 0..4, pad below
        let px: Vec<u8> = (0..8).flat_map(|_| [255u8, 0, 0, 255]).collect();
        let (t, sx, sy) = to_tensor(&Rgba { w: 4, h: 2, px: &px }, 8, 8, Some(114.0), [2, 1, 0], &|_, v| v);
        assert_eq!((sx, sy), (2.0, 2.0));
        assert_eq!(t[2 * 64], 255.0); assert_eq!(t[0], 0.0); // plane 2 is R (BGR order)
        assert_eq!(t[2 * 64 + 7 * 8], 114.0); // padded row
    }
    #[test] fn yunet_decode_one_anchor() {
        // one confident anchor at stride 32, row 2 col 3 of a 640 input, centre offset 0.5, size e^0 * 32 = 32 px; scale 0.5
        let size = 640; let mk = |s: usize, c: usize| vec![0.0f32; (size / s) * (size / s) * c];
        let (mut cls32, mut obj32, mut bb32, mut kp32) = (mk(32, 1), mk(32, 1), mk(32, 4), mk(32, 10));
        let i = 2 * 20 + 3; cls32[i] = 0.9; obj32[i] = 0.9; bb32[i * 4] = 0.5; bb32[i * 4 + 1] = 0.5; for k in 0..5 { kp32[i * 10 + 2 * k] = 0.5; kp32[i * 10 + 2 * k + 1] = 0.5; }
        let (c8, c16, b8, b16, k8, k16) = (mk(8, 1), mk(16, 1), mk(8, 4), mk(16, 4), mk(8, 10), mk(16, 10));
        let outs: Vec<&[f32]> = vec![&c8, &c16, &cls32, &c8, &c16, &obj32, &b8, &b16, &bb32, &k8, &k16, &kp32];
        let f = decode_yunet(&outs, size, 0.5, 2000, 2000, 0.6);
        assert_eq!(f.len(), 1); let b = f[0].bbox;
        assert!((b.x - 192.0).abs() < 1e-3 && (b.y - 128.0).abs() < 1e-3 && (b.w - 64.0).abs() < 1e-3, "{b:?}");
        assert!((f[0].score - 0.9).abs() < 1e-5); assert_eq!(f[0].landmarks[0], [224.0, 160.0]);
    }
    #[test] fn yolox_decode_one_person() {
        let mut out = vec![0.0f32; 3549 * 85]; let i = 10 * 52 + 20; // stride 8, row 10, col 20
        out[i * 85 + 2] = (4.0f32).ln(); out[i * 85 + 3] = (8.0f32).ln(); out[i * 85 + 4] = 0.9; out[i * 85 + 5] = 0.8;
        let d = decode_yolox(&out, 416, 1.0, 416, 416, 0.3);
        assert_eq!(d.len(), 1); assert_eq!(d[0].class, "person");
        let b = d[0].bbox; assert!((b.x - (160.0 - 16.0)).abs() < 1e-3 && (b.y - (80.0 - 32.0)).abs() < 1e-3 && (b.w - 32.0).abs() < 1e-3 && (b.h - 64.0).abs() < 1e-3, "{b:?}");
    }
}
