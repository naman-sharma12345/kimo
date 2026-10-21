//! 3D layer system: every layer is a textured plane with a pose (x, y, z position, rot_x/rot_y/rot_z, scale) seen through a
//! virtual pinhole camera (position, pan/tilt/roll, fov). Rendering is exact inverse perspective mapping with 2x2 supersampling.
//! At default camera, a layer at z=0 with no rotation maps 1:1 to the 2D renderer. +z is away from the camera.
use crate::effects::Frame;
use std::collections::BTreeMap;
use crate::keyframe::Prop;

#[derive(Clone, Copy, Debug)]
pub struct Camera { pub pos: [f64; 3], pub pan: f64, pub tilt: f64, pub roll: f64, pub fov: f64 }
impl Default for Camera { fn default() -> Self { Camera { pos: [0.0; 3], pan: 0.0, tilt: 0.0, roll: 0.0, fov: 50.0 } } }
impl Camera {
    pub fn from_props(m: &BTreeMap<String, Prop>, t: f64) -> Camera {
        let g = |k: &str, d: f64| m.get(k).map(|p| p.at(t)).unwrap_or(d);
        Camera { pos: [g("cam_x", 0.0), g("cam_y", 0.0), g("cam_z", 0.0)], pan: g("cam_pan", 0.0), tilt: g("cam_tilt", 0.0), roll: g("cam_roll", 0.0), fov: g("cam_fov", 50.0).clamp(5.0, 170.0) }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Pose { pub pos: [f64; 3], pub rot: [f64; 3], pub scale: f64 }

type M = [[f64; 3]; 3];
fn mul(a: &M, b: &M) -> M { let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { for k in 0..3 { r[i][j] += a[i][k] * b[k][j]; } } } r }
fn tr(a: &M) -> M { let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { r[i][j] = a[j][i]; } } r }
fn app(a: &M, v: [f64; 3]) -> [f64; 3] { [a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2], a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2], a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2]] }
pub fn rx(d: f64) -> M { let (s, c) = d.to_radians().sin_cos(); [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]] }
fn ry(d: f64) -> M { let (s, c) = d.to_radians().sin_cos(); [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]] }
fn rz(d: f64) -> M { let (s, c) = d.to_radians().sin_cos(); [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]] }
/// Rotation order: Z then Y then X applied to the local plane (R = Rx * Ry * Rz).
pub fn rot_matrix(r: [f64; 3]) -> M { mul(&rx(r[0]), &mul(&ry(r[1]), &rz(r[2]))) }

