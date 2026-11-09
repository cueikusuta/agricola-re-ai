#!/usr/bin/env python3
"""Merge the timing report (plain build) and the allocation report
(instrumented build) into one baseline JSON.

Timing numbers come from the uninstrumented build; allocation counters are
valid only in the `perf-count-alloc` build (review P1). This script keeps the
two apart on purpose: `*_allocations` / `*_allocated_bytes` fields are taken
from the allocation report and everything else from the timing report, and
each scenario is matched by name. A scenario present in one file but missing
in the other is an error: silent defaults would fake a measurement that was
never taken.

The allocation report MUST be run with `--alloc-only` and `--features
perf-count-alloc`. It produces only `scenarios` with allocation fields
(clone, observation, sample_game) and omits `fixed_sim_searches` and
`ten_second_searches` (empty arrays). The timing report is the plain build
without `--alloc-only` and contains all timing measurements.
"""
import json
import sys

ALLOC_FIELDS_BY_KEY = {
    "clone_allocations": "clone_allocations",
    "clone_allocated_bytes": "clone_allocated_bytes",
    "observation_allocations": "observation_allocations",
    "observation_allocated_bytes": "observation_allocated_bytes",
    "sample_game_allocations": "sample_game_allocations",
    "sample_game_allocated_bytes": "sample_game_allocated_bytes",
}

REQUIRED_SCENARIO_FIELDS = [
    "players",
    "round",
    "phase",
    "additional_action_space_enabled",
    "uses_cards",
    "card_definitions",
]

REQUIRED_TOP_LEVEL_FIELDS = [
    "harness_version",
    "engine_version",
    "rules_version",
    "log_version",
    "seed",
    "quick_mode",
]


def verify_identity(timing: dict, alloc: dict) -> None:
    """Verify that timing and allocation reports describe identical conditions.
    Any mismatch is a hard error (fail-closed)."""
    # Top-level identity fields
    for field in REQUIRED_TOP_LEVEL_FIELDS:
        if timing.get(field) != alloc.get(field):
            raise ValueError(
                f"{field} mismatch: timing={timing.get(field)} alloc={alloc.get(field)}"
            )

    # alloc_instrumented must be true for alloc, false for timing
    if not alloc.get("alloc_instrumented"):
        raise ValueError("allocation report claims it is not instrumented")
    if timing.get("alloc_instrumented"):
        raise ValueError("timing report claims to be instrumented; it must be the plain build")

    # alloc_only must be true for alloc, false for timing
    if not alloc.get("alloc_only"):
        raise ValueError("allocation report must have alloc_only=true")
    if timing.get("alloc_only"):
        raise ValueError("timing report must have alloc_only=false")

    # Scenario name sets must match exactly (bidirectional, no duplicates)
    timing_names = [s["name"] for s in timing["scenarios"]]
    alloc_names = [s["name"] for s in alloc["scenarios"]]

    # Check for duplicates within each report
    if len(timing_names) != len(set(timing_names)):
        raise ValueError("timing report contains duplicate scenario names")
    if len(alloc_names) != len(set(alloc_names)):
        raise ValueError("allocation report contains duplicate scenario names")

    timing_set = set(timing_names)
    alloc_set = set(alloc_names)
    if timing_set != alloc_set:
        missing = timing_set - alloc_set
        extra = alloc_set - timing_set
        raise ValueError(
            f"scenario name set mismatch: missing in alloc={sorted(missing)} extra in alloc={sorted(extra)}"
        )

    # Per-scenario identity verification
    alloc_by_name = {s["name"]: s for s in alloc["scenarios"]}
    for t_scenario in timing["scenarios"]:
        name = t_scenario["name"]
        a_scenario = alloc_by_name[name]
        for field in REQUIRED_SCENARIO_FIELDS:
            if t_scenario.get(field) != a_scenario.get(field):
                raise ValueError(
                    f"scenario {name}: {field} mismatch: timing={t_scenario.get(field)} alloc={a_scenario.get(field)}"
                )
        # Verify position_identity state_hash matches (semantic identity of the measured position)
        t_pos = t_scenario.get("position_identity", {})
        a_pos = a_scenario.get("position_identity", {})
        if t_pos.get("state_hash") != a_pos.get("state_hash"):
            raise ValueError(
                f"scenario {name}: position_identity.state_hash mismatch: timing={t_pos.get('state_hash')} alloc={a_pos.get('state_hash')}"
            )


def main() -> int:
    if len(sys.argv) != 4:
        print(f"usage: {sys.argv[0]} timing.json alloc.json merged.json", file=sys.stderr)
        return 2

    with open(sys.argv[1]) as f:
        timing = json.load(f)
    with open(sys.argv[2]) as f:
        alloc = json.load(f)

    # Verify identity before merging (fail-closed)
    try:
        verify_identity(timing, alloc)
    except ValueError as e:
        print(f"identity verification failed: {e}", file=sys.stderr)
        return 1

    alloc_scenarios = {s["name"]: s for s in alloc["scenarios"]}
    merged_scenarios = []
    for s in timing["scenarios"]:
        other = alloc_scenarios.get(s["name"])
        if other is None:
            print(f"scenario {s['name']} missing from allocation report", file=sys.stderr)
            return 1
        merged = dict(s)
        for field in ALLOC_FIELDS_BY_KEY:
            if field not in other:
                print(f"scenario {s['name']} missing field {field} in allocation report", file=sys.stderr)
                return 1
            merged[field] = other[field]
        merged_scenarios.append(merged)

    merged = {
        **timing,
        "scenarios": merged_scenarios,
        "alloc_report_environment": alloc["environment"],
    }
    merged["alloc_instrumented_fields_from"] = {
        "timing_build": "plain (System allocator)",
        "allocation_build": "perf-count-alloc (counting allocator) with --alloc-only",
    }

    with open(sys.argv[3], "w") as f:
        json.dump(merged, f, indent=2)
        f.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
