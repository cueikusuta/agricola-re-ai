#!/usr/bin/env python3
"""Print human-readable summary of perf_baseline.json to stdout."""
import json
import sys

with open(sys.argv[1]) as f:
    d = json.load(f)

print(f"Harness: {d['harness']} v{d['harness_version']}")
print(f"Engine: {d['engine_version']}  Rules: {d['rules_version']}  Log: {d['log_version']}")
print(f"Seed: {d['seed']}  Quick: {d['quick_mode']}")
env = d["environment"]
print(f"CPU: {env.get('cpu_model', 'N/A')}  Cores: {env.get('logical_cores', 'N/A')}  RSS: {env.get('rss_kib', 'N/A')} KiB")
print()
print("=== Scenarios ===")
for s in d["scenarios"]:
    print(
        f"  {s['name']:18s} clone={s['clone_us']:7.1f}us  cs_validate={s['cardset_validate_us']:6.1f}us  "
        f"state_validate={s['cardstate_validate_us']:6.1f}us  obs={s['observation_us']:6.1f}us  "
        f"val_act={s['validate_action_us']:7.1f}us  apply_s={s['apply_search_us']:6.1f}us  "
        f"apply_c={s['apply_committed_us']:6.1f}us  sample={s['sample_game_us']:6.1f}us  "
        f"save={s['save_json_bytes']}B  clone_alloc={s['clone_allocated_bytes']:.0f}B  "
        f"sample_allocs={s['sample_game_allocations']:.1f}"
    )
print()
print("=== Fixed-sim searches (32 sims) ===")
for s in d["fixed_sim_searches"]:
    print(
        f"  {s['name']:18s} {s['policy']:4s}  completed={s['simulations_completed']:3d}  "
        f"{s['completed_per_sec']:7.1f}/s  elapsed={s['elapsed_ms']}ms  choice={s['choice']}"
    )
print()
if d["ten_second_searches"]:
    print("=== 10s wall-clock searches ===")
    for s in d["ten_second_searches"]:
        print(
            f"  {s['name']:18s} {s['policy']:4s}  completed={s['simulations_completed']:6d}  "
            f"sims/s={s['completed_per_sec']:7.1f}  fallback={s['used_fallback']}"
        )