# Kimo director mode — design taste guide

For the small model driving the 35 director tools. Every tool named below is on that list
(checked live against `tools/list {"mode":"director"}` on this build); nothing here needs a tool
you don't have. Each rule names why, from KR1 (editing/pacing) or KR2 (graphics), or is marked
unverified.

## 1. The hook (first 1-2s)
Open on the sharpest cut you have, not a title card alone. 6 of 7 creators measured cut faster in
their first 15s than their body (KR1 Rule 1; up to 3.3x on Ali Abdaal) — speed itself is the hook,
before any text. Add a title (`add_title`) only if you have a real 2-4 word claim or question
("WORST $9 BOTTLE?", not "BOTTLE REVIEW" — a label is not a hook). If the footage's own first
line of dialogue already says the hook, skip the title entirely rather than stacking a flat label
over it. `style: "impact"` (huge caps rising from a line mask) or `"kinetic"` (word pop) read as
a hook; plain centered text does not. Keep it on screen 1.5-2s, never longer than the first cut.

## 2. Typography
One family for the whole edit, repeated across every `add_title`/`add_text` call — switching
fonts mid-edit is the fastest way to look amateur. `add_title` has no `size` arg: its style IS the
size choice (`impact`/`cinematic` for a hook, `lower_third` for a name/role — don't resize by hand).
For `add_text`/labels, where `size` is a real pixel number: target 9-11% of frame width per line
(roughly 65-80px on a 720-wide canvas), never below 48px. Max 3-4 words per line, 2 lines max —
matching `auto_captions`'s own `pop`-style default (`max_words_per_line`: 3). Leave caption `size`
unset; both caption tools already shrink to the safe width. Keep text out of the bottom ~12%
(platform UI) and top ~8% (status bar) — `auto_captions`'s `position: "auto"` already moves to the
top band when a face sits low; trust it over a hardcoded `"bottom"`. Never place text over a face
or the product — check with `render_frame`, since only `lint_edit`'s `text_over_subject` catches
this, and only for the single strongest subject in frame.

## 3. Color
Pull 2-3 colors by eye from a `render_frame` of the actual footage (not a stock palette) — the
wall, the product, a wardrobe color — and reuse exactly those as `accent`/`color`/`keyword_color`
across every title and caption in the edit, not a new color per graphic. `auto_grade` only fixes
exposure and cast (`target_luma` 0.2-0.7, default 0.42) — it has no warm/cool knob, so "grading
warm" in director mode means choosing a warm accent color for your text, not pushing the grade.
Run `auto_grade` once near the start at its defaults; don't fight it with manual color calls it
doesn't expose. KR1 measured every sampled creator above a raw/neutral look, from 0.155 to 0.50
mean saturation (Rule 6) — `auto_grade`'s default is close to this band; no override needed unless
`lint_edit` flags a specific dark/blown clip.

## 4. Pacing
Median shot length runs under 2.5s for high-production/entertainment footage, 3-5s for
single-camera commentary (KR1 Rule 2: 0.92-1.6s vs 4.56-4.94s measured). Let `auto_rough_cut` set
the baseline, then drop any shot running past ~5s with nothing changing on screen — that long reads
as dead, not calm. Mix to -14 LUFS (`finish_audio`'s own default) — 7 of 8 creators measured within
-12.4 to -18 LUFS (KR1 Rule 4). True punch-in zoom is rare — at most 1 per ~90s window across every
creator measured (KR1 Rule 7) — so `add_smart_punch`/`add_punch_ins` are an occasional accent, not
a per-cut habit; pattern `"alternate"`, not `"all"`.

## 5. Restraint
One graphic idea on screen at a time. Never run a title and a caption simultaneously —
`director_mode.md` already says this for titles vs labels; extend it to any two text-bearing
clips. At most one new on-screen text element (title, label, or caption cue) every 2-3s — pushing
past that reads as cluttered, not energetic. If `lint_edit` flags the same class of problem twice
(two `text_outside_safe_area`s, say), fix the cause once rather than patching each instance.

## 6. Motion
Use each tool's own built-in motion rather than improvising a new one — `add_title`'s styles
already carry a designed entrance (blur settle, line-mask rise, bounce), and `add_punch_ins`'
`ease` (seconds of push-in, 0 = hard cut, max 2) is the only exposed easing knob. Keep every title
in an edit on the same style family (don't mix `impact` and `elegant` in one short) and every
punch-in at the same `ease`/`scale` — consistency across the whole short reads as intentional,
alternating styles reads as indecisive.

## 7. Captions
Prefer `auto_captions` (word-synced, auto-positioned, Hinglish-aware) over `caption_from_words`'s
plain default unless you don't trust its recognition on this audio. Pick a style that matches the
hook energy: `"pop"` (default, 3 words/line, built-in highlight) for fast/casual content, `"clean"`
for calmer commentary, `"karaoke"` when the words themselves are the joke. Set `keywords: true`
with one `keyword_color` pulled from your palette (section 3) — this alone gives you KR2's #1
measured graphic, the keyword pop (6/8 creators, highest density on Ali Abdaal/Johnny Harris),
with no extra tool. Don't hand-color every word; 1-2 stress words per page is the measured norm.
Always run `lint_edit` after captions and fix every `caption_low_confidence` cue it names before
export — a wrong word on screen is worse than no caption.

## 8. Self-critique before export
Before the final `export`, call `contact_sheet {}` (several timestamps, one grid) and look at it.
Check, in order: (1) is exactly one text element on screen at any instant — never two; (2) does
every title/label clear faces and the product; (3) is the same font/style family used throughout;
(4) is there a real hook claim in the first 1.5s, not a plain label; (5) run `lint_edit` one more
time and treat any remaining item as a real problem, not a suggestion. Fix the single worst thing
you see, then `export` again — don't rebuild from scratch.

## Baseline critique: tonight's two outputs against this guide
Both baselines read as amateur footage with professional audio, not a creator's edit. The hook
titles are plain labels, not claims — "BOTTLE REVIEW" (/home/user/tab/tasks/81b7b466-b2b9-4701-87b9-fa561c560b87/frames/real_review_0_0.0s.jpg) and "DESK
VLOG" (/home/user/tab/tasks/81b7b466-b2b9-4701-87b9-fa561c560b87/frames/real_vlog_0_0.0s.jpg) are centered white caps with no outline, no accent color pulled
from either scene, and say what the video is rather than giving a reason to watch (Rule 1 fix:
replace with a real claim via `add_title(style="impact")`, sized by style, 1.5-2s only). The
captions are the generic `caption_from_words` default boxed style — a flat dark bar spanning
nearly the full frame width with no keyword emphasis, visible across every caption frame sampled
(e.g. /home/user/tab/tasks/81b7b466-b2b9-4701-87b9-fa561c560b87/frames/real_review_3_13.0s.jpg, /home/user/tab/tasks/81b7b466-b2b9-4701-87b9-fa561c560b87/frames/real_vlog_3_9.2s.jpg); switching to
`auto_captions(style="pop", keywords=true, keyword_color=<palette accent>)` would fix both the
box-spanning look and the missing keyword-pop graphic in one call (section 7). No accent color
repeats between the title and the captions in either video — two different generic whites instead
of one pulled palette (section 3). Pacing wasn't checked frame-by-frame here, but nothing in either
export suggests a faster-cut hook versus body (section 4/KR1 Rule 1) — worth confirming on the next
tuned run.
