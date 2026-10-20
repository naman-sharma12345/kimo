//! Deterministic display-domain scope images. No GUI, auto exposure, or mastering claim.
//! IRE here means full-range normalized encoded signal * 100, not legal-range YUV.
//! PQ nits are ST2084 absolute luminance. HLG nits use an explicitly declared
//! peak and system gamma (BT2100-style luminance OOTF), not a measured display.
use crate::effects::Frame;
use serde_json::{json, Value};

pub const WIDTH: usize = 768;
pub const HEIGHT: usize = 448;
const LEFT: usize = 64;
const RIGHT: usize = 735;
const TOP: usize = 52;
const BOTTOM: usize = 395;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Waveform,
    Parade,
    Vectorscope,
    Histogram,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gamut {
    Rec709,
    Rec2020,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Encoding {
    Bt709,
    Srgb,
    Linear,
    Pq,
    Hlg,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Units {
    Ire,
    Nits,
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub kind: Kind,
    pub gamut: Gamut,
    pub encoding: Encoding,
    pub units: Units,
    /// Vertical waveform/parade or horizontal histogram scale, starting at zero.
    pub ceiling: f64,
    pub reference_white_nits: f64,
    pub hlg_peak_nits: f64,
    pub hlg_system_gamma: f64,
    /// Vectorscope PNG only: also draw half-size, dimmer boxes at the 75% targets.
    pub targets_75: bool,
}
impl Options {
    pub fn sdr(kind: Kind) -> Self {
        Self {
            kind,
            gamut: Gamut::Rec709,
            encoding: Encoding::Bt709,
            units: Units::Ire,
            ceiling: 100.,
            reference_white_nits: 100.,
            hlg_peak_nits: 1000.,
            hlg_system_gamma: 1.2,
            targets_75: false,
        }
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if !self.ceiling.is_finite()
            || self.ceiling <= 0.
            || self.ceiling > 10000.
            || !self.reference_white_nits.is_finite()
            || !(1. ..=10000.).contains(&self.reference_white_nits)
            || !self.hlg_peak_nits.is_finite()
            || !(100. ..=10000.).contains(&self.hlg_peak_nits)
            || !self.hlg_system_gamma.is_finite()
            || !(1. ..=2.).contains(&self.hlg_system_gamma)
        {
            return Err("invalid scope scale/reference white/HLG display declaration".into());
        }
        if self.encoding == Encoding::Linear && self.units == Units::Ire {
            return Err("linear RGB requires nits mode; IRE/vector require encoded RGB".into());
        }
        if self.kind == Kind::Vectorscope && self.units != Units::Ire {
            return Err("vectorscope requires IRE mode: encoded CbCr, not nits".into());
        }
        Ok(())
    }
    fn weights(&self) -> [f64; 3] {
        match self.gamut {
            Gamut::Rec709 => [0.2126, 0.7152, 0.0722],
            Gamut::Rec2020 => [0.2627, 0.6780, 0.0593],
        }
    }
    /// Encoded Cb on x, Cr on y. Positive Cr points upward in the image.
    pub fn cbcr(&self, rgb: [f64; 3]) -> [f64; 2] {
        let w = self.weights();
        let y = dot(w, rgb);
        [
            (rgb[2] - y) / (2. * (1. - w[2])),
            (rgb[0] - y) / (2. * (1. - w[0])),
        ]
    }
    /// Three RGB and one luma/luminance values, before plotting clamps.
    pub fn values(&self, rgb: [f64; 3]) -> [f64; 4] {
        let w = self.weights();
        let rgb = match self.units {
            Units::Ire => rgb.map(|x| x * 100.),
            Units::Nits => {
                let id = match self.encoding {
                    Encoding::Linear => 0,
                    Encoding::Srgb => 1,
                    Encoding::Pq => 6,
                    Encoding::Hlg => 7,
                    Encoding::Bt709 => 8,
                };
                let lin = rgb.map(|x| super::managed::decode(id, x, self.reference_white_nits));
                let scale = if self.encoding == Encoding::Hlg {
                    self.hlg_peak_nits * dot(w, lin).max(0.).powf(self.hlg_system_gamma - 1.)
                } else {
                    self.reference_white_nits
                };
                lin.map(|x| x * scale)
            }
        };
        [rgb[0], rgb[1], rgb[2], dot(w, rgb)]
    }
}
fn dot(w: [f64; 3], rgb: [f64; 3]) -> f64 {
    w[0] * rgb[0] + w[1] * rgb[1] + w[2] * rgb[2]
}
/// PNG bytes plus exact counts and axis declarations. Zero-alpha and invalid pixels are excluded;
/// all other visible pixels count once (no alpha weighting or sampling).
pub struct ScopeImage {
    pub png: Vec<u8>,
    pub metadata: Value,
}
fn check_frame(frame: &Frame) -> Result<(), String> {
    let count = frame.w.checked_mul(frame.h).ok_or("scope frame overflow")?;
    if count == 0
        || count as u128 > crate::guard::limits().max_pixels as u128
        || count.checked_mul(4) != Some(frame.d.len())
    {
        return Err("invalid or oversized scope frame".into());
    }
    Ok(())
}
enum Sample {
    Invalid,
    Transparent,
    Visible([f64; 3], [f64; 4]),
}
/// Shared pixel selection for `render` and `data`: encoded RGB plus `o.values` levels.
fn sample(p: &[f32], o: &Options) -> Sample {
    if p.iter().any(|v| !v.is_finite()) || !(0. ..=1.).contains(&p[3]) {
        return Sample::Invalid;
    }
    if p[3] == 0. {
        return Sample::Transparent;
    }
    let rgb = [p[0] as f64, p[1] as f64, p[2] as f64];
    // PQ/HLG nominal encoded range enforced before their bounded EOTF.
    if matches!(o.encoding, Encoding::Pq | Encoding::Hlg) && rgb.iter().any(|v| *v < 0. || *v > 1.)
    {
        return Sample::Invalid;
    }
    let values = o.values(rgb);
    if values.iter().any(|v| !v.is_finite()) {
        return Sample::Invalid;
    }
    Sample::Visible(rgb, values)
}
pub fn render(frame: &Frame, o: Options) -> Result<ScopeImage, String> {
    o.validate()?;
    check_frame(frame)?;
    // Includes accumulator, raster, scanlines, stored-zlib PNG and metadata headroom.
    crate::guard::reserve(WIDTH * HEIGHT * 40 + 65536);
    let mut acc = vec![[0u64; 3]; WIDTH * HEIGHT];
    let mut hist = vec![[0u64; 3]; RIGHT - LEFT + 1];
    let (mut visible, mut transparent, mut invalid, mut excursions) = (0u64, 0u64, 0u64, 0u64);
    let (mut under, mut over) = ([0u64; 4], [0u64; 4]);
    let (mut lo, mut hi) = ([f64::INFINITY; 4], [f64::NEG_INFINITY; 4]);
    for (i, p) in frame.d.chunks_exact(4).enumerate() {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        let (rgb, values) = match sample(p, &o) {
            Sample::Invalid => {
                invalid += 1;
                continue;
            }
            Sample::Transparent => {
                transparent += 1;
                continue;
            }
            Sample::Visible(rgb, values) => (rgb, values),
        };
        visible += 1;
        for j in 0..4 {
            lo[j] = lo[j].min(values[j]);
            hi[j] = hi[j].max(values[j]);
            under[j] += u64::from(values[j] < 0.);
            over[j] += u64::from(values[j] > o.ceiling);
        }
        let x = i % frame.w;
        match o.kind {
            Kind::Waveform => {
                let x = LEFT + scaled(x, frame.w, RIGHT - LEFT);
                let y = level_y(values[3], o.ceiling);
                acc[y * WIDTH + x][1] += 1;
            }
            Kind::Parade => {
                for j in 0..3 {
                    let panel = (RIGHT - LEFT + 1) / 3;
                    let x = LEFT + j * panel + scaled(x, frame.w, panel - 1);
                    let y = level_y(values[j], o.ceiling);
                    acc[y * WIDTH + x][j] += 1;
                }
            }
            Kind::Vectorscope => {
                let c = o.cbcr(rgb);
                if c.iter().any(|v| v.abs() > 0.5) {
                    excursions += 1;
                }
                let (x, y) = vector_xy(c);
                for j in 0..3 {
                    acc[y * WIDTH + x][j] += 1;
                }
            }
            Kind::Histogram => {
                for j in 0..3 {
                    let bin = ((values[j] / o.ceiling).clamp(0., 1.) * (hist.len() - 1) as f64)
                        .round() as usize;
                    hist[bin][j] += 1;
                }
            }
        }
    }
    if o.kind == Kind::Histogram {
        let peak = hist.iter().flatten().copied().max().unwrap_or(0).max(1);
        for (x, b) in hist.iter().enumerate() {
            if x % 64 == 0 {
                crate::guard::check();
            }
            for j in 0..3 {
                let height = if b[j] == 0 {
                    0
                } else {
                    ((b[j] as f64 / peak as f64 * (BOTTOM - TOP) as f64).round() as usize).max(1)
                };
                for dy in 0..height {
                    acc[(BOTTOM - dy) * WIDTH + LEFT + x][j] = 1;
                }
            }
        }
    }
    let mut image = vec![0u8; WIDTH * HEIGHT * 3];
    for pixel in image.chunks_exact_mut(3) {
        pixel.copy_from_slice(&[12, 17, 23]);
    }
    let name = match o.kind {
        Kind::Waveform => "LUMA WAVEFORM",
        Kind::Parade => "RGB PARADE",
        Kind::Vectorscope => "VECTORSCOPE CB CR",
        Kind::Histogram => "RGB HISTOGRAM",
    };
    let gamut = if o.gamut == Gamut::Rec709 {
        "REC709"
    } else {
        "REC2020"
    };
    let unit = if o.units == Units::Ire { "IRE" } else { "NITS" };
    text(
        &mut image,
        LEFT,
        15,
        &format!("{name}  {gamut}  {:?}  {unit}", o.encoding).to_uppercase(),
        [194, 211, 227],
    );
    if o.kind == Kind::Vectorscope {
        vector_grid(&mut image, o);
    } else {
        for fraction in [0., 0.25, 0.5, 0.75, 1.] {
            let v = fraction * o.ceiling;
            if o.kind == Kind::Histogram {
                let x = LEFT + ((RIGHT - LEFT) as f64 * fraction).round() as usize;
                line(&mut image, x, TOP, x, BOTTOM, [40, 50, 64]);
                text(
                    &mut image,
                    x.saturating_sub(10),
                    BOTTOM + 10,
                    &format!("{v:.0}"),
                    [148, 165, 184],
                );
            } else {
                let y = level_y(v, o.ceiling);
                line(&mut image, LEFT, y, RIGHT, y, [40, 50, 64]);
                text(
                    &mut image,
                    5,
                    y.saturating_sub(3),
                    &format!("{v:.0}"),
                    [148, 165, 184],
                );
            }
        }
        if o.kind == Kind::Histogram {
            for fraction in [0., 0.25, 0.5, 0.75, 1.] {
                let y = level_y(fraction, 1.);
                line(&mut image, LEFT, y, RIGHT, y, [40, 50, 64]);
                text(
                    &mut image,
                    5,
                    y.saturating_sub(3),
                    &format!("{:.0}", fraction * 100.),
                    [148, 165, 184],
                );
            }
        }
        if o.kind == Kind::Parade {
            for j in 0..3 {
                let x = LEFT + j * (RIGHT - LEFT + 1) / 3;
                line(&mut image, x, TOP, x, BOTTOM, [70, 80, 90]);
                text(
                    &mut image,
                    x + 10,
                    TOP + 6,
                    ["R", "G", "B"][j],
                    [170, 184, 198],
                );
            }
        }
    }
    let peak = acc.iter().flatten().copied().max().unwrap_or(0).max(1) as f64;
    for (i, a) in acc.iter().enumerate() {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        for j in 0..3 {
            if a[j] > 0 {
                image[i * 3 + j] =
                    image[i * 3 + j].max((80. + 175. * (a[j] as f64 / peak).sqrt()).round() as u8);
            }
        }
    }
    text(
        &mut image,
        LEFT,
        HEIGHT - 17,
        &format!("VISIBLE {visible}  INVALID {invalid}  TRANSPARENT {transparent}"),
        [148, 165, 184],
    );
    if o.kind == Kind::Histogram {
        text(
            &mut image,
            LEFT,
            TOP - 14,
            "COUNT NORMALIZED TO GLOBAL RGB PEAK",
            [148, 165, 184],
        );
    }
    let metadata = json!({"width":WIDTH,"height":HEIGHT,"kind":format!("{:?}",o.kind),"gamut":gamut,"encoding":format!("{:?}",o.encoding),"units":unit,"ceiling":o.ceiling,
        "histogram_peak_count":if o.kind==Kind::Histogram {Some(hist.iter().flatten().copied().max().unwrap_or(0))}else{None},"visible_pixels":visible,"invalid_pixels":invalid,"transparent_pixels":transparent,"channel_order":["r","g","b","luma_or_luminance"],"under_range":under,"over_range":over,
        "min":if visible>0{Some(lo)}else{None},"max":if visible>0{Some(hi)}else{None},"vector_outside_grid_pixels":excursions,
        "reference_white_nits":o.reference_white_nits,"hlg_peak_nits":o.hlg_peak_nits,"hlg_system_gamma":o.hlg_system_gamma,
        "sampling":"all valid visible pixels, unweighted","vector_axes":"encoded Cb horizontal, Cr vertical positive up, -0.5..0.5; 100% RGB primary/secondary targets",
        "note":"IRE is full-range encoded signal*100, not legal YUV. PQ nits use ST2084. HLG uses declared peak and system-gamma luminance OOTF. SDR/linear nits use declared reference white. Histogram global peak normalization; excursions count before plot clamp. No calibrated HDR/broadcast certification."});
    Ok(ScopeImage {
        png: png_rgb(&image),
        metadata,
    })
}
const TARGETS: [(&str, [f64; 3]); 6] = [
    ("R", [1., 0., 0.]),
    ("Y", [1., 1., 0.]),
    ("G", [0., 1., 0.]),
    ("C", [0., 1., 1.]),
    ("B", [0., 0., 1.]),
    ("M", [1., 0., 1.]),
];
/// Grid cell [row, col]: row 0 is Cr +0.5 (top), col 0 is Cb -0.5 (left), clamped to the grid.
fn vector_cell(c: [f64; 2], grid: usize) -> [usize; 2] {
    let g = grid as f64;
    [
        ((0.5 - c[1]) * g).floor().clamp(0., g - 1.) as usize,
        ((c[0] + 0.5) * g).floor().clamp(0., g - 1.) as usize,
    ]
}
/// Exact integer scope counts with the same pixel selection and level mapping as `render`.
/// Levels span 0..ceiling; out-of-range values land in the edge bins (the plot clamp) and are
/// also counted in under/over. The vectorscope needs IRE mode (encoded CbCr), else it is null.
pub fn data(
    frame: &Frame,
    o: Options,
    bins: usize,
    columns: usize,
    grid: usize,
) -> Result<Value, String> {
    o.validate()?;
    if !(16..=4096).contains(&bins) {
        return Err("bins must be 16..4096".into());
    }
    if !(1..=4096).contains(&columns) {
        return Err("columns must be 1..4096".into());
    }
    if !(16..=1024).contains(&grid) {
        return Err("grid must be 16..1024".into());
    }
    check_frame(frame)?;
    let vector = o.units == Units::Ire;
    // Counts plus their JSON values and headroom.
    let cells = columns * bins * 4 + bins * 4 + if vector { grid * grid } else { 0 };
    crate::guard::reserve(cells * 56 + 65536);
    let (min, max) = (0., o.ceiling);
    let bin = |v: f64| {
        ((v - min) / (max - min) * (bins - 1) as f64 + 0.5)
            .floor()
            .clamp(0., (bins - 1) as f64) as usize
    };
    let mut wave = vec![0u64; columns * bins];
    let mut parade = vec![vec![0u64; columns * bins]; 3];
    let mut hist = vec![vec![0u64; bins]; 4];
    let mut vec_counts = vec![0u64; if vector { grid * grid } else { 0 }];
    let (mut visible, mut transparent, mut invalid, mut excursions) = (0u64, 0u64, 0u64, 0u64);
    let (mut under, mut over) = ([0u64; 4], [0u64; 4]);
    for (i, p) in frame.d.chunks_exact(4).enumerate() {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        let (rgb, values) = match sample(p, &o) {
            Sample::Invalid => {
                invalid += 1;
                continue;
            }
            Sample::Transparent => {
                transparent += 1;
                continue;
            }
            Sample::Visible(rgb, values) => (rgb, values),
        };
        visible += 1;
        let col = ((i % frame.w) as u128 * columns as u128 / frame.w as u128) as usize;
        for j in 0..4 {
            under[j] += u64::from(values[j] < min);
            over[j] += u64::from(values[j] > max);
            hist[j][bin(values[j])] += 1;
        }
        wave[col * bins + bin(values[3])] += 1;
        for j in 0..3 {
            parade[j][col * bins + bin(values[j])] += 1;
        }
        if vector {
            let c = o.cbcr(rgb);
            if c.iter().any(|v| v.abs() > 0.5) {
                excursions += 1;
            }
            let [row, col] = vector_cell(c, grid);
            vec_counts[row * grid + col] += 1;
        }
    }
    crate::guard::check();
    let rows = |v: &[u64], n: usize| v.chunks(n).map(<[u64]>::to_vec).collect::<Vec<_>>();
    let per = |a: [u64; 4]| json!({"r":a[0],"g":a[1],"b":a[2],"luma":a[3]});
    let vectorscope = if vector {
        let targets = |scale: f64| {
            let mut m = serde_json::Map::new();
            for (label, rgb) in TARGETS {
                let c = o.cbcr(rgb.map(|v| v * scale));
                m.insert(label.into(), json!({"cbcr":c,"cell":vector_cell(c, grid)}));
            }
            Value::Object(m)
        };
        json!({"grid":grid,"counts":rows(&vec_counts,grid),
            "axes":"row 0 = Cr +0.5 (top), col 0 = Cb -0.5 (left); col = floor((cb+0.5)*grid), row = floor((0.5-cr)*grid), clamped",
            "targets":{"100":targets(1.),"75":targets(0.75)},"skin_line_deg":123.0,"skin_line_note":"I-line, angle from +Cb counter-clockwise",
            "outside_grid_pixels":excursions})
    } else {
        Value::Null
    };
    Ok(json!({
        "levels":{"units":if o.units==Units::Ire{"IRE"}else{"NITS"},"min":min,"max":max,"bins":bins,
            "bin":"floor((v-min)/(max-min)*(bins-1)+0.5) clamped; out-of-range values sit in the edge bins and are also counted in under/over",
            "under":per(under),"over":per(over)},
        "waveform":{"columns":columns,"column":"x*columns/width","luma_weights":o.weights(),"counts":rows(&wave,bins)},
        "parade":{"columns":columns,"r":rows(&parade[0],bins),"g":rows(&parade[1],bins),"b":rows(&parade[2],bins)},
        "histogram":{"r":hist[0],"g":hist[1],"b":hist[2],"luma":hist[3]},
        "vectorscope":vectorscope,
        "pixels":{"visible":visible,"invalid":invalid,"transparent":transparent},
        "vectorscope_note":if vector{Value::Null}else{json!("vectorscope requires IRE mode: encoded CbCr, not nits")}
    }))
}
fn scaled(x: usize, n: usize, span: usize) -> usize {
    if n <= 1 {
        span / 2
    } else {
        (x as u128 * span as u128 / (n - 1) as u128) as usize
    }
}
fn level_y(v: f64, ceiling: f64) -> usize {
    BOTTOM - ((v / ceiling).clamp(0., 1.) * (BOTTOM - TOP) as f64).round() as usize
}
fn vector_xy(c: [f64; 2]) -> (usize, usize) {
    let center_x = (LEFT + RIGHT) / 2;
    let center_y = (TOP + BOTTOM) / 2;
    let radius = (BOTTOM - TOP) / 2;
    (
        (center_x as f64 + c[0].clamp(-0.5, 0.5) * 2. * radius as f64).round() as usize,
        (center_y as f64 - c[1].clamp(-0.5, 0.5) * 2. * radius as f64).round() as usize,
    )
}
fn pixel(image: &mut [u8], x: usize, y: usize, c: [u8; 3]) {
    if x < WIDTH && y < HEIGHT {
        image[(y * WIDTH + x) * 3..(y * WIDTH + x) * 3 + 3].copy_from_slice(&c);
    }
}
fn line(image: &mut [u8], x0: usize, y0: usize, x1: usize, y1: usize, c: [u8; 3]) {
    let n = x0.abs_diff(x1).max(y0.abs_diff(y1)).max(1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        pixel(
            image,
            (x0 as f64 + (x1 as f64 - x0 as f64) * t).round() as usize,
            (y0 as f64 + (y1 as f64 - y0 as f64) * t).round() as usize,
            c,
        );
    }
}
fn vector_grid(image: &mut [u8], o: Options) {
    let center = vector_xy([0., 0.]);
    let radius = (BOTTOM - TOP) / 2;
    line(
        image,
        center.0 - radius,
        center.1,
        center.0 + radius,
        center.1,
        [55, 65, 80],
    );
    line(
        image,
        center.0,
        center.1 - radius,
        center.0,
        center.1 + radius,
        [55, 65, 80],
    );
    for r in [0.25, 0.5] {
        let mut prev = vector_xy([r, 0.]);
        for k in 1..=360 {
            let a = (k as f64).to_radians();
            let xy = vector_xy([r * a.cos(), r * a.sin()]);
            line(image, prev.0, prev.1, xy.0, xy.1, [55, 65, 80]);
            prev = xy;
        }
    }
    for (label, rgb, c) in [
        ("R", [1., 0., 0.], [255, 90, 90]),
        ("Y", [1., 1., 0.], [235, 235, 90]),
        ("G", [0., 1., 0.], [90, 255, 90]),
        ("C", [0., 1., 1.], [90, 235, 235]),
        ("B", [0., 0., 1.], [100, 130, 255]),
        ("M", [1., 0., 1.], [235, 90, 235]),
    ] {
        let (x, y) = vector_xy(o.cbcr(rgb));
        line(image, x - 3, y - 3, x + 3, y - 3, c);
        line(image, x - 3, y + 3, x + 3, y + 3, c);
        line(image, x - 3, y - 3, x - 3, y + 3, c);
        line(image, x + 3, y - 3, x + 3, y + 3, c);
        text(image, x + 7, y.saturating_sub(3), label, c);
        if o.targets_75 {
            // Half-size box, half brightness, no label.
            let (x, y) = vector_xy(o.cbcr(rgb.map(|v| v * 0.75)));
            let c = c.map(|v| v / 2);
            line(image, x - 2, y - 2, x + 2, y - 2, c);
            line(image, x - 2, y + 2, x + 2, y + 2, c);
            line(image, x - 2, y - 2, x - 2, y + 2, c);
            line(image, x + 2, y - 2, x + 2, y + 2, c);
        }
    }
    text(
        image,
        center.0 + radius + 10,
        center.1,
        "CB",
        [148, 165, 184],
    );
    text(image, center.0 + 8, TOP + 5, "CR", [148, 165, 184]);
}
// Compact native 5x7 bitmap font; no system fonts or encoder subprocesses.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        _ => [0; 7],
    }
}
fn text(image: &mut [u8], x: usize, y: usize, s: &str, c: [u8; 3]) {
    for (i, ch) in s.chars().enumerate() {
        for (dy, row) in glyph(ch).iter().enumerate() {
            for dx in 0..5 {
                if row & (1 << (4 - dx)) != 0 {
                    pixel(image, x + i * 6 + dx, y + dy, c);
                }
            }
        }
    }
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in data {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}
/// PNG RGB8, filter0, zlib stored blocks. Deterministic and dependency-free.
fn png_rgb(rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + HEIGHT);
    for row in rgb.chunks_exact(WIDTH * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let chunks = raw.len().div_ceil(65535);
    for (i, block) in raw.chunks(65535).enumerate() {
        crate::guard::check();
        z.push(u8::from(i + 1 == chunks));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for v in raw {
        a = (a + v as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&(WIDTH as u32).to_be_bytes());
    header.extend_from_slice(&(HEIGHT as u32).to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    // Decode our stored-block PNG for raster assertions (Pillow independently validates fixtures).
    fn rgb_from_png(png: &[u8]) -> Vec<u8> {
        let n = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        assert_eq!(&png[37..41], b"IDAT");
        let z = &png[41..41 + n];
        let mut offset = 2;
        let mut scan = vec![];
        loop {
            let final_block = z[offset] & 1;
            offset += 1;
            let len = u16::from_le_bytes(z[offset..offset + 2].try_into().unwrap()) as usize;
            offset += 4;
            scan.extend_from_slice(&z[offset..offset + len]);
            offset += len;
            if final_block == 1 {
                break;
            }
        }
        let mut out = vec![];
        for row in scan.chunks_exact(WIDTH * 3 + 1) {
            assert_eq!(row[0], 0);
            out.extend_from_slice(&row[1..]);
        }
        out
    }
    #[test]
    fn known_pixels_land_on_correct_plot_coordinates() {
        let f = frame([1., 0., 0.]);
        for kind in [
            Kind::Waveform,
            Kind::Parade,
            Kind::Vectorscope,
            Kind::Histogram,
        ] {
            let o = Options::sdr(kind);
            let out = render(&f, o).unwrap();
            let rgb = rgb_from_png(&out.png);
            let points = match kind {
                Kind::Waveform => vec![(LEFT + (RIGHT - LEFT) / 2, level_y(21.26, 100.), 1)],
                Kind::Parade => {
                    let panel = (RIGHT - LEFT + 1) / 3;
                    vec![
                        (LEFT + (panel - 1) / 2, TOP, 0),
                        (LEFT + panel + (panel - 1) / 2, BOTTOM, 1),
                        (LEFT + 2 * panel + (panel - 1) / 2, BOTTOM, 2),
                    ]
                }
                Kind::Vectorscope => {
                    let (x, y) = vector_xy(o.cbcr([1., 0., 0.]));
                    vec![(x, y, 0), (x, y, 1), (x, y, 2)]
                }
                Kind::Histogram => vec![(RIGHT, BOTTOM, 0), (LEFT, BOTTOM, 1), (LEFT, BOTTOM, 2)],
            };
            for (x, y, c) in points {
                assert_eq!(rgb[(y * WIDTH + x) * 3 + c], 255, "{kind:?} {x} {y} {c}");
            }
            if kind == Kind::Histogram {
                assert_eq!(out.metadata["histogram_peak_count"], 1);
            }
        }
    }
    fn frame(rgb: [f32; 3]) -> Frame {
        Frame {
            w: 1,
            h: 1,
            d: vec![rgb[0], rgb[1], rgb[2], 1.],
        }
    }
    fn near(a: f64, b: f64, t: f64) {
        assert!((a - b).abs() < t, "{a} vs {b}");
    }
    #[test]
    fn primary_vector_angles_and_neutral() {
        let o = Options::sdr(Kind::Vectorscope);
        let red = o.cbcr([1., 0., 0.]);
        near(red[0], -0.114572106, 1e-8);
        near(red[1], 0.5, 1e-12);
        near(red[1].atan2(red[0]).to_degrees(), 102.907, 0.01);
        assert_eq!(o.cbcr([0.5; 3]), [0., 0.]);
        let (x, y) = vector_xy(red);
        assert!(x < (LEFT + RIGHT) / 2);
        assert_eq!(y, TOP);
        let mut wide = o;
        wide.gamut = Gamut::Rec2020;
        near(wide.cbcr([1., 0., 0.])[0], -0.13963006, 1e-8);
        assert_ne!(vector_xy(wide.cbcr([1., 0., 0.])), vector_xy(red));
    }
    #[test]
    fn known_sdr_luma_and_histogram_counts() {
        for gamut in [Gamut::Rec709, Gamut::Rec2020] {
            let mut o = Options::sdr(Kind::Histogram);
            o.gamut = gamut;
            let image = render(&frame([1., 0., 0.]), o).unwrap();
            near(
                image.metadata["min"][3].as_f64().unwrap(),
                if gamut == Gamut::Rec709 { 21.26 } else { 26.27 },
                1e-8,
            );
            assert_eq!(image.metadata["visible_pixels"], 1);
            assert_eq!(image.metadata["over_range"], json!([0, 0, 0, 0]));
        }
    }
    #[test]
    fn pq_absolute_luminance() {
        let mut o = Options::sdr(Kind::Waveform);
        o.encoding = Encoding::Pq;
        o.units = Units::Nits;
        o.ceiling = 10000.;
        near(o.values([1.; 3])[3], 10000., 1e-6);
        near(o.values([0.508078421517399; 3])[3], 100., 1e-6);
        near(o.values([0.751827096247041; 3])[3], 1000., 1e-5);
        o.reference_white_nits = 203.;
        near(o.values([0.751827096247041; 3])[3], 1000., 1e-5);
        near(o.values([0.; 3])[3], 0., 1e-12);
    }
    #[test]
    fn hlg_ootf_declared_display_not_scene_nits() {
        let mut o = Options::sdr(Kind::Waveform);
        o.encoding = Encoding::Hlg;
        o.units = Units::Nits;
        o.ceiling = 1000.;
        near(o.values([1.; 3])[3], 1000., 0.001);
        near(o.values([0.5; 3])[3], 1000. * (1f64 / 12.).powf(1.2), 1e-9);
        near(o.values([0.75; 3])[3], 203.15214594, 0.001);
        o.hlg_peak_nits = 2000.;
        near(o.values([1.; 3])[3], 2000., 0.001);
    }
    #[test]
    fn shape_limits_scales_and_nonfinite_are_safe() {
        let o = Options::sdr(Kind::Waveform);
        for f in [
            Frame {
                w: 0,
                h: 1,
                d: vec![],
            },
            Frame {
                w: usize::MAX,
                h: 2,
                d: vec![],
            },
            Frame {
                w: 1,
                h: 1,
                d: vec![0.; 3],
            },
        ] {
            assert!(render(&f, o).is_err());
        }
        for ceiling in [0., -1., f64::NAN, f64::INFINITY, 10001.] {
            let mut bad = o;
            bad.ceiling = ceiling;
            assert!(render(&frame([0.; 3]), bad).is_err());
        }
        let mut f = frame([0.; 3]);
        f.d[0] = f32::NAN;
        let out = render(&f, o).unwrap();
        assert_eq!(out.metadata["invalid_pixels"], 1);
        assert_eq!(out.metadata["min"], Value::Null);
        f.d = vec![1., 0., 0., 0.];
        assert_eq!(render(&f, o).unwrap().metadata["transparent_pixels"], 1);
        f.d = vec![-1., 2., 0., 1.];
        let out = render(&f, o).unwrap();
        assert_eq!(out.metadata["under_range"][0], 1);
        assert_eq!(out.metadata["over_range"][1], 1);
        let mut pq = o;
        pq.encoding = Encoding::Pq;
        assert_eq!(render(&f, pq).unwrap().metadata["invalid_pixels"], 1);
        let mut linear = o;
        linear.encoding = Encoding::Linear;
        assert!(render(&frame([0.; 3]), linear).is_err());
        let mut vector = Options::sdr(Kind::Vectorscope);
        vector.units = Units::Nits;
        assert!(render(&frame([0.; 3]), vector).is_err());
    }
    #[test]
    fn all_scope_png_bytes_deterministic() {
        for kind in [
            Kind::Waveform,
            Kind::Parade,
            Kind::Vectorscope,
            Kind::Histogram,
        ] {
            let o = Options::sdr(kind);
            let a = render(&frame([1., 0., 0.]), o).unwrap();
            let b = render(&frame([1., 0., 0.]), o).unwrap();
            assert_eq!(a.png, b.png);
            assert_eq!(a.metadata, b.metadata);
            assert_eq!(&a.png[..8], b"\x89PNG\r\n\x1a\n");
        }
    }
    #[test]
    fn sample_examples() {
        // Opt-in fixture output only. A normal test run has no file writes.
        let Ok(dir) = std::env::var("KIMO_SCOPE_FIXTURES") else {
            return;
        };
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = Frame {
            w: 256,
            h: 96,
            d: vec![0.; 256 * 96 * 4],
        };
        let bars = [
            [1., 1., 1.],
            [1., 1., 0.],
            [0., 1., 1.],
            [0., 1., 0.],
            [1., 0., 1.],
            [1., 0., 0.],
            [0., 0., 1.],
            [0., 0., 0.],
        ];
        for y in 0..96 {
            for x in 0..256 {
                let p = (y * 256 + x) * 4;
                let rgb = if y < 48 {
                    bars[x / 32]
                } else {
                    [x as f32 / 255.; 3]
                };
                f.d[p..p + 3].copy_from_slice(&rgb);
                f.d[p + 3] = 1.;
            }
        }
        for (name, kind) in [
            ("waveform", Kind::Waveform),
            ("parade", Kind::Parade),
            ("vectorscope", Kind::Vectorscope),
            ("histogram", Kind::Histogram),
        ] {
            let image = render(&f, Options::sdr(kind)).unwrap();
            std::fs::write(format!("{dir}/{name}.png"), image.png).unwrap();
        }
        let mut pq = Options::sdr(Kind::Waveform);
        pq.gamut = Gamut::Rec2020;
        pq.encoding = Encoding::Pq;
        pq.units = Units::Nits;
        pq.ceiling = 1000.;
        for x in 0..f.w {
            for y in 0..f.h {
                let value =
                    super::super::managed::encode(6, x as f64 / (f.w - 1) as f64 * 10., 100.)
                        as f32;
                let p = (y * f.w + x) * 4;
                f.d[p..p + 3].fill(value);
            }
        }
        std::fs::write(
            format!("{dir}/pq-waveform.png"),
            render(&f, pq).unwrap().png,
        )
        .unwrap();
        let mut hlg = pq;
        hlg.encoding = Encoding::Hlg;
        for x in 0..f.w {
            for y in 0..f.h {
                let v = x as f32 / (f.w - 1) as f32;
                let p = (y * f.w + x) * 4;
                f.d[p..p + 3].fill(v);
            }
        }
        std::fs::write(
            format!("{dir}/hlg-waveform.png"),
            render(&f, hlg).unwrap().png,
        )
        .unwrap();
    }
}
