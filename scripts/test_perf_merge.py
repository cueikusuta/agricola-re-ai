#!/usr/bin/env python3
"""Test fixtures and rejection tests for perf_merge.py identity verification."""
import json
import subprocess
import sys
import tempfile
import os

# Base valid reports for testing
BASE_TIMING = {
    "harness": "perf_baseline",
    "harness_version": 2,
    "purpose": "test",
    "engine_version": "0.1.0",
    "rules_version": "test-rules",
    "log_version": 5,
    "seed": 0x23E0_0000_0000_0BA5,
    "quick_mode": True,
    "alloc_only": False,
    "alloc_instrumented": False,
    "environment": {"cpu_model": "test", "logical_cores": 4, "rss_kib": 100000},
    "scenarios": [
        {
            "name": "test_scenario_early",
            "players": 2,
            "round": 1,
            "phase": "early",
            "additional_action_space_enabled": False,
            "uses_cards": False,
            "card_definitions": 0,
            "legal_spaces": 10,
            "position_identity": {
                "seed": 0x23E0_0000_0000_0BA5,
                "method": "initial",
                "moves": 0,
                "state_hash": "abc123"
            },
            "clone": {"median_us": 100.0, "p95_us": 110.0, "min_us": 90.0, "max_us": 120.0, "mean_us": 100.0, "stddev_us": 5.0},
            "clone_allocations": 10.0,
            "clone_allocated_bytes": 1000.0,
            "state_hash": {"median_us": 50.0, "p95_us": 55.0, "min_us": 45.0, "max_us": 60.0, "mean_us": 50.0, "stddev_us": 3.0},
            "cardset_validate": {"median_us": 10.0, "p95_us": 12.0, "min_us": 8.0, "max_us": 15.0, "mean_us": 10.0, "stddev_us": 1.0},
            "cardstate_validate": {"median_us": 20.0, "p95_us": 22.0, "min_us": 18.0, "max_us": 25.0, "mean_us": 20.0, "stddev_us": 1.0},
            "definition_lookup_mean_us": None,
            "definition_lookup_worst_us": None,
            "observation": {"median_us": 30.0, "p95_us": 33.0, "min_us": 28.0, "max_us": 35.0, "mean_us": 30.0, "stddev_us": 1.0},
            "observation_allocations": 5.0,
            "observation_allocated_bytes": 500.0,
            "validate_action": {"median_us": 15.0, "p95_us": 17.0, "min_us": 13.0, "max_us": 18.0, "mean_us": 15.0, "stddev_us": 1.0},
            "apply_search": {"median_us": 200.0, "p95_us": 220.0, "min_us": 180.0, "max_us": 250.0, "mean_us": 200.0, "stddev_us": 10.0},
            "apply_committed": {"median_us": 250.0, "p95_us": 270.0, "min_us": 230.0, "max_us": 280.0, "mean_us": 250.0, "stddev_us": 10.0},
            "save_json_bytes": 5000,
            "sample_game": {"median_us": 1000.0, "p95_us": 1100.0, "min_us": 900.0, "max_us": 1200.0, "mean_us": 1000.0, "stddev_us": 50.0},
            "sample_game_allocations": 50.0,
            "sample_game_allocated_bytes": 5000.0,
        }
    ],
    "fixed_sim_searches": [],
    "ten_second_searches": [],
}

