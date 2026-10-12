# Project checks

Before changing the client pipeline, record the clean starting commit with
`git rev-parse HEAD` for a local performance comparison. Preserve unrelated user
changes and do not include them in the comparison as though they were task changes.

Run `python3 tools/check.py` after development. For changes affecting simulation,
network requests, scene preparation, navigation, animation, UI updates, or drawing,
also run `python3 tools/check.py --performance --baseline <starting-commit>`.
Development setup is documented in README.md. These checks need local HTTP sockets
and Chromium; request the necessary sandbox escalation if execution is restricted.

Read the Markdown/JSON report, including per-scenario failures and absolute budgets.
Report performance as inconclusive when the runner does; never describe skipped,
unavailable, or noisy measurements as passing. Record baseline and candidate
revisions, validation results, and material limitations in the final response.
If a test fails, check whether it also fails at the starting commit and report
pre-existing failures separately. Do not silently exclude failing tests or loosen
performance thresholds to produce a pass. Do not automatically update baselines.

The host remains authoritative for gameplay and saved state. Client scene indexes,
collision overlays, animation jobs, and drawing caches must not mutate authoritative
snapshots. Follow existing Git commit instructions after the logical task is complete.
