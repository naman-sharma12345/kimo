# Preview proxies

Kimo can generate lighter media for editing previews while keeping each asset's
original path, dimensions, duration and frame rate authoritative. This is a first
implementation of roadmap F06; it is not a full media-conform system.

## Tools

| Command | Behavior |
|---|---|
| `proxy_generate` | Transcode and register one imported video; one undo step |
| `proxy_set_enabled` | Undoable preview preference, initially disabled |
| `proxy_list` | Fast health, eligibility, dimensions, frame counts and byte sizes |
| `proxy_validate` | Uncached SHA256 verification; `deep:false` uses stat checks |
| `proxy_relink` | Recover a moved byte-identical .mp4 proxy and refresh fingerprints |
| `proxy_remove` | Detach a binding; retain both original and proxy files |

Example tool calls (substitute the asset id from `import_media` and an existing
output directory):

```json
{"tool":"proxy_generate","args":{"asset_id":"asset1","path":"C:/media/proxies/interview.mp4","max_side":960,"timeout_seconds":300}}
{"tool":"proxy_set_enabled","args":{"enabled":true}}
{"tool":"render_frame","args":{"time":2,"path":"C:/media/proxies/edit-preview.png"}}
{"tool":"preview_range","args":{"start":0,"end":5,"use_proxies":true}}
{"tool":"proxy_validate","args":{"asset_id":"asset1","deep":true}}
```

`render_frame` PNG previews and `preview_range` honor enabled, eligible proxies.
Both return `proxy_assets` listing the assets actually used. Set
`preview_range.use_proxies:false` to compare a draft against original-media
export frames. Export tools, high-bit stills, audio, tracking, scopes and analysis
use originals. Managed-color projects always bypass proxies.

The core API has an explicit `render::prepare_preview_range` entry point.
`prepare_at`, `prepare_range` and `prepare_export_range` continue to prepare
original media. Decode signatures include the selected file; rendered-frame
cache keys distinguish the proxy selection per clip, preventing proxy/original
cache contamination. Nested compositions carry the proxy table too.

## Fidelity and file lifecycle

Generation supports opaque 8-bit SDR video with square pixels, zero start time,
no display rotation, and verified constant frame rate from 1 to 240 fps.
Every decoded source timestamp is checked against the rational rate, then the
output frame count, timestamps, dimensions and rate are checked again. The limit
is 216,000 frames and a 32 MiB probe response. Unsupported media is refused;
there is no silent variable-frame-rate conversion, HDR conversion or alpha loss.
The proxy is an intra-frame H.264 MP4 without audio, using CRF 23. Dimensions
preserve the exact source aspect ratio, stay within `max_side` (default 960,
64–4096) and never upscale. Some unusual aspect ratios need a larger bound.
Proxies are lossy editing aids, so they are unsuitable for final color or
pixel-critical assessment. Smaller decode inputs can reduce source decoding
work; the compositor still runs at project resolution. No speedup is claimed
without measuring the user's footage and hardware.

The destination must be a new `.mp4` file in an existing directory. A uniquely
reserved sibling temporary file is encoded under a deadline with cooperative
cancellation. Source content is hashed again after encoding to reject concurrent changes.
Validation precedes atomic, no-overwrite hard-link publication;
the filesystem must support hard links within that directory. Failure or
cancellation cleans the temporary file. Successful output files survive undo,
detachment, replacement bindings and project closure. No automatic disk deletion
or cache eviction is performed. `proxy_generate` is forbidden in atomic batches
because its filesystem side effects cannot be rolled back with project state.
Generation is synchronous; a background queue and progress UI remain future work.

Original and proxy SHA256 hashes plus file-size/modification-time fingerprints
are persisted in an optional `Project.proxies` table. Projects without proxies
retain their previous serialization. Undo/redo and save/load preserve bindings
and the preference. `external_refs` / `set_ref` include proxy paths so dependency
listing, collection and relinking can discover them. Missing proxy files remain
optional for rendering; originals are still required.

Preview selection uses inexpensive size/mtime checks. Missing or changed files
disable proxy selection automatically, and proxy decode failures retry the
original. Fast checks cannot detect adversarial changes preserving size/mtime;
`proxy_validate` hashes the full files without the dependency hash cache.
Deep validation reports content identity separately from whether the stat
fingerprints are current. After copying/collecting files, use `proxy_relink`
to rehash both original and proxy against their stored hashes and refresh the
binding. A changed original or different replacement is refused: generate a
new proxy instead. Deep validation is read-only and does not update fingerprints.

## Validation

`crates/kimo-core/tests/proxy_workflow.rs` exercises real FFmpeg generation,
fractional CFR, VFR/high-bit refusal, preview and actual MP4 export isolation,
frame-cache isolation, range preview overrides, original audio, missing/stale
and corrupt-file fallback, deep integrity checks, relink, persistence,
undo/redo, no-overwrite behavior and dependency locators. Synthetic fixtures
establish contracts; real-camera codecs, long-form throughput and interactive
playback still require separate qualification.

Verified on Windows/MSVC on 2026-10-23: 158 selected gate-profile tests passed,
including all 19 proxy workflow tests, five dependency-reference checks and
three registry/render/project golden checks. Frame and project baselines were
not rewritten. The selected regression run uses `--test-threads=1`: the existing
split-edit tests share process-wide temporary filenames and one failed when run
concurrently. One existing ignored contract test remains ignored; the existing
POSIX-specific `resolve_path_rules` test was excluded from the Windows dependency
run. The full Windows gate remains blocked by pre-existing Unix-only test
harnesses such as `export_job_kill`.

Both executables built with `cargo build --locked --profile gate -j2 --workspace
--bins --features serde/alloc` (the allocation feature matches the test dependency
graph; no extra product capability gate is enabled). A live stdio MCP run found
447 flat entries, verified all six proxy tools, and exercised generation,
enabled preview rendering, deep validation, detachment and undo. CLI version
reporting also passed. Logs are in the ignored `target/proxy-*.log` files.
