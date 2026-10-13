import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('checks', Path(__file__).parents[1] / 'tools' / 'check.py')
checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checks)


class PerformanceComparison(unittest.TestCase):
    def test_meaningful_consistent_regression_fails(self):
        result = checks.compare([10, 10, 10, 10, 10], [14, 14, 14, 14, 14])
        self.assertEqual(result['status'], 'regression')

    def test_small_absolute_change_is_not_a_regression(self):
        self.assertEqual(checks.compare([1] * 5, [1.5] * 5)['status'], 'pass')

    def test_improvement_passes(self):
        self.assertEqual(checks.compare([12] * 5, [4] * 5)['status'], 'pass')

    def test_disagreeing_or_missing_runs_are_inconclusive(self):
        self.assertEqual(checks.compare([10] * 5, [10, 10, 14, 14, 14])['status'], 'inconclusive')
        self.assertEqual(checks.compare([10], [10])['status'], 'inconclusive')

    def test_noisy_or_invalid_samples_cannot_pass(self):
        self.assertEqual(checks.compare([10, 30, 10], [10, 10, 10])['status'], 'inconclusive')
        self.assertEqual(checks.compare([10] * 3, [float('nan')] * 3)['status'], 'inconclusive')

    def test_unavailable_tools_write_a_failure_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'missing.log'
            self.assertEqual(checks.run(['/unavailable-lantern-test-tool'], log), 127)
            self.assertTrue(log.read_text())

    def test_timed_out_checks_cannot_pass_or_hang_the_runner(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'timeout.log'
            with patch.object(checks.subprocess, 'run', side_effect=checks.subprocess.TimeoutExpired(['fixture'], 300)):
                self.assertEqual(checks.run(['fixture'], log), 124)
            self.assertIn('timed out', log.read_text())


class SelectedCorrectness(unittest.TestCase):
    def test_selected_python_case_and_javascript_file_are_the_only_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            selected = {'test_logistics.ResourceLogistics.test_camp_wood_can_pay_for_construction_before_hauling': ['explicit'],
                        'tests/test_hauling.js': ['explicit']}
            with patch.object(checks, 'run', return_value=0) as run:
                results = checks.correctness(output, selected)
            self.assertEqual(len(results), 2)
            python, javascript = [call.args[0] for call in run.call_args_list]
            self.assertEqual(python[-1], next(iter(selected)))
            self.assertNotIn('discover', python)
            self.assertEqual(javascript, ['node', 'tests/test_hauling.js'])

    def test_empty_selection_runs_nothing(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(checks, 'run') as run:
            self.assertEqual(checks.correctness(Path(directory), {}), [])
            run.assert_not_called()

    def test_reusing_output_cannot_report_old_python_timings(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            timings = output / 'python-timings.json'
            timings.write_text('{"timings": [{"test": "stale", "seconds": 99}]}')
            with patch.object(checks, 'run', return_value=1):
                results = checks.correctness(output, {'tests/test_logistics.py': ['explicit']})
            self.assertEqual(results[0]['status'], 'fail')
            self.assertFalse(timings.exists())


if __name__ == '__main__':
    unittest.main()
