"""Run only the supplied unittest modules/cases and record individual timings."""
import argparse
import json
from pathlib import Path
import sys
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests'), str(ROOT)]


class TimedResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.timings = []

    def startTest(self, test):
        self.started = time.perf_counter()
        self.status = 'pass'
        super().startTest(test)

    def addFailure(self, test, error):
        self.status = 'fail'
        super().addFailure(test, error)

    def addError(self, test, error):
        self.status = 'fail'
        super().addError(test, error)

    def addSubTest(self, test, subtest, error):
        if error is not None:
            self.status = 'fail'
        super().addSubTest(test, subtest, error)

    def addSkip(self, test, reason):
        self.status = 'skip'
        super().addSkip(test, reason)

    def stopTest(self, test):
        self.timings.append({'test': test.id(), 'seconds': time.perf_counter() - self.started,
                             'status': self.status})
        super().stopTest(test)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('tests', nargs='+', help='unittest module, class or method names')
    args = parser.parse_args()
    started = time.perf_counter()
    suite = unittest.defaultTestLoader.loadTestsFromNames(args.tests)
    result = unittest.TextTestRunner(verbosity=2, resultclass=TimedResult).run(suite)
    args.output.write_text(json.dumps({'elapsed': time.perf_counter() - started,
                                      'timings': result.timings,
                                      'failures': [test.id() for test, _ in result.failures],
                                      'errors': [test.id() for test, _ in result.errors]}, indent=2))
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    sys.exit(main())
