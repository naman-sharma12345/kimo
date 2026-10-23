use kimo_core::{
    commands::{dispatch, Session},
    model::{Asset, Source},
    transcript::{CueBinding, TakeStatus},
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

fn call(ss: &mut Session, tool: &str, args: Value) -> Value {
    dispatch(ss, tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
}
fn project(ss: &Session) -> Value {
    serde_json::to_value(&ss.project).unwrap()
}
fn fixture() -> (Session, String, String) {
    let mut ss = Session::new();
    call(
        &mut ss,
        "project_set",
        json!({"width":64,"height":36,"fps":10}),
    );
    let track = call(&mut ss, "add_track", json!({}))["track_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clip = call(
        &mut ss,
        "add_clip",
        json!({"track_id":track,"source_type":"solid","color":"#ff0000","start":0,"duration":4}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    ss.project.assets.insert(
        "asset1".into(),
        Asset {
            id: "asset1".into(),
            path: "fixture.mp4".into(),
            width: 64,
            height: 36,
            duration: 4.,
            fps: 10.,
            origin: None,
            hash: None,
            kind: None,
        },
    );
    ss.project.find_clip_mut(&clip).unwrap().source = Source::Media {
        asset: "asset1".into(),
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "kimo_edit_{}_{}.csv",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(
        &path,
        "word,start,end\nHello,0,1\num,1,2\nworld,2,3\nagain,3,4\n",
    )
    .unwrap();
    call(
        &mut ss,
        "transcript_import",
        json!({"path":path.to_str().unwrap(),"take":{"clip_id":clip,"asset_id":"asset1","timeline_start":0}}),
    );
    std::fs::remove_file(path).unwrap();
    (ss, track, clip)
}
fn preview(ss: &mut Session, edits: Value) -> Value {
    call(ss, "transcript_edit_preview", json!({"edits":edits}))
}
fn delete(ss: &mut Session, from: usize, to: usize) -> Value {
    let edits = json!([{"op":"delete","from_index":from,"to_index":to}]);
    let p = preview(ss, edits.clone());
    call(
        ss,
        "transcript_edit_apply",
        json!({"expect_revision":p["revision"],"edits":edits}),
    )
}
fn fails_unchanged(ss: &mut Session, args: Value, expected: &str) {
    let before = project(ss);
    let (next, history) = (ss.project.next_id, ss.undo.len());
    let err = dispatch(ss, "transcript_edit_apply", &args).unwrap_err();
    assert!(err.contains(expected), "{err}");
    assert_eq!(project(ss), before);
    assert_eq!((ss.project.next_id, ss.undo.len()), (next, history));
}
fn caption(ss: &mut Session, track: &str, words: &[&str], start: f64, end: f64) -> String {
    let id = call(ss, "add_text", json!({"track_id":track,"text":"Hello um world again","start":start,"duration":end-start,"size":12}))["clip_id"].as_str().unwrap().to_string();
    ss.project.editorial.cue_words.insert(
        id.clone(),
        CueBinding {
            word_ids: words.iter().map(|w| w.to_string()).collect(),
            breaks_after: vec![1],
        },
    );
    ss.project.editorial.captions.insert(
        id.clone(),
        kimo_core::editorial::CaptionGroup {
            track_id: track.into(),
            clips: vec![id.clone()],
            style: "subtitle".into(),
            burn_in: true,
            transcript_id: Some("tr1".into()),
        },
    );
    id
}

#[test]
fn deletion_splits_media_retains_source_and_keyframe_clock_and_is_one_undo_step() {
    let (mut ss, track, clip) = fixture();
    call(
        &mut ss,
        "set_keyframe",
        json!({"clip_id":clip,"property":"x","time":0,"value":0}),
    );
    call(
        &mut ss,
        "set_keyframe",
        json!({"clip_id":clip,"property":"x","time":4,"value":40}),
    );
    let before = project(&ss);
    let depth = ss.undo.len();
    let result = delete(&mut ss, 1, 1);
    assert_eq!(result["removed_s"], 1.);
    assert_eq!(result["removed_words"], 1);
    assert_eq!(ss.undo.len(), depth + 1);
    let clips = &ss
        .project
        .tracks
        .iter()
        .find(|t| t.id == track)
        .unwrap()
        .clips;
    assert_eq!(clips.len(), 2);
    assert_eq!(
        (clips[0].start, clips[0].duration, clips[0].in_point),
        (0., 1., 0.)
    );
    assert_eq!(
        (clips[1].start, clips[1].duration, clips[1].in_point),
        (1., 2., 2.)
    );
    assert!((clips[1].props["x"].at(0.5) - 25.).abs() < 1e-9);
    let tr = &ss.project.editorial.transcripts["tr1"];
    assert_eq!(
        tr.script_words()
            .iter()
            .map(|w| w.display.as_str())
            .collect::<Vec<_>>(),
        ["Hello", "world", "again"]
    );
    assert_eq!(tr.words.iter().find(|w| w.id == "w3").unwrap().start, 1.);
    let mut derived = tr.clone();
    derived.derive_times();
    assert_eq!(&derived, tr);
    assert!(ss.project.dangling_refs().is_empty());
    let after = project(&ss);
    call(&mut ss, "undo", json!({}));
    assert_eq!(project(&ss), before);
    call(&mut ss, "redo", json!({}));
    assert_eq!(project(&ss), after);
    // Rebuilt take/clip bindings support another edit, not just the first cut.
    delete(&mut ss, 1, 1);
    assert_eq!(ss.project.duration(), 2.);
    assert_eq!(ss.project.editorial.transcripts["tr1"].words.len(), 2);
}

#[test]
fn all_tracks_captions_and_range_markers_follow_multiple_merged_cuts() {
    let (mut ss, _, _) = fixture();
    let other = call(&mut ss, "add_track", json!({}))["track_id"]
        .as_str()
        .unwrap()
        .to_string();
    let bed = call(
        &mut ss,
        "add_clip",
        json!({"track_id":other,"source_type":"solid","start":0,"duration":5}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cue = caption(&mut ss, &other, &["w1", "w2", "w3", "w4"], 0., 4.);
    call(
        &mut ss,
        "add_marker",
        json!({"time":1.5,"duration":2.5,"name":"range"}),
    );
    let edits = json!([{"op":"delete","from_index":1},{"op":"delete","from_index":1},{"op":"delete","from_index":3}]);
    let p = preview(&mut ss, edits.clone());
    let r = call(
        &mut ss,
        "transcript_edit_apply",
        json!({"edits":edits,"expect_revision":p["revision"]}),
    );
    assert_eq!(r["removed_s"], 2.);
    assert_eq!(r["removed_words"], 2);
    assert_eq!(ss.project.duration(), 3.);
    assert_eq!(ss.project.find_clip(&bed).unwrap().duration, 1.);
    let group = &ss.project.editorial.captions[&cue];
    assert_eq!(group.clips.len(), 2);
    let text: Vec<_> = group
        .clips
        .iter()
        .map(|id| match &ss.project.find_clip(id).unwrap().source {
            Source::Text { text, .. } => text.as_str(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(text, ["Hello", "world"]);
    assert!(ss
        .project
        .editorial
        .cue_words
        .values()
        .all(|b| !b.word_ids.contains(&"w2".into()) && !b.word_ids.contains(&"w4".into())));
    assert_eq!(
        (ss.project.markers[0].time, ss.project.markers[0].duration),
        (1., 1.)
    );
    assert!(ss.project.dangling_refs().is_empty());
}

#[test]
fn full_deletion_removes_media_captions_bindings_and_takes() {
    let (mut ss, track, clip) = fixture();
    let cue = caption(&mut ss, &track, &["w1", "w2", "w3", "w4"], 0., 4.);
    delete(&mut ss, 0, 3);
    assert!(ss.project.find_clip(&clip).is_none());
    assert!(ss.project.find_clip(&cue).is_none());
    assert!(ss.project.editorial.transcripts["tr1"].takes.is_empty());
    assert!(ss.project.editorial.transcripts["tr1"].words.is_empty());
    assert!(ss.project.editorial.cue_words.is_empty());
    assert!(ss.project.editorial.captions[&cue].clips.is_empty());
    assert!(ss.project.dangling_refs().is_empty());
}

#[test]
fn dry_run_preserves_project_ids_history_and_redo() {
    let (mut ss, _, _) = fixture();
    call(&mut ss, "add_marker", json!({"time":5,"name":"temporary"}));
    call(&mut ss, "undo", json!({}));
    let redo = ss.redo.len();
    assert!(redo > 0);
    let edits = json!([{"op":"delete","from_index":1}]);
    let p = preview(&mut ss, edits.clone());
    let before = project(&ss);
    let next = ss.project.next_id;
    let depth = ss.undo.len();
    let r = call(
        &mut ss,
        "transcript_edit_apply",
        json!({"edits":edits,"expect_revision":p["revision"],"dry_run":true}),
    );
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["revision"], p["revision"]);
    assert_eq!(project(&ss), before);
    assert_eq!(ss.project.next_id, next);
    assert_eq!(ss.redo.len(), redo);
    assert_eq!(ss.undo.len(), depth);
    let actual = call(
        &mut ss,
        "transcript_edit_apply",
        json!({"edits":edits,"expect_revision":p["revision"]}),
    );
    assert_eq!(r["created_clip_ids"], actual["created_clip_ids"]);
}

#[test]
fn stale_revision_and_invalid_index_types_are_refused_without_mutation() {
    let (mut ss, _, clip) = fixture();
    let edits = json!([{"op":"delete","from_index":1}]);
    let p = preview(&mut ss, edits.clone());
    call(
        &mut ss,
        "set_property",
        json!({"clip_id":clip,"property":"x","value":12}),
    );
    fails_unchanged(
        &mut ss,
        json!({"edits":edits,"expect_revision":p["revision"]}),
        "project_drift",
    );
    let rev = preview(&mut ss, edits.clone())["revision"].clone();
    for to in [json!(-1), json!(1.5), json!("2"), Value::Null] {
        fails_unchanged(
            &mut ss,
            json!({"edits":[{"op":"delete","from_index":1,"to_index":to}],"expect_revision":rev}),
            "to_index",
        );
    }
    fails_unchanged(
        &mut ss,
        json!({"edits":[{"op":"replace","from_index":1}],"expect_revision":rev}),
        "delete",
    );
}

#[test]
fn locked_tracks_and_ambiguous_take_bindings_are_atomic_failures() {
    let (mut ss, track, _) = fixture();
    let edits = json!([{"op":"delete","from_index":1}]);
    ss.project
        .tracks
        .iter_mut()
        .find(|t| t.id == track)
        .unwrap()
        .locked = true;
    let rev = preview(&mut ss, edits.clone())["revision"].clone();
    fails_unchanged(
        &mut ss,
        json!({"edits":edits,"expect_revision":rev}),
        "locked",
    );
    ss.project.tracks[0].locked = false;
    ss.project
        .editorial
        .transcripts
        .get_mut("tr1")
        .unwrap()
        .takes[0]
        .source
        .clip_id = None;
    let rev = preview(&mut ss, edits.clone())["revision"].clone();
    fails_unchanged(
        &mut ss,
        json!({"edits":edits,"expect_revision":rev}),
        "unbound_take",
    );
}

#[test]
fn speed_changes_external_references_and_partial_words_are_refused() {
    for kind in ["speed", "reference", "overlap", "alternate"] {
        let (mut ss, track, clip) = fixture();
        match kind {
            "speed" => ss.project.find_clip_mut(&clip).unwrap().speed = 2.,
            "reference" => {
                let c = call(
                    &mut ss,
                    "add_clip",
                    json!({"track_id":track,"source_type":"solid","start":5,"duration":1}),
                )["clip_id"]
                    .as_str()
                    .unwrap()
                    .to_string();
                ss.project.find_clip_mut(&c).unwrap().parent = Some(clip.clone());
            }
            "overlap" => {
                let tr = ss.project.editorial.transcripts.get_mut("tr1").unwrap();
                tr.words[0].src_end = 1.5;
                tr.derive_times();
            }
            "alternate" => {
                ss.project
                    .editorial
                    .transcripts
                    .get_mut("tr1")
                    .unwrap()
                    .takes[0]
                    .status = TakeStatus::Alternate
            }
            _ => unreachable!(),
        }
        let edits = json!([{"op":"delete","from_index":1}]);
        if kind == "alternate" {
            // keep a script word for constructing the plan
            let tr = ss.project.editorial.transcripts.get_mut("tr1").unwrap();
            let mut t = tr.takes[0].clone();
            t.id = "alternate".into();
            tr.takes[0].status = TakeStatus::Chosen;
            tr.takes.push(t);
        }
        let rev = preview(&mut ss, edits.clone())["revision"].clone();
        fails_unchanged(
            &mut ss,
            json!({"edits":edits,"expect_revision":rev}),
            "unsupported_edit",
        );
    }
}

#[test]
fn search_is_read_only_uses_corrected_words_and_provides_cut_edits() {
    let (mut ss, _, _) = fixture();
    let before = project(&ss);
    let depth = ss.undo.len();
    let r = call(&mut ss, "transcript_search", json!({"query":"UM!"}));
    assert_eq!(r["matches"][0]["from_index"], 1);
    assert_eq!(r["untrusted"], true);
    assert_eq!(project(&ss), before);
    assert_eq!(ss.undo.len(), depth);
    let edit = r["matches"][0]["edit"].clone();
    call(
        &mut ss,
        "transcript_edit_apply",
        json!({"edits":[edit],"expect_revision":r["revision"]}),
    );
    assert!(
        call(&mut ss, "transcript_search", json!({"query":"um"}))["matches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(
        &mut ss,
        "transcript_edit_word",
        json!({"word_id":"w1","display":"Hi"}),
    );
    assert_eq!(
        call(&mut ss, "transcript_search", json!({"query":"hi world"}))["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(dispatch(&mut ss, "transcript_search", &json!({"query":"!!!"})).is_err());
}

#[test]
fn search_limit_reports_truncation_and_respects_whole_words() {
    let (mut ss, _, _) = fixture();
    {
        let tr = ss.project.editorial.transcripts.get_mut("tr1").unwrap();
        for w in &mut tr.words {
            w.display = "um".into();
        }
    }
    let r = call(
        &mut ss,
        "transcript_search",
        json!({"query":"um um","limit":1}),
    );
    assert_eq!(r["matches"].as_array().unwrap().len(), 1);
    assert_eq!(r["truncated"], true);
    assert_eq!(r["matches"][0]["to_index"], 1);
    assert!(
        call(&mut ss, "transcript_search", json!({"query":"u"}))["matches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(dispatch(
        &mut ss,
        "transcript_search",
        &json!({"query":"um","limit":0})
    )
    .is_err());
}

#[test]
fn real_ffmpeg_media_keeps_surviving_frames_and_audio_in_sync() {
    use kimo_core::{audio, render};
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("SKIP real media proof: ffmpeg unavailable");
        return;
    }
    let (mut ss, _, _) = fixture();
    let path =
        std::env::temp_dir().join(format!("kimo_transcript_media_{}.mkv", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x36:rate=10:duration=4",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.1*sin(2*PI*(220*t+37*t*t)):s=48000:d=4",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-shortest",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    ss.project.assets.get_mut("asset1").unwrap().path = path.to_str().unwrap().to_string();
    let original = ss.project.clone();
    delete(&mut ss, 1, 1);
    for (now, then) in [(0.5, 0.5), (1.5, 2.5), (2.5, 3.5)] {
        let mut old_cache = render::Cache::new();
        render::prepare_at(&original, &mut old_cache, Some(then)).unwrap();
        let mut new_cache = render::Cache::new();
        render::prepare_at(&ss.project, &mut new_cache, Some(now)).unwrap();
        let expected = render::render_frame(&original, then, &old_cache).unwrap();
        let actual = render::render_frame(&ss.project, now, &new_cache).unwrap();
        assert_eq!(actual.d, expected.d, "video source mismatch at {now}");
    }
    let old = audio::mix_strict(&original).unwrap();
    let new = audio::mix_strict(&ss.project).unwrap();
    assert_eq!(new.len(), 3 * 48000 * 2);
    let old_pos = 5 * 48000; // stereo samples at 2.5 seconds
    let new_pos = 3 * 48000; // stereo samples at 1.5 seconds
    let fixture_difference = old[new_pos..new_pos + 2048]
        .iter()
        .zip(&old[old_pos..old_pos + 2048])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        fixture_difference > 0.01,
        "fixture must distinguish an incorrect source offset"
    );
    let error = new[new_pos..new_pos + 2048]
        .iter()
        .zip(&old[old_pos..old_pos + 2048])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(error < 1e-4, "audio alignment error {error}");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn explicit_source_bounds_bind_pretrimmed_media_and_validate_atomically() {
    let (mut ss, _, clip) = fixture();
    ss.project.find_clip_mut(&clip).unwrap().in_point = 10.;
    ss.project.find_clip_mut(&clip).unwrap().start = 3.;
    ss.project.assets.get_mut("asset1").unwrap().duration = 14.;
    ss.project.editorial.transcripts.clear();
    let path = std::env::temp_dir().join(format!(
        "kimo_trimmed_transcript_{}.csv",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "word,start,end\nHello,10,11\num,11,12\nworld,12,13\nagain,13,14\n",
    )
    .unwrap();
    call(
        &mut ss,
        "transcript_import",
        json!({"path":path.to_str().unwrap(),"take":{"clip_id":clip,"timeline_start":3,"src_start":10,"src_end":14}}),
    );
    let r = delete(&mut ss, 1, 1);
    let right = ss
        .project
        .find_clip(r["created_clip_ids"][0].as_str().unwrap())
        .unwrap();
    assert_eq!((right.start, right.in_point, right.duration), (4., 12., 2.));
    assert_eq!(
        ss.project.editorial.transcripts["tr1"]
            .words
            .iter()
            .find(|w| w.id == "w3")
            .unwrap()
            .start,
        4.
    );
    let before = project(&ss);
    let history = ss.undo.len();
    for bounds in [
        json!({"src_start":11}),
        json!({"src_end":13}),
        json!({"src_start":"10"}),
    ] {
        assert!(dispatch(
            &mut ss,
            "transcript_import",
            &json!({"path":path.to_str().unwrap(),"take":bounds})
        )
        .is_err());
        assert_eq!(project(&ss), before);
        assert_eq!(ss.undo.len(), history);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn ripple_refuses_parent_relationships_even_when_neither_layer_is_trimmed() {
    let (mut ss, track, _) = fixture();
    let parent = call(
        &mut ss,
        "add_clip",
        json!({"track_id":track,"source_type":"solid","start":0,"duration":0.5}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    let child = call(
        &mut ss,
        "add_clip",
        json!({"track_id":track,"source_type":"solid","start":5,"duration":1}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    ss.project.find_clip_mut(&child).unwrap().parent = Some(parent);
    let edits = json!([{"op":"delete","from_index":1}]);
    let p = preview(&mut ss, edits.clone());
    fails_unchanged(
        &mut ss,
        json!({"edits":edits,"expect_revision":p["revision"]}),
        "parented",
    );
}

#[test]
fn split_audio_extending_into_cut_is_refused_even_if_picture_is_outside_cut() {
    for (start, lead, tail) in [(2., 1., 0.), (0., 0., 2.)] {
        let (mut ss, track, _) = fixture();
        let id = call(
            &mut ss,
            "add_clip",
            json!({"track_id":track,"source_type":"solid","start":start,"duration":0.5}),
        )["clip_id"]
            .as_str()
            .unwrap()
            .to_string();
        let c = ss.project.find_clip_mut(&id).unwrap();
        c.audio.lead = lead;
        c.audio.tail = tail;
        let edits = json!([{"op":"delete","from_index":1}]);
        let p = preview(&mut ss, edits.clone());
        fails_unchanged(
            &mut ss,
            json!({"edits":edits,"expect_revision":p["revision"]}),
            "split-audio",
        );
    }
}

#[test]
fn posterized_source_clocks_are_not_silently_restarted_at_the_new_cut() {
    let (mut ss, _, clip) = fixture();
    ss.project.find_clip_mut(&clip).unwrap().posterize_fps = 3.;
    let edits = json!([{"op":"delete","from_index":1}]);
    let p = preview(&mut ss, edits.clone());
    fails_unchanged(
        &mut ss,
        json!({"edits":edits,"expect_revision":p["revision"]}),
        "unsupported_edit",
    );
}
