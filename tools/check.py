"""Run correctness and repeatable performance comparisons. Never promotes baselines."""
import argparse
import io
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def compare(baseline, candidate, absolute=2, relative=.20):
    """Require a meaningful difference in at least 80% of paired repetitions."""
    if len(baseline) != len(candidate) or len(candidate) < 3:
        return {'status': 'inconclusive', 'reason': 'At least three paired runs are required.'}
    if any(not math.isfinite(v) or v < 0 for v in baseline + candidate):
        return {'status': 'inconclusive', 'reason': 'Invalid timing samples.'}
    before, after = statistics.median(baseline), statistics.median(candidate)
    threshold = max(absolute, before * relative)
    votes = sum(b - a > max(absolute, a * relative) for a, b in zip(baseline, candidate))
    result = {'baseline': before, 'candidate': after, 'delta': after - before,
              'percent': (after / before - 1) * 100 if before else None,
              'threshold': threshold, 'regression_runs': votes, 'runs': len(candidate)}
    if votes >= math.ceil(len(candidate) * .8):
        result['status'] = 'regression'
    elif (after - before > threshold) or votes:
        result.update(status='inconclusive', reason='Repeated runs disagree about regression.')
    else:
        # Wide dispersion can mask regressions; tiny absolute noise is harmless.
        spread = max(max(baseline) - min(baseline), max(candidate) - min(candidate))
        if spread > max(absolute * 2, max(before, after) * .5):
            result.update(status='inconclusive', reason='Timing variation is too high.')
        else:
            result['status'] = 'pass'
    return result


def run(command, log, cwd=ROOT):
    environment = os.environ.copy()
    environment['PYTHONPATH'] = os.pathsep.join(filter(None, (
        str(ROOT / 'server'), environment.get('PYTHONPATH'))))
    with log.open('w') as output:
        try:
            completed = subprocess.run(command, cwd=cwd, env=environment,
                                       stdout=output, stderr=subprocess.STDOUT, timeout=300)
        except (OSError, subprocess.TimeoutExpired) as error:
            output.write(str(error) + '\n')
            return 124 if isinstance(error, subprocess.TimeoutExpired) else 127
    return completed.returncode


