//! RV-8 step 1: transcript data model, time derivation, byte stability.
use kimo_core::model::Project;
use kimo_core::transcript::*;

fn take(id: &str, order: u32, src: (f64, f64), tl: Option<f64>, speed: f64, st: TakeStatus) -> Take { Take { id: id.into(), source: TakeSource::default(), src_start: src.0, src_end: src.1, timeline_start: tl, speed, order, status: st, note: String::new() } }
fn fixture() -> Transcript {
    // take order fixture: file/import order is A,B but script order is B,A. B is 2 s long, A is 1 s.
    Transcript { id: "tr1".into(), name: "n".into(), language: "en".into(), script_text: Some("hello brave world again".into()), version: 0,
        takes: vec![take("A", 2, (5.0, 6.0), None, 1.0, TakeStatus::Chosen), take("B", 1, (0.0, 2.0), None, 1.0, TakeStatus::Chosen), take("C", 3, (0.0, 1.0), None, 1.0, TakeStatus::Rejected)],
        words: vec![Word::new("w3", "A", "world", 5.0, 5.4, Some(0.9)), Word::new("w4", "A", "again", 5.5, 5.9, None), Word::new("w1", "B", "hello", 0.0, 0.3, Some(0.99)), Word::new("w2", "B", "brave", 0.4, 0.8, Some(0.4)), Word::new("w9", "C", "junk", 0.0, 0.2, None)] }
}
fn times(t: &Transcript) -> Vec<(String, f64, f64)> { t.words.iter().map(|w| (w.id.clone(), (w.start * 1e6).round() / 1e6, (w.end * 1e6).round() / 1e6)).collect() }

#[test] fn timeline_is_assembled_by_order_not_by_file_order() {
    let mut t = fixture(); t.derive_times(); let m: std::collections::BTreeMap<_, _> = times(&t).into_iter().map(|x| (x.0, (x.1, x.2))).collect();
    assert_eq!(m["w1"], (0.0, 0.3), "B is first in script order"); assert_eq!(m["w2"], (0.4, 0.8));
    assert_eq!(m["w3"], (2.0, 2.4), "A follows B (B is 2 s long): 2.0 + (5.0-5.0)"); assert_eq!(m["w4"], (2.5, 2.9));
    assert_eq!(t.script_words().iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["w1", "w2", "w3", "w4"], "rejected take words are not in the script");
}
#[test] fn the_time_formula_handles_speed_trim_and_explicit_start() {
    let tk = take("T", 1, (10.0, 14.0), Some(100.0), 2.0, TakeStatus::Chosen);
    assert_eq!(tk.to_timeline(100.0, 10.0), 100.0); assert_eq!(tk.to_timeline(100.0, 12.0), 101.0, "2x speed halves source time"); assert_eq!(tk.timeline_len(), 2.0);
    let slow = take("S", 1, (3.0, 4.0), None, 0.5, TakeStatus::Chosen); assert_eq!(slow.to_timeline(7.0, 3.5), 8.0);
    let mut t = Transcript { id: "t".into(), name: "n".into(), takes: vec![tk, take("N", 2, (0.0, 1.0), None, 1.0, TakeStatus::Chosen)], words: vec![Word::new("a", "T", "x", 12.0, 13.0, None), Word::new("b", "N", "y", 0.5, 0.75, None)], ..Default::default() };
    t.derive_times(); assert_eq!(times(&t), [("a".to_string(), 101.0, 101.5), ("b".to_string(), 102.5, 102.75)], "N starts where T ends (100 + 4/2)");
}
#[test] fn derive_times_is_deterministic_and_idempotent() { let mut a = fixture(); a.derive_times(); let j = serde_json::to_string(&a).unwrap(); a.derive_times(); assert_eq!(serde_json::to_string(&a).unwrap(), j); }
#[test] fn validate_catches_bad_structure() {
    assert!(fixture().validate().is_ok());
    let mut a = fixture(); a.words[1].id = "w3".into(); assert!(a.validate().unwrap_err().contains("duplicate word"));
    let mut b = fixture(); b.words[0].src_end = -1.0; assert!(b.validate().is_err());
    let mut c = fixture(); c.words[0].take_id = "ZZ".into(); assert!(c.validate().unwrap_err().contains("unknown take"));
    let mut d = fixture(); d.takes[1].id = "A".into(); assert!(d.validate().unwrap_err().contains("duplicate take"));
    let mut e = fixture(); e.takes[0].speed = 0.0; assert!(e.validate().unwrap_err().contains("speed"));
}
#[test] fn old_projects_stay_byte_stable_and_transcripts_round_trip() {
    let p = Project::default(); let before = serde_json::to_string(&p).unwrap(); assert!(!before.contains("transcripts"), "empty transcripts must not be serialized");
    let back: Project = serde_json::from_str(&before).unwrap(); assert_eq!(serde_json::to_string(&back).unwrap(), before);
    let mut q = Project::default(); let mut f = fixture(); f.derive_times(); q.editorial.transcripts.insert("tr1".into(), f.clone()); let j = serde_json::to_string(&q).unwrap();
    let r: Project = serde_json::from_str(&j).unwrap(); assert_eq!(r.editorial.transcripts["tr1"], f); assert_eq!(serde_json::to_string(&r).unwrap(), j);
}
