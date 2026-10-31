//! RV-28 C1: text::measure_text uses the renderer's own layout; its ink box and line breaks match a real render.
use kimo_core::effects::Frame;
use kimo_core::model::TextFx;
use kimo_core::text::{measure_text, render_text};
fn alpha_bbox(f: &Frame) -> Option<[usize; 4]> { let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0); let mut any = false; for y in 0..f.h { for x in 0..f.w { if f.d[(y * f.w + x) * 4 + 3] > 0.02 { any = true; x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } } any.then_some([x0, y0, x1, y1]) }
const STRINGS: [&str; 4] = ["Hello", "Quick brown fox", "two\nlines here", "Wrap this long sentence at a narrow box width please"];
#[test] fn ink_box_equals_rendered_alpha_box_within_1px() {
    let (w, h) = (3200usize, 400usize);
    for font in ["Montserrat", "DejaVuSans", "Inter"] { for size in [24.0, 48.0, 72.0] { for s in STRINGS {
        let m = measure_text(s, font, size, 0.0, 0.0, "center", w as f64, h as f64, None).unwrap();
        let mut fr = Frame::new(w, h); render_text(&mut fr, s, font, size, [1.0; 3], 0.0, 0.0, [0.0; 3], 0.0, "center", 1.0, None, 0.0).unwrap();
        let (Some(b), Some(i)) = (alpha_bbox(&fr), m.ink) else { panic!("no ink {font} {size} {s}") };
        for (k, (a, e)) in [b[0] as f64, b[1] as f64, b[2] as f64, b[3] as f64].iter().zip(i).enumerate() { assert!((a - e).abs() <= 1.0, "{font} {size} {s:?} edge {k}: render {a} measure {e}"); }
    } } }
}
#[test] fn wrap_breaks_match_the_renderer_and_font_substitution_is_explicit() {
    let fx = TextFx { box_width: 300.0, ..Default::default() };
    // font availability differs per box: pick the first common family that resolves without substitution
    let fam = ["Montserrat", "DejaVu Sans", "Liberation Sans", "Arial"].into_iter().find(|f| !measure_text("x", f, 20.0, 0.0, 0.0, "left", 100.0, 0.0, None).unwrap().substituted).expect("one of the common families is installed");
    let m = measure_text("Wrap this long sentence at a narrow box width please", fam, 40.0, 0.0, 0.0, "left", 640.0, 360.0, Some(&fx)).unwrap();
    assert!(m.lines.len() >= 3 && m.lines.iter().all(|l| l.w <= 300.5), "{:?}", m.lines.iter().map(|l| (&l.text, l.w)).collect::<Vec<_>>());
    assert_eq!(m.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" "), "Wrap this long sentence at a narrow box width please");
    assert!(!m.substituted); assert!(measure_text("x", "NoSuchFamilyZZ", 20.0, 0.0, 0.0, "left", 100.0, 0.0, None).unwrap().substituted);
}

#[test] fn boxed_caption_pill_hugs_the_text_not_the_wrap_width() {
    // RV-28 WIP: a background-box caption (set_caption_style "boxed", text_over_subject's own fix path) used to
    // draw its pill at the full configured wrap width (box_width) even for one or two short words, leaving a big
    // empty strip beside the text. The pill should hug the actual glyphs instead.
    let (w, h) = (1280usize, 720usize);
    let fam = ["Montserrat", "DejaVu Sans", "Liberation Sans", "Arial"].into_iter().find(|f| !measure_text("x", f, 20.0, 0.0, 0.0, "left", 100.0, 0.0, None).unwrap().substituted).expect("one of the common families is installed");
    let fx = TextFx { box_width: 1000.0, bg: Some([0.0, 0.0, 0.0, 1.0]), bg_pad: 24.0, ..Default::default() };
    let mut fr = Frame::new(w, h);
    render_text(&mut fr, "Hey there", fam, 48.0, [1.0; 3], 0.0, 0.0, [0.0; 3], 0.0, "center", 1.0, Some(&fx), 0.0).unwrap();
    let b = alpha_bbox(&fr).expect("pill should render visible pixels");
    let pill_w = (b[2] - b[0]) as f64;
    // the wrap width is a limit the layout could use for multi-line long captions; a 2-word caption's real pill
    // must be nowhere near it -- comfortably under half, where the pre-fix bug drew it at ~the full box_width.
    assert!(pill_w < 500.0, "pill width {pill_w} should hug \"Hey there\", not the {}px wrap width", fx.box_width);
}
