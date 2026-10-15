"""Cargo workspace selection without fetching dependencies or starting a host."""
from pathlib import Path
import json
import re
import subprocess

SELECTION_ERROR = 'rust:selection-error'
RUNNER_FILES = {'tools/check.py', 'tools/test_selection.py', 'tools/rust_checks.py'}
ROOT_CONFIG = {'.cargo/config', '.cargo/config.toml', 'rustfmt.toml', '.rustfmt.toml'}
SHARED_FIXTURES = ('tests/replay/', 'tests/fixtures/', 'fixtures/')


def packages(root):
    """Use Cargo's manifest interpretation, including renamed/dev/build deps."""
    result = subprocess.run(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked', '--offline'],
        cwd=root / 'rust', capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise ValueError(f'cargo metadata exited {result.returncode}: {result.stderr.strip()}')
    data = json.loads(result.stdout)
    members = set(data['workspace_members'])
    selected = {p['name']: p for p in data['packages'] if p['id'] in members}
    if not selected:
        raise ValueError('Cargo metadata returned no workspace members')
    return selected


def source_references(root):
    """Literal include!/include_str!/include_bytes! and #[path] references.

    Dynamic fixture paths use the conservative shared-fixture directory rule.
    Keep missing reference targets so deletions still select their consumers.
    """
    references = {}
    for source in (root / 'rust').rglob('*.rs'):
        if 'target' in source.relative_to(root / 'rust').parts:
            continue
        text = source.read_text()
        literals = re.findall(
            r'(?:include(?:_str|_bytes)?!\s*\(\s*|#\[path\s*=\s*)(?:r#*)?"([^"\n]+)"', text)
        for literal in literals:
            target = (source.parent / literal).resolve()
            try:
                name = target.relative_to(root.resolve()).as_posix()
            except ValueError:
                continue
            references.setdefault(name, set()).add(source.relative_to(root).as_posix())
    return references


def rust_checks(root, changed=(), all_packages=False):
    changed = set(changed)
    if not (root / 'rust').exists() and not any(p.startswith('rust/') for p in changed):
        return {}
    try:
        references = source_references(root)
        relevant = {p for p in changed if p.startswith(('rust/', *SHARED_FIXTURES))
                    or p in RUNNER_FILES | ROOT_CONFIG or p in references}
        # Documentation and ignored output do not require Cargo to be installed.
        relevant = {p for p in relevant if (not p.endswith('.md') or p in references)
                    and not p.startswith('rust/target/')}
        if not relevant and not all_packages:
            return {}
        workspace = packages(root)
        directories = {name: Path(p['manifest_path']).parent.resolve()
                       for name, p in workspace.items()}
        graph = {name: {other for other, directory in directories.items()
                        for dep in p['dependencies']
                        if dep.get('path') and Path(dep['path']).resolve() == directory}
                 for name, p in workspace.items()}
        selected = {}
        for path in sorted(relevant):
            targets = {path} | references.get(path, set())
            owners = {name for name, directory in directories.items()
                      if any(directory in (root / target).resolve().parents for target in targets)}
            reason = path if (root / path).exists() else f'Removed file: {path}'
            # Workspace configuration, shared fixture directories, and unmapped
            # Rust inputs conservatively affect every member.
            if path in RUNNER_FILES | ROOT_CONFIG or not owners:
                owners = set(workspace)
            for name in workspace:
                dependencies, pending = set(), [name]
                while pending:
                    dependency = pending.pop()
                    if dependency not in dependencies:
                        dependencies.add(dependency)
                        pending.extend(graph[dependency])
                if dependencies & owners:
                    selected.setdefault('rust:' + name, []).append(reason)
        if all_packages:
            for name in workspace:
                selected.setdefault('rust:' + name, []).append('Explicit Rust package selection')
        return dict(sorted(selected.items()))
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        return {SELECTION_ERROR: [f'Rust selection failed: {error}']}
