import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import check, rust_checks
from tools.test_selection import affected_tests, changed_files, explicit_tests


class RustSelection(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workspace = {}
        for name in ('core', 'host', 'other'):
            directory = self.root / 'rust' / name
            self.write(f'rust/{name}/src/lib.rs')
            self.workspace[name] = {'name': name, 'id': name,
                                    'manifest_path': str(directory / 'Cargo.toml'),
                                    'dependencies': []}
        self.workspace['host']['dependencies'] = [
            {'path': str(self.root / 'rust/core'), 'rename': 'rules', 'kind': 'build'}]
        self.metadata = patch.object(rust_checks, 'packages', return_value=self.workspace)
        self.metadata.start()
        self.addCleanup(self.metadata.stop)

    def write(self, name, source=''):
        file = self.root / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(source)

    def selected(self, *paths):
        return affected_tests(self.root, paths)

    def test_core_selects_reverse_dependencies_but_host_does_not_select_core(self):
        self.assertEqual(set(self.selected('rust/core/src/lib.rs')), {'rust:core', 'rust:host'})
        self.assertEqual(set(self.selected('rust/host/src/lib.rs')), {'rust:host'})

    def test_transitive_and_cyclic_dependency_graph_terminates(self):
        self.workspace['other']['dependencies'] = [{'path': str(self.root / 'rust/host')}]
        self.workspace['core']['dependencies'] = [{'path': str(self.root / 'rust/other')}]
        self.assertEqual(set(self.selected('rust/core/src/lib.rs')), {'rust:core', 'rust:host', 'rust:other'})

    def test_package_manifest_and_removed_source_select_reverse_dependencies(self):
        for path in ('rust/core/Cargo.toml', 'rust/core/src/deleted.rs'):
            with self.subTest(path=path):
                selected = self.selected(path)
                self.assertEqual(set(selected), {'rust:core', 'rust:host'})
                self.assertIn(f'Removed file: {path}', selected['rust:core'])

    def test_workspace_configuration_and_unmapped_removed_package_select_all(self):
        for path in ('rust/Cargo.toml', 'rust/Cargo.lock', 'rust/rust-toolchain.toml',
                     'rust/.cargo/config.toml', '.cargo/config.toml',
                     'rust/deleted/src/lib.rs', 'tools/check.py'):
            with self.subTest(path=path):
                self.assertEqual(set(rust_checks.rust_checks(self.root, [path])),
                                 {'rust:core', 'rust:host', 'rust:other'})

    def test_shared_fixture_selects_rust_and_existing_python_and_js_consumers(self):
        self.write('tests/replay/small.json', '{}')
        self.write('tests/test_fixture.py', "fixture='tests/replay/small.json'")
        self.write('tests/test_fixture.js', "const fixture='tests/replay/small.json';")
        self.assertEqual(set(self.selected('tests/replay/small.json')),
                         {'rust:core', 'rust:host', 'rust:other',
                          'tests/test_fixture.py', 'tests/test_fixture.js'})

    def test_literal_include_fixture_outside_shared_directories_is_targeted_even_when_removed(self):
        for literal in ('"../../../data/small.txt"', 'r#"../../../data/small.txt"#'):
            with self.subTest(literal=literal):
                self.write('rust/core/src/lib.rs', f'const X: &str = include_str!({literal});')
                selected = self.selected('data/small.txt')
                self.assertEqual(set(selected), {'rust:core', 'rust:host'})
                self.assertIn('Removed file: data/small.txt', selected['rust:core'])

    def test_unrelated_python_test_docs_and_clean_checkout_do_not_need_cargo(self):
        self.write('tests/test_one.py')
        with patch.object(rust_checks, 'packages', side_effect=AssertionError('Cargo is unrelated')):
            self.assertEqual(self.selected('README.md', 'rust/README.md', 'rust/target/output.json'), {})
            self.assertEqual(self.selected(), {})
            self.assertEqual(set(self.selected('tests/test_one.py')), {'tests/test_one.py'})

    def test_explicit_rust_packages_and_globs_preserve_python_cases(self):
        self.write('tests/test_one.py')
        self.assertEqual(set(explicit_tests(self.root, ['rust:c*', 'test_one.Cases.test_case'])),
                         {'rust:core', 'test_one.Cases.test_case'})
        self.assertEqual(set(explicit_tests(self.root, ['rust:*'])),
                         {'rust:core', 'rust:host', 'rust:other'})
        with self.assertRaisesRegex(ValueError, 'No tests match'):
            explicit_tests(self.root, ['rust:missing'])

    def test_missing_failed_timed_out_or_invalid_metadata_is_a_selected_failure(self):
        for error in (FileNotFoundError('cargo'), ValueError('invalid metadata'),
                      subprocess.TimeoutExpired('cargo', 60)):
            with self.subTest(error=error), patch.object(rust_checks, 'packages', side_effect=error):
                selected = self.selected('rust/core/src/lib.rs')
                self.assertEqual(set(selected), {rust_checks.SELECTION_ERROR})
                self.assertIn('Rust selection failed:', selected[rust_checks.SELECTION_ERROR][0])

    def test_metadata_uses_offline_locked_cargo_and_filters_nonmembers(self):
        data = {'packages': list(self.workspace.values()), 'workspace_members': ['core', 'host']}
        # Restore the real metadata loader for this API contract test.
        self.metadata.stop()
        with patch.object(rust_checks.subprocess, 'run', return_value=
                          subprocess.CompletedProcess([], 0, json.dumps(data), '')) as run:
            self.assertEqual(set(rust_checks.packages(self.root)), {'core', 'host'})
        self.assertEqual(run.call_args.args[0],
                         ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked', '--offline'])
        self.assertEqual(run.call_args.kwargs['cwd'], self.root / 'rust')
        with patch.object(rust_checks.subprocess, 'run', return_value=
                          subprocess.CompletedProcess([], 101, '', 'invalid manifest')):
            with self.assertRaisesRegex(ValueError, 'invalid manifest'):
                rust_checks.packages(self.root)

    def test_git_rename_keeps_removed_and_new_paths(self):
        def git(*args):
            return subprocess.check_output(['git', *args], cwd=self.root, text=True)
        git('init', '-q')
        git('add', 'rust')
        git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'fixture')
        old = self.root / 'rust/core/src/lib.rs'
        old.rename(old.with_name('renamed.rs'))
        git('add', 'rust')
        self.assertEqual(set(changed_files(self.root)),
                         {'rust/core/src/lib.rs', 'rust/core/src/renamed.rs'})


class RustReporting(unittest.TestCase):
    def test_rust_commands_use_package_scope_and_workspace_working_directory(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(check, 'run', return_value=0) as run:
            results = check.correctness(Path(directory), {'rust:host': ['changed']})
        self.assertEqual([r['name'] for r in results],
                         ['rust-host-fmt', 'rust-host-test', 'rust-host-clippy'])
        for call in run.call_args_list:
            self.assertEqual(call.kwargs['cwd'], check.ROOT / 'rust')
            self.assertEqual(call.args[0][:4], ['cargo', call.args[0][1], '-p', 'host'])
        self.assertIn('--locked', results[1]['command'])
        self.assertIn('--offline', results[1]['command'])
        self.assertIn('--all-targets', results[2]['command'])
        self.assertEqual(results[2]['command'][-3:], ['--', '-D', 'warnings'])

    def run_report(self, output, selected):
        with patch.object(sys, 'argv', ['check.py', '--output', str(output)]), \
                patch.object(check, 'changed_files', return_value=[]), \
                patch.object(check, 'affected_tests', return_value=selected):
            code = check.main()
        return code, json.loads((output / 'report.json').read_text()), (output / 'report.md').read_text()

    def test_each_failed_rust_command_fails_json_markdown_and_process(self):
        for index in range(3):
            codes = [0, 0, 0]
            codes[index] = 101
            with self.subTest(command=index), tempfile.TemporaryDirectory() as directory, \
                    patch.object(check, 'run', side_effect=codes):
                code, report, markdown = self.run_report(Path(directory), {'rust:core': ['source changed']})
                self.assertEqual(code, 1)
                self.assertEqual(report['status'], 'fail')
                self.assertEqual(report['correctness'][index]['exit_code'], 101)
                self.assertIn('Overall: fail', markdown)
                self.assertIn('source changed', markdown)

    def test_missing_cargo_and_selection_errors_cannot_pass(self):
        run = check.run
        def unavailable_cargo(command, log, **kwargs):
            return run(['/unavailable-lantern-cargo', *command[1:]], log, **kwargs)
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(check, 'run', side_effect=unavailable_cargo):
            code, report, markdown = self.run_report(Path(directory), {'rust:core': ['source']})
            self.assertEqual(code, 1)
            self.assertTrue(all(r['status'] == 'fail' and r['exit_code'] == 127
                                for r in report['correctness']))
            for result in report['correctness']:
                self.assertIn('cargo', Path(result['log']).read_text())
        with tempfile.TemporaryDirectory() as directory:
            selected = {rust_checks.SELECTION_ERROR: ['Rust selection failed: missing cargo']}
            code, report, markdown = self.run_report(Path(directory), selected)
            self.assertEqual(code, 1)
            self.assertEqual(report['correctness'][0]['status'], 'fail')
            self.assertIn('missing cargo', markdown)

    def test_list_mode_returns_failure_when_selection_fails(self):
        with patch.object(sys, 'argv', ['check.py', '--list']), \
                patch.object(check, 'changed_files', return_value=[]), \
                patch.object(check, 'affected_tests', return_value={rust_checks.SELECTION_ERROR: ['missing cargo']}):
            self.assertEqual(check.main(), 1)

    def test_full_suite_includes_rust_and_explicit_python_does_not(self):
        with patch.object(sys, 'argv', ['check.py', '--all', '--list']), \
                patch.object(check, 'test_files', return_value=[]), \
                patch.object(check, 'rust_checks', return_value={'rust:core': ['all']}) as select:
            self.assertEqual(check.main(), 0)
            select.assert_called_once_with(check.ROOT, all_packages=True)
        with patch.object(sys, 'argv', ['check.py', '--tests', 'test_performance_checks', '--list']), \
                patch.object(rust_checks, 'packages', side_effect=AssertionError('unrelated cargo')):
            self.assertEqual(check.main(), 0)
