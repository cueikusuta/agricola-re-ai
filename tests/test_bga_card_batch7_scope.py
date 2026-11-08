"""Scope guard for the 28-card independent Batch 7 (#229).

The batch must not add a second copy of another AI's A070–A114 work.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = set("""A003 A004 A009 A010 A014 A015 A018 A019 A020 A022
A023 A025 A028 A031 A034 A035 A036 A037 A039 A040 A042
A043 A044 A045 A046 A047 A048 A049""".split())


class Batch7ScopeTest(unittest.TestCase):
    def test_all_28_cards_have_spec_and_test(self) -> None:
        self.assertEqual(len(EXPECTED), 28)
        for card in sorted(EXPECTED):
            with self.subTest(card=card):
                self.assertTrue((ROOT / "data/bga-card-specs" / f"{card}.json").exists())
                self.assertTrue((ROOT / "data/bga-card-tests" / f"{card}.json").exists())

    def test_all_28_present_in_builder(self) -> None:
        path = ROOT / "scripts/build_bga_card_specs.py"
        spec = importlib.util.spec_from_file_location("bga_builder_batch7", path)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)
        self.assertTrue(EXPECTED <= set(module.CARDS))
        self.assertEqual(set(module.CARDS) & EXPECTED, EXPECTED)


if __name__ == "__main__":
    unittest.main()
