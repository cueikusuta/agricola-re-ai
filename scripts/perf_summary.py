#!/usr/bin/env python3
"""Generate GitHub Actions step summary from perf_baseline.json output."""
import json
import os
import sys

with open(sys.argv[1]) as f:
    d = json.load(f)

sha = os.environ.get("GITHUB_SHA", "unknown")[:12]
summary_path = os.environ["GITHUB_STEP_SUMMARY"]

with open(summary_path, "a") as out:
    out.write("# Performance Baseline (Step 0, #237)\n")
    out.write(f"**SHA:** {sha}\n")
    out.write(f"**Harness:** perf_baseline v1\n")
    out.write(f"**Engine:** {d['engine_version']}  **Rules:** {d['rules_version']}  **Log:** {d['log_version']}\n")
    out.write(f"**Seed:** {d['seed']}  **Quick mode:** {d['quick_mode']}\n\n")

    out.write("## Scenarios\n")
    out.write("| Scenario | Clone (µs) | CardSet validate (µs) | CardState validate (µs) | Observation (µs) | Validate action (µs) | Apply search (µs) | Apply committed (µs) | Sample game (µs) | Save (B) |\n")
    out.write("|---|---|---|---|---|---|---|---|---|---|\n")
    for s in d["scenarios"]:
        out.write(
            f"| {s['name']} | {s['clone_us']:.1f} | {s['cardset_validate_us']:.1f} | "
            f"{s['cardstate_validate_us']:.1f} | {s['observation_us']:.1f} | "
            f"{s['validate_action_us']:.1f} | {s['apply_search_us']:.1f} | "
            f"{s['apply_committed_us']:.1f} | {s['sample_game_us']:.1f} | "
            f"{s['save_json_bytes']} |\n"
        )
    out.write("\n")

    out.write("## Fixed-sim searches (32 sims)\n")
    out.write("| Scenario | Policy | Completed | Sims/s | Elapsed (ms) | Choice |\n")
    out.write("|---|---|---|---|---|---|\n")
    for s in d["fixed_sim_searches"]:
        out.write(
            f"| {s['name']} | {s['policy']} | {s['simulations_completed']} | "
            f"{s['completed_per_sec']:.1f} | {s['elapsed_ms']} | {s['choice']} |\n"
        )
    out.write("\n")

    if d["ten_second_searches"]:
        out.write("## 10s wall-clock searches\n")
        out.write("| Scenario | Policy | Completed | Sims/s | Fallback |\n")
        out.write("|---|---|---|---|---|\n")
        for s in d["ten_second_searches"]:
            out.write(
                f"| {s['name']} | {s['policy']} | {s['simulations_completed']} | "
                f"{s['completed_per_sec']:.1f} | {s['used_fallback']} |\n"
            )
        out.write("\n")

    env = d["environment"]
    out.write("## Environment\n")
    out.write(f"- **CPU:** {env.get('cpu_model', 'N/A')}\n")
    out.write(f"- **Logical cores:** {env.get('logical_cores', 'N/A')}\n")
    out.write(f"- **RSS:** {env.get('rss_kib', 'N/A')} KiB\n")