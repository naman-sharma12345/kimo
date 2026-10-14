# Static source evidence

Checkpoint 273cf0b88c1d6d3d5bceb9af16ca7a1a5ef02384. Kimo original source only, no third-party implementation material.

## lighting.rs:15-35

```rust
15: #[derive(Clone, Debug, Serialize, Deserialize)]
16: pub struct LightDef {
17:     #[serde(default = "dkind")] pub kind: String, // directional | point | spot
18:     #[serde(default = "dpos")] pub pos: [f64; 3],
19:     /// Direction the light travels (directional and spot). For a point light this is ignored.
20:     #[serde(default)] pub dir: [f64; 3],
21:     #[serde(default = "dw")] pub color: [f64; 3], #[serde(default = "d1")] pub intensity: f64,
22:     /// Distance falloff: 0 = none, 1 = inverse-square from 1 frame-height away.
23:     #[serde(default)] pub falloff: f64,
24:     #[serde(default = "dcone")] pub cone: f64, #[serde(default = "dsoft")] pub softness: f64,
25:     #[serde(default = "dtrue")] pub enabled: bool, #[serde(default)] pub keys: Vec<LightKey>,
26: }
27: #[derive(Clone, Debug, Serialize, Deserialize)]
28: pub struct Material {
29:     /// None = use the layer's own colour as albedo.
30:     #[serde(default)] pub albedo: Option<[f64; 3]>, #[serde(default = "dr")] pub roughness: f64, #[serde(default)] pub metal: f64,
31:     #[serde(default = "dior")] pub ior: f64, #[serde(default)] pub clearcoat: f64, #[serde(default)] pub emissive: [f64; 3],
32: }
33: impl Default for Material { fn default() -> Self { Material { albedo: None, roughness: 0.5, metal: 0.0, ior: 1.5, clearcoat: 0.0, emissive: [0.0; 3] } } }
34: pub fn presets() -> Vec<(&'static str, Material)> {
35:     let m = |a: Option<[f64; 3]>, r, me, ior, cc| Material { albedo: a, roughness: r, metal: me, ior, clearcoat: cc, emissive: [0.0; 3] };
```

## lighting.rs:44-55

```rust
44: pub struct Scene { pub lights: Vec<(String, LightDef)>, pub materials: BTreeMap<String, Material> }
45: thread_local! { static SCENE: RefCell<Option<Rc<Scene>>> = Default::default(); }
46: pub fn scene() -> Option<Rc<Scene>> { SCENE.with(|s| s.borrow().clone()) }
47: pub fn set_scene(s: Option<Rc<Scene>>) { SCENE.with(|c| *c.borrow_mut() = s); }
48: pub fn eval_light(l: &LightDef, t: f64) -> LightDef {
49:     let mut o = l.clone(); if l.keys.is_empty() { return o; }
50:     let mut ks: Vec<&LightKey> = l.keys.iter().collect(); ks.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
51:     let (mut pa, mut ia) = (l.pos, l.intensity); let mut prev: Option<(f64, [f64; 3], f64)> = None;
52:     for k in &ks { let kp = k.pos.unwrap_or(pa); let ki = k.intensity.unwrap_or(ia);
53:         if t <= k.t { match prev { Some((pt, pp, pi)) if k.t > pt => { let u = ((t - pt) / (k.t - pt)).clamp(0.0, 1.0); let u = u * u * (3.0 - 2.0 * u); for j in 0..3 { o.pos[j] = pp[j] + (kp[j] - pp[j]) * u; } o.intensity = pi + (ki - pi) * u; } _ => { o.pos = kp; o.intensity = ki; } } return o; }
54:         prev = Some((k.t, kp, ki)); pa = kp; ia = ki; }
55:     if let Some((_, pp, pi)) = prev { o.pos = pp; o.intensity = pi; } o
```

## lighting.rs:77-87

```rust
77:     if m.albedo.map_or(false, |a| a.iter().any(|x| !x.is_finite() || *x < 0.0 || *x > 4.0)) || m.emissive.iter().any(|x| !x.is_finite() || *x < 0.0) { return Err("albedo/emissive must be finite and >= 0".into()); }
78:     Ok(())
79: }
80:
81: fn norm(v: [f64; 3]) -> [f64; 3] { let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12); [v[0] / m, v[1] / m, v[2] / m] }
82: fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
83:
84: /// Cook-Torrance BRDF value times n.l for one light. Returns (diffuse weight, specular weight); the caller multiplies diffuse by albedo.
85: pub fn brdf(n: [f64; 3], l: [f64; 3], v: [f64; 3], m: &Material, albedo: [f64; 3]) -> ([f64; 3], [f64; 3]) {
86:     let ndl = dot(n, l).max(0.0); if ndl <= 0.0 { return ([0.0; 3], [0.0; 3]); }
87:     let h = norm([l[0] + v[0], l[1] + v[1], l[2] + v[2]]); let ndh = dot(n, h).max(0.0); let ndv = dot(n, v).max(1e-3); let vdh = dot(v, h).max(0.0);
```

## lighting.rs:102-114

