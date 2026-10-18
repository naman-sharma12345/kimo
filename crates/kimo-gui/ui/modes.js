// The sixteen modes. Rects are [x0, y0, x1, y1] measured on the 1376x768
// mockups (page coordinates, border box). `top`/`foot` are the top bar and
// footer heights for that screen. Panels name a renderer kind (panels.js).
const AG = (o = {}) => ({ kind: 'agent', title: 'Agent', opts: { rowH: 37, top: 42, noCheckCur: true, ...o } });

export const MODES = {
  edit: { activity: '#calls', top: 46, foot: 27,
    rects: { agent: [10, 55, 377, 480], viewer: [387, 55, 1070, 480], insp: [1080, 55, 1365, 480], tl: [10, 489, 1365, 741] },
    panels: { agent: AG({ count: true, fade: 5 }), viewer: { kind: 'viewer', opts: { pad: [14, 14, 42, 15] } }, insp: { kind: 'kv', title: 'Inspector', opts: { lines: true } },
      tl: { kind: 'timeline', opts: { x0: 57, xr: 9, rulerY: 6, trackY: 38, trackH: 35, trackGap: 9.5, phTop: 16, phBot: 0 } } } },
  captions: { activity: 'captions', top: 48, foot: 27,
    vars: { '--call-fs': '16px', '--call-ls': '-.085em', '--dur-fs': '14px', '--ck-fs': '14px', '--call-pr': '12px', '--cur-ck': 'none', '--args': '#b3afa5', '--fn': 'var(--cream)', '--dur-w': '4ch', '--call-gap': '5px',
      '--tc-w': '400', '--tc-c': '#d8d4cb', '--tc-fs': '16px', '--klabel-fs': '17px', '--pt-fs': '18px',
      '--kv-row': '34px', '--sec-h': '34px', '--klabel': '#e6e2d8', '--clip-fs': '17px', '--lab-x': '9px', '--clip-r': '6px',
      '--mauve': '#706971', '--cap-bg': '#716c72', '--cap-c': 'var(--cream)', '--cap-emb-bg': '#2a2b2f' },
    rects: { agent: [10, 60, 394, 480], viewer: [404, 60, 1083, 480], insp: [1093, 60, 1364, 526], trans: [10, 489, 1083, 741], tl: [572, 535, 1364, 741] },
    panels: { agent: AG({ top: 39 }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 42, 12] } }, insp: { kind: 'kv', title: 'Text', opts: { lines: true, topLine: true, top: 66, pad: 12 } },
      trans: { kind: 'transcript', title: 'Transcript', opts: { y0: 47, step: 40.5 } },
      tl: { kind: 'timeline', z: 2, opts: { x0: 55, xr: 7, rulerY: 3, trackY: 37, trackH: 35, trackGap: 9, laneInset: 0 } } } },
  batch: { activity: 'batch', top: 50, foot: 0, vars: { '--radius': '10px', '--pt-fs': '19px', '--call-fs': '16px', '--call-ls': '0em', '--dur-fs': '16px', '--args': 'inherit',
      '--text': '#d9d5cb', '--faded': '#86837c', '--hist-fs': '16.5px', '--hist-lh': '25px', '--hist-ago-fs': '16px', '--diff-fs': '17px', '--diff-gap': '29px', '--batch-t-fs': '17px',
      '--media-fs': '16.5px', '--call-pl': '9px', '--ck-c': 'var(--cream)', '--cur-text': 'var(--cream)', '--hist-line': '#8f8b83', '--hist-line-w': '2px', '--ck-fs': '15px', '--badge-fs': '15px', '--ago-c': '#d9d5cb', '--pts-w': '600', '--pts-c': 'var(--cream)', '--pts-ml': '7px', '--pts-fs': '19px', '--batch-t-w': '400', '--batch-cols': '110px 1fr 90px',
      '--pfoot-c': '#e3dfd5', '--pfoot-fs': '15.5px', '--pfoot-ls': '-.02em', '--pfoot-l': '17px', '--pfoot-b': '11px', '--badge-bg': 'transparent', '--badge-l': 'auto', '--badge-r': '5px', '--badge-t': '3px', '--tc-w': '400', '--tc-fs': '18px', '--tc-c': '#e8e4da', '--ph-pad': '19px 17px 0 17px' },
    rects: { agent: [14, 66, 440, 527], viewer: [452, 66, 1021, 527], hist: [1032, 66, 1362, 527], diff: [14, 539, 1021, 757], media: [1032, 539, 1362, 757] },
    panels: { agent: AG({ progress: true, runMark: '○', noCheckCur: false, top: 50, rowH: 37, fade: 5, curH: 63, curPadT: 9, count: false, barL: 30, barR: 100, progFs: 13 }),
      viewer: { kind: 'viewer', opts: { pad: [14, 15, 121, 14], thumbY: 359, thumbH: 54, thumbW: 84, thumbGap: 9.5, metaY: 429 } },
      hist: { kind: 'history', title: 'History', opts: { y0: 58, step: 76, th: 46, tw: 80, nameCream: true } },
      diff: { kind: 'diffbatch', title: 'Diff', opts: { y0: 55, step: 25, split: 556, splitTop: 60, bx: 30, br: 41, bty: 18, by0: 55, bstep: 25,
        col: { '+': '#a9cf9a', '~': '#f0b47e', '-': '#e2806c' }, txt: { '+': '#b9dcab', '~': '#f4c08e', '-': '#e8907e' },
        activeC: 'var(--cream)', doneC: '#c9c8c4', nameC: '#cac9c4', lookC: '#868581', metaInline: true } },
      media: { kind: 'media', title: 'Media', phPad: '13px 17px 0 17px', opts: { y0: 44, step: 35, th: 26, tw: 44, thh: 26, gap: 9, noteC: '#bdb9b0', noteFs: 15 } } } },
  graph: { activity: 'animating', top: 52, foot: 0, rightCls: 'boxed',
    vars: { '--call-fs': '19.5px', '--call-ls': '-.035em', '--call-w': '480', '--dur-fs': '19px', '--dur-w': '4ch', '--ck-c': 'var(--cream)', '--ck-fs': '19px', '--ck-w': '20px', '--ck-wt': '600', '--chip-inset': '3px', '--call-gap': '11px', '--pt-fs': '19px', '--code-fs2': '18px', '--cur-args': 'var(--ember-text)', '--cur-text': 'var(--cream)',
      '--kv-fs': '17px', '--klabel-fs': '19px', '--kv-row': '29px', '--sec-h': '44px', '--sec-pb': '4px', '--sec-fs': '19.5px', '--vmeta-font': 'var(--sans)', '--vmeta-fs': '17px', '--klabel': '#e6e2d8', '--code-fs': '17px', '--code-lh': '22px', '--code-pad': '9px 12px',
      '--pt-fs': '19px', '--sub-top': '27px', '--chip-h': '40px', '--chip-fs': '18px', '--chip-bg': '#1d1c1a', '--chip-r': '6px', '--chip-c': 'var(--cream)', '--chip-px': '14px',
      '--ruler-fs': '17px', '--ruler-font': 'var(--sans)', '--tc-fs': '17px' },
    rects: { agent: [11, 60, 460, 427], viewer: [470, 60, 1046, 427], insp: [1058, 60, 1364, 427], graph: [11, 438, 1364, 758] },
    panels: { agent: AG({ chips: true, rowH: 38, gap: 6, top: 45 }), viewer: { kind: 'viewer', opts: { pad: [3, 12, 40, 12] } },
      insp: { kind: 'kv', title: 'Inspector', phPad: '1px 15px 0 15px', opts: { top: 50 } },
      graph: { kind: 'curves', title: 'Graph', phPad: '16px 15px 0 15px', opts: { x0: 215, xr: 0, gy0: 89, gyb: 2, hlines: 4, vTop: 75, beatY: 43, beatS: 4.5, rulerY: 55,
        chipX: 6, chipW: 204, laneY: 89, laneStep: 47, band: [89, 129], bandX: 210, bandC: '#171714', gridC: '#1c1c1a', keyS: 8, lineW: 2.2, emW: 2.4, hR: 3.5, hFill: true,
        phTop: 55, phBot: 2, phCls: 'rtri' } } } },
  mixing: { activity: 'mixing', top: 49, foot: 0, cls: 'calls-sans',
    vars: { '--kv-fs': '17px', '--kv-row': '38.6px', '--klabel': '#e6e2d8', '--lab-c': 'var(--text)', '--lab-fs': '16.5px' },
    rects: { agent: [10, 60, 394, 482], viewer: [404, 60, 1077, 482], audio: [1087, 60, 1364, 482], tracks: [10, 491, 1126, 759], loud: [1136, 491, 1364, 759] },
    panels: { agent: AG(), viewer: { kind: 'viewer', opts: { pad: [14, 14, 50, 14] } }, audio: { kind: 'kv', title: 'Audio', opts: { lines: true } },
      tracks: { kind: 'timeline', opts: { x0: 88, xr: 8, rulerY: 6, trackY: 39, trackH: 65, trackGap: 7.5, laneInset: 0, envW: 2, beatY0: 2, beatY1: 10, beatW: 2 } }, loud: { kind: 'loudness', title: 'Loudness' } } },
  color: { activity: 'color science', top: 48, foot: 0,
    vars: { '--call-fs': '16px', '--call-ls': '-.1em', '--dur-fs': '15px', '--dur-w': '4ch', '--call-gap': '5px', '--call-pr': '11px',
      '--kv-fs': '17px', '--kv-row': '41px', '--sec-h': '52px', '--klabel': '#e6e2d8', '--kv-vfont': 'var(--sans)', '--emb-row-bg': '#2a1912' },
    rects: { agent: [10, 60, 392, 505], viewer: [404, 60, 1094, 505], pipe: [1106, 60, 1364, 505], scopes: [10, 516, 1094, 759], cie: [1106, 516, 1364, 759] },
    panels: { agent: AG({ top: 39, rowH: 38, boldLast: 3 }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 44, 12] } }, pipe: { kind: 'kv', title: 'Color pipeline', opts: { lines: true, top: 44 } },
      scopes: { kind: 'scopes', opts: { axFs: 12.5, labFs: 11.5, labB: 6 } },
      cie: { kind: 'cie', opts: { scx: 250, scy: 232, ox: 31, oy: 213.4, grid: true, fillOp: 1, white: true, gFs: 12, labFs: 11.5, labB: 7, labFont: 'var(--mono)' } } } },
  effects: { activity: 'adding effects', top: 46, foot: 27,
    vars: { '--dur-w': '4ch', '--dur-fs': '15px', '--call-gap': '7px', '--tc-w': '400', '--li-h': '28px', '--li-fs': '16px', '--li-font': 'var(--sans)', '--clip': '#3c382f', '--clip-r': '6px' },
    rects: { agent: [10, 55, 377, 480], viewer: [387, 55, 1070, 480], insp: [1080, 55, 1365, 741], tl: [10, 489, 1070, 741] },
    panels: { agent: AG({ count: true }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 42, 12] } }, insp: { kind: 'kv', title: 'Inspector' },
      tl: { kind: 'timeline', opts: { x0: 57, xr: 0, rulerY: 3, trackY: 38, trackGap: 9, fxRow: true, phTop: 4, phBot: 0 } } } },
  tracking: { activity: 'tracking', top: 46, foot: 27, cls: 'calls-sans',
    vars: { '--graph-bg': '#282924', '--chip-h': '36px', '--chip-px': '11px', '--chip-bg': '#252620', '--chip-fs': '16px', '--chip-font': 'var(--sans)', '--chip-c': 'var(--cream)', '--sec-h': '34px', '--sec-pb': '2px', '--cap-font': 'var(--sans)', '--cap-fs': '16px', '--sec-line': 'transparent', '--ruler-font': 'var(--sans)', '--ruler-fs': '12.5px', '--call-fs': '16.8px', '--call-ls': '0em', '--call-pl': '20px', '--ck-fs': '16px', '--call-gap': '3px', '--dur-w': '3.2ch', '--cur-text': '#efc3a8', '--cur-args': '#efc3a8', '--fn': 'var(--cream)', '--args': 'var(--cream)', '--pt-fs': '18px', '--sub-fs': '17px', '--tag-fs': '17px', '--vmeta-font': 'var(--sans)', '--vmeta-fs': '16.5px', '--klabel-fs': '16.5px' },
    rects: { agent: [10, 55, 377, 480], viewer: [387, 55, 1070, 480], insp: [1080, 55, 1365, 741], res: [10, 489, 1070, 741] },
    panels: { agent: AG({ count: true }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 42, 12] } }, insp: { kind: 'kv', title: 'Inspector', opts: { lines: true } },
      res: { kind: 'keys', title: 'Track residual', opts: { x0: 203, xr: 15, rulerY: 10, graphY: 36, graphH: 78.5, laneY: 120, laneH: 44, chipX: 56, chipW: 122, barH: 16 } } } },
  modelling: { activity: 'modelling', top: 46, foot: 0,
    vars: { '--radius': '8px', '--panel': '#141412', '--ph-pad': '16px 15px 0 15px', '--pt-fs': '18px', '--sub-top': '43px', '--sub-fs': '18px', '--sub-c': '#7e7e79',
      '--call-fs': '16.4px', '--call-ls': '0em', '--empty-pp': 'none', '--call-pl': '14px', '--call-pr': '17px', '--call-gap': '13px', '--ck-c': 'var(--cream)', '--ck-fs': '19px', '--ck-w': '18px',
      '--fn': 'var(--cream)', '--args': 'var(--cream)', '--dur-fs': '16px', '--dur-w': '4ch', '--cur-text': '#dc9e82', '--cur-args': '#dc9e82', '--cur-dur': '#c4765e',
      '--kv-fs': '18.4px', '--kv-row': '31.5px', '--klabel': 'var(--cream)', '--sec-fs': '18px', '--sec-h': '45px', '--sec-pb': '8px', '--vmeta-font': 'var(--sans)', '--vmeta-fs': '17.5px', '--res-fs': '17.5px', '--ember-text': '#c98a78' },
    rects: { agent: [12, 55, 377, 330], stack: [12, 357, 377, 758], viewer: [388, 55, 1074, 580], insp: [1084, 55, 1363, 580], prof: [388, 592, 1363, 758] },
    panels: { agent: AG({ rowH: 35, top: 53 }), stack: { kind: 'stack', title: 'Stack', opts: { center: true, y0: 62, step: 59, nodeH: 34, fs: 16.5 } },
      viewer: { kind: 'viewer', opts: { pad: [0, 0, 0, 0], metaY: 495, metaL: 16, metaR: 19 } }, insp: { kind: 'kv', title: 'Inspector', opts: { secRule: true, top: 77.5 } },
      prof: { kind: 'profile', title: 'Profile /History', opts: { ax: 16, x1: 596, base: 149.5, top: 48, smooth: true, hollow: true, tx: 623, ty: 47, tw: 80, th: 80, gap: 6, labY: 144 } } } },
  graphics: { activity: 'building graphics', top: 46, foot: 27,
    vars: { '--kv-row': '32px', '--sec-h': '46px', '--li-h': '19px', '--li-fs': '15.5px', '--li-font': 'var(--sans)', '--li-c': 'var(--cream)', '--list-border': '0', '--li-sel-r': '6px', '--li-sel-bg': '#3a3731', '--sub-c': 'var(--dim)', '--sec-line': 'transparent' },
    rects: { agent: [10, 55, 377, 480], viewer: [387, 55, 1070, 480], insp: [1080, 55, 1365, 480], tl: [10, 489, 1365, 741] },
    panels: { agent: AG({ count: true }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 42, 12] } }, insp: { kind: 'kv', title: 'Inspector', opts: { lines: true, top: 58 } },
      tl: { kind: 'timeline', opts: { x0: 56.5, xr: 8, rulerY: 1, trackY: 37, trackH: 37, trackGap: 7.5 } } } },
  particles: { activity: 'simulating', top: 46, foot: 0, rightCls: 'chips', vars: { '--radius': '10px', '--panel': '#121211', '--empty-pp': 'none', '--kv-row': '28.5px', '--sec-h': '42px', '--sec-mt': '10px', '--raise': '#1e1d1b', '--sub-c': '#7a776f', '--ro-pad': '3px 9px', '--ro-c': 'var(--cream)', '--ro-fs': '17px', '--ro-bg': '#1e1d1b', '--ro-bd': 'transparent', '--clk-c': '#9d988b', '--clk-font': 'var(--sans)', '--clk-w': '400', '--clk-bg': '#131312', '--clk-ml': '-4px', '--call-fs': '16px', '--call-ls': '-0.06em', '--call-w': '500', '--ck-fs': '17px', '--ck-c': 'var(--cream)', '--fn': 'var(--cream)', '--args': 'var(--cream)', '--call-pl': '12px', '--call-pr': '9px', '--call-gap': '9px', '--chip-inset': '7px', '--dur-w': '3.4ch', '--tcl-font': 'var(--sans)', '--tcl-fs': '15px', '--vmeta-font': 'var(--sans)', '--res-fs': '15px', '--chip-h': '28px', '--chip-font': 'var(--sans)', '--chip-fs': '15px', '--chip-c': 'var(--cream)', '--chip-bg': '#1f1e1c', '--chip-px': '6px', '--ruler-font': 'var(--sans)', '--ruler-fs': '15px', '--ruler-ty': '2px' },
    rects: { agent: [11, 55, 353, 421], emit: [11, 432, 353, 579], viewer: [363, 55, 1112, 579], insp: [1122, 55, 1364, 579], sim: [11, 590, 1364, 759] },
    panels: { agent: AG({ chips: true, rowH: 28, gap: 4, top: 42 }), emit: { kind: 'list', title: 'Emitters', opts: { chips: true, rowH: 28, gap: 5, y0: 41, fs: 16, padX: 12, vCream: true } },
      viewer: { kind: 'viewer', opts: { pad: [0, 0, 0, 0], metaY: 492, tcLeft: true } }, insp: { kind: 'kv', title: 'Inspector', opts: { top: 66 } },
      sim: { kind: 'keys', title: 'Simulation', opts: { x0: 160, rulerY: 34, laneY: 66.5, laneH: 32, chipX: 12, chipW: 137, laneBg: '#1c1b19', laneBgH: 28, cacheH: 26, cacheC: '#c9c7c1', phTop: 66 } } } },
  lighting: { activity: 'lighting', top: 56, foot: 0, rightCls: 'sep', vars: { '--radius': '8px', '--panel': '#181817', '--brand-fs': '19px', '--proj-fs': '17px', '--pill-fs': '17px', '--pill-gap': '5px', '--logo': '26px', '--right-gap': '11px', '--ro-fs': '17px', '--clock-fs': '17px', '--clock-font': 'var(--sans)', '--pt-fs': '18px', '--call-font': 'var(--sans)', '--call-fs': '19px', '--call-w': '450', '--call-ls': '-0.01em', '--call-gap': '13px', '--cur-text': '#e3876a', '--cur-args': '#e3876a', '--cur-dur': '#ec7651', '--ember-tint': '#55301f', '--tcl-font': 'var(--sans)', '--tcl-fs': '17px', '--vmeta-font': 'var(--sans)', '--res-fs': '17px', '--call-line': 'transparent', '--ck-c': 'var(--cream)', '--ck-fs': '19px', '--sec-pb': '6px', '--call-pl': '14px', '--kv-row': '36.75px', '--kv-fs': '17px', '--klabel': 'var(--cream)', '--ch-ul': 'underline', '--sec-line': 'transparent', '--sub-fs': '17px', '--sub-c': '#8a867f' },
    rects: { agent: [20, 66, 357, 330], lights: [20, 349, 357, 510], viewer: [376, 66, 1044, 472], insp: [1062, 66, 1355, 512], rig: [20, 529, 1355, 750] },
    panels: { agent: AG({ rowH: 35, top: 45.5 }), lights: { kind: 'list', title: 'Lights', opts: { rowH: 35, y0: 45.5, fs: 18.5, iconFill: true } }, viewer: { kind: 'viewer', cls: 'vis', opts: { pad: [0, 0, 0, 0], metaY: 413, metaL: 0, metaR: 2, tcLeft: true } },
      insp: { kind: 'kv', title: 'Inspector', opts: { lines: true } }, rig: { kind: 'lightrig', title: 'Light rig', opts: {cx: 165, cy: 129, R: 78, fs: 17, hfs: 14, cols: [355, 558, 754, 999], hy: 46, ry: 89, rh: 41.5, bar: [636, 960], vmax: 0.45, thumbs: {x: 1151, y: 18, w: 79, h: 67, gx: 9, gy: 31, ly: 15, r: 6}} } } },
  roto: { activity: 'rotoscoping', top: 46, foot: 27, cls: 'calls-sans', vars: { '--kv-fs': '17px', '--klabel': 'var(--cream)', '--sec-line': 'transparent', '--tight-h': '28.5px', '--chip-font': 'var(--sans)', '--chip-fs': '16.5px', '--chip-c': 'var(--cream)', '--chip-h': '36px', '--chip-r': '6px', '--chip-bg': '#252421', '--chip-px': '10px', '--ruler-font': 'var(--sans)', '--ruler-fs': '13.5px' },
    rects: { agent: [10, 55, 377, 480], viewer: [387, 55, 1070, 480], insp: [1080, 55, 1365, 741], keys: [10, 489, 1070, 741] },
    panels: { agent: AG({ count: true }), viewer: { kind: 'viewer', opts: { pad: [12, 12, 42, 12] } }, insp: { kind: 'kv', title: 'Inspector', opts: { lines: true } },
      keys: { kind: 'keys', title: 'Mask keys', opts: { x0: 238, xr: 12, rulerY: 3, laneY: 79, laneH: 44, chipX: 57, chipW: 171, laneBg: '#1f1e1c', laneBgH: 36, laneBgR: 6, areaH: 36, sepX: 230 } } } },
  scene3d: { activity: 'animating', top: 56, foot: 0, rightCls: 'big', vars: { '--brand-fs': '20px', '--proj-fs': '17px', '--logo': '26px', '--chip-bg': 'transparent', '--chip-fs': '14.5px', '--chip-font': 'var(--mono)', '--chip-h': '30px', '--chip-px': '0px', '--dur-w': '3.2ch', '--call-gap': '6px', '--call-fs': '14.6px', '--call-ls': '-0.03em', '--ck-c': 'var(--cream)', '--ck-fs': '15px', '--kv-fs': '15.5px', '--line2': '#272725', '--sec-line': '#272725', '--kv-row': '35.5px', '--sec-h': '39px', '--sec-pb': '8px', '--klabel': 'var(--cream)', '--ruler-font': 'var(--sans)', '--ruler-fs': '14px', '--panel': '#0f0f0e' },
    rects: { agent: [14, 66, 329, 290], scene: [14, 305, 329, 548], viewer: [344, 66, 1080, 505], insp: [1095, 66, 1360, 548], keys: [14, 563, 1360, 756] },
    panels: { agent: AG({ rowH: 33.25, top: 43.4 }), scene: { kind: 'tree', title: 'Scene', opts: { y0: 52, rowH: 30.7, fs: 16.5, font: 'var(--sans)', selL: 8, selR: 7, padL: 26, gap: 7, iconW: 17, selBg: '#262523', selRad: 6, guide: 20, headLine: 43, textCream: true, iconCream: true } },
      viewer: { kind: 'viewer', cls: 'vis', opts: { pad: [0, 0, 0, 0], metaY: 450, metaL: 0, metaR: 0 } }, insp: { kind: 'kv', title: 'Inspector', opts: { lines: true, topLine: true, top: 65 } },
      keys: { kind: 'keys', title: 'Keyframes', phPad: '19px 15px 0 15px', opts: { x0: 342, xr: 1, rulerY: 35, laneY: 56, laneH: 32.6, stripY: 7, stripH: 27, stripX: 323, stripBg: '#222120', stripR: 3, stripPad: 1, stripW: 1.3, stripC: '#eeeae2', stripEmOp: .75, chipX: 13.5, chipW: 300, divX: 316, headLine: 55, laneSep: '#242321', keyLine: '#66645e', selBg: '#1d1a15', altBg: '#1a1917', selX: 317, selGlow: ['#26190f', 260, 170], rLabelK: 1.006, keySize: 6.3, emKeyScale: 1.15, rTickDx: -8, rTickY: 3, rTickH: 15, rulerHl: true, phDx: -5, phTop: 2, phBot: 0 } } } },
  export: { activity: 'rendering', top: 44, foot: 0, rightCls: 'chipclock',
    vars: { '--ph-pad': '12px 14px 0 13px', '--call-fs': '16px', '--call-ls': '-0.08em', '--call-pl': '12px', '--ck-fs': '17px', '--rh-fs': '16px', '--call-pr': '13px', '--call-gap': '9.5px', '--dur-w': '4ch', '--dur-fs': '15px', '--call-c': '#c2c0bb', '--ck-c': '#b6b4af', '--dur-c': '#b6b4af',
      '--kv-pl': '13px', '--kv-pr': '12px', '--kv-vls': '-0.04em', '--klabel': '#ece4d6', '--rh-c': '#aba8a1', '--rd-c': '#b2afa8', '--rf-c': '#b4b3af', '--rf-fs': '13px', '--rf-ls': '-0.05em' },
    rects: { agent: [11, 50, 345, 759], viewer: [357, 50, 1051, 535], exp: [1063, 50, 1364, 535], renders: [357, 547, 1364, 759] },
    panels: { agent: AG({ checkRight: true }), viewer: { kind: 'viewer', opts: { pad: [0, 13, 111, 12], progY: 13, progH: 4, thumbY: 425, thumbH: 47, thumbW: 82, thumbGap: 7, thumbR: 0 } }, exp: { kind: 'kv', title: 'Export', opts: { lines: true } },
      renders: { kind: 'renders', title: 'Renders', opts: { colW: [329, 223, 115, '1fr', 'auto'], top: 43, headH: 28.5, rowH: 28.5, left: 13, right: 12, footLine: 182 } } } },
  grading: { activity: 'grading', top: 48, foot: 0, cls: 'lab-mono',
    vars: { '--title-font': 'var(--sans)', '--pt-fs': '19px', '--call-fs': '15.6px', '--call-ls': '-0.07em', '--kv-row': '39.5px', '--ch-label': 'var(--ember-text)', '--li-h': '39.2px', '--li-fs': '15.5px', '--li-c': 'var(--cream)', '--list-border': '0', '--list-sep': 'var(--line2)', '--li-sel-r': '5px', '--thl-fs': '14px', '--thl-c': 'var(--cream)', '--wl-c': 'var(--cream)', '--wv-c': 'var(--cream)', '--sec-line': 'transparent', '--line2': '#272725', '--call-line': 'transparent', '--klabel': 'var(--cream)', '--kv-fs': '16px', '--sec-font': 'var(--mono)', '--grade-title-font': 'var(--mono)', '--plab-font': 'var(--sans)', '--tag-font': 'var(--mono)', '--tag-fs': '12.5px', '--tag-h': '21px', '--tag-px': '6px', '--tag-r': '4px', '--tag-bg': 'rgba(70,70,66,.62)', '--call-pl': '12px' },
    rects: { agent: [10, 60, 392, 505], viewer: [404, 60, 1094, 505], grade: [1106, 60, 1364, 759], bottom: [10, 516, 1094, 759] },
    panels: { agent: AG({ rowH: 38.6, top: 36.8 }), viewer: { kind: 'viewer', opts: { pad: [9, 8, 46, 8] } }, grade: { kind: 'kv', title: 'Grade', opts: { lines: true, topLine: true, top: 71 } },
      bottom: { kind: 'wheels', opts: { conic: true, noSep: true, cx: [115.5, 341.4, 568.5], cy: 119.2, r: 64.5, ringW: 8.5, labY: 32.8, labFs: 18, valY: 224.2, valFs: 15.5, valGap: 9, dotR: 8, knobR: 9, cross: 6, crossC: '#34342f', px0: 734.4, pxr: 1.1, py0: 22.3, pyb: 20.2, imgPadT: 6.4, imgPadB: 6.6, plabGap: 9, plabFs: 14, plabDy: 5 } } } },
};

