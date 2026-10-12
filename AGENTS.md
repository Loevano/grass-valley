# Project checks

During development, run the targeted tests relevant to the change. After the
logical task is complete, run `python3 tools/check.py` once before committing.
Repeat checks only when subsequent edits or unresolved failures warrant it.

Run a performance comparison when the change could plausibly affect runtime
performance, such as changing simulation work, request frequency or payload size,
scene preparation, routing algorithms, animation scheduling, rendering work, or
cache behavior. Documentation, tests, copy, styling, and UI wiring changes do not
require a performance comparison unless they affect those costs.

For a performance-sensitive change, record the starting commit with
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