```rust
102:             for y in 0..h { if y + r < h { acc += t[(y + r) * w + x]; cnt += 1.0; } if y > r { acc -= t[(y - r - 1) * w + x]; cnt -= 1.0; } a[y * w + x] = acc / cnt; } } }
103: }
104:
105: /// Lighting vector, attenuation and cone factor for pixel position `pt` (frame-height units, z = surface height).
106: pub fn light_at(l: &LightDef, pt: [f64; 3]) -> ([f64; 3], f64) {
107:     match l.kind.as_str() {
108:         "directional" => { let d = norm(l.dir); ([-d[0], -d[1], -d[2]], 1.0) }
109:         _ => { let v = [l.pos[0] - pt[0], l.pos[1] - pt[1], l.pos[2] - pt[2]]; let dist = dot(v, v).sqrt().max(1e-6); let ln = [v[0] / dist, v[1] / dist, v[2] / dist];
110:             let mut at = if l.falloff > 0.0 { 1.0 / (1.0 + l.falloff * dist * dist) } else { 1.0 };
111:             if l.kind == "spot" { let d = norm(l.dir); let c = dot([-ln[0], -ln[1], -ln[2]], d).clamp(-1.0, 1.0).acos().to_degrees(); let inner = l.cone * (1.0 - l.softness); at *= if c <= inner { 1.0 } else if c >= l.cone { 0.0 } else { let u = (l.cone - c) / (l.cone - inner).max(1e-6); u * u * (3.0 - 2.0 * u) }; }
112:             (ln, at) }
113:     }
114: }
```

## scene3d.rs:10-23

```rust
10: pub struct Key { pub t: f64, #[serde(default)] pub pos: Option<[f64; 3]>, #[serde(default)] pub rot: Option<[f64; 3]>, #[serde(default)] pub scale: Option<[f64; 3]> }
11: #[derive(Clone, Debug, Serialize, Deserialize)]
12: pub struct Obj {
13:     #[serde(default = "dkind")] pub kind: String, // box | rbox | cylinder | capsule | sphere | plane | null
14:     #[serde(default = "dsz")] pub size: [f64; 3], #[serde(default)] pub round: f64, // rbox corner radius
15:     #[serde(default = "d3")] pub pos: [f64; 3], #[serde(default = "d3")] pub rot: [f64; 3], #[serde(default = "d1")] pub scale: [f64; 3],
16:     #[serde(default)] pub parent: Option<usize>, #[serde(default)] pub keys: Vec<Key>,
17:     #[serde(default = "dcol")] pub color: [f64; 3], #[serde(default = "dr")] pub rough: f64, #[serde(default)] pub metal: f64, #[serde(default = "d3")] pub emissive: [f64; 3], #[serde(default = "dop")] pub opacity: f64,
18:     #[serde(default)] pub flat: bool,
19: }
20: fn dlk() -> String { "directional".into() } fn dli() -> f64 { 1.0 }
21: #[derive(Clone, Debug, Serialize, Deserialize)]
22: pub struct Light { #[serde(default = "dlk")] pub kind: String, #[serde(default = "d3")] pub dir: [f64; 3], #[serde(default = "d3")] pub pos: [f64; 3], #[serde(default = "d1")] pub color: [f64; 3], #[serde(default = "dli")] pub intensity: f64 }
23: fn dam() -> [f64; 3] { [0.08, 0.09, 0.11] }
```

## scene3d.rs:98-100

```rust
98: /// premultiplied results. Object animation and camera moves (via `cam_at`, which gets the offset in seconds) both blur, because each sample re-poses the whole scene.
99: /// shutter_deg = 0 renders once, identical to `render`.
100: pub fn render_blur(sc: &Scene3D, f: &mut Frame, t: f64, fps: f64, cam_at: &dyn Fn(f64) -> Camera) {
```

## scene3d.rs:116-139

