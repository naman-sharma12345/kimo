# A reference film, one step at a time

> Draft measured on v3 3f0384e. Sixteen calls below were run on this head. Foreach and the repeated-structure call budget are measured in their own section below; frame lint, range preview, project diff and incremental export/job status are still pending. This is not the final runnable example.

## Version and scope

docs_version: W1 draft 2. API availability is tied to the verified source commit, not an invented release version. Names/roles/tags are present on 3f0384e; transform nodes on 14a67ca; export manifests on 8281187. Registry-level docs_version/`since` metadata is deferred (N7); this document only records the source commits measured.

The film is ten seconds, 160 x 90, 24 fps. Three generated plates occupy 0..3, 3..6 and 6..10 seconds. No preset hides the operations. Final example data will add a generated narration tone; it will not claim to contain spoken narration.

## 1. Set project settings

Changes the canvas and frame rate directly.

### `project_set`

Arguments:
```json
{
  "width": 160,
  "height": 90,
  "fps": 24,
  "name": "Reference film"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "fps": 24.0,
  "height": 90,
  "width": 160
}
```

## 2. Build named elements in one atomic batch

Eight ordinary operations: add_track, set_meta for the track, then add_clip and set_meta for each plate. Symbolic refs read earlier operation results. The dry run discards its changes; the real call demands its revision. Stored names let later calls use @shot2 and @plates. U2 foreach remains pending; these are explicit example data.

### `batch`

Arguments:
```json
{
  "ops": [
    {
      "id": "plates",
      "tool": "add_track",
      "args": {}
    },
    {
      "id": "plate_name",
      "tool": "set_meta",
      "args": {
        "target": "track",
        "id": "$plates.track_id",
        "name": "plates"
      }
    },
    {
      "id": "shot1",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.08,
          0.12,
          0.22
        ],
        "start": 0,
        "duration": 3
      }
    },
    {
      "id": "name1",
      "tool": "set_meta",
      "args": {
        "id": "$shot1.clip_id",
        "name": "shot1",
        "role": "plate"
      }
    },
    {
      "id": "shot2",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.6,
          0.16,
          0.08
        ],
        "start": 3,
        "duration": 3
      }
    },
    {
      "id": "name2",
      "tool": "set_meta",
      "args": {
        "id": "$shot2.clip_id",
        "name": "shot2",
        "role": "plate"
      }
    },
    {
      "id": "shot3",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.08,
          0.35,
          0.18
        ],
        "start": 6,
        "duration": 4
      }
    },
    {
      "id": "name3",
      "tool": "set_meta",
      "args": {
        "id": "$shot3.clip_id",
        "name": "shot3",
        "role": "plate"
      }
    }
  ],
  "atomic": true,
  "dry_run": true,
  "return": "full"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "ok": true,
  "applied": 8,
  "dry_run": true,
  "revision": "25a6e822f1b065fd",
  "created": {
    "asset_ids": [],
    "clip_ids": [
      "clip2",
      "clip3",
      "clip4"
    ],
    "effect_ids": [],
    "track_ids": [
      "track1"
    ]
  },
  "warnings": [],
  "results": [
    {
      "id": "plates",
      "tool": "add_track",
      "result": {
        "track_id": "track1"
      }
    },
    {
      "id": "plate_name",
      "tool": "set_meta",
      "result": {
        "id": "track1",
        "name": "plates",
        "target": "track"
      }
    },
    {
      "id": "shot1",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip2"
      }
    },
    {
      "id": "name1",
      "tool": "set_meta",
      "result": {
        "id": "clip2",
        "name": "shot1",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    },
    {
      "id": "shot2",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip3"
      }
    },
    {
      "id": "name2",
      "tool": "set_meta",
      "result": {
        "id": "clip3",
        "name": "shot2",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    },
    {
      "id": "shot3",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip4"
      }
    },
    {
      "id": "name3",
      "tool": "set_meta",
      "result": {
        "id": "clip4",
        "name": "shot3",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    }
  ]
}
```

### `batch`

