from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from tools.test_python import TimedResult
import io
import json


class PythonTimings(unittest.TestCase):
    def test_subtest_failure_remains_a_failure_in_the_timing_report(self):
        class Cases(unittest.TestCase):
            def test_bad_subtest(self):
                with self.subTest(item=1):
                    self.fail('deliberate failure')
            def test_pass(self):
                pass
        result = unittest.TextTestRunner(stream=io.StringIO(), resultclass=TimedResult).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(Cases))
        self.assertFalse(result.wasSuccessful())
        self.assertEqual([case['status'] for case in result.timings], ['fail', 'pass'])
        self.assertTrue(all(case['seconds'] >= 0 for case in result.timings))

    def test_cli_runs_the_requested_case_and_rejects_an_invalid_one(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'timings.json'
            command = [sys.executable, 'tools/test_python.py', '--output', str(output)]
            result = subprocess.run(command + ['test_fixtures.TownFixtures.test_fixture_has_valid_ground_and_real_town_defaults'],
                                    cwd=root, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output.read_text())
            self.assertEqual(len(report['timings']), 1)
            result = subprocess.run(command + ['test_fixtures.TownFixtures.missing_case'],
                                    cwd=root, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(json.loads(output.read_text())['errors'])
