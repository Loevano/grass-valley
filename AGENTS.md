# Project checks

Do not run tests or performance comparisons by default during development or
before committing. Run them only when investigating or fixing a performance-related
problem, or when the user explicitly requests them. For a performance-related
problem, run targeted tests during development and `python3 tools/check.py` once
after the logical task is complete, before committing. Repeat checks only when
subsequent edits or unresolved failures warrant it.

Run a performance comparison when investigating or fixing a performance-related
problem, such as slow simulation work, excessive request frequency or payload size,
scene preparation, routing algorithms, animation scheduling, rendering work, or
cache behavior. A change that could affect runtime performance does not by itself
require tests or a performance comparison.

For a performance-related problem, record the starting commit with
`git rev-parse HEAD` before editing. Preserve unrelated user changes and do not
include them in the comparison as though they were task changes. Once correctness
checks have passed for the final code, run
`python3 tools/check.py --performance-only --baseline <starting-commit>` to avoid
running correctness checks twice. Development setup is documented in README.md.
These checks need local HTTP sockets and Chromium; request the necessary sandbox
escalation if execution is restricted.

Read the Markdown/JSON report, including per-scenario failures and absolute budgets.
Report performance as inconclusive when the runner does; never describe skipped,
unavailable, or noisy measurements as passing. Record baseline and candidate
revisions when performance is measured, validation results, and material
limitations in the final response. When performance checks are not needed, state
that they were not run and why.
If a test fails, check whether it also fails at the starting commit and report
pre-existing failures separately. Do not silently exclude failing tests or loosen
performance thresholds to produce a pass. Do not automatically update baselines.

The host remains authoritative for gameplay and saved state. Client scene indexes,
collision overlays, animation jobs, and drawing caches must not mutate authoritative
snapshots. Follow existing Git commit instructions after the logical task is complete.

# Roadmap maintenance

After completing each task, review `docs/ROADMAP.md` and update any related entries
to reflect the remaining work. Remove a task's entry and associated roadmap details
when the task is completely finished. Keep partially completed tasks in the roadmap
with an accurate status and the remaining work. Include any roadmap changes in the
same commit as the completed task.