```rust
116:     for (i, o) in sc.objects.iter().enumerate() { if o.kind == "null" { continue; } let (rm, wp, ws) = world(&sc.objects, i, t, 0); let tris = mesh(o);
117:         let tr = |p: V3| { let q = [p[0] * ws[0], p[1] * ws[1], p[2] * ws[2]]; let r = mv(rm, q); [r[0] + wp[0], r[1] + wp[1], r[2] + wp[2]] };
118:         let tn = |n: V3| norm(mv(rm, [n[0] / ws[0].abs().max(1e-6), n[1] / ws[1].abs().max(1e-6), n[2] / ws[2].abs().max(1e-6)]));
119:         let wt: Vec<Tri> = tris.iter().map(|q| Tri { p: [tr(q.p[0]), tr(q.p[1]), tr(q.p[2])], n: [tn(q.n[0]), tn(q.n[1]), tn(q.n[2])] }).collect();
120:         draws.push(Draw { tris: wt, oi: i, zc: wp[2] - eye[2] }); }
121:     draws.sort_by(|a, b| { let (ao, bo) = (sc.objects[a.oi].opacity >= 0.999, sc.objects[b.oi].opacity >= 0.999); bo.cmp(&ao).then(b.zc.partial_cmp(&a.zc).unwrap_or(std::cmp::Ordering::Equal)).then(std::cmp::Ordering::Equal) });
122:     // opaque (true) first: cmp(bo,ao) puts opaque first; within group farther first (fine for z-buffer)
123:     let lights: Vec<(V3, V3, f64, bool)> = sc.lights.iter().map(|l| (if l.kind == "point" { l.pos } else { norm([-l.dir[0], -l.dir[1], -l.dir[2]]) }, l.color, l.intensity, l.kind == "point")).collect();
124:     let shadows: Vec<Option<Shadow>> = if sc.shadows { let all: Vec<&Tri> = draws.iter().filter(|d| sc.objects[d.oi].opacity >= 0.999).flat_map(|d| d.tris.iter()).collect(); lights.iter().map(|l| if l.3 { None } else { Some(Shadow::build(l.0, &all)) }).collect() } else { lights.iter().map(|_| None).collect() };
125:     for d in &draws { let o = &sc.objects[d.oi]; let opaque = o.opacity >= 0.999; let rough = o.rough.clamp(0.04, 1.0); let shin = (2.0 / (rough * rough) - 2.0).clamp(2.0, 2000.0); let f0 = |c: f64| 0.04 * (1.0 - o.metal) + c * o.metal;
126:         for tri in &d.tris { let mut sp = [(0.0, 0.0, 0.0); 3]; let mut ok = true; for k in 0..3 { match cv.project(tri.p[k]) { Some(q) => sp[k] = q, None => ok = false } } if !ok { continue; }
127:             let area = (sp[1].0 - sp[0].0) * (sp[2].1 - sp[0].1) - (sp[2].0 - sp[0].0) * (sp[1].1 - sp[0].1); if area.abs() < 1e-9 { continue; }
128:             let (x0, x1) = (sp[0].0.min(sp[1].0).min(sp[2].0).floor().max(0.0) as usize, (sp[0].0.max(sp[1].0).max(sp[2].0).ceil().min(sw as f64 - 1.0)).max(0.0) as usize); let (y0, y1) = (sp[0].1.min(sp[1].1).min(sp[2].1).floor().max(0.0) as usize, (sp[0].1.max(sp[1].1).max(sp[2].1).ceil().min(sh as f64 - 1.0)).max(0.0) as usize);
129:             if sp[0].0.max(sp[1].0).max(sp[2].0) < 0.0 || sp[0].1.max(sp[1].1).max(sp[2].1) < 0.0 { continue; }
130:             let iz = [1.0 / sp[0].2, 1.0 / sp[1].2, 1.0 / sp[2].2];
131:             for y in y0..=y1 { for x in x0..=x1 { let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
132:                 let w0 = ((sp[1].0 - px) * (sp[2].1 - py) - (sp[2].0 - px) * (sp[1].1 - py)) / area; let w1 = ((sp[2].0 - px) * (sp[0].1 - py) - (sp[0].0 - px) * (sp[2].1 - py)) / area; let w2 = 1.0 - w0 - w1;
133:                 if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 { continue; } let izp = w0 * iz[0] + w1 * iz[1] + w2 * iz[2]; let z = 1.0 / izp; let o_ = y * sw + x; if (z as f32) >= zb[o_] { continue; }
134:                 let (b0, b1, b2) = (w0 * iz[0] * z, w1 * iz[1] * z, w2 * iz[2] * z); let pw = [0, 1, 2].map(|a| b0 * tri.p[0][a] + b1 * tri.p[1][a] + b2 * tri.p[2][a]); let mut n = norm([0, 1, 2].map(|a| b0 * tri.n[0][a] + b1 * tri.n[1][a] + b2 * tri.n[2][a]));
135:                 if o.flat { n = norm(cross(sub(tri.p[1], tri.p[0]), sub(tri.p[2], tri.p[0]))); }
136:                 let vdir = norm(sub(eye, pw)); if dotv(n, vdir) < 0.0 { n = [-n[0], -n[1], -n[2]]; }
137:                 let nv = dotv(n, vdir).max(0.0); let mut col = [sc.ambient[0] * o.color[0], sc.ambient[1] * o.color[1], sc.ambient[2] * o.color[2]];
138:                 for (li_i, (lp, lc, li, point)) in lights.iter().enumerate() { let (l, att) = if *point { let dv = sub(*lp, pw); let d2 = dotv(dv, dv).max(1.0); (norm(dv), 1.0e5 / d2) } else { (*lp, 1.0) }; let ndl = dotv(n, l).max(0.0); if ndl <= 0.0 { continue; }
139:                     let hv = norm([l[0] + vdir[0], l[1] + vdir[1], l[2] + vdir[2]]); let ndh = dotv(n, hv).max(0.0); let vdh = dotv(vdir, hv).max(0.0); let fr = (1.0 - vdh).powi(5);
```

## scene3d.rs:147-159