#[derive(Clone)]
pub struct Projector { w: usize, h: usize, f: f64, c: [f64; 3], rc: M, p0: [f64; 3], r: M, n: [f64; 3], s: f64 }
impl Projector {
    pub fn new(w: usize, h: usize, cam: &Camera, pose: &Pose) -> Projector {
        let f = (h as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan(); let rc = mul(&ry(cam.pan), &mul(&rx(cam.tilt), &rz(cam.roll))); let r = rot_matrix(pose.rot);
        Projector { w, h, f, c: [cam.pos[0], cam.pos[1], cam.pos[2] - f], rc, p0: pose.pos, n: app(&r, [0.0, 0.0, -1.0]), r, s: pose.scale.max(1e-6) }
    }
    /// Screen pixel -> (source x, source y, depth t, facing) on the layer plane; None if the ray misses the plane or points backward.
    pub fn dest_to_src(&self, x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
        let d = app(&self.rc, [x - self.w as f64 / 2.0, y - self.h as f64 / 2.0, self.f]); let den = d[0] * self.n[0] + d[1] * self.n[1] + d[2] * self.n[2]; if den.abs() < 1e-12 { return None; }
        let t = ((self.p0[0] - self.c[0]) * self.n[0] + (self.p0[1] - self.c[1]) * self.n[1] + (self.p0[2] - self.c[2]) * self.n[2]) / den; if t <= 0.0 { return None; }
        let p = [self.c[0] + t * d[0] - self.p0[0], self.c[1] + t * d[1] - self.p0[1], self.c[2] + t * d[2] - self.p0[2]]; let l = app(&tr(&self.r), p);
        Some((self.w as f64 / 2.0 + l[0] / self.s, self.h as f64 / 2.0 + l[1] / self.s, t, if den < 0.0 { 1.0 } else { -1.0 }))
    }
    /// Focal length in pixels: f = (h / 2) / tan(fov / 2).
    pub fn focal(&self) -> f64 { self.f }
    /// Camera-space depth (px along the camera's view axis) of the plane point seen at screen pixel (x, y). The eye ray
    /// d = Rc (x - w/2, y - h/2, f) has view-axis component f, so the hit c + t d lies at depth z = t f. A layer at pos_z = 0
    /// under the default camera has z = f. None if the ray misses the plane or points backward (as dest_to_src).
    pub fn depth_at(&self, x: f64, y: f64) -> Option<f64> { self.dest_to_src(x, y).map(|(_, _, t, _)| t * self.f) }
    /// World point c + t d of the plane seen at screen pixel (x, y); None as dest_to_src.
    pub fn world_at(&self, x: f64, y: f64) -> Option<[f64; 3]> {
        let (_, _, t, _) = self.dest_to_src(x, y)?; let d = app(&self.rc, [x - self.w as f64 / 2.0, y - self.h as f64 / 2.0, self.f]);
        Some([self.c[0] + t * d[0], self.c[1] + t * d[1], self.c[2] + t * d[2]])
    }
    /// World ray o + u dir against this layer's (infinite) plane: (u, source x, source y) of the hit, u = ((p0 - o).n) / (dir.n).
    /// None when the ray is parallel to the plane. The caller bounds u and the source coordinates.
    pub fn ray_hit(&self, o: [f64; 3], dir: [f64; 3]) -> Option<(f64, f64, f64)> {
        let den = dir[0] * self.n[0] + dir[1] * self.n[1] + dir[2] * self.n[2]; if den.abs() < 1e-12 { return None; }
        let u = ((self.p0[0] - o[0]) * self.n[0] + (self.p0[1] - o[1]) * self.n[1] + (self.p0[2] - o[2]) * self.n[2]) / den;
        let p = [o[0] + u * dir[0] - self.p0[0], o[1] + u * dir[1] - self.p0[1], o[2] + u * dir[2] - self.p0[2]]; let l = app(&tr(&self.r), p);
        Some((u, self.w as f64 / 2.0 + l[0] / self.s, self.h as f64 / 2.0 + l[1] / self.s))
    }
    /// World-space unit view direction (camera to pixel) and the unit plane normal facing the camera, at a screen pixel.
    pub fn view_and_normal(&self, x: f64, y: f64) -> ([f64; 3], [f64; 3]) {
        let d0 = app(&self.rc, [x - self.w as f64 / 2.0, y - self.h as f64 / 2.0, self.f]); let dl = (d0[0] * d0[0] + d0[1] * d0[1] + d0[2] * d0[2]).sqrt(); let d = [d0[0] / dl, d0[1] / dl, d0[2] / dl];
        let dn = d[0] * self.n[0] + d[1] * self.n[1] + d[2] * self.n[2]; let sg = if dn > 0.0 { -1.0 } else { 1.0 }; (d, [sg * self.n[0], sg * self.n[1], sg * self.n[2]])
    }
    /// Slab refraction at a screen pixel: a glass slab of `thickness` (world px) and index `ior` lies on the layer plane.
    /// The eye ray hits the front face at angle thetai to the normal, bends to thetat (Snell) and leaves the back face displaced
    /// in-plane by thickness*(tan thetai - tan thetat). Returns the screen offset (dx, dy) from the straight-through position to the
    /// backdrop point the eye actually sees. Zero at normal incidence, grows with the angle. None if the ray misses the plane.
    pub fn slab_offset(&self, x: f64, y: f64, thickness: f64, ior: f64) -> Option<(f64, f64)> {
        let d0 = app(&self.rc, [x - self.w as f64 / 2.0, y - self.h as f64 / 2.0, self.f]); let dl = (d0[0] * d0[0] + d0[1] * d0[1] + d0[2] * d0[2]).sqrt(); let d = [d0[0] / dl, d0[1] / dl, d0[2] / dl];
        let dn = d[0] * self.n[0] + d[1] * self.n[1] + d[2] * self.n[2]; if dn.abs() < 1e-9 { return None; }
        let t = ((self.p0[0] - self.c[0]) * self.n[0] + (self.p0[1] - self.c[1]) * self.n[1] + (self.p0[2] - self.c[2]) * self.n[2]) / dn; if t <= 0.0 { return None; }
        let p = [self.c[0] + t * d[0], self.c[1] + t * d[1], self.c[2] + t * d[2]];
        let ci = dn.abs().min(1.0); let si = (1.0 - ci * ci).sqrt(); let st = si / ior.max(1.0); let ct = (1.0 - st * st).sqrt();
        let shift = thickness * (si / ci.max(1e-6) - st / ct.max(1e-6));
        let tang = [d[0] - dn * self.n[0], d[1] - dn * self.n[1], d[2] - dn * self.n[2]]; let tl = (tang[0] * tang[0] + tang[1] * tang[1] + tang[2] * tang[2]).sqrt(); if tl < 1e-9 { return Some((0.0, 0.0)); }
        let q = [p[0] - shift * tang[0] / tl, p[1] - shift * tang[1] / tl, p[2] - shift * tang[2] / tl];
        let sc = |w: [f64; 3]| { let v = app(&tr(&self.rc), [w[0] - self.c[0], w[1] - self.c[1], w[2] - self.c[2]]); if v[2] <= 1e-6 { None } else { Some((self.w as f64 / 2.0 + self.f * v[0] / v[2], self.h as f64 / 2.0 + self.f * v[1] / v[2])) } };
        let (a, b) = (sc(p)?, sc(q)?); Some((b.0 - a.0, b.1 - a.1))
    }
    /// Layer-local pixel (source coords) -> screen pixel and view depth (>0 in front of camera).
    pub fn src_to_screen(&self, sx: f64, sy: f64) -> Option<(f64, f64, f64)> {
        let l = [(sx - self.w as f64 / 2.0) * self.s, (sy - self.h as f64 / 2.0) * self.s, 0.0]; let wp = app(&self.r, l); let p = [wp[0] + self.p0[0] - self.c[0], wp[1] + self.p0[1] - self.c[1], wp[2] + self.p0[2] - self.c[2]];
        let v = app(&tr(&self.rc), p); if v[2] <= 1e-6 { return None; } Some((self.w as f64 / 2.0 + self.f * v[0] / v[2], self.h as f64 / 2.0 + self.f * v[1] / v[2], v[2])) }
}

/// Render `src` (full-frame layer) as a 3D plane. `backface`: draw the plane when seen from behind.
pub fn render_layer(src: &Frame, cam: &Camera, pose: &Pose, backface: bool) -> Frame {
    let (w, h) = (src.w, src.h); let pj = Projector::new(w, h, cam, pose); let mut o = Frame::new(w, h);
    const SS: [f64; 2] = [0.25, 0.75];
    for y in 0..h { for x in 0..w { let mut acc = [0f32; 4]; for oy in SS { for ox in SS {
        if let Some((sx, sy, _t, face)) = pj.dest_to_src(x as f64 + ox, y as f64 + oy) { if face < 0.0 && !backface { continue; } if sx < 0.0 || sy < 0.0 || sx >= w as f64 || sy >= h as f64 { continue; }
            let (fx, fy) = (sx - 0.5, sy - 0.5); let (x0, y0) = (fx.floor(), fy.floor()); let (ax, ay) = ((fx - x0) as f32, (fy - y0) as f32); let (x0, y0) = (x0 as i64, y0 as i64);
            for (dx, dy, wt) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] { let (px, py) = ((x0 + dx).clamp(0, w as i64 - 1) as usize, (y0 + dy).clamp(0, h as i64 - 1) as usize); let i = (py * w + px) * 4; let a = src.d[i + 3] * wt; acc[0] += src.d[i] * a; acc[1] += src.d[i + 1] * a; acc[2] += src.d[i + 2] * a; acc[3] += a; } } } }
        let i = (y * w + x) * 4; if acc[3] > 0.0 { for j in 0..3 { o.d[i + j] = acc[j] / acc[3]; } o.d[i + 3] = acc[3] / 4.0; } } }
    o
}

/// World point -> screen projection through a camera (for particle-like 3D objects).
pub struct CamView { w: usize, h: usize, f: f64, c: [f64; 3], rct: M }
impl CamView {
    pub fn new(w: usize, h: usize, cam: &Camera) -> CamView { let f = (h as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan(); let rc = mul(&ry(cam.pan), &mul(&rx(cam.tilt), &rz(cam.roll))); CamView { w, h, f, c: [cam.pos[0], cam.pos[1], cam.pos[2] - f], rct: tr(&rc) } }
    /// (screen x, screen y, view depth). None when behind the camera.
    pub fn project(&self, p: [f64; 3]) -> Option<(f64, f64, f64)> { let v = app(&self.rct, [p[0] - self.c[0], p[1] - self.c[1], p[2] - self.c[2]]); if v[2] < 1.0 { return None; } Some((self.w as f64 / 2.0 + self.f * v[0] / v[2], self.h as f64 / 2.0 + self.f * v[1] / v[2], v[2])) }
    pub fn focal(&self) -> f64 { self.f }
    pub fn eye(&self) -> [f64; 3] { self.c }
}
