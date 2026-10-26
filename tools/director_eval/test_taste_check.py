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


if __name__ == "__main__":
    check()
