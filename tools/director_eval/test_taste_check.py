"""Tiny synthetic test for the Stage C taste_check / brief_banned_words logic (MT5).

Run directly: python3 tools/director_eval/test_taste_check.py
No kimo-mcp binary or network needed -- this only exercises the pure Python
banned-word and taste_check logic against synthetic tool-call state dicts.
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import agent  # noqa: E402

REAL_REVIEW_BRIEF = (
    'BRIEF: a 15-25 s 9:16 short ... Open with a short title naming the review '
    '(2-4 words, e.g. "BOTTLE REVIEW"). Captions for every spoken word ...'
)
REAL_VLOG_BRIEF = (
    'BRIEF: a 12-20 s 9:16 short ... Open with a short title (2-4 words, '
    'e.g. "DESK VLOG"). Captions for every spoken word ...'
)


def base_state(title):
    """A minimal state dict that otherwise fully satisfies rules 2-5, so the only thing
    under test in these cases is the title/banned-word check (rule 1)."""
    return {
        "title_text": title,
        "title_style": "impact",
        "title_accent": (0.76, 0.52, 0.99),
        "title_color": None,
        "caption_tool": "auto_captions",
        "caption_keywords": True,
        "caption_keyword_color": (0.76, 0.52, 0.99),
        "caption_active_color": (0.76, 0.52, 0.99),
    }


def check():
    failures = []

    def expect(label, cond):
        print(("PASS " if cond else "FAIL ") + label)
        if not cond:
            failures.append(label)

    # (1) brief_banned_words only takes the trailing genre word, not the whole example.
    rev_words = agent.brief_banned_words(REAL_REVIEW_BRIEF)
    vlog_words = agent.brief_banned_words(REAL_VLOG_BRIEF)
    expect('brief_banned_words("...BOTTLE REVIEW...") == {"REVIEW"}', rev_words == {"REVIEW"})
    expect('brief_banned_words("...DESK VLOG...") == {"VLOG"}', vlog_words == {"VLOG"})
    expect('"BOTTLE" is not banned by the real_review brief', "BOTTLE" not in rev_words)
    expect('"DESK" is not banned by the real_vlog brief', "DESK" not in vlog_words)

    # (2) end-to-end through taste_check, the thing the harness actually calls at export time.
    banned_review = agent.BANNED_LABEL_WORDS | rev_words
    banned_vlog = agent.BANNED_LABEL_WORDS | vlog_words

    v_good = agent.taste_check(base_state("THE BEST BOTTLE"), banned_review)
    expect('"THE BEST BOTTLE" passes the real_review taste_check (was a false-positive before MT5)',
           not any("banned category word" in x for x in v_good))

    v_bad = agent.taste_check(base_state("BOTTLE REVIEW"), banned_review)
    expect('"BOTTLE REVIEW" still fails the real_review taste_check',
           any("banned category word" in x for x in v_bad))

    v_vlog_good = agent.taste_check(base_state("MY DESK SETUP"), banned_vlog)
    expect('"MY DESK SETUP" passes the real_vlog taste_check (DESK is a product/topic noun, not a label)',
           not any("banned category word" in x for x in v_vlog_good))

    v_vlog_bad = agent.taste_check(base_state("DESK VLOG"), banned_vlog)
    expect('"DESK VLOG" still fails the real_vlog taste_check',
           any("banned category word" in x for x in v_vlog_bad))

    v_fixed_label = agent.taste_check(base_state("QUICK UNBOXING"), banned_review)
    expect('a fixed-list label word (UNBOXING) still fails regardless of the brief text',
           any("banned category word" in x for x in v_fixed_label))

    if failures:
        print(f"\n{len(failures)} FAILED")
        sys.exit(1)
    print(f"\nall {7} checks passed")


# ---------------------------------------------------------------------------
# MT7: find_passing_y's candidate selection (incl. the caption-band skip) and
# the no-op / unresolved-fix detection the harness safety net relies on.
# Still no kimo-mcp binary or network needed: _measure_contrast_at_y and
# caption_y_values are monkeypatched so only find_passing_y's own selection
# logic (order, skip, stop-on-first-pass, restore-on-no-pass) is under test.
# ---------------------------------------------------------------------------
import json as _json


class _FakeKimo:
    """Stands in for mcp.Kimo: find_passing_y only ever calls set_property directly on
    a real Kimo (the compute_text_bounds/render_frame calls happen inside
    _measure_contrast_at_y, which these tests monkeypatch out) -- so recording
    set_property's value is all this fake needs to do."""

    def __init__(self):
        self.set_calls = []

    def call(self, name, args, timeout=600):
        if name == "set_property":
            self.set_calls.append(args["value"])
            return False, "{}", []
        raise AssertionError(f"_FakeKimo got an unexpected call: {name}")


