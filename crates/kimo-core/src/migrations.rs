//! Versioned project-file migrations (FEATURES.md L560: "project format v2 with compositions, versioned
//! migrations"). Nested compositions already exist (Source::Precomp, FEATURES.md's L473 neighbourhood) --
//! the missing half was a real migration *mechanism*: every schema change to date has been purely additive
//! (a new field read via #[serde(default)], so an old file loads unchanged with the new field at its
//! default) which never needed one. This is the first schema bump (1 -> 2) and its one real migration,
//! proving the mechanism on a real legacy fixture rather than a synthetic one.
//!
//! v1 -> v2: a track saved before this pass could carry name: "" (add_track has always created tracks with
//! no name; renaming was opt-in via set_track/set_meta). Give every still-unnamed track on a v1 file a
//! readable default ("Track 1", "Track 2", ... in the project's own bottom-to-top stacking order) the first
//! time it loads under this build -- the same name a freshly created v2 project would never need because
//! nothing about empty-string names changes rendering or compositing, only how the project reads.
//!
//! Migrations run on the raw JSON `Value` (before it becomes a `Project`), in load_project, keyed off the
//! file's own `kimo.schema_version` header (absent header = 1, the version every file has carried until
//! now). `migrate` walks forward one version at a time so a file several versions behind picks up every
//! migration between its version and this build's `SCHEMA_VERSION`, in order, even as more are added later.
use serde_json::Value;

/// One migration: the schema_version a file must be AT for this to apply (it upgrades from that version to
/// version+1), a note (what/why, for anyone reading the registry), and the transform itself.
type Migration = fn(&mut Value);
pub const MIGRATIONS: &[(u32, &str, Migration)] = &[
    (1, "fill blank track names (empty string) with Track N, bottom-to-top stacking order -- cosmetic only, no rendering/compositing effect", migrate_v1_track_names),
];

fn migrate_v1_track_names(v: &mut Value) {
    let Some(tracks) = v.get_mut("tracks").and_then(|t| t.as_array_mut()) else { return; };
    for (i, t) in tracks.iter_mut().enumerate() {
        let blank = t.get("name").and_then(|n| n.as_str()).map(|n| n.is_empty()).unwrap_or(true);
        if blank { if let Some(o) = t.as_object_mut() { o.insert("name".into(), Value::String(format!("Track {}", i + 1))); } }
    }
}

/// Apply every migration from `from_version` up to `crate::buildinfo::SCHEMA_VERSION`, in order, mutating
/// `v` in place. Returns how many ran (0 for a file already at the current version). The caller is
/// responsible for refusing a file whose version is NEWER than this build first (load_project does, before
/// calling this) -- a newer file has nothing here to walk forward from.
pub fn migrate(v: &mut Value, from_version: u32) -> usize {
    let mut n = 0;
    let mut cur = from_version;
    while cur < crate::buildinfo::SCHEMA_VERSION {
        let Some((_, _note, f)) = MIGRATIONS.iter().find(|(from, _, _)| *from == cur) else { break; };
        f(v);
        n += 1;
        cur += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v1_blank_tracks_get_readable_names() {
        let mut v = json!({"tracks": [{"id": "t1", "clips": [], "locked": false, "muted": false, "name": ""}, {"id": "t2", "clips": [], "locked": false, "muted": false, "name": "Keep Me"}, {"id": "t3", "clips": [], "locked": false, "muted": false}]});
        let n = migrate(&mut v, 1);
        assert_eq!(n, 1);
        assert_eq!(v["tracks"][0]["name"], "Track 1");
        assert_eq!(v["tracks"][1]["name"], "Keep Me", "a track that already has a name is left alone");
        assert_eq!(v["tracks"][2]["name"], "Track 3", "a track with no name field at all (older than name existing) also gets one");
    }

    #[test]
    fn a_file_already_at_the_current_version_is_untouched() {
        let mut v = json!({"tracks": [{"id": "t1", "clips": [], "locked": false, "muted": false, "name": ""}]});
        let n = migrate(&mut v, crate::buildinfo::SCHEMA_VERSION);
        assert_eq!(n, 0);
        assert_eq!(v["tracks"][0]["name"], "", "no migration ran, so nothing changed");
    }

    #[test]
    fn a_project_with_no_tracks_field_is_a_no_op_not_a_panic() {
        let mut v = json!({"width": 64});
        let n = migrate(&mut v, 1);
        assert_eq!(n, 1, "the migration still counts as applied -- it just had nothing to do");
        assert_eq!(v, json!({"width": 64}));
    }
}