```rust
147:     let ssn = (ss * ss) as f32; let mut tmp = vec![0f32; w * h * 4];
148:     for y in 0..h { for x in 0..w { let mut acc = [0f32; 4]; let mut zs = 0.0f32; let mut zn = 0f32; for sy in 0..ss { for sx in 0..ss { let o = ((y * ss + sy) * sw + x * ss + sx) * 4; for c in 0..4 { acc[c] += cb[o + c]; } let z = zb[(y * ss + sy) * sw + x * ss + sx]; if z.is_finite() { zs += z; zn += 1.0; } } }
149:         let o = (y * w + x) * 4; for c in 0..4 { tmp[o + c] = acc[c] / ssn; } zout[y * w + x] = if zn > 0.0 { zs / zn } else { 1e9 }; } }
150:     if sc.ao > 0.0 { let src = tmp.clone(); let taps: Vec<(f32, f32)> = (0..14).map(|i| { let a = i as f32 * 2.399963; let r = ((i as f32 + 0.5) / 14.0).sqrt(); (a.cos() * r, a.sin() * r) }).collect();
151:         for y in 0..h { for x in 0..w { let z = zout[y * w + x]; if z > 1e8 { continue; } let rad = (60.0 * (cv.focal() as f32 / ss as f32 / z) * (h as f32 / 720.0) * 12.0).clamp(3.0, 40.0); let mut occ = 0.0f32;
152:             for (tx, ty) in &taps { let (sx, sy) = (((x as f32 + tx * rad) as i32).clamp(0, w as i32 - 1) as usize, ((y as f32 + ty * rad) as i32).clamp(0, h as i32 - 1) as usize); let zn = zout[sy * w + sx]; let dz = z - zn; if dz > 4.0 && dz < 160.0 { occ += 1.0 - dz / 160.0; } }
153:             let k = 1.0 - (sc.ao as f32 * occ / 14.0 * 1.6).clamp(0.0, 0.85); let o = (y * w + x) * 4; for c in 0..3 { tmp[o + c] = src[o + c] * k; } } } }
154:     let focus = if sc.focus > 0.0 { sc.focus } else { cv.focal() };
155:     if sc.dof > 0.0 { let src = tmp.clone(); let taps: Vec<(f32, f32)> = (0..24).map(|i| { let a = i as f32 * 2.399963; let r = ((i as f32 + 0.5) / 24.0).sqrt(); (a.cos() * r, a.sin() * r) }).collect();
156:         for y in 0..h { for x in 0..w { let z = zout[y * w + x] as f64; let zz = if z > 1e8 { focus * 4.0 } else { z }; let coc = (sc.dof * (zz - focus).abs() / zz * fl * 0.04 * (h as f64 / 720.0)).min(40.0) as f32; if coc < 0.6 { continue; }
157:             let mut acc = [0f32; 4]; for (tx, ty) in &taps { let (sx, sy) = (((x as f32 + tx * coc).round() as i32).clamp(0, w as i32 - 1) as usize, ((y as f32 + ty * coc).round() as i32).clamp(0, h as i32 - 1) as usize); let o = (sy * w + sx) * 4; for c in 0..4 { acc[c] += src[o + c]; } }
158:             let o = (y * w + x) * 4; for c in 0..4 { tmp[o + c] = acc[c] / 24.0; } } } }
159:     for i in 0..w * h { let a = tmp[i * 4 + 3]; let o = i * 4; if a > 1e-4 { for c in 0..3 { f.d[o + c] = (tmp[o + c] / a).min(4.0); } f.d[o + 3] = a.min(1.0); } }
```

## three_d.rs:9-16

```rust
9: pub struct Camera { pub pos: [f64; 3], pub pan: f64, pub tilt: f64, pub roll: f64, pub fov: f64 }
10: impl Default for Camera { fn default() -> Self { Camera { pos: [0.0; 3], pan: 0.0, tilt: 0.0, roll: 0.0, fov: 50.0 } } }
11: impl Camera {
12:     pub fn from_props(m: &BTreeMap<String, Prop>, t: f64) -> Camera {
13:         let g = |k: &str, d: f64| m.get(k).map(|p| p.at(t)).unwrap_or(d);
14:         Camera { pos: [g("cam_x", 0.0), g("cam_y", 0.0), g("cam_z", 0.0)], pan: g("cam_pan", 0.0), tilt: g("cam_tilt", 0.0), roll: g("cam_roll", 0.0), fov: g("cam_fov", 50.0).clamp(5.0, 170.0) }
15:     }
16: }
```

## three_d.rs:32-35

```rust
32: impl Projector {
33:     pub fn new(w: usize, h: usize, cam: &Camera, pose: &Pose) -> Projector {
34:         let f = (h as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan(); let rc = mul(&ry(cam.pan), &mul(&rx(cam.tilt), &rz(cam.roll))); let r = rot_matrix(pose.rot);
35:         Projector { w, h, f, c: [cam.pos[0], cam.pos[1], cam.pos[2] - f], rc, p0: pose.pos, n: app(&r, [0.0, 0.0, -1.0]), r, s: pose.scale.max(1e-6) }
```

## three_d.rs:86-91

```rust
86:     pub fn new(w: usize, h: usize, cam: &Camera) -> CamView { let f = (h as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan(); let rc = mul(&ry(cam.pan), &mul(&rx(cam.tilt), &rz(cam.roll))); CamView { w, h, f, c: [cam.pos[0], cam.pos[1], cam.pos[2] - f], rct: tr(&rc) } }
87:     /// (screen x, screen y, view depth). None when behind the camera.
88:     pub fn project(&self, p: [f64; 3]) -> Option<(f64, f64, f64)> { let v = app(&self.rct, [p[0] - self.c[0], p[1] - self.c[1], p[2] - self.c[2]]); if v[2] < 1.0 { return None; } Some((self.w as f64 / 2.0 + self.f * v[0] / v[2], self.h as f64 / 2.0 + self.f * v[1] / v[2], v[2])) }
89:     pub fn focal(&self) -> f64 { self.f }
90:     pub fn eye(&self) -> [f64; 3] { self.c }
91: }
```

## pixel_expr.rs:283-289

