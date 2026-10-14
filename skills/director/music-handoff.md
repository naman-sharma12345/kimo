# Music timing, readability and evidence handoff

A song-led edit takes its clock from the approved master. Identify phrase starts,
phrase ends, pauses, accents and releases before allocating coverage. Detected
onsets are candidates, not proven beats or downbeats. Preserve uncertainty and
source evidence; do not turn an algorithm's guess into a lyric timestamp.

## Phrase and cue contracts
A cue has an ID, absolute time, kind, basis and evidence. A shot can refer to a cue
with a declared offset. A lead-in cut is valid if intentional, not an invisible
"always two frames early" rule. Re-audit cue placement after inserting a title,
changing speed, deleting a segment or replacing the song. Keep the approved audio
clock unless the user asked for a changed edit. Timing validation cannot prove
that an accent is actually heard at the claimed time.

For mouth-sync, bind a complete vocal phrase and compare the final exported
window against the source audio. A stable mouth plate and eligible geometry do
not prove matching phonemes. An intermediate take does not prove the final mux.
Document proof as unverified until the final artifact is inspected. No fixed
3-4s quota or compulsory singer-at-every-climax rule is applied.

## Readability budget
Give a visual beat enough time for anticipation, the action and the viewer's
recognition. Explicit `minimum_readable_seconds` is a director's estimate, not
an objectively measured rule. If the planned interval is shorter, report it as a
review warning: reduce action, change coverage or revise the estimate. A detail
insert, gesture, fast body action and text hold need different budgets.

## Assets have different jobs
A clean identity/wardrobe sheet supports asset review. A single cinematic
keyframe supports the planned shot. A labeled grid supports a human review.
Do not animate a contact sheet as if it were a single frame. State which role a
reference serves, and never assume one provider consumes all these roles.
Prompts compile from a continuity contract; they do not overwrite it. A model
family or prompt dialect is not an account credential or generation grant.

## Takes and handoff
Preserve every source take. Record selected, rejected, revision-needed or
unreviewed status with reasons. Selection needs actual media inspection and a
source range; metadata alone cannot prove identity, rhythm or optics. Avoid
choosing only the newest or lowest-defect take: weigh the intended story/action
and blocking requirements as well. Do not hide missing coverage with fabricated
media or silently delete failed takes.

A review record names scene, media, checked frames/audio, status and blockers.
Paths are evidence pointers, not proof the evidence exists or was read. The
validator reports planned structure; it never certifies a render or silently
promotes unverified pointers. WIP contracts can be shared on WIP branches, with
unit-test status and absent real-frame checks stated. Nothing is described as
production-ready solely because a JSON file passes.

## Source practices not carried over
No automatic queue upload, provider API execution, cloud GPU workflow, parallel
paid generation, stored chat secrets, source-media overwrite, algorithmically
forced cutting, asset-grid animation or blanket generated-audio replacement.
Provider defaults and personal project locks in source repos remain their own
context, not instructions for Kimo.
