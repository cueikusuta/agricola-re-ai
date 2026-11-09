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

## Position identity evidence

Each scenario in the baseline JSON includes a `position_identity` object
that records how the measured position was reached:

- `seed`: the fixed RNG seed used for `advance_to_round`
- `method`: the advancement method name (e.g. `"RandomPolicy"` or `"initial"`)
- `moves`: number of moves applied to reach the position
- `state_hash`: canonical SHA-256 hash of the full game state (config,
  public, hidden deck, RNG, card set, card state)

**Semantic comparison across harness/engine versions**: when the harness
format or engine changes, `state_hash` may differ even for semantically
equivalent positions. To compare performance fairly:

1. Verify `position_identity.seed`, `method`, and `moves` match — this
   confirms the same deterministic advancement was used.
2. If `state_hash` differs, replay the advancement from the common seed
   on both versions and compare the resulting public state (round, phase,
   board, scores, visible cards). Identical public state confirms semantic
   equivalence; only then are timing/allocation numbers comparable.
3. Document any format/hash divergence in the comparison report.

## Policy

- Do NOT edit archived baseline JSONs; supersede them with a new official run.
- Speed claims ("X got faster") require numbers from this archive before and
  after the change, measured by this same harness version or with the
  difference in harness versions stated explicitly (#236 §8).
- Speed gains and AI strength gains are validated separately:

  **Gate 1 — Semantic equivalence gate (requires exact match):**
  - Legality: every measured choice is a legal action in its position
  - Replay: identical seed + advancement produces identical public state
  - Privacy: no hidden cards, future cards, or internal seeds leak to AI

  **Gate 2 — Win-rate gate (statistical non-inferiority / improvement):**
  - Identical time budget, seed, seat conditions
  - First-place rate non-inferiority (95% CI, pre-computed required games)
  - Improvement demonstrated by statistically significant first-place gain

  Win-rate **exact equality is not required**; the gate is non-inferiority
  with a pre-specified margin and statistical power.

> **Note**: Early/late `sims/s` values (e.g. ~800 vs ~12,000) reflect
> remaining game length, not engine speed. Improvement comparisons must
> only be made between the *same phase* (early vs early, mid vs mid,
> late vs late) under identical conditions.
