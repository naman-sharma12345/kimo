# Code clips

A code clip is a clip whose pictures come from code instead of footage: a small web project (an `index.html` plus
JavaScript, for example a three.js scene) that draws the exact frame for any time `t`. Kimo renders it frame by frame in a
sandboxed headless browser, caches the frames, and from then on treats them like media: cut, split, trim, retime, key,
grade, mask, glow, blend, track-matte and composite them in 3D with the same tools as any other clip.

## The contract

```js
window.renderFrame = (t, params) => { /* draw the frame for t seconds */ };   // may return a Promise
window.__ready = true;                                                      // once assets are loaded (optional)
```

- Draw only from `t` and `params`. No `Date.now` / `performance.now` timing, no unseeded randomness, no state carried
  from one frame to the next: Kimo renders frames in any order, in separate batches and on fresh pages.
- Keep every file inside the folder (vendor three.js next to your code). The page cannot reach the network or files
  outside its folder.
- A transparent background composites over the tracks below (three.js: `alpha: true`, `setClearColor(0x000000, 0)`).
- Optional `kimo.json` declares the entry page and numeric params:
  `{"entry": "index.html", "params": {"speed": {"default": 1, "min": 0, "max": 4}}}`.

`examples/codeclip_orbit` is a complete example (three.js r186, MIT).

## Tools

| Tool | What it does |
| --- | --- |
| `add_code_clip` | `track_id`, `path` (the folder), `duration`, optional `start`, `width`/`height` (the page's viewport; scaled to fit the canvas when it differs), `params` (initial values). |
| `set_code_clip_param` | A declared param: constant, or a keyframe at code time `time` with an `ease`. Keys live on the clip's own clock (the `t` the page gets), so splits, trims and speed changes keep the animation seamless. |

## How it renders

- Time: clip-local time on the project frame grid, mapped through in-point, speed, speed ramp, time remap, reverse and
  freeze, exactly like media.
- Frames are rendered in one headless-browser batch per range (Chromium with SwiftShader WebGL through Playwright for
  Python; `KIMO_CHROMIUM`, `KIMO_PYTHON` override). Sequential access (playback, export, frame loops) renders ahead.
- Each frame is keyed by the content of every file in the folder, the param values at that frame, `t`, and the sizes.
  Keys go to an in-memory window and a disk cache of compressed TIFFs: `<project dir>/.kimo_cache/codeclip` (or
  `KIMO_CODECLIP_CACHE`; `off` disables it), capped at `KIMO_CODECLIP_CACHE_MAX_MB` (default 256), least recently used
  first. Editing any file in the folder changes every key, so nothing stale is shown.
- Sandbox: the page is served from a virtual origin that maps only to files inside the folder; every other request is
  aborted and DNS resolves nothing. `WebSocket`, `RTCPeerConnection` and `EventSource` are removed. `Date` and
  `performance.now` are frozen, and `Math.random` is a seeded generator restarted on every page load.
- Limits: each page load and each frame has a timeout (`KIMO_CODECLIP_FRAME_TIMEOUT_MS`, default 20000), and a whole
  batch has a deadline after which the renderer's process group is killed. Errors are loud (`renderer_failed`).
- `KIMO_CODECLIP_LOCK=<file>` makes every browser launch take an exclusive lock on that file (shared machines).

## Not yet

- Software WebGL: speed depends on the scene (the example renders at about the rate measured in the change log).
- A new browser starts for each batch; a persistent renderer and a render-path determinism check are the next step.
- Motion blur on a code clip moves the layer, not the content inside the page (sub-frame times are not rendered).