def _title_state(y=-400.0, clip_id="clip18"):
    return {"title_clip_id": clip_id, "title_start": 0.0, "title_duration": 1.8, "title_color": None, "title_y": y}


def check_mt7():
    failures = []

    def expect(label, cond):
        print(("PASS " if cond else "FAIL ") + label)
        if not cond:
            failures.append(label)

    orig_measure = agent._measure_contrast_at_y
    orig_caption_y = agent.caption_y_values

    # --- (a) candidate selection: picks the first candidate (in LEGIBILITY_Y_CANDIDATES order)
    # that actually passes, and stops there without testing the rest.
    table_a = {-400.0: 2.1, -300.0: 2.5, -500.0: 4.2, 250.0: 9.9, 350.0: 9.9}
    k = _FakeKimo()
    agent.caption_y_values = lambda proj: []
    agent._measure_contrast_at_y = lambda k_, cid, t, out, c, tag="": {"ratio_p10": table_a[k_.set_calls[-1]]}
    try:
        passing_y, detail = agent.find_passing_y(k, _title_state(), "/tmp")
    finally:
        agent._measure_contrast_at_y = orig_measure; agent.caption_y_values = orig_caption_y
    expect("find_passing_y stops at the first passing candidate (-500, the 3rd tried)", passing_y == -500.0)
    expect("find_passing_y does not test candidates after the first pass (250, 350 skipped)",
           len(detail["candidates_tried"]) == 3)
    expect("find_passing_y tried candidates in LEGIBILITY_Y_CANDIDATES order (-400, -300, -500)",
           k.set_calls == [-400.0, -300.0, -500.0])
    expect("find_passing_y marks any_passed True when one did", detail["any_passed"] is True)
    expect("find_passing_y's best_ratio_p10 is the winning candidate's own measured ratio",
           detail["best_ratio_p10"] == 4.2 and detail["best_y"] == -500.0)

    # --- (b) caption-band skip: a candidate that sits on top of a live caption clip (read via
    # caption_y_values) is skipped outright, never set_property'd or measured -- and when every
    # remaining candidate still fails, the clip is restored to its ORIGINAL y, not left on the
    # last-tried loser (MT7 rule 3: no candidate passing means leave the edit as is).
    table_b = {-400.0: 2.0, -300.0: 2.0, -500.0: 2.0}
    k2 = _FakeKimo()
    agent.caption_y_values = lambda proj: [256.0]  # this run's auto-picked bottom caption band
    agent._measure_contrast_at_y = lambda k_, cid, t, out, c, tag="": {"ratio_p10": table_b[k_.set_calls[-1]]}
    try:
        passing_y2, detail2 = agent.find_passing_y(k2, _title_state(y=-400.0), "/tmp")
    finally:
        agent._measure_contrast_at_y = orig_measure; agent.caption_y_values = orig_caption_y
    expect("candidates near the caption band (250, 350 within 140px of y=256) are skipped, not measured",
           all(c.get("skipped_caption_band") for c in detail2["candidates_tried"] if c["y"] in (250.0, 350.0)))
    expect("a skipped candidate never calls set_property", 250.0 not in k2.set_calls and 350.0 not in k2.set_calls)
    expect("no candidate passes here, so find_passing_y returns None", passing_y2 is None)
    expect("none-pass is reported honestly (any_passed False)", detail2["any_passed"] is False)
    expect("none-pass restores the ORIGINAL y (-400), not the last candidate tried (-500)",
           k2.set_calls[-1] == -400.0 and detail2.get("restored_y") == -400.0)

    # --- (c) the no-op / unresolved-fix signal the harness safety net gates on: true exactly when
    # the post-correction legibility/placement re-check still finds a problem (a y no-op, a garbled
    # turn with no re-export, and a real-but-still-bad re-export all look the same here).
    expect("no violations left -> harness does not need to step in", agent.followup_needs_harness_fix([], []) is False)
    expect("a legibility violation alone -> harness needs to step in",
           agent.followup_needs_harness_fix(["contrast too low"], []) is True)
    expect("a placement violation alone -> harness needs to step in",
           agent.followup_needs_harness_fix([], ["title overlaps subject"]) is True)
    expect("a model that re-set the SAME y (classic no-op) with the violation still present -> needs the harness fix",
           agent.followup_needs_harness_fix(["contrast too low"], []) is True)

    if failures:
        print(f"\n{len(failures)} FAILED")
        sys.exit(1)
    print(f"\nall {13} MT7 checks passed")


if __name__ == "__main__":
    check()
    check_mt7()
