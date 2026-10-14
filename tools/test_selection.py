"""Select tests from local imports and source/fixture file references."""
import ast
from fnmatch import fnmatch
from pathlib import Path
import re
import subprocess

SOURCE_SUFFIXES = {'.py', '.js', '.mjs', '.json', '.html', '.css'}


def imported_modules(source):
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()
    imports = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imports.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            imports.add(node.module or '')
            imports.update(f'{node.module}.{alias.name}' for alias in node.names)
    return imports


def test_files(root):
    return sorted(p.relative_to(root).as_posix() for p in (root / 'tests').glob('test_*.*')
                  if p.suffix in {'.py', '.js'})


def changed_files(root, since=None):
    commands = [['git', 'diff', '--name-only', '-z', since or 'HEAD', '--'],
                ['git', 'ls-files', '--others', '--exclude-standard', '-z']]
    return sorted({name for command in commands
                   for name in subprocess.check_output(command, cwd=root, text=True).split('\0') if name})


def dependency_graph(root, changed=()):
    known = {p.relative_to(root).as_posix()
             for folder in ('server', 'web', 'tests', 'tools')
             for p in (root / folder).rglob('*')
             if p.is_file() and p.suffix in SOURCE_SUFFIXES}
    # Keep removed modules resolvable so their former callers are still tested.
    known.update(changed)
    basenames = {}
    modules = {}
    for name in known:
        path = Path(name)
        basenames.setdefault(path.name, set()).add(name)
        if path.suffix == '.py':
            modules[path.with_suffix('').as_posix().replace('/', '.')] = name
            modules.setdefault(path.stem, name)
    graph = {name: set() for name in known}
    for name in known:
        path = root / name
        if not path.is_file() or path.suffix not in {'.py', '.js', '.mjs'}:
            continue
        source = path.read_text()
        if path.suffix == '.py':
            graph[name].update(modules[module] for module in imported_modules(source) if module in modules)
        # Includes require/readFile calls and dynamic loaders whose file names
        # are listed in arrays, such as the browser harness's script list.
        for literal in re.findall(r'''['"]([^'"\n]+)['"]''', source):
            # Catalog checks launch short Python programs from JavaScript;
            # runner contract tests also invoke dotted unittest case names.
            imports = imported_modules(literal) if 'import ' in literal else set()
            imports.add(literal.split('.', 1)[0])
            graph[name].update(modules[module] for module in imports if module in modules)
            references = ([literal] if Path(literal).suffix in SOURCE_SUFFIXES else
                          [literal + suffix for suffix in ('.js', '.mjs', '.json')]
                          + [literal + '/index.js'] if literal.startswith(('./', '../')) else [])
            if not references:
                continue
            for reference in references:
                for candidate in (root / reference, path.parent / reference):
                    try:
                        relative = candidate.resolve().relative_to(root.resolve()).as_posix()
                    except ValueError:
                        continue
                    if relative in known:
                        graph[name].add(relative)
                matches = basenames.get(Path(reference).name, set())
                if len(matches) == 1:
                    graph[name].update(matches)
        graph[name].discard(name)
    return graph


def affected_tests(root, changed):
    changed = set(changed)
    graph = dependency_graph(root, changed)
    selected = {}
    for test in test_files(root):
        dependencies, pending = set(), [test]
        while pending:
            name = pending.pop()
            if name in dependencies:
                continue
            dependencies.add(name)
            pending.extend(graph.get(name, ()))
        reasons = sorted(changed & dependencies)
        if reasons:
            selected[test] = reasons
    covered = {name for reasons in selected.values() for name in reasons}
    unknown = sorted(name for name in changed - covered
                     if Path(name).suffix in SOURCE_SUFFIXES and not name.startswith('tests/'))
    if unknown:
        # A new/unreferenced runtime file or configuration cannot safely be
        # assumed irrelevant. Explain the conservative expansion to the user.
        for test in test_files(root):
            selected.setdefault(test, []).extend(f'Unmapped runtime file: {name}' for name in unknown)
    return selected


def explicit_tests(root, patterns):
    available = test_files(root)
    selected = {}
    for pattern in patterns:
        normalized = pattern.removeprefix('./')
        matches = [name for name in available if any(fnmatch(value, normalized)
                   for value in (name, Path(name).name, Path(name).stem))]
        # Python module.Class.method is also supported without loading every
        # test module to resolve a single requested case.
        if not matches and '.' in normalized:
            module = normalized.split('.', 1)[0]
            file = f'tests/{module}.py'
            if file in available:
                selected[normalized] = [f'Explicit test: {pattern}']
                continue
        if not matches:
            raise ValueError(f'No tests match {pattern!r}. Use a test file, glob, or Python module.Class.method.')
        for name in matches:
            selected[name] = [f'Explicit test: {pattern}']
    return selected
