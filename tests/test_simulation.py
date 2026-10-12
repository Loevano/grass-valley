import unittest
from unittest.mock import patch

import server


class SimulationTiming(unittest.TestCase):
    def run_cycles(self, durations, save_duration=.02):
        now = 0
        starts, sleeps = [], []

        class Finished(Exception):
            pass

        def sleep(seconds):
            nonlocal now
            if len(starts) == len(durations):
                raise Finished
            sleeps.append(seconds)
            now += seconds

        def tick(world):
            nonlocal now
            starts.append(now)
            now += durations[len(starts) - 1]

        def save():
            nonlocal now
            now += save_duration

        with patch.object(server.time, 'monotonic', side_effect=lambda: now), \
                patch.object(server.time, 'sleep', side_effect=sleep), \
                patch.object(server, 'tick', side_effect=tick), \
                patch.object(server, 'save', side_effect=save):
            with self.assertRaises(Finished):
                server.simulation()
        return starts, sleeps

    def test_simulation_and_saving_do_not_lengthen_worker_update_intervals(self):
        starts, _ = self.run_cycles([.23, .30, .18, .25] * 10)
        self.assertEqual(starts, [3 * (i + 1) for i in range(40)])

    def test_overrun_resumes_without_replaying_missed_ticks(self):
        starts, sleeps = self.run_cycles([7, .23, .23])
        self.assertEqual(starts, [3, 10.02, 13.02])
        self.assertEqual(sleeps[1], 0)
        self.assertAlmostEqual(sleeps[2], 2.75)


if __name__ == '__main__':
    unittest.main()
