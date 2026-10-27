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


# ---------------------------------------------------------------------------
# MT8: (1) don't invite retitle/restyle when no position fixes legibility,
# (2) safe-area check via compute_text_bounds + query_safe_margins,
# (3) accent consistency read across every caption clip at final export,
# (4) a minimum accent luminance so a dark "matching" accent still fails.
# Still no kimo-mcp binary or network needed: tool calls are faked.
# ---------------------------------------------------------------------------

def check_mt8():
    failures = []

    def expect(label, cond):
        print(("PASS " if cond else "FAIL ") + label)
        if not cond:
            failures.append(label)

    # --- (1) legibility_notable_violations: the pure decision the corrective note is built from.
    expect("no position violations -> nothing to exclude, returns them unchanged (empty)",
           agent.legibility_notable_violations([], None) == [])
    expect("position violations + a passing y found -> kept in the note",
           agent.legibility_notable_violations(["contrast too low"], -300.0) == ["contrast too low"])
    expect("position violations + NO passing y -> dropped from the note entirely (no retitle/restyle invite)",
           agent.legibility_notable_violations(["contrast too low"], None) == [])
    expect("placement-only violation + no passing y -> also dropped",
           agent.legibility_notable_violations(["title sits near centre"], None) == [])

    # --- (4) accent luminance floor, exercised through taste_check (rule 4), same entry point the
    # harness actually calls. A state that is otherwise fully compliant (matching accent everywhere)
    # should only fail on luminance when the colour itself is too dark.
    def state_with_accent(accent):
        return {"title_text": "NOT THAT BAD", "title_style": "impact", "title_accent": accent, "title_color": None,
                "caption_tool": "auto_captions", "caption_keywords": True, "caption_keyword_color": accent, "caption_active_color": accent}

    dark_navy = (0.2, 0.35, 0.65)  # the unseen2 re-run's actual pick
    v_dark = agent.taste_check(state_with_accent(dark_navy), agent.BANNED_LABEL_WORDS)
    expect(f"dark navy {dark_navy} (luminance {agent._rel_luminance(dark_navy):.2f}) fails the luminance rule",
           any("luminance" in x for x in v_dark))

    bright_orange = (0.98, 0.55, 0.18)  # real_vlog's accent, already shipped and known-good
    v_bright = agent.taste_check(state_with_accent(bright_orange), agent.BANNED_LABEL_WORDS)
    expect(f"bright orange {bright_orange} (luminance {agent._rel_luminance(bright_orange):.2f}) passes the luminance rule",
           not any("luminance" in x for x in v_bright))

    lavender = (0.76, 0.52, 0.99)  # real_review's accent, already shipped and known-good
    v_lav = agent.taste_check(state_with_accent(lavender), agent.BANNED_LABEL_WORDS)
    expect(f"the already-shipped lavender accent {lavender} still passes (no regression on existing clips)",
           not any("luminance" in x for x in v_lav))

    # --- (2) safe_area_check: a fake Kimo returns a fixed compute_text_bounds/query_safe_margins pair.
    class _FakeKimoSafe:
        def __init__(self, rect, sm_result):
            self.rect = rect; self.sm_result = sm_result; self.calls = []

        def call(self, name, args, timeout=600):
            self.calls.append((name, args))
            if name == "compute_text_bounds":
                return False, _json.dumps({"canvas_rect": self.rect}), []
            if name == "query_safe_margins":
                return False, _json.dumps(self.sm_result), []
            raise AssertionError(f"unexpected call: {name}")

    title_state = {"title_clip_id": "clip35", "title_start": 0.0, "title_duration": 1.8, "title_x": 80.0}
    # clipped off the right edge -- the exact numbers measured on MT7's unseen2 run.
    # the real shape query_safe_margins actually returns (verified live against the kimo-mcp binary on
    # MT7's unseen2 project): suggested_shift lives per-zone, not directly on the overlap.
    sm_clipped = {"rects_outside_safe_max_pct": 11.91, "worst_edge": "right", "worst_edge_px": 76.0,
                  "overlaps": [{"outside_canvas_pct": 6.27, "zones": [{"suggested_shift": {"dx": -76.0, "dy": 0.0}}]}]}
    kf = _FakeKimoSafe([122.0, 263.0, 638.0, 53.0], sm_clipped)
    v_safe, d_safe = agent.safe_area_check(kf, title_state, "/tmp")
    expect("safe_area_check flags a title box that query_safe_margins says is outside the safe area", len(v_safe) == 1)
    expect("the violation names the measured suggested x (80 + -76 = 4)", "value: 4" in v_safe[0] or "value: 4.0" in v_safe[0])
    expect("detail carries the raw query_safe_margins result for the report", d_safe.get("query_safe_margins") == sm_clipped)

    sm_clean = {"rects_outside_safe_max_pct": 0.0, "overlaps": [{"outside_canvas_pct": 0.0, "suggested_shift": {"dx": 0.0, "dy": 0.0}}]}
    kf2 = _FakeKimoSafe([36.0, 213.0, 400.0, 53.0], sm_clean)
    v_safe2, _ = agent.safe_area_check(kf2, title_state, "/tmp")
    expect("safe_area_check passes a title box fully inside the safe area", v_safe2 == [])

    # --- (3) accent_consistency_check: reads caption clip colours from a fake project state
    # (_project_state monkeypatched), independent of whatever taste_state's own last-call tracking saw.
    orig_project_state = agent._project_state
    title_accent = (0.2, 0.55, 0.95)  # a blue accent, clearly unlike red on every channel
    stale_red = [1.0, 0.0, 0.0]  # "NEXT" stayed on the OLD accent -- MT8 bug (c)
    fake_project = {
        "editorial": {"captions": {"g1": {"track_id": "track36"}}},
        "tracks": [{"id": "track36", "clips": [
            {"id": "clip37", "source": {"text": "Hi I'm Maya"}, "text_fx": {"animators": [
                {"id": "kw0", "fill": list(title_accent)}, {"id": "active", "fill": list(title_accent)}]}},
            {"id": "clip44", "source": {"text": "NEXT"}, "text_fx": {"animators": [
                {"id": "kw0", "fill": stale_red}, {"id": "active", "fill": stale_red}]}},
        ]}],
    }
    agent._project_state = lambda out_dir: fake_project
    try:
        state_match = {"title_accent": title_accent, "title_color": None}
        v_acc, d_acc = agent.accent_consistency_check(state_match, "/tmp")
        expect("accent_consistency_check catches the one caption clip left on the old red accent", len(v_acc) == 1 and "clip44" in v_acc[0])
        expect("detail lists both clips read from the project", set(d_acc["clips"].keys()) == {"clip37", "clip44"})

        fake_project_fixed = {**fake_project, "tracks": [{"id": "track36", "clips": [
            {"id": "clip37", "source": {"text": "Hi I'm Maya"}, "text_fx": {"animators": [
                {"id": "kw0", "fill": list(title_accent)}, {"id": "active", "fill": list(title_accent)}]}},
            {"id": "clip44", "source": {"text": "NEXT"}, "text_fx": {"animators": [
                {"id": "kw0", "fill": list(title_accent)}, {"id": "active", "fill": list(title_accent)}]}},
        ]}]}
        agent._project_state = lambda out_dir: fake_project_fixed
        v_acc_ok, _ = agent.accent_consistency_check(state_match, "/tmp")
        expect("accent_consistency_check passes once every caption clip matches the title accent", v_acc_ok == [])
    finally:
        agent._project_state = orig_project_state

    if failures:
        print(f"\n{len(failures)} FAILED")
        sys.exit(1)
    print(f"\nall {13} MT8 checks passed")



