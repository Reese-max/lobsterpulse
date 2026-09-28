import unittest

from scripts.check_counter_migration import validate_closed_phase


class ClosedPhaseGateTest(unittest.TestCase):
    def test_closed_phase_without_linked_successors_fails(self):
        with self.assertRaisesRegex(ValueError, "successor tracking"):
            validate_closed_phase("status: closed\n", {"phase_one": {"status": "closed"}})

    def test_closed_phase_with_missing_required_step_fails(self):
        metadata = "status: closed\nsuccessor_tracking: docs/metrics/counter-migration.json\n"
        record = {
            "phase_one": {
                "status": "closed",
                "successor_tracking": "https://github.com/Reese-max/lobsterpulse/issues/11",
                "required_successors": [{"id": "T-2", "state": "blocked", "reason": "pending"}],
            }
        }
        with self.assertRaisesRegex(ValueError, "missing a required successor"):
            validate_closed_phase(metadata, record)


if __name__ == "__main__":
    unittest.main()
