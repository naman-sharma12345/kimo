# Perf audit, lane B (timeline_perf split/move outliers)

Profiled 2026-10-16 04:15-04:16Z under `flock /tmp/kimo_perf.lock`, quiet box. 3 runs per tree,
240 split/move edits each on the 1k-clip load. Chrome tracing + V8 CPU sampler on (inflates frames
slightly). "Frame" = the rAF app callback.

Outlier frames >=10 ms: B 4, base b784e55 4.

| tree | run | frame ms | GC | style | layout | parse | main contributors | lane-B code |
|---|---|---|---|---|---|---|---|---|
| B | r0 | 13.2 | 0 | 0.43 | 0.56 | 2.94 | app paint 3.5, panels agent 1.9, timeline 1.3, router timelineOps 1.2 | covbLearn 0.23 ms |
| B | r0 | 10.1 | 0 | 0.62 | 0.64 | 1.01 | panels agent 2.6 + anon 1.5, router route 1.1, app paint 1.0 | covbHistory 0.18 ms |
| B | r0 | 10.1 | 0 | 1.00 | 0.51 | 0.79 | app paint 4.7, panels agent 2.2 | none |
| B | r2 | 11.2 | 0 | 0.23 | 0.55 | 0.40 | app paint 8.0 incl. 7.8 ms sampler gap (descheduled) | none |
| base | r1 | 16.2 | 0 | 0.29 | 0.25 | 0.47 | 13.4 ms sampler gap (descheduled), app renderPanel | n/a |
| base | r1 | 13.5 | 0 | 0.17 | 0.31 | 0.57 | 10.0 ms gap at getBoundingClientRect (panels) | n/a |
| base | r2 | 10.6 | 0 | 0.34 | 0.41 | 2.42 | app paint 4.3, parse 2.4 | n/a |
| base | r2 | 11.7 | 0 | 3.04 | 0.56 | 1.00 | panels show 4.3 + agent 4.3 | n/a |

GC = V8 minor/major + Blink GC: none in any outlier frame.
Whole-run cov_b.js cost: 0.6-1.1 ms over 240 edits (~0.003 ms/edit); total script per run
B 1030-1085 ms vs base 1010-1172 ms. p50/p95 parity (timeline_perf sets: p95 3-5 ms both trees).

Without its covbHistory hook (0.18 ms) the 10.1 ms B frame would have been 9.92 ms.

Verdict: outliers are equal in count, GC-free, and come from shared app/panels script and main-thread
descheduling; base b784e55 also shows them (move 11.7 ms in the 04:01Z set). B's hook appears in 2
outlier frames at its normal sub-0.25 ms cost (under 2% of each); it is not the driver.
Base worst frame 16.2 ms is a pure scheduling stall; B worst 13.2 ms.