Arguments:
```json
{
  "ops": [
    {
      "id": "plates",
      "tool": "add_track",
      "args": {}
    },
    {
      "id": "plate_name",
      "tool": "set_meta",
      "args": {
        "target": "track",
        "id": "$plates.track_id",
        "name": "plates"
      }
    },
    {
      "id": "shot1",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.08,
          0.12,
          0.22
        ],
        "start": 0,
        "duration": 3
      }
    },
    {
      "id": "name1",
      "tool": "set_meta",
      "args": {
        "id": "$shot1.clip_id",
        "name": "shot1",
        "role": "plate"
      }
    },
    {
      "id": "shot2",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.6,
          0.16,
          0.08
        ],
        "start": 3,
        "duration": 3
      }
    },
    {
      "id": "name2",
      "tool": "set_meta",
      "args": {
        "id": "$shot2.clip_id",
        "name": "shot2",
        "role": "plate"
      }
    },
    {
      "id": "shot3",
      "tool": "add_clip",
      "args": {
        "track_id": "$plates.track_id",
        "source_type": "solid",
        "color": [
          0.08,
          0.35,
          0.18
        ],
        "start": 6,
        "duration": 4
      }
    },
    {
      "id": "name3",
      "tool": "set_meta",
      "args": {
        "id": "$shot3.clip_id",
        "name": "shot3",
        "role": "plate"
      }
    }
  ],
  "atomic": true,
  "expect_revision": "25a6e822f1b065fd",
  "return": "full"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "ok": true,
  "applied": 8,
  "dry_run": false,
  "revision": "25a6e822f1b065fd",
  "created": {
    "asset_ids": [],
    "clip_ids": [
      "clip2",
      "clip3",
      "clip4"
    ],
    "effect_ids": [],
    "track_ids": [
      "track1"
    ]
  },
  "warnings": [],
  "results": [
    {
      "id": "plates",
      "tool": "add_track",
      "result": {
        "track_id": "track1"
      }
    },
    {
      "id": "plate_name",
      "tool": "set_meta",
      "result": {
        "id": "track1",
        "name": "plates",
        "target": "track"
      }
    },
    {
      "id": "shot1",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip2"
      }
    },
    {
      "id": "name1",
      "tool": "set_meta",
      "result": {
        "id": "clip2",
        "name": "shot1",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    },
    {
      "id": "shot2",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip3"
      }
    },
    {
      "id": "name2",
      "tool": "set_meta",
      "result": {
        "id": "clip3",
        "name": "shot2",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    },
    {
      "id": "shot3",
      "tool": "add_clip",
      "result": {
        "clip_id": "clip4"
      }
    },
    {
      "id": "name3",
      "tool": "set_meta",
      "result": {
        "id": "clip4",
        "name": "shot3",
        "role": "plate",
        "tags": [],
        "target": "clip"
      }
    }
  ]
}
```

## 3. Bind generated assets

Generated tone binding and collected relocation are captured in the last section. Those calls were added after the sixteen-request prefix below, so keep their separate budget until the final runnable example orders the whole path.

## 4. Add motion

The invisible null is named shot2_motion and becomes the plate parent with keep_local. Two clip-local x keys and one sin rotation expression use normal property evaluation. These are not bundled motion presets.

### `add_null`

Arguments:
```json
{
  "track_id": "@plates",
  "start": 3,
  "duration": 3,
  "x": 0,
  "y": 0
}
```

Captured result (selected fields, values unchanged):
```json
{
  "clip_id": "clip5",
  "note": "invisible transform-only clip: parent other clips to it with set_parent",
  "null": true
}
```

### `set_meta`

Arguments:
```json
{
  "id": "clip5",
  "name": "shot2_motion",
  "role": "motion"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "id": "clip5",
  "name": "shot2_motion",
  "role": "motion",
  "tags": [],
  "target": "clip"
}
```

### `set_parent`

Arguments:
```json
{
  "clip_id": "@shot2",
  "parent_id": "@shot2_motion",
  "mode": "keep_local"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "clip_id": "clip3",
  "inherit": {
    "opacity": true,
    "position": true,
    "rotation": true,
    "scale": true
  },
  "mode": "keep_local",
  "parent": "clip5"
}
```