BASE_ALLOC = {
    "harness": "perf_baseline",
    "harness_version": 2,
    "purpose": "test",
    "engine_version": "0.1.0",
    "rules_version": "test-rules",
    "log_version": 5,
    "seed": 0x23E0_0000_0000_0BA5,
    "quick_mode": True,
    "alloc_only": True,
    "alloc_instrumented": True,
    "environment": {"cpu_model": "test", "logical_cores": 4, "rss_kib": 100000},
    "scenarios": [
        {
            "name": "test_scenario_early",
            "players": 2,
            "round": 1,
            "phase": "early",
            "additional_action_space_enabled": False,
            "uses_cards": False,
            "card_definitions": 0,
            "legal_spaces": 10,
            "position_identity": {
                "seed": 0x23E0_0000_0000_0BA5,
                "method": "initial",
                "moves": 0,
                "state_hash": "abc123"
            },
            "clone": {"median_us": 100.0, "p95_us": 110.0, "min_us": 90.0, "max_us": 120.0, "mean_us": 100.0, "stddev_us": 5.0},
            "clone_allocations": 10.0,
            "clone_allocated_bytes": 1000.0,
            "state_hash": {"median_us": 50.0, "p95_us": 55.0, "min_us": 45.0, "max_us": 60.0, "mean_us": 50.0, "stddev_us": 3.0},
            "cardset_validate": {"median_us": 10.0, "p95_us": 12.0, "min_us": 8.0, "max_us": 15.0, "mean_us": 10.0, "stddev_us": 1.0},
            "cardstate_validate": {"median_us": 20.0, "p95_us": 22.0, "min_us": 18.0, "max_us": 25.0, "mean_us": 20.0, "stddev_us": 1.0},
            "definition_lookup_mean_us": None,
            "definition_lookup_worst_us": None,
            "observation": {"median_us": 30.0, "p95_us": 33.0, "min_us": 28.0, "max_us": 35.0, "mean_us": 30.0, "stddev_us": 1.0},
            "observation_allocations": 5.0,
            "observation_allocated_bytes": 500.0,
            "validate_action": {"median_us": 15.0, "p95_us": 17.0, "min_us": 13.0, "max_us": 18.0, "mean_us": 15.0, "stddev_us": 1.0},
            "apply_search": {"median_us": 200.0, "p95_us": 220.0, "min_us": 180.0, "max_us": 250.0, "mean_us": 200.0, "stddev_us": 10.0},
            "apply_committed": {"median_us": 250.0, "p95_us": 270.0, "min_us": 230.0, "max_us": 280.0, "mean_us": 250.0, "stddev_us": 10.0},
            "save_json_bytes": 5000,
            "sample_game": {"median_us": 1000.0, "p95_us": 1100.0, "min_us": 900.0, "max_us": 1200.0, "mean_us": 1000.0, "stddev_us": 50.0},
            "sample_game_allocations": 50.0,
            "sample_game_allocated_bytes": 5000.0,
        }
    ],
    "fixed_sim_searches": [],
    "ten_second_searches": [],
}


