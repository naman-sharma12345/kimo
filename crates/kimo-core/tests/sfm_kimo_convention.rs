//! Verifies that a solved pose converted with sfm::to_kimo reprojects world points to the same pixels through Kimo's own 3D projector.
use kimo_core::sfm::*; use kimo_core::three_d::{Camera, Pose as KPose, Projector};
#[test] fn solved_camera_matches_kimo_projector() {
    let (w, h) = (1280usize, 720usize); let f = 1000.0; let scale = 800.0;
    for (rv, c) in [([0.1, -0.3, 0.05], [0.4, -0.2, 0.3]), ([-0.2, 0.25, -0.1], [-0.7, 0.1, -0.5]), ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])] {
        let r = exp_so3(&rv); let tv = mv(&r, &c); let pose = Pose { r, t: [-tv[0], -tv[1], -tv[2]] };
        let (pos, rot, fov) = to_kimo(&pose, f, scale, h as f64); let cam = Camera { pos, pan: rot[0], tilt: rot[1], roll: rot[2], fov };
        // a plane layer at kimo z=z0 (rot 0, scale 1): layer pixel (sx,sy) is world (sx-w/2, sy-h/2, z0)
        let z0 = 900.0; let kp = KPose { pos: [0.0, 0.0, z0], rot: [0.0; 3], scale: 1.0 }; let pj = Projector::new(w, h, &cam, &kp);
        let mut worst = 0.0f64;
        for (sx, sy) in [(100.0, 80.0), (640.0, 360.0), (1100.0, 600.0), (300.0, 500.0)] {
            let Some((u, v, _)) = pj.src_to_screen(sx, sy) else { continue };
            // same point in solver coordinates: world_kimo = solver*scale + (0,0,-f)
            let xs = [(sx - w as f64 / 2.0) / scale, (sy - h as f64 / 2.0) / scale, (z0 + f) / scale]; let cc = pose.apply(&xs);
            if cc[2] <= 0.0 { continue; } let (pu, pv) = (f * cc[0] / cc[2] + w as f64 / 2.0, f * cc[1] / cc[2] + h as f64 / 2.0);
            worst = worst.max(((u - pu).powi(2) + (v - pv).powi(2)).sqrt()); }
        eprintln!("rv {rv:?}: worst px diff {worst:.6}"); assert!(worst < 1e-3, "kimo projector disagrees by {worst}px");
    }
}