### `set_keyframes`

Arguments:
```json
{
  "clip_id": "@shot2",
  "property": "x",
  "keys": [
    {
      "t": 0,
      "v": -12
    },
    {
      "t": 3,
      "v": 12
    }
  ],
  "mode": "replace_all"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "keys": 2,
  "mode": "replace_all",
  "replaced": 0,
  "written": 2
}
```

### `set_expression`

Arguments:
```json
{
  "clip_id": "@shot2",
  "property": "rotation",
  "op": "sin",
  "freq": 0.5,
  "amp": 2,
  "phase": 0
}
```

Captured result (selected fields, values unchanged):
```json
{
  "applied": {
    "amp": 2.0,
    "freq": 0.5,
    "op": "sin",
    "phase": 0.0
  },
  "ops": 1
}
```

## 5. Inspect the preview

contact_sheet renders timeline times through the normal renderer. describe_frame reports numeric pixels and visible clips, not a semantic description. Range preview stays pending its landed API.

### `contact_sheet`

Arguments:
```json
{
  "path": "/tmp/w1-capture/sheet.png",
  "times": [
    1,
    4,
    8
  ],
  "cols": 3,
  "tile_px": 160
}
```

Captured result (selected fields, values unchanged):
```json
{
  "cols": 3,
  "height": 98,
  "path": "/tmp/w1-capture/sheet.png",
  "rows": 1,
  "tile": {
    "h": 90,
    "w": 160
  },
  "tile_count": 3,
  "tiles": [
    {
      "index": 0,
      "timeline_time": 1.0,
      "x": 4,
      "y": 4
    },
    {
      "index": 1,
      "timeline_time": 4.0,
      "x": 168,
      "y": 4
    },
    {
      "index": 2,
      "timeline_time": 8.0,
      "x": 332,
      "y": 4
    }
  ],
  "width": 496
}
```

### `describe_frame`

Arguments:
```json
{
  "time": 4
}
```

Captured result (selected fields, values unchanged):
```json
{
  "time": 4.0,
  "canvas": {
    "height": 90,
    "width": 160
  },
  "phash64": "d5689d6a95629d62",
  "visible_clips": [
    {
      "blend": "normal",
      "clip_id": "clip3",
      "opacity": 1.0,
      "source_type": "Solid",
      "track": 0
    },
    {
      "blend": "normal",
      "clip_id": "clip5",
      "opacity": 1.0,
      "source_type": "Shape",
      "track": 0
    }
  ],
  "warnings": [],
  "frame_stats": {
    "mean_luma": 0.24157377122383977,
    "stddev_luma": 0.038682761993959085,
    "low_detail": false
  }
}
```

## 6. Check cuts and audio timing

detect_cuts samples actual rendered pixels; timeline boundaries are separate data. match_events checks explicit marks [3,6] within one frame without edits. Audio transients on the generated tone are captured in the last section. Frame lint integration waits for the staged tool to land.

### `detect_cuts`

Arguments:
```json
{}
```

Captured result (selected fields, values unchanged):
```json
{
  "count": 2,
  "cuts": [
    {
      "after_frame_hint": 72,
      "before_frame_hint": 71,
      "confidence": 1.0,
      "frame": 72,
      "kind": "hard_cut",
      "score": 1.0,
      "time": 3.0
    },
    {
      "after_frame_hint": 144,
      "before_frame_hint": 143,
      "confidence": 1.0,
      "frame": 144,
      "kind": "hard_cut",
      "score": 1.0,
      "time": 6.0
    }
  ],
  "fps_analysed": 12.0,
  "timeline_clip_boundaries": [
    3.0,
    6.0
  ]
}
```

### `match_events`

Arguments:
```json
{
  "a": [
    3,
    6
  ],
  "b": [
    3.0,
    6.0
  ],
  "tolerance_s": 0.041666666666666664
}
```