# ---------------------------------------------------------------------------
# OUT1: add_title's own outline_width/outline_color (stroke, off by default, same Source::Text path
# auto_captions already draws with), and the harness that applies it itself once find_passing_y has
# proved no title position fixes contrast against this background.
# Still no kimo-mcp binary or network needed: tool calls are faked.
# ---------------------------------------------------------------------------


class _FakeKimoOutline:
    """Stands in for mcp.Kimo for apply_outline_fix: records delete_clip/add_title/set_property/
    save_project/export calls in order, returns a fresh clip id from add_title, and lets a test force
    any one call to fail by name."""

    def __init__(self, fail_on=None, new_clip_id="clip99"):
        self.calls = []
        self.fail_on = fail_on or set()
        self.new_clip_id = new_clip_id

    def call(self, name, args, timeout=600):
        self.calls.append((name, args))
        if name in self.fail_on:
            return True, f"{name} failed (forced)", []
        if name == "delete_clip":
            return False, "{}", []
        if name == "add_title":
            return False, _json.dumps({"clip_ids": [self.new_clip_id], "style": args.get("style")}), []
        if name == "set_property":
            return False, "{}", []
        if name == "save_project":
            return False, "{}", []
        if name == "export":
            return False, "{}", []
        raise AssertionError(f"_FakeKimoOutline got an unexpected call: {name}")