// Engines that do not announce a mode still drive the view: the tool name
// picks the workspace. First match wins.
const RULES = [
  [/^(set_color_pipeline|set_clip_input_color|set_hdr_mastering|set_ocio_view_delivery)$/, 'color'],   // the Color pipeline list (m05)
  [/^set_meta$/, 'edit'],                                                                              // names a clip / track on the timeline
  [/^(load_project|save_project|checkpoint_delete|restore|template_define|template_delete)$/, 'batch'],   // m02 History / Diff / Media
  [/^(add_marker|remove_marker|add_multicam|switch_angle|sync_clips|set_time_mode|match_events|detect_cuts|remove_silence_cuts)$/, 'edit'], [/^burn_in$/, 'captions'],   /* cov_a.js */
  [/^export|^set_export|render_queue|^(join_export|preview_range|collect_project|contact_sheet|cancel_job|resume_job)$/, 'export'],
  [/^render_scene3d_sequence$/, 'export'],   // GUI-FIX-2: a frame sequence written to disk is a render job: its row is on the Renders list
  [/caption|transcri|title|text_/, 'captions'],
  [/batch|history|checkpoint|recipe|template_apply|project_diff|relink|undo|redo|collect_project/, 'batch'],
  [/keyframe_bezier|set_motion_path|bind_beats|set_expression|add_null|set_parent/, 'graph'],
  [/^(apply_motion_preset|bake_property|copy_curve|move_key|remove_key|remove_motion_path|set_animator_keyframe|simplify_curve|smooth_curve|motion_diff|motion_extract)$/, 'graph'],   // graph edit
  [/^solve_constraints$/, 'graphics'],   // layout solve: titles / logos / CTAs placed on the m09 graphics vieweror tools (cov_b.js)
  [/^preview_subject_match$/, 'grading'],
  [/loudness|audio|duck|detect_beats|mix_audio|normalize|^(set_)?(gain|pan|eq|compress|limit)|_eq$|compressor|limiter|stem/, 'mixing'],
  [/grade|white_balance|match_shot|match_color|cdl|color_curve|acescct|lut_node|^(add|connect|disconnect)_nodes?$|^set_node_(param|mask)$/, 'grading'],   // grade graph / balance tools before scopes
  [/scope|exposure|ocio|gamut|hdr_|cie/, 'color'],
  [/effect|adjustment_layer|posterize|transition|speed_lines/, 'effects'],
  [/track_matte$/, 'roto'],
  [/^(add|remove|delete|move|rename|lock)_track|^set_track$/, 'edit'],          // timeline tracks, not camera tracking
  [/track|stabiliz|undistort|camera_solve|solve_camera|corner_disagreement|lk_residual/, 'tracking'],
  [/mesh|sdf|lathe/, 'modelling'],
  [/hud|detection_box|counter|prompt_box|path_shape|shape_modifier|layer_style/, 'graphics'],
  [/emitter|advection|shard|simulate|particle/, 'particles'],
  [/light_rig|set_light|remove_light|material|relight|pbr/, 'lighting'],
  [/set_camera|scene3d|gltf/, 'scene3d'],                                // before roto: set_camera_keyframe is not a key/matte
  [/^(set_|add_|remove_|move_)?keyframes?$|^set_keyframes$/, 'edit'],                // a clip property's keys (animate group): the transform family draws them on the clip + viewer gizmo, not roto
  [/key|spill|mask|roto|matte/, 'roto'],
  [/scene_add|light_add|camera_set|keyframe/, 'scene3d'],
  [/grade|white_balance|match_shot|analyze_frame/, 'grading'],
  [/trim|cut|add_clip|split|ripple/, 'edit'],
  [/^(duplicate_clip|swap_clips|loop_clip|freeze_frame|set_speed|rate_stretch|set_speed_ramp|group_clips|ungroup_clip|distribute_clips|set_blend|set_echo|set_flow_blur|add_camera_shake|project_set)$/, 'edit'],   // whole-clip timeline ops
];
// Read-only tools (queries, probes, samples, listings, descriptions): they change nothing, so they
// never move the workspace; the router gives them the "inspect" HUD state instead.
export const READONLY = /^(get_|list_|inspect_|probe_|sample_|query|describe_|read_|find$|search_tools$|job_status$|session_stats$|cache_stats$|kimo_info$|kimo_describe$|history_list$|ext_list$|recipe_list$|template_list$|path_info$|fit_info$|explain_|transform_ocio_|transform_vertex_stream$|camera_project_points$|camera_clip_triangle$|track_long_gap$|analyze_color$|audit_color_output$|compare_aces2_reference$|compare_linear_exposure$|contrast_ratio$|text_contrast_series$|edit_check$|saliency_map$|evaluate_grade_graph_samples$|evaluate_hlg_reference_display$|validate_hdr_transfer_math$|effect_uv_sample$|shards_project$|snap_time$)|_describe$|_info$|_metrics$|_residuals$/;
export function inferMode(name) {
  if (!name) return null;
  if (READONLY.test(String(name).toLowerCase())) return null;
  for (const [re, m] of RULES) if (re.test(name)) return m;
  return null;
}