Captured result (selected fields, values unchanged):
```json
{
  "pairs": [
    {
      "a_index": 0,
      "b_index": 0,
      "offset_s": 0.0,
      "within_tol": true
    },
    {
      "a_index": 1,
      "b_index": 1,
      "offset_s": 0.0,
      "within_tol": true
    }
  ],
  "stats": {
    "count_a": 2,
    "count_b": 2,
    "jitter_s": 0.0,
    "matched": 2,
    "max_abs_offset_s": 0.0,
    "mean_offset_s": 0.0,
    "median_offset_s": 0.0,
    "p90_abs_offset_s": 0.0,
    "within_tol_ratio": 1.0
  },
  "unmatched_a": [],
  "unmatched_b": []
}
```

## 7. Review and apply one edit

The update changes only shot2 rotation. project_drift is a batch error code, not a tool. Pass the dry-run revision as expect_revision; if another edit intervened, stop and rehearse again. Project diff capture stays pending.

### `batch`

Arguments:
```json
{
  "ops": [
    {
      "tool": "set_property",
      "args": {
        "clip_id": "@shot2",
        "property": "rotation",
        "value": 1
      }
    }
  ],
  "dry_run": true
}
```

Captured result (selected fields, values unchanged):
```json
{
  "ok": true,
  "applied": 1,
  "dry_run": true,
  "revision": "8674efb5eb65a390",
  "created": {
    "asset_ids": [],
    "clip_ids": [],
    "effect_ids": [],
    "track_ids": []
  },
  "warnings": [],
  "results": [
    {
      "id": "1",
      "tool": "set_property",
      "result": {
        "ok": true
      }
    }
  ]
}
```

### `batch`

Arguments:
```json
{
  "ops": [
    {
      "tool": "set_property",
      "args": {
        "clip_id": "@shot2",
        "property": "rotation",
        "value": 1
      }
    }
  ],
  "expect_revision": "8674efb5eb65a390"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "ok": true,
  "applied": 1,
  "dry_run": false,
  "revision": "8674efb5eb65a390",
  "created": {
    "asset_ids": [],
    "clip_ids": [],
    "effect_ids": [],
    "track_ids": []
  },
  "warnings": [],
  "results": [
    {
      "id": "1",
      "tool": "set_property",
      "result": {
        "ok": true
      }
    }
  ]
}
```

## 8. Explain changed export chunks

Capture export_manifest before the edit in step 7, then run explain_invalidation afterward. Five chunks of 48 frames cover ten seconds. Only indices 1 and 2 overlap shot2 and become stale; three stay fresh. These are dependency decisions, not a completed incremental export. Export and job status remain pending.

### `export_manifest`

Arguments:
```json
{
  "chunk_s": 2
}
```

Captured result (selected fields, values unchanged):
```json
{
  "project_hash": "d86be782cea814ea2332bd1cb5bb1788585c29377b5cb02453162666fc922ab7",
  "manifest_hash": "818286f075ad52c0525ddc2e63f1532dbb12827bc53cd074fd15dd8badd1649f",
  "settings": {
    "chunk_frames": 48,
    "fps": 24.0,
    "hash": "6fef600aae58d3c4ef98a8f1b6bef9b450c196596c44bdbe612d83be5dbbe6e4",
    "height": 90,
    "include_audio": true,
    "width": 160
  },
  "chunks": [
    {
      "index": 0,
      "range": [
        0,
        48
      ],
      "clips": [
        "clip2"
      ],
      "key": "d867f4c20b4c78e05bef1dec71a2681b5e8d5968629a0a73fbc58efec8a48e51"
    },
    {
      "index": 1,
      "range": [
        48,
        96
      ],
      "clips": [
        "clip2",
        "clip3",
        "clip5"
      ],
      "key": "5fab4e622ed9cf99170a2b0b4cd3da4e1952dfcea9c48756e21a5cb122be969c"
    },
    {
      "index": 2,
      "range": [
        96,
        144
      ],
      "clips": [
        "clip3",
        "clip5"
      ],
      "key": "16e60e26e649ec6992a7b5d8d0e2337c4cf9496ce618828334680f495f8e1821"
    },
    {
      "index": 3,
      "range": [
        144,
        192
      ],
      "clips": [
        "clip4"
      ],
      "key": "b41a0fcd0c1aa9480732f49a698559d6ec7b751fbb464f9da9f6e95e5d79a567"
    },
    {
      "index": 4,
      "range": [
        192,
        240
      ],
      "clips": [
        "clip4"
      ],
      "key": "b41a0fcd0c1aa9480732f49a698559d6ec7b751fbb464f9da9f6e95e5d79a567"
    }
  ]
}
```