def _outline_state(**over):
    s = {"title_clip_id": "clip18", "title_track_id": "track1", "title_text": "BEST BOWL THIS YEAR?",
         "title_style": "impact", "title_color": None, "title_accent": None, "title_subtitle": None,
         "title_font": None, "title_start": 0.0, "title_duration": 4.0, "title_y": -400.0, "title_x": 0.0}
    s.update(over)
    return s


def check_out1():
    failures = []

    def expect(label, cond):
        print(("PASS " if cond else "FAIL ") + label)
        if not cond:
            failures.append(label)

    orig_measure = agent._measure_contrast_at_y

    # --- (a) the Rust contract this harness change leans on: add_title's new args default to
    # no outline, so this lane never changes default renders -- just records intent here since the
    # Rust side is exercised by the Rust test suite, not this Python one.
    expect("HARNESS_OUTLINE_WIDTH_AT_1280H is a small positive px value, not accidentally 0",
           agent.HARNESS_OUTLINE_WIDTH_AT_1280H > 0)
    expect("HARNESS_OUTLINE_COLOR defaults to black, same as auto_captions' own stroke",
           tuple(agent.HARNESS_OUTLINE_COLOR) == (0.0, 0.0, 0.0))

    # --- (b) happy path: deletes the old title, recreates it with outline_width/outline_color on the
    # SAME text/style/position/accent, restores y/x, re-measures, re-exports -- and updates the
    # harness's own state to the new clip id (so a later followup step doesn't touch a deleted clip).
    agent._measure_contrast_at_y = lambda k_, cid, t, out, c, tag="", stroke_c=None: {"ratio_p10": 4.1, "ratio_mean": 5.0}
    try:
        k = _FakeKimoOutline(new_clip_id="clip99")
        state = _outline_state()
        detail = agent.apply_outline_fix(k, state, "/tmp")
    finally:
        agent._measure_contrast_at_y = orig_measure
    expect("apply_outline_fix reports attempted", detail["attempted"] is True)
    expect("apply_outline_fix deletes the old title clip first", k.calls[0] == ("delete_clip", {"clip_id": "clip18"}))
    add_call = next(c for c in k.calls if c[0] == "add_title")
    expect("add_title is called with the SAME text/style", add_call[1]["text"] == "BEST BOWL THIS YEAR?" and add_call[1]["style"] == "impact")
    expect("add_title is called with a non-zero outline_width and a colour", add_call[1]["outline_width"] > 0 and "outline_color" in add_call[1])
    expect("apply_outline_fix restores the original y/x on the new clip",
           any(c == ("set_property", {"clip_id": "clip99", "property": "y", "value": -400.0}) for c in k.calls)
           and any(c == ("set_property", {"clip_id": "clip99", "property": "x", "value": 0.0}) for c in k.calls))
    expect("apply_outline_fix updates the harness's own state to the new clip id", state["title_clip_id"] == "clip99")
    expect("apply_outline_fix records the new clip id in its own detail", detail["new_clip_id"] == "clip99")
    expect("apply_outline_fix records the stroke-aware measurement", detail["ratio_p10_with_outline"] == 4.1)
    expect("apply_outline_fix re-exports after applying the outline", any(c[0] == "export" for c in k.calls))
    expect("apply_outline_fix saves the project after applying the outline", any(c[0] == "save_project" for c in k.calls))

    # --- (c) missing state (no title_clip_id/track_id/text/style) refuses rather than guessing.
    k2 = _FakeKimoOutline()
    detail2 = agent.apply_outline_fix(k2, {"title_clip_id": None, "title_track_id": "track1"}, "/tmp")
    expect("apply_outline_fix skips cleanly when the title's own state is incomplete", detail2["attempted"] is False and "skipped_reason" in detail2)
    expect("apply_outline_fix makes no calls at all when it skips", k2.calls == [])

    # --- (d) a delete_clip failure stops before add_title ever runs (never creates a second title).
    k3 = _FakeKimoOutline(fail_on={"delete_clip"})
    detail3 = agent.apply_outline_fix(k3, _outline_state(), "/tmp")
    expect("a delete_clip failure is reported and not attempted", detail3["attempted"] is False and "delete_error" in detail3)
    expect("a delete_clip failure never calls add_title", not any(c[0] == "add_title" for c in k3.calls))

    # --- (e) an add_title failure after a successful delete is reported, not swallowed.
    k4 = _FakeKimoOutline(fail_on={"add_title"})
    detail4 = agent.apply_outline_fix(k4, _outline_state(), "/tmp")
    expect("an add_title failure is reported with the tool's own error", "add_title_error" in detail4)

    # --- (f) the stroke-aware contrast measurement itself: a light background where the plain WHITE
    # fill reads poorly but a BLACK outline reads well must report the BETTER of the two, not the fill
    # alone -- this is the actual perceptual claim an outline makes. Built on a real tiny PNG + a fake
    # compute_text_bounds/render_frame, the same style the MT8 safe-area tests already use (no kimo
    # binary needed).
    import tempfile
    from PIL import Image
    bg_path_holder = {}

    class _FakeKimoMeasure:
        def __init__(self, rect):
            self.rect = rect

        def call(self, name, args, timeout=600):
            if name == "compute_text_bounds":
                return False, _json.dumps({"canvas_rect": self.rect}), []
            if name == "render_frame":
                im = Image.new("RGB", (20, 20), (235, 235, 235))  # light grey background, fails on white fill
                im.save(args["path"])
                bg_path_holder["path"] = args["path"]
                return False, "{}", []
            raise AssertionError(f"unexpected call: {name}")

    with tempfile.TemporaryDirectory() as td:
        kmeasure = _FakeKimoMeasure([0.0, 0.0, 20.0, 20.0])
        white = (1.0, 1.0, 1.0)
        fill_only = agent._measure_contrast_at_y(kmeasure, "clip1", 0.5, td, white, tag="_fillonly")
        with_stroke = agent._measure_contrast_at_y(kmeasure, "clip1", 0.5, td, white, tag="_stroke", stroke_c=(0.0, 0.0, 0.0))
        fill_only_again = agent._measure_contrast_at_y(kmeasure, "clip1", 0.5, td, white, tag="_again")
    expect("a white fill alone on a light-grey background measures poor contrast", fill_only["ratio_p10"] < 2.0)
    expect("adding a black stroke raises the measured ratio (the better of fill/stroke per sampled point)",
           with_stroke["ratio_p10"] > fill_only["ratio_p10"])
    expect("a black stroke's own high-contrast number is what's reported once added",
           with_stroke["ratio_p10"] > 10.0)
    expect("without a stroke_c argument, the measurement is UNCHANGED from before this lane (default None)",
           "ratio_p10" in fill_only and fill_only == fill_only_again)

    if failures:
        print(f"\n{len(failures)} FAILED")
        sys.exit(1)
    print(f"\nall {11} OUT1 checks passed")


if __name__ == "__main__":
    check()
    check_mt7()
    check_mt8()
    check_out1()
