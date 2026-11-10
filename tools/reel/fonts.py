"""Prepare the caption fonts Kimo loads via KIMO_FONTS.

Kimo's text renderer reads TTFs from the directory named by KIMO_FONTS. The reel uses the
SIL OFL fonts bundled with the GUI (Geist, Geist Mono; shipped as woff2) plus the repo's test
Montserrat. This converts/copies them into <out_dir> (needs fontTools + brotli for woff2) and
checks glyph coverage for the typographic characters the captions use.

Usage: python fonts.py <repo_root> <out_dir>
"""
import os, shutil, sys

WOFF2 = ['Geist-Variable', 'GeistMono-Variable']
NEED = {0x2014: 'em dash', 0x2026: 'ellipsis', 0x2019: 'right single quote',
        0x2018: 'left single quote', 0x201C: 'left double quote', 0x201D: 'right double quote',
        0x00B7: 'middle dot', 0x25CF: 'black circle'}


def ensure_fonts(repo, out_dir):
    from fontTools.ttLib import TTFont
    os.makedirs(out_dir, exist_ok=True)
    ui = os.path.join(repo, 'crates', 'kimo-gui', 'ui', 'fonts')
    for n in WOFF2:
        dst = os.path.join(out_dir, n + '.ttf')
        if not os.path.exists(dst):
            f = TTFont(os.path.join(ui, n + '.woff2')); f.flavor = None; f.save(dst)
        cm = TTFont(dst).getBestCmap()
        missing = [NEED[c] for c in NEED if c not in cm]
        if missing:
            raise SystemExit(f'{n}: missing glyphs {missing}')
    mont = os.path.join(repo, 'crates', 'kimo-core', 'tests', 'fonts', 'Montserrat-Var.ttf')
    if os.path.exists(mont) and not os.path.exists(os.path.join(out_dir, 'Montserrat-Var.ttf')):
        shutil.copy(mont, out_dir)
    lic = os.path.join(ui, 'OFL.txt')
    if os.path.exists(lic) and not os.path.exists(os.path.join(out_dir, 'OFL.txt')):
        shutil.copy(lic, out_dir)
    return out_dir


if __name__ == '__main__':
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    print(ensure_fonts(sys.argv[1], sys.argv[2]))