### `explain_invalidation`

Arguments:
```json
{
  "previous": "<the complete result of export_manifest, not a string placeholder in a real call>"
}
```

Captured result (selected fields, values unchanged):
```json
{
  "stale": 2,
  "unchanged": 3,
  "project_hash": {
    "current": "eb2de866044b0b416d69186173155a5f7127b93266c68815ad63744fe0c0493c",
    "previous": "d86be782cea814ea2332bd1cb5bb1788585c29377b5cb02453162666fc922ab7"
  },
  "chunks": [
    {
      "index": 0,
      "range": [
        0,
        48
      ],
      "stale": false,
      "reason": []
    },
    {
      "index": 1,
      "range": [
        48,
        96
      ],
      "stale": true,
      "reason": [
        {
          "clip": "clip3",
          "kind": "clip_changed",
          "new_hash": "9cf07c3fd4d26ce8eecacc3e0183ceaf1b97cc7b721e19ecfa3d5ddee4ea961f",
          "old_hash": "1a117e40ff2d74a4dea9f409fb77abd4f53ac214bd34080340ab5c9b257d4cd2",
          "property": [
            "props"
          ]
        }
      ]
    },
    {
      "index": 2,
      "range": [
        96,
        144
      ],
      "stale": true,
      "reason": [
        {
          "clip": "clip3",
          "kind": "clip_changed",
          "new_hash": "9cf07c3fd4d26ce8eecacc3e0183ceaf1b97cc7b721e19ecfa3d5ddee4ea961f",
          "old_hash": "1a117e40ff2d74a4dea9f409fb77abd4f53ac214bd34080340ab5c9b257d4cd2",
          "property": [
            "props"
          ]
        }
      ]
    },
    {
      "index": 3,
      "range": [
        144,
        192
      ],
      "stale": false,
      "reason": []
    },
    {
      "index": 4,
      "range": [
        192,
        240
      ],
      "stale": false,
      "reason": []
    }
  ]
}
```

## Measured call budget

Counts are MCP tools/call requests. Initialization is excluded. Build rehearsal and apply each expand to eight primitive operations. The update rehearsal and apply each expand to one.

| Stage | Requests | Primitive operations |
| --- | ---: | ---: |
| Settings | 1 | 1 |
| Named build, rehearsal and apply | 2 | 16 |
| Motion | 5 | 5 |
| Preview | 2 | 2 |
| Cut check and match | 2 | 2 |
| Update, rehearsal and apply | 2 | 2 |
| Manifest and invalidation | 2 | 2 |
| Measured prefix | 16 | 30 |

The sixteen-request prefix excludes audio binding and collect/reopen (captured later), foreach, lint, range preview, project diff and export jobs. Sixteen is not an end-to-end budget.

## Call budget for repeated structure

The sixteen-request prefix above builds eight named elements. Repeated structure (captions, titles, per-clip keyframes) is where call count explodes if every item is its own request. This section measures one such segment three ways. Counts are top-level tool calls and response bytes as reported by `session_stats` (reset first; the `session_stats` calls themselves are excluded). Request bytes are not measured. Measured on v3 3aabd9d, 160 x 90.

The segment: one track, 40 caption text clips (`start` = index x 1.5 s), and 12 named title clips, each with `opacity` and `y` keyframes written by `set_keyframes` against the clip it was created with.

| Route | Calls | Response bytes | Primitive operations |
| --- | ---: | ---: | ---: |
| Serial: add_track, then one call per clip and per keyframe set | 77 | 2397 | 77 |
| One `batch` with two `foreach` loops, `$calc` for times and `$title.clip_id` refs, `return: minimal` | 1 | 549 | 77 |
| `template_define` (library file) once, then `template_apply` | 2 | 752 | 77 |