```rust
283: fn noise(seed: f32, x: f32, y: f32) -> f32 {
284:     let mut z = (seed.to_bits() as u64) ^ ((x.to_bits() as u64) << 32) ^ (y.to_bits() as u64);
285:     z = z.wrapping_add(0x9e3779b97f4a7c15);
286:     z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
287:     z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
288:     ((z ^ (z >> 31)) >> 40) as f32 / 16777216.
289: }
```

## particles.rs:120-133

```rust
120:         // integrate
121:         let steps = ((age * 60.0).ceil() as usize).clamp(1, 400); let dt = age / steps as f64; let mut hist: Vec<(f64, f64)> = Vec::new(); let every = (steps / (e.trail as usize + 1).max(1)).max(1);
122:         for s in 0..steps { let tt = birth + s as f64 * dt;
123:             let mut ax = e.wind * sc; let mut ay = e.gravity * sc;
124:             if e.turbulence != 0.0 { let k = 6.0 / (e.turb_scale.max(0.05) * w.max(hh)); ax += e.turbulence * sc * ((py * k * 0.9 + tt * 1.3 + rx * 6.0).sin() + 0.5 * (py * k * 2.1 - tt * 0.7).cos()); ay += e.turbulence * sc * ((px * k * 1.1 - tt * 1.1 + ry * 6.0).cos() + 0.5 * (px * k * 1.9 + tt * 0.9).sin()); }
125:             if e.attractor != 0.0 { let (dx, dy) = (e.attractor_x * w - px, e.attractor_y * hh - py); let d = (dx * dx + dy * dy).sqrt().max(20.0 * sc); ax += dx / d * e.attractor * sc; ay += dy / d * e.attractor * sc; }
126:             if e.vortex != 0.0 { let (dx, dy) = (px - e.vortex_x * w, py - e.vortex_y * hh); let d = (dx * dx + dy * dy).sqrt().max(20.0 * sc); ax += -dy / d * e.vortex * sc; ay += dx / d * e.vortex * sc; }
127:             let dr = (-e.drag * dt).exp(); vx = (vx + ax * dt) * dr; vy = (vy + ay * dt) * dr; px += vx * dt; py += vy * dt;
128:             if e.floor && py > hh { py = hh; vy = -vy.abs() * e.bounce; vx *= 0.85; }
129:             for (ci, c) in e.colliders.iter().enumerate() { let hh_ = if c.kind == "layer" { layer_hit(ci, c.follow, tt, c.threshold, px, py, (vx, vy)) } else { hit(c, px, py, w, hh) }; if let Some(((nx, ny), pen)) = hh_ { px += nx * pen; py += ny * pen; let vn = vx * nx + vy * ny; if vn < 0.0 { let (tx, ty) = (vx - vn * nx, vy - vn * ny); let k = 1.0 - e.friction.clamp(0.0, 1.0); vx = tx * k - vn * nx * e.bounce; vy = ty * k - vn * ny * e.bounce; } } }
130:             if e.trail > 0 && s % every == 0 { hist.push((px, py)); } }
131:         let u = age / life; let size = lerp(e.size0, e.size1, u) * sc * (1.0 + (h(e.seed, i, 5) - 0.5) * 2.0 * e.size_var); let mut alpha = lerp(e.alpha0, e.alpha1, u).clamp(0.0, 1.5); if e.fade_in > 0.0 { let t = (u / e.fade_in).clamp(0.0, 1.0); alpha *= t * t * (3.0 - 2.0 * t); }
132:         let col = [lerp(e.color0[0], e.color1[0], u), lerp(e.color0[1], e.color1[1], u), lerp(e.color0[2], e.color1[2], u)];
133:         let mut pts: Vec<(f64, f64, f64)> = hist.iter().rev().enumerate().map(|(k, p)| (p.0, p.1, alpha * (1.0 - (k as f64 + 1.0) / (hist.len() as f64 + 1.0)) * 0.6)).collect(); pts.insert(0, (px, py, alpha));
```

## physics.rs:22-48