def correctness(output):
    results = []
    commands = [('python', [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests', '-v'])]
    commands.extend((test.stem, ['node', str(test.relative_to(ROOT))])
                    for test in sorted((ROOT / 'tests').glob('test_*.js')))
    for name, command in commands:
        log = output / f'{name}.log'
        code = run(command, log)
        results.append({'name': name, 'status': 'pass' if code == 0 else 'fail', 'log': str(log)})
        if code:
            print(f'FAIL {name}: {log}', flush=True)
    print(f'Correctness: {sum(r["status"] == "pass" for r in results)}/{len(results)} test commands passed.', flush=True)
    return results


def extract_revision(revision, destination):
    data = subprocess.check_output(['git', 'archive', revision], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            target = (destination / member.name).resolve()
            if not str(target).startswith(str(destination.resolve()) + os.sep) or not (member.isfile() or member.isdir()):
                raise ValueError('Baseline archive contains an unsupported path or symlink.')
        archive.extractall(destination)


def performance(args, output):
    report = {'status': 'inconclusive', 'runs': [], 'comparisons': {}, 'budgets': [], 'reasons': []}
    if not args.baseline:
        report['reasons'].append('Supply --baseline <commit> for a meaningful comparison.')
        return report
    revision = subprocess.check_output(['git', 'rev-parse', '--verify', args.baseline + '^{commit}'], cwd=ROOT, text=True).strip()
    report['baseline_commit'] = revision
    with tempfile.TemporaryDirectory(prefix='lantern-baseline-') as temporary:
        baseline = Path(temporary)
        extract_revision(revision, baseline)
        samples = {'baseline': [], 'candidate': []}
        for repetition in range(args.runs):
            order = ['baseline', 'candidate'] if repetition % 2 == 0 else ['candidate', 'baseline']
            for label in order:
                source = baseline if label == 'baseline' else ROOT
                prefix = output / f'{label}-{repetition + 1}'
                print(f'Performance: {label}, repetition {repetition + 1}/{args.runs}', flush=True)
                browser_file, server_file = Path(str(prefix) + '-browser.json'), Path(str(prefix) + '-server.json')
                browser_command = ['node', 'tools/client_perf.mjs', str(source), str(browser_file)]
                if args.headed:
                    browser_command.append('--headed')
                browser_code = run(browser_command, Path(str(prefix) + '-browser.log'))
                server_code = run([sys.executable, 'tools/server_perf.py', str(source), str(server_file), '--cycles', str(args.cycles)], Path(str(prefix) + '-server.log'))
                if browser_code or server_code:
                    report['reasons'].append(f'{label} repetition {repetition + 1} failed; inspect {prefix}-*.log.')
                    return report
                browser, server = json.loads(browser_file.read_text()), json.loads(server_file.read_text())
                if not browser['checks'] or any(not c['ok'] for c in browser['checks'] + server['checks']):
                    report['reasons'].append('A scenario did not complete correctly.')
                    return report
                environment = {'browser': browser['environment'], 'server': server['environment']}
                if report.get('environment', environment) != environment:
                    report['reasons'].append('Baseline and candidate environments differ.')
                    return report
                report['environment'] = environment
                metrics = dict(browser['metrics'], **server['metrics'])
                samples[label].append(metrics)
                report['runs'].append({'label': label, 'repetition': repetition + 1,
                                       'browser': str(browser_file), 'server': str(server_file)})
        keys = set(samples['baseline'][0])
        if any(set(run) != keys for runs in samples.values() for run in runs):
            report['reasons'].append('Metric sets differ between runs.')
            return report
        for key in sorted(keys):
            # Callback intervals depend on display scheduling and are diagnostic.
            # Maxima are retained in artifacts; scored tails use repeated p95s.
            if 'callback_interval' in key or key.endswith('_max') or key.endswith('.max_ms'):
                continue
            absolute = .1 if key.endswith('draw_calls_per_frame') else 5 if key.startswith('server.') else 20 if key.endswith(('refresh_ms','snapshot_to_draw_ms','workers_ready_ms','input_to_movement_ms','command_ack_ms')) else 2
            comparison = compare([run[key] for run in samples['baseline']], [run[key] for run in samples['candidate']], absolute)
            report['comparisons'][key] = comparison
            if comparison['status'] != 'pass':
                print(f'{comparison["status"].upper()} {key}: {comparison["baseline"]:.2f} -> {comparison["candidate"]:.2f}', flush=True)
            if key.endswith('frame_ms_p95'):
                value = comparison['candidate']
                report['budgets'].append({'metric': key, 'limit_ms': 8, 'value_ms': value, 'ok': value <= 8})
        failed = any(c['status'] == 'regression' for c in report['comparisons'].values()) or any(not b['ok'] for b in report['budgets'])
        uncertain = any(c['status'] == 'inconclusive' for c in report['comparisons'].values())
        report['status'] = 'regression' if failed else 'inconclusive' if uncertain else 'pass'
        # Capture a separate diagnostic trace without changing the scored samples.
        if args.trace or report['status'] != 'pass':
            run(['node', 'tools/client_perf.mjs', str(ROOT), str(output / 'diagnostic.json'), '--trace'] + (['--headed'] if args.headed else []), output / 'diagnostic.log')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--performance', action='store_true')
    parser.add_argument('--baseline', help='Committed revision to compare on this machine')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--cycles', type=int, default=10)
    parser.add_argument('--headed', action='store_true', help='Measure in a visible browser')
    parser.add_argument('--trace', action='store_true', help='Also save diagnostic Chrome traces')
    parser.add_argument('--performance-only', action='store_true', help='Skip correctness, explicitly reported')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.runs < 3 or args.cycles < 1:
        parser.error('Use at least three paired runs and one server cycle.')
    output = (args.output or ROOT / 'performance-results' / time.strftime('%Y%m%d-%H%M%S')).resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {'schema': 1, 'candidate_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'candidate_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT)),
              'host': {'system': platform.system(), 'machine': platform.machine()},
              'correctness': [], 'performance': {'status': 'not_run'}}
    if not args.performance_only:
        report['correctness'] = correctness(output)
    if args.performance or args.performance_only:
        try:
            report['performance'] = performance(args, output)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            report['performance'] = {'status': 'inconclusive', 'reasons': [str(error)]}
    failed = any(r['status'] == 'fail' for r in report['correctness']) or report['performance']['status'] == 'regression'
    uncertain = report['performance']['status'] == 'inconclusive'
    report['status'] = 'fail' if failed else 'inconclusive' if uncertain else 'pass'
    (output / 'report.json').write_text(json.dumps(report, indent=2))
    lines = [f'Overall: {report["status"]}', f'Performance: {report["performance"]["status"]}',
             f'Correctness: {"skipped" if args.performance_only else str(sum(r["status"] == "fail" for r in report["correctness"])) + " failing commands"}',
             '', '| Metric | Baseline | Candidate | Change | Result |', '|---|---:|---:|---:|---|']
    for metric, value in report['performance'].get('comparisons', {}).items():
        percent = f'{value["percent"]:+.1f}%' if value['percent'] is not None else 'n/a'
        lines.append(f'| {metric} | {value["baseline"]:.2f} | {value["candidate"]:.2f} | {percent} | {value["status"]} |')
    lines.extend(['', 'Absolute frame budgets:'] + [f'- {b["metric"]}: {b["value_ms"]:.2f} ms / {b["limit_ms"]} ms ({"pass" if b["ok"] else "fail"})' for b in report['performance'].get('budgets', [])])
    lines.extend(report['performance'].get('reasons', []))
    (output / 'report.md').write_text('\n'.join(lines) + '\n')
    print(f'Overall: {report["status"]}; performance: {report["performance"]["status"]}. Report: {output / "report.md"}', flush=True)
    return 1 if failed else 2 if uncertain else 0


if __name__ == '__main__':
    sys.exit(main())
