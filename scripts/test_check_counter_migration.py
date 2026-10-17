import unittest

from scripts.check_counter_migration import validate_closed_phase, validate_exporter_diagnostics


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


class ExporterDiagnosticsGateTest(unittest.TestCase):
    @staticmethod
    def renderer(help_text):
        return 'fn render_prometheus_body(\n) -> String {\n    "' + help_text + '".to_owned()\n}\n'

    def test_negative_test_literal_does_not_announce_expired_removal(self):
        source = self.renderer("migration {} since {}; review {}")
        source += '#[cfg(test)]\nmod tests {\n    assert!(!body.contains("scheduled removal week 4"));\n}\n'
        validate_exporter_diagnostics(source)

    def test_expired_renderer_help_is_rejected(self):
        source = self.renderer("migration {} since {}; review {}; scheduled removal week 4")
        with self.assertRaisesRegex(ValueError, "still announces expired removal"):
            validate_exporter_diagnostics(source)

    def test_diagnostic_text_outside_renderer_does_not_satisfy_gate(self):
        source = self.renderer("migration state: POSTPONED")
        source += '// migration {} since {}; review {}\n'
        with self.assertRaisesRegex(ValueError, "lacks decision/review diagnostics"):
            validate_exporter_diagnostics(source)

    def test_missing_renderer_is_rejected(self):
        for source in ('// migration {} since {}; review {}\n',
                       self.renderer("migration {} since {}; review {}") * 2):
            with self.subTest(source=source):
                with self.assertRaisesRegex(ValueError, "cannot locate unique exporter renderer"):
                    validate_exporter_diagnostics(source)


if __name__ == "__main__":
    unittest.main()