```rust
22:     let dt = 1.0 / 240.0; let n = (total / dt).ceil() as usize + 2; let nb = bodies.len(); let mut data: Vec<Vec<[f32; 7]>> = vec![Vec::with_capacity(n); nb];
23:     let rmax = bodies.iter().map(|b| b.r).fold(1.0, f64::max); let cell = rmax * 2.0;
24:     for s in 0..n { let t = s as f64 * dt;
25:         for (i, b) in bodies.iter().enumerate() { data[i].push([b.p[0] as f32, b.p[1] as f32, b.p[2] as f32, b.q[0] as f32, b.q[1] as f32, b.q[2] as f32, b.q[3] as f32]); }
26:         for b in bodies.iter_mut() { if t < b.birth { continue; }
27:             for a in 0..3 { b.v[a] += par.gravity[a] * dt; } let dk = (-par.drag * dt).exp(); for a in 0..3 { b.v[a] *= dk; } let ak = (-par.ang_drag * dt).exp(); for a in 0..3 { b.w[a] *= ak; }
28:             for a in 0..3 { b.p[a] += b.v[a] * dt; } let wq = [0.0, b.w[0], b.w[1], b.w[2]]; let dq = qmul(wq, b.q); for a in 0..4 { b.q[a] += 0.5 * dq[a] * dt; } b.q = qnorm(b.q); }
29:         if let Some(fy) = par.floor_y { for b in bodies.iter_mut() { if t < b.birth { continue; } let pen = b.p[1] + b.r - fy; if pen > 0.0 { b.p[1] -= pen;
30:             let rc = [0.0, b.r, 0.0]; let vc = [b.v[0] + cross(b.w, rc)[0], b.v[1] + cross(b.w, rc)[1], b.v[2] + cross(b.w, rc)[2]]; let nrm = [0.0, -1.0, 0.0]; let vn = dot(vc, nrm);
31:             if vn < 0.0 { let inv_i = 1.0 / (0.4 * b.m * b.r * b.r); let e = if -vn < 30.0 { 0.0 } else { par.restitution }; let jn = -(1.0 + e) * vn / (1.0 / b.m + b.r * b.r * inv_i * 0.0 + 0.0);
32:                 for a in 0..3 { b.v[a] += nrm[a] * jn / b.m; }
33:                 let vt = [vc[0] - vn * nrm[0], vc[1] - vn * nrm[1], vc[2] - vn * nrm[2]]; let vtm = (dot(vt, vt)).sqrt();
34:                 if vtm > 1e-6 { let jt = (par.friction * jn).min(vtm * b.m / 3.5); let td = [vt[0] / vtm, vt[1] / vtm, vt[2] / vtm]; for a in 0..3 { b.v[a] -= td[a] * jt / b.m; } let tq = cross(rc, [-td[0] * jt, -td[1] * jt, -td[2] * jt]); for a in 0..3 { b.w[a] += tq[a] * inv_i; } } } } } }
35:         if par.collide && nb > 1 { let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
36:             for (i, b) in bodies.iter().enumerate() { if t < b.birth { continue; } grid.entry(((b.p[0] / cell).floor() as i32, (b.p[1] / cell).floor() as i32, (b.p[2] / cell).floor() as i32)).or_default().push(i); }
37:             let mut pairs: Vec<(usize, usize)> = vec![]; for (&(cx, cy, cz), v) in grid.iter() { for dx in -1..=1 { for dy in -1..=1 { for dz in -1..=1 { if let Some(o) = grid.get(&(cx + dx, cy + dy, cz + dz)) { for &i in v { for &j in o { if i < j { pairs.push((i, j)); } } } } } } } }
38:             pairs.sort_unstable(); pairs.dedup();
39:             for (i, j) in pairs { let (a, b) = { let (l, r) = bodies.split_at_mut(j); (&mut l[i], &mut r[0]) }; let d = [b.p[0] - a.p[0], b.p[1] - a.p[1], b.p[2] - a.p[2]]; let dist = dot(d, d).sqrt(); let rr = (a.r + b.r) * 0.8; if dist >= rr || dist < 1e-9 { continue; }
40:                 let nrm = [d[0] / dist, d[1] / dist, d[2] / dist]; let (ia, ib) = (1.0 / a.m, 1.0 / b.m); let pen = rr - dist; for k in 0..3 { a.p[k] -= nrm[k] * pen * ia / (ia + ib); b.p[k] += nrm[k] * pen * ib / (ia + ib); }
41:                 let rv = [b.v[0] - a.v[0], b.v[1] - a.v[1], b.v[2] - a.v[2]]; let vn = dot(rv, nrm); if vn < 0.0 { let jn = -(1.0 + par.restitution) * vn / (ia + ib); for k in 0..3 { a.v[k] -= nrm[k] * jn * ia; b.v[k] += nrm[k] * jn * ib; }
42:                     let vt = [rv[0] - vn * nrm[0], rv[1] - vn * nrm[1], rv[2] - vn * nrm[2]]; let vtm = dot(vt, vt).sqrt(); if vtm > 1e-6 { let jt = (par.friction * jn).min(vtm / (ia + ib) / 3.5); for k in 0..3 { let td = vt[k] / vtm; a.v[k] += td * jt * ia; b.v[k] -= td * jt * ib; } let sp = cross(nrm, [vt[0] / vtm, vt[1] / vtm, vt[2] / vtm]); for k in 0..3 { a.w[k] += sp[k] * jt * ia / a.r * 0.8; b.w[k] += sp[k] * jt * ib / b.r * 0.8; } } } } }
43:     }
44:     Traj { dt, n, data }
45: }
46: impl Traj { pub fn at(&self, i: usize, t: f64) -> ([f64; 3], [f64; 4]) { let f = (t / self.dt).max(0.0); let a = (f.floor() as usize).min(self.n - 1); let b = (a + 1).min(self.n - 1); let u = f - f.floor(); let (x, y) = (self.data[i][a], self.data[i][b]);
47:     let l = |k: usize| x[k] as f64 * (1.0 - u) + y[k] as f64 * u; ([l(0), l(1), l(2)], qnorm([l(3), l(4), l(5), l(6)])) } }
48:
```

## render.rs:428-448

