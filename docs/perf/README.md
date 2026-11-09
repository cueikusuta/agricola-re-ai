# Performance baselines (long-term archive)

This directory is the **long-term archive of official performance baseline
measurements** for the card engine v2 migration (#236, Step 0).

Artifacts uploaded by `perf-baseline.yml` are kept for 90 days only. A
baseline intended as the reference standard for a comparison (e.g. before
starting a Step 1/Step 2 optimization branch) must **also be committed here**
by the orchestrator after the official workflow run, so the raw JSON survives
past the artifact retention window.

## Layout

- `baseline-<full-sha>.json` — merged report from the official run at that
  commit (`scripts/perf_merge.py` output: timing numbers from the plain
  build, allocation counters from the `perf-count-alloc` build).
- Each archived JSON is self-describing: engine/rules/log versions, seed,
  harness version, environment, scenario conditions (players, extra spaces
  on/off, cards, round/phase) and the batch statistics (median/p95/min/max).

## Policy

- Do NOT edit archived baseline JSONs; supersede them with a new official run.
- Speed claims ("X got faster") require numbers from this archive before and
  after the change, measured by this same harness version or with the
  difference in harness versions stated explicitly (#236 §8).
- Speed gains and AI strength gains are validated separately: the final
  acceptance criteria for v2 include identical 10-second-budget win rates and
  legal-move accuracy, not just sims/s.
