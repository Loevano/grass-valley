from pathlib import Path
import subprocess
import tempfile
import unittest

from tools.test_selection import affected_tests, changed_files, explicit_tests


class ApplicableTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def write(self, path, source=''):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(source)

    def test_editing_a_test_runs_that_test_only(self):
        self.write('tests/test_one.py')
        self.write('tests/test_other.py')
        self.assertEqual(set(affected_tests(self.root, ['tests/test_one.py'])), {'tests/test_one.py'})

    def test_local_python_imports_follow_transitive_dependencies(self):
        self.write('server/navigation.py')
        self.write('server/work_cycles.py', 'import navigation')
        self.write('tests/test_work.py', 'import work_cycles')
        self.write('tests/test_unrelated.py', 'import json')
        self.assertEqual(set(affected_tests(self.root, ['server/navigation.py'])), {'tests/test_work.py'})

    def test_shared_python_fixture_selects_its_callers(self):
        self.write('tests/fixture.py')
        self.write('tests/test_one.py', 'from fixture import town')
        self.write('tests/test_other.py')
        self.assertEqual(set(affected_tests(self.root, ['tests/fixture.py'])), {'tests/test_one.py'})

    def test_browser_harness_loads_scripts_listed_in_dynamic_arrays(self):
        self.write('web/app.js')
        self.write('web/scene.js')
        self.write('tests/client_harness.js', "const files=['app.js','scene.js'];")
        self.write('tests/test_ui.js', "require('./client_harness');")
        self.write('tests/test_other.js')
        self.assertEqual(set(affected_tests(self.root, ['web/scene.js'])), {'tests/test_ui.js'})
        self.assertEqual(set(affected_tests(self.root, ['tests/client_harness.js'])), {'tests/test_ui.js'})

    def test_deleted_module_still_selects_former_importers(self):
        self.write('tests/test_work.py', 'from navigation import route')
        self.write('tests/test_other.py')
        self.assertEqual(set(affected_tests(self.root, ['server/navigation.py'])), {'tests/test_work.py'})

    def test_javascript_python_catalog_checks_follow_python_imports(self):
        self.write('server/progression.py')
        self.write('tests/test_recipes.js',
                   "execFileSync('python3',['-c','import json, progression; print(progression.public_catalog())']);")
        self.write('tests/test_other.js')
        self.assertEqual(set(affected_tests(self.root, ['server/progression.py'])), {'tests/test_recipes.js'})

    def test_dotted_cases_invoked_by_runner_tests_remain_dependencies(self):
        self.write('tests/test_fixture.py')
        self.write('tests/test_runner.py', "case='test_fixture.Cases.test_method'")
        self.assertEqual(set(affected_tests(self.root, ['tests/test_fixture.py'])),
                         {'tests/test_fixture.py', 'tests/test_runner.py'})

    def test_docs_and_unchanged_workspaces_select_nothing(self):
        self.write('tests/test_one.py')
        self.assertEqual(affected_tests(self.root, ['README.md', 'docs/ROADMAP.md']), {})
        self.assertEqual(affected_tests(self.root, []), {})

    def test_unknown_runtime_changes_conservatively_select_all_and_explain_why(self):
        self.write('tests/test_one.py')
        self.write('tests/test_other.js')
        selected = affected_tests(self.root, ['server/new_feature.py'])
        self.assertEqual(set(selected), {'tests/test_one.py', 'tests/test_other.js'})
        self.assertIn('Unmapped runtime file: server/new_feature.py', selected['tests/test_one.py'])

    def test_explicit_files_globs_and_python_cases(self):
        self.write('tests/test_work.py')
        self.write('tests/test_ui.js')
        self.write('tests/test_other.py')
        self.assertEqual(set(explicit_tests(self.root, ['tests/test_w*.py'])), {'tests/test_work.py'})
        self.assertEqual(set(explicit_tests(self.root, ['test_ui.js'])), {'tests/test_ui.js'})
        self.assertEqual(set(explicit_tests(self.root, ['test_work.Work.test_case'])), {'test_work.Work.test_case'})
        with self.assertRaisesRegex(ValueError, 'No tests match'):
            explicit_tests(self.root, ['missing'])

    def test_git_selection_includes_staged_unstaged_untracked_and_committed_changes(self):
        def git(*args):
            return subprocess.check_output(['git', *args], cwd=self.root, text=True)
        git('init', '-q')
        self.write('tests/test_staged.py')
        self.write('tests/test_unstaged.py')
        git('add', '.')
        git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'initial fixture')
        initial = git('rev-parse', 'HEAD').strip()
        self.assertEqual(changed_files(self.root), [])
        self.write('tests/test_staged.py', '# staged')
        git('add', '.')
        self.write('tests/test_unstaged.py', '# unstaged')
        self.write('tests/test_new.py')
        self.assertEqual(set(changed_files(self.root)),
                         {'tests/test_staged.py', 'tests/test_unstaged.py', 'tests/test_new.py'})
        git('add', '.')
        git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '-qm', 'changed fixture')
        self.assertEqual(changed_files(self.root), [])
        self.assertEqual(set(changed_files(self.root, initial)),
                         {'tests/test_staged.py', 'tests/test_unstaged.py', 'tests/test_new.py'})