```rust
428: fn blend_region(dst: &mut Frame, src: &Frame, opacity: f32, mode: &str, mask: Option<&dyn Fn(usize, usize) -> f32>, region: Option<(usize, usize, usize, usize)>) {
429:     let w = dst.w; let m_id = match mode { "add" => 1, "multiply" => 2, "screen" => 3, "overlay" => 4, _ => 0 };
430:     let (ry0, ry1, rx0, rx1) = match region { Some((a, b, c, d)) => (b.min(dst.h), (d + 1).min(dst.h), a.min(w), (c + 1).min(w)), None => (0, dst.h, 0, w) };
431:     for y in ry0..ry1 {
432:         let srow = &src.d[y * w * 4..(y + 1) * w * 4];
433:         // skip the transparent margins of the row (most layers are small)
434:         let (mut x0, mut x1) = (rx0, rx1); while x0 < rx1 && srow[x0 * 4 + 3] <= 0.0 { x0 += 1; } if x0 == rx1 { continue; } while x1 > x0 && srow[(x1 - 1) * 4 + 3] <= 0.0 { x1 -= 1; }
435:         for x in x0..x1 { let i = (y * w + x) * 4;
436:             let a = srow[x * 4 + 3] * opacity * if let Some(m) = mask { m(x, y) } else { 1.0 }; if a <= 0.0 { continue; }
437:             for j in 0..3 {
438:                 let (b, s) = (dst.d[i + j], srow[x * 4 + j]);
439:                 let r = match m_id { 1 => b + s, 2 => b * s, 3 => 1.0 - (1.0 - b) * (1.0 - s), 4 => if b < 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) }, _ => s };
440:                 dst.d[i + j] = b * (1.0 - a) + r * a;
441:             }
442:             dst.d[i + 3] = dst.d[i + 3] + a * (1.0 - dst.d[i + 3]);
443:         }
444:     }
445: }
446:
447: pub fn render_frame(p: &Project, t: f64, cache: &Cache) -> Result<Frame, String> { render_frame_opts(p, t, cache, false) }
448:
```

## render.rs:541-542

```rust
541: /// The compositor keeps premultiplied color on a transparent canvas (rgb = sum of src*a over below); alpha deliveries need straight alpha.
542: pub(crate) fn unpremultiply(f: &mut Frame) { for px in f.d.chunks_exact_mut(4) { let a = px[3]; if a > 0.0 && a < 1.0 { for j in 0..3 { px[j] = (px[j] / a).min(1.0); } } } }
```

## color/node_graph.rs:1-16

```rust
1: //! Per-clip grade DAG. Evaluation-local node cache: no stale cross-frame pixels.
2: use std::collections::{BTreeMap,BTreeSet};use std::sync::Arc;use serde::{Serialize,Deserialize};use crate::{effects::Frame,keyframe::Prop,masks::{MaskLayer,Ctx},model::Effect};
3: pub const INPUT:&str="input";
4: pub const GRADES:&[&str]=&["lut","blur","sharpen","exposure","contrast","saturation","temperature","tint","color_wheels","curves","levels","brightness_contrast","channel_mixer","colorize","invert","sepia","hsl","rgb_curves","hue_curves","film_print","print_stock","bleach_bypass","white_balance","color_transfer","tonemap","highlight_rolloff","split_toning","color_space","primaries","log_wheels","linear_grade","luminance_shoulder","color_managed","point_curves","cdl","asc_cdl","acescct_primaries","film_soft_shoulder","film_luma_grain","white_adaptation","soft_gamut_ray"];
5: #[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Node{pub id:String,pub kind:String,#[serde(default)]pub inputs:Vec<String>,#[serde(default)]pub effect:Option<Effect>,#[serde(default)]pub weights:Vec<Prop>,#[serde(default)]pub mask:Option<MaskLayer>,#[serde(default)]pub bypass:bool}
6: #[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Graph{pub nodes:BTreeMap<String,Node>,pub output:String,#[serde(default="budget")]pub cache_budget_mb:usize}
7: fn budget()->usize{256}impl Default for Graph{fn default()->Self{Self{nodes:BTreeMap::new(),output:INPUT.into(),cache_budget_mb:budget()}}}
8: #[derive(Default,Debug,Serialize)]pub struct Stats{pub evaluated_nodes:usize,pub cache_hits:usize,pub peak_cached_bytes:usize,pub unreachable_nodes:usize}
9: fn finite(p:&Prop)->bool{serde_json::to_value(p).ok().and_then(|v|crate::guard::sanitize(&v,crate::guard::limits(),0).ok()).is_some()&&p.base.is_finite()&&p.keys.windows(2).all(|k|k[0].t<k[1].t)&&p.keys.iter().all(|k|k.t.is_finite()&&k.v.is_finite()&&k.bez.map(|b|b.iter().all(|v|v.is_finite())).unwrap_or(true))}
10: impl Graph{
11:  pub fn validate(&self,complete:bool)->Result<Vec<String>,String>{
12:  crate::guard::sanitize(&serde_json::to_value(self).map_err(|e|e.to_string())?,crate::guard::limits(),0)?;
13:  if self.nodes.len()>64||!(1..=1024).contains(&self.cache_budget_mb){return Err("grade graph max64 nodes; cache budget1..1024MiB".into())}if self.output!=INPUT&&!self.nodes.contains_key(&self.output){return Err("missing graph output".into())}
14:  for(id,n)in &self.nodes{if id==INPUT||id!=&n.id||id.is_empty()||id.len()>128{return Err("invalid grade node identity".into())}if !["grade","parallel","layer"].contains(&n.kind.as_str()){return Err("node kind grade/parallel/layer".into())}if n.inputs.len()>16||n.inputs.iter().collect::<BTreeSet<_>>().len()!=n.inputs.len(){return Err("max16 unique ordered node inputs".into())}if n.kind=="grade"{if n.inputs.len()>1||complete&&n.inputs.len()!=1{return Err(format!("grade node {id} requires one input"))}let e=n.effect.as_ref().ok_or("grade requires effect")?;if !GRADES.contains(&e.kind.as_str())||e.strings.contains_key("mask_layer"){return Err("unsupported/non-color grade effect or implicit clip mask binding".into())}if !e.params.values().all(finite){return Err("nonfinite node property".into())}}else if n.effect.is_some()||complete&&n.inputs.len()<2{return Err(format!("mixer node {id} requires2..16inputs and no effect"))}
15:  if n.weights.len()!=n.inputs.len()||!n.weights.iter().all(finite){return Err("one finite weight property per input".into())}for src in &n.inputs{if src!=INPUT&&!self.nodes.contains_key(src){return Err("connection source does not exist".into())}}
16:  if let Some(m)=&n.mask{if !m.strings.is_empty()||!m.path_keys.windows(2).all(|k|k[0].t<k[1].t)||!["rect","ellipse","path","linear","radial","luma","color","hsl","alpha","noise"].contains(&m.kind.as_str())||!m.props.values().all(finite)||!m.path_keys.iter().all(|k|k.t.is_finite()&&k.points.iter().all(|p|[p.x,p.y,p.ix,p.iy,p.ox,p.oy,p.f].iter().all(|v|v.is_finite())))||!m.points.iter().all(|p|[p.x,p.y,p.ix,p.iy,p.ox,p.oy,p.f].iter().all(|v|v.is_finite())){return Err("unsupported or nonfinite node mask; no external matte references".into())}}
```