def run_merge(timing_data, alloc_data) -> int:
    """Run perf_merge.py with given data, return exit code."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as tf:
        json.dump(timing_data, tf)
        timing_path = tf.name
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as af:
        json.dump(alloc_data, af)
        alloc_path = af.name
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as mf:
        merged_path = mf.name

    try:
        script_dir = os.path.dirname(os.path.abspath(__file__))
        result = subprocess.run([
            sys.executable, os.path.join(script_dir, "perf_merge.py"),
            timing_path, alloc_path, merged_path
        ], capture_output=True, text=True, cwd=script_dir)
        return result.returncode, result.stdout, result.stderr
    finally:
        for p in (timing_path, alloc_path, merged_path):
            try:
                os.unlink(p)
            except OSError:
                pass


def test_case(name: str, mutate_timing=None, mutate_alloc=None, should_pass=True):
    """Run a test case with optional mutations."""
    timing = json.loads(json.dumps(BASE_TIMING))
    alloc = json.loads(json.dumps(BASE_ALLOC))

    if mutate_timing:
        mutate_timing(timing)
    if mutate_alloc:
        mutate_alloc(alloc)

    code, stdout, stderr = run_merge(timing, alloc)
    passed = (code == 0) == should_pass
    status = "PASS" if passed else "FAIL"
    print(f"[{status}] {name}: exit={code} (expected {'0' if should_pass else '!=0'})")
    if not passed:
        print(f"  stdout: {stdout.strip()}")
        print(f"  stderr: {stderr.strip()}")
    return passed


def main():
    all_passed = True

    # Valid baseline
    all_passed &= test_case("valid baseline", should_pass=True)

    # Top-level field mismatches
    all_passed &= test_case(
        "harness_version mismatch",
        mutate_timing=lambda t: t.update({"harness_version": 3}),
        should_pass=False
    )
    all_passed &= test_case(
        "engine_version mismatch",
        mutate_timing=lambda t: t.update({"engine_version": "1.0.0"}),
        should_pass=False
    )
    all_passed &= test_case(
        "rules_version mismatch",
        mutate_timing=lambda t: t.update({"rules_version": "other-rules"}),
        should_pass=False
    )
    all_passed &= test_case(
        "log_version mismatch",
        mutate_timing=lambda t: t.update({"log_version": 4}),
        should_pass=False
    )
    all_passed &= test_case(
        "seed mismatch",
        mutate_timing=lambda t: t.update({"seed": 12345}),
        should_pass=False
    )
    all_passed &= test_case(
        "quick_mode mismatch",
        mutate_timing=lambda t: t.update({"quick_mode": False}),
        should_pass=False
    )

    # alloc_instrumented flags
    all_passed &= test_case(
        "alloc report not instrumented",
        mutate_alloc=lambda a: a.update({"alloc_instrumented": False}),
        should_pass=False
    )
    all_passed &= test_case(
        "timing report claims instrumented",
        mutate_timing=lambda t: t.update({"alloc_instrumented": True}),
        should_pass=False
    )

    # alloc_only flags
    all_passed &= test_case(
        "alloc report missing alloc_only",
        mutate_alloc=lambda a: a.update({"alloc_only": False}),
        should_pass=False
    )
    all_passed &= test_case(
        "timing report has alloc_only",
        mutate_timing=lambda t: t.update({"alloc_only": True}),
        should_pass=False
    )

    # Scenario name set mismatches
    all_passed &= test_case(
        "scenario missing in alloc",
        mutate_timing=lambda t: t["scenarios"].append({
            **t["scenarios"][0], "name": "extra_scenario"
        }),
        should_pass=False
    )
    all_passed &= test_case(
        "extra scenario in alloc",
        mutate_alloc=lambda a: a["scenarios"].append({
            **a["scenarios"][0], "name": "extra_scenario"
        }),
        should_pass=False
    )
    all_passed &= test_case(
        "duplicate scenario names in timing",
        mutate_timing=lambda t: t["scenarios"].append(t["scenarios"][0]),
        should_pass=False
    )
    all_passed &= test_case(
        "duplicate scenario names in alloc",
        mutate_alloc=lambda a: a["scenarios"].append(a["scenarios"][0]),
        should_pass=False
    )

    # Per-scenario field mismatches
    for field in ["players", "round", "phase", "additional_action_space_enabled", "uses_cards", "card_definitions"]:
        all_passed &= test_case(
            f"scenario {field} mismatch",
            mutate_timing=lambda t, f=field: t["scenarios"][0].update({f: 999 if isinstance(t["scenarios"][0][f], int) else not t["scenarios"][0][f]}),
            should_pass=False
        )

    # position_identity state_hash mismatch
    all_passed &= test_case(
        "position_identity.state_hash mismatch",
        mutate_timing=lambda t: t["scenarios"][0]["position_identity"].update({"state_hash": "different_hash"}),
        should_pass=False
    )

    # Missing allocation fields
    all_passed &= test_case(
        "missing clone_allocations in alloc",
        mutate_alloc=lambda a: a["scenarios"][0].pop("clone_allocations"),
        should_pass=False
    )

    print()
    if all_passed:
        print("ALL TESTS PASSED")
        return 0
    else:
        print("SOME TESTS FAILED")
        return 1


if __name__ == "__main__":
    sys.exit(main())