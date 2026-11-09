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


def main() -> int:
    if len(sys.argv) != 4:
        print(f"usage: {sys.argv[0]} timing.json alloc.json merged.json", file=sys.stderr)
        return 2

    with open(sys.argv[1]) as f:
        timing = json.load(f)
    with open(sys.argv[2]) as f:
        alloc = json.load(f)

    if timing["harness_version"] != alloc["harness_version"]:
        print(
            f"harness version mismatch: timing={timing['harness_version']} "
            f"alloc={alloc['harness_version']}",
            file=sys.stderr,
        )
        return 1
    if not alloc.get("alloc_instrumented"):
        print("allocation report claims it is not instrumented", file=sys.stderr)
        return 1
    if timing.get("alloc_instrumented"):
        print("timing report claims to be instrumented; it must be the plain build", file=sys.stderr)
        return 1

    alloc_scenarios = {s["name"]: s for s in alloc["scenarios"]}
    timing_names = {s["name"] for s in timing["scenarios"]}
    missing = timing_names - set(alloc_scenarios)
    extra = set(alloc_scenarios) - timing_names
    if missing:
        print(f"scenarios missing from allocation report: {sorted(missing)}", file=sys.stderr)
        return 1
    if extra:
        # The merge keeps only timing scenarios; extra alloc scenarios would be
        # silently dropped, so reject instead of losing data.
        print(f"scenarios extra in allocation report: {sorted(extra)}", file=sys.stderr)
        return 1
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
        "allocation_build": "perf-count-alloc (counting allocator)",
    }

    with open(sys.argv[3], "w") as f:
        json.dump(merged, f, indent=2)
        f.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