## color/node_graph.rs:35-43

```rust
35:  let weights=n.weights.iter().map(|p|p.at(t)).collect::<Vec<_>>();if weights.iter().any(|v|!v.is_finite()||*v<0.||*v>1.){return Err("mixer weight must be0..1 at evaluated time".into())}
36:  if n.kind=="parallel"{let sum:f64=weights.iter().sum();if sum<=0.{return Err("parallel mixer positive total weight required".into())}for(i,px)in out.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}for j in 0..3{px[j]=(ins.iter().zip(&weights).map(|(a,w)|a.d[i*4+j] as f64*w).sum::<f64>()/sum) as f32;}}}
37:  else{for (a,w)in ins.iter().zip(&weights).skip(1){for(i,px)in out.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}for j in 0..3{px[j]=(px[j] as f64*(1.-w)+a.d[i*4+j] as f64*w) as f32;}}}}
38:  }
39:  if let Some(mask)=&n.mask{let refs=|_:&str|None;let ctx=Ctx{refs:&refs,fps};let cov=crate::masks::layer_coverage(mask,&ins[0],t,&ctx);for(i,px)in out.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}let a=cov[i];if !a.is_finite(){return Err("nonfinite node mask coverage".into())}for j in 0..3{px[j]=ins[0].d[i*4+j]*(1.-a)+px[j]*a;}}}
40:  }
41:  // Grading/mixers own RGB only, and retain the first input alpha exactly.
42:  for(i,px)in out.d.chunks_exact_mut(4).enumerate(){px[3]=ins[0].d[i*4+3];}super::frame::validate(&out)?;stats.evaluated_nodes+=1;cache.insert(id.clone(),Arc::new(out));drop(ins);
43:  for src in &n.inputs{let count=uses.get_mut(src).unwrap();*count-=1;if *count==0{cache.remove(src);}}
```

## lighting.rs:84-96 (BRDF full relevant range)

```rust
84: /// Cook-Torrance BRDF value times n.l for one light. Returns (diffuse weight, specular weight); the caller multiplies diffuse by albedo.
85: pub fn brdf(n: [f64; 3], l: [f64; 3], v: [f64; 3], m: &Material, albedo: [f64; 3]) -> ([f64; 3], [f64; 3]) {
86:     let ndl = dot(n, l).max(0.0); if ndl <= 0.0 { return ([0.0; 3], [0.0; 3]); }
87:     let h = norm([l[0] + v[0], l[1] + v[1], l[2] + v[2]]); let ndh = dot(n, h).max(0.0); let ndv = dot(n, v).max(1e-3); let vdh = dot(v, h).max(0.0);
88:     let lobe = |rough: f64| { let a = (rough * rough).max(1e-4); let a2 = a * a; let d = a2 / (std::f64::consts::PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2)); let k = (rough + 1.0).powi(2) / 8.0; let g = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k)); (d, g) };
89:     let f0d = ((m.ior - 1.0) / (m.ior + 1.0)).powi(2); let fw = (1.0 - vdh).powi(5);
90:     let (d, g) = lobe(m.roughness); let mut spec = [0.0; 3]; let mut diff = [0.0; 3];
91:     for j in 0..3 { let f0 = f0d * (1.0 - m.metal) + albedo[j] * m.metal; let f = f0 + (1.0 - f0) * fw; spec[j] = d * g * f / (4.0 * ndv);
92:         diff[j] = albedo[j] * (1.0 - m.metal) * (1.0 - f) * ndl; }
93:     if m.clearcoat > 0.0 { let (d2, g2) = lobe(0.06); let f = 0.04 + 0.96 * fw; let c = m.clearcoat * d2 * g2 * f / (4.0 * ndv); for j in 0..3 { spec[j] += c; diff[j] *= 1.0 - m.clearcoat * f; } }
94:     (diff, spec)
95: }
96:
```
