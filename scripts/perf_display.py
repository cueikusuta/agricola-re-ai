#!/usr/bin/env python3
"""Print human-readable summary of perf_baseline.json to stdout."""
import json
import sys

with open(sys.argv[1]) as f:
    d = json.load(f)

print(f"Harness: {d['harness']} v{d['harness_version']} (alloc_instrumented={d.get('alloc_instrumented')}, alloc_only={d.get('alloc_only')})")
print(f"Engine: {d['engine_version']}  Rules: {d['rules_version']}  Log: {d['log_version']}")
print(f"Seed: {d['seed']}  Quick: {d['quick_mode']}")
env = d["environment"]
print(f"CPU: {env.get('cpu_model', 'N/A')}  Cores: {env.get('logical_cores', 'N/A')}  RSS: {env.get('rss_kib', 'N/A')} KiB")
print()
print("=== Scenarios (median µs per operation) ===")
for s in d["scenarios"]:
    print(
        f"  {s['name']:24s} r{s['round']:>2d} {s['phase']:5s} "
        f"clone={s['clone']['median_us']:7.1f} obs={s['observation']['median_us']:6.1f} "
        f"val_act={s['validate_action']['median_us']:7.1f} "
        f"apply_s={s['apply_search']['median_us']:7.1f} "
        f"apply_c={s['apply_committed']['median_us']:7.1f} "
        f"sample={s['sample_game']['median_us']:7.1f} save={s['save_json_bytes']}B"
    )
print()
print("=== Fixed-sim searches (32 sims, 3 reps) ===")
for s in d["fixed_sim_searches"]:
    print(
        f"  {s['name']:24s} {s['policy']:4s}  completed={s['simulations_completed']:3d}  "
        f"{s['completed_per_sec']:8.1f}/s  elapsed={s['elapsed_ms']}ms  choice={s['choice']}"
    )
print()
if d["ten_second_searches"]:
    print("=== 10s wall-clock searches ===")
    for s in d["ten_second_searches"]:
        print(
            f"  {s['name']:24s} {s['policy']:4s}  completed={s['simulations_completed']:6d}  "
            f"sims/s={s['completed_per_sec']:8.1f}  fallback={s['used_fallback']}"
        )
    print()
    print("NOTE: Early/late sims/s differ due to remaining game length, not engine speed.")
    print("      Compare only same phase (early/early, mid/mid, late/late) under identical conditions.")