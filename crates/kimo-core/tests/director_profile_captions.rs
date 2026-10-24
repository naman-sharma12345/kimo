//! KM1: director-mode profile must expose auto_captions. director_mode.md step 7 and
//! SMALL_MODEL_PREP.md's own "failure (seen in the eval)" row tell a small model to call
//! auto_captions whenever the brief's transcript is not trustworthy (accented/informal
//! speech) -- but the director tool profile a small model actually sees over MCP
//! (tools/list {mode: "director"}) never listed it, so a real small model restricted to
//! that profile could never call it and would fall back to caption_from_words with its
//! own guessed words -- precisely the nonsense-caption failure mode auto_captions exists
//! to fix (see tools/director_eval/demo/ "bohozhada"/"kettlebanyan" case).
use kimo_core::toolgroups::DIRECTOR;

#[test]
fn director_profile_lists_auto_captions() {
    assert!(
        DIRECTOR.iter().any(|(name, _)| *name == "auto_captions"),
        "director profile must list auto_captions: director_mode.md tells a small model to \
         prefer it over caption_from_words whenever the brief's transcript isn't trustworthy, \
         but the tool is missing from the profile a small model actually sees via tools/list"
    );
}