All three produce the same project: the serial build and the one-call batch are byte-identical project JSON, and the template build is equal once the stored template itself is excluded. The test `docs_call_budget` asserts this and prints the numbers above.

Reading the table honestly:

- The saving is in calls (77 to 1), not response bytes. A single-op reply is already small (about 30 bytes), so the serial build only returns 2397 bytes in total. The win is that an agent makes one decision instead of 77.
- `return` matters for big batches. A 40-op batch replies 3626 bytes with the default `summary`, 3426 with `ids` and 432 with `minimal` (measured on 40 `add_text` ops). Use `minimal` when you do not need per-op results.
- `template_apply` returns a compact result by default (`ok`, `template`, `applied_ops`, created ids, the resolved `args` and `expansion_hash`, a sha256 of the expanded op list; `return` defaults to `minimal`). The exact flat ops come back in `expansion` only for `dry_run` or `expansion: true`, and as `ops` for `expand_only`. This took the template route from 16569 bytes (when it always echoed the ops) to 752 bytes for define plus apply. `recipe_apply` works the same way.
- Template calls pay off when the same structure is applied more than once: define once, then each use is one call with its arguments.
- `foreach` expands before anything runs and is atomic with the rest of the batch: one undo entry, and any failure restores the project exactly. Pass `expand_only` to see the flat operation list.

Reference data (plain JSON, runnable as written):

- `docs/examples/call_budget/segment_one_batch.json` is the body of the one-call `batch`.
- `docs/examples/call_budget/segment_template.json` is the template library for `template_define` (`library_path`), applied with `template_apply` name `codex_segment` (arguments `captions` and `titles` default to 40 and 12).

Limits: `$calc` is arithmetic over the loop variables (`i`, `n`, the named variable), parameters and numbers; it does not call tools. A range is bounded by the batch size limit of 500 operations after expansion.

## Reproduction and acceptance

The final examples/reference_film/run.sh will rebuild from plain data over MCP/CLI. Full-tier checks will parse documented tool names against the registry, validate example batches with dry_run, assert project and three frame hashes, assert manifest chunk count and one-clip invalidation, then repeat after collection into another directory. Fresh checkout and relocation must pass before the example is called complete.

## Generated tone and collection capture

Additional capture on the same3f0384e source: a ten-second,220Hz stereo48k16-bit WAV tone was generated locally with Python wave/struct. This is a narration stand-in, not speech. It has no silent passages or transcript. No sample payload is bundled in a preset. The current explicit build is24 requests (38 expanded operations including rehearsal). Relocation verification adds5 requests, for29 in this capture. U2 foreach and final output jobs are still excluded.

### `save_project`

Arguments:
```json
{
  "path": "/tmp/w1-capture/reference.kimo"
}
```

Captured result (selected fields):
```json
{
  "path": "/tmp/w1-capture/reference.kimo",
  "stale_history_removed": false
}
```

### `import_media`

Arguments:
```json
{
  "path": "/tmp/w1-capture/tone.wav"
}
```

Captured result (selected fields):
```json
{
  "asset_id": "asset1",
  "duration": 10.0,
  "fps": 0.0,
  "height": 0,
  "width": 0
}
```

### `add_audio_clip`

Arguments:
```json
{
  "asset_id": "asset1",
  "start": 0,
  "duration": 10,
  "volume": 0.2
}
```

Captured result (selected fields):
```json
{
  "clip_id": "clip3",
  "track_id": "track2"
}
```

### `set_meta`

Arguments:
```json
{
  "id": "clip3",
  "name": "narration_tone",
  "role": "narration"
}
```

Captured result (selected fields):
```json
{
  "id": "clip3",
  "name": "narration_tone",
  "role": "narration",
  "tags": [],
  "target": "clip"
}
```

### `audio_transients`

Arguments:
```json
{
  "target": "/tmp/w1-capture/tone.wav",
  "mode": "onset"
}
```

