"""Regression contract: G0 may only attest fixed simulation-count evaluations (#187)."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

class G0WorkflowContractTests(unittest.TestCase):
    def test_workflow_rejects_time_budgeted_evaluation(self):
        yaml = (ROOT / ".github/workflows/g0-gate.yml").read_text(encoding="utf-8")
        self.assertIn("options: ['simulation-capped']", yaml)
        self.assertNotIn("options: ['simulation-capped', 'time-budgeted']", yaml)
        self.assertIn('test "$SEARCH_MODE" == "simulation-capped"', yaml)
        self.assertIn('test "$DECISION_BUDGET_MS" -eq 0', yaml)

    def test_checks_are_produced_by_same_actions_run(self):
        yaml = (ROOT / ".github/workflows/g0-gate.yml").read_text(encoding="utf-8")
        self.assertIn("--trusted-current-run", yaml)
        self.assertIn("--code-sha \"$GITHUB_SHA\"", yaml)
        self.assertIn("--run-url \"$RUN_URL\"", yaml)
        self.assertNotIn("run-id: ", yaml)
        self.assertNotIn("repository: ", yaml)

if __name__ == "__main__":
    unittest.main()