Captured result (selected fields):
```json
{
  "count": 0,
  "duration_s": 10.0,
  "method": {
    "adaptive_window_s": 0.5,
    "channels": "mono arithmetic stereo mean",
    "envelope_peak_floor": 0.005,
    "envelope_reference_ms": [
      -20,
      -10
    ],
    "envelope_rise_ratio": 1.25,
    "fft": "fixed radix-2 f64 in-tree, fixed reduction order",
    "flatness_threshold": 0.2,
    "floor": 0.001,
    "frame": 1024,
    "grid_confidence_threshold": 0.5,
    "hop": 256,
    "k": 3.5,
    "kind": "percussive if spectral flatness > .2 else tonal",
    "level": "10 ms RMS centered at envelope peak",
    "onset": "half-wave log(1+magnitude) spectral flux",
    "peak_pick": "local maximum; earliest refined sample anchors min_gap before threshold eligibility (monotonic sensitivity)",
    "refinement": "maximum absolute PCM sample within +/-1024 of flux center",
    "sample_rate": 48000,
    "source": "path decoded source seconds",
    "tempo": "onset-envelope autocorrelation 60..200 BPM, median event spacing for matching octave, keep observed 60 BPM; no downbeat inference",
    "threshold": "median + k*MAD + floor",
    "window": "Hann symmetric"
  },
  "truncated": false,
  "params": {
    "band": [
      0.0,
      24000.0
    ],
    "end": 10.0,
    "frame_rate": 24.0,
    "image": "none",
    "max_items": 500,
    "min_gap_ms": 80.0,
    "mode": "onset",
    "sensitivity": 0.5,
    "start": 0.0,
    "target": "/tmp/w1-capture/tone.wav"
  }
}
```

### `save_project`

Arguments:
```json
{
  "path": "/tmp/w1-capture/reference.kimo"
}
```

Captured result (selected fields):
```json
{
  "path": "/tmp/w1-capture/reference.kimo",
  "stale_history_removed": false
}
```

### `list_dependencies`

Arguments:
```json
{
  "check_hash": true
}
```

Captured result (selected fields):
```json
{
  "items": [
    {
      "field": "assets[asset1].path",
      "hash_mode": "full",
      "kind": "audio",
      "owner": {
        "asset_id": "asset1"
      },
      "path": "/tmp/w1-capture/tone.wav",
      "resolved": "/tmp/w1-capture/tone.wav",
      "sha256": "fb6b66d5eaf1f00b18c1df83079e9382d04b727698ec51fa8e631092780c886e",
      "size": 1920044,
      "status": "ok",
      "used_range": [
        0.0,
        10.0
      ]
    }
  ],
  "summary": {
    "changed": 0,
    "missing": 0,
    "ok": 1,
    "total": 1
  }
}
```

### `collect_project`

Arguments:
```json
{
  "dest_dir": "/tmp/w1-capture/relocated",
  "overwrite": true
}
```

Captured result (selected fields):
```json
{
  "bytes": 1920044,
  "dest_dir": "/tmp/w1-capture/relocated",
  "files": 1,
  "fonts": [],
  "manifest": "/tmp/w1-capture/relocated/collect_manifest.json",
  "missing": 0,
  "project": "/tmp/w1-capture/relocated/project.kimo",
  "skipped": [],
  "source_project_sha": "89fb3807b5e166e0455028239129b90de04d3fd3db58a75c6e86f8e55d331bd0",
  "warnings": []
}
```

The project was then loaded from the collected directory while the original tone file was temporarily renamed out of the way. describe_frame at4s was exactly equal to the original loaded project, and list_dependencies reported missing0. This establishes the collected binding path for this fixture, not final project/frame-hash acceptance.

Relocation calls:
```json
[
  {
    "tool": "load_project",
    "args": {
      "path": "/tmp/w1-capture/reference.kimo"
    }
  },
  {
    "tool": "describe_frame",
    "args": {
      "time": 4
    }
  },
  {
    "tool": "load_project",
    "args": {
      "path": "/tmp/w1-capture/relocated/project.kimo"
    }
  },
  {
    "tool": "describe_frame",
    "args": {
      "time": 4
    }
  },
  {
    "tool": "list_dependencies",
    "args": {
      "check_hash": true
    }
  }
]
```
