# Manual source tools

CI build matrices, build-check wrappers, the verification runner and its matrix
summarizer were removed on 2026-10-15. No CI workflow is configured.

This directory retains the authored `hal-capabilities.yaml`, capability report
renderers, source-evidence auditors and deterministic source packager. These are
manual maintenance tools, not an automatic CI pipeline. Some auditors can invoke
compilers or other checks; do not execute them under the current no-local-test
and no-local-build work policy. `d` retains its data-generation and historical
manual validation commands; no command is automatically scheduled.

Historical `ci/<file>` references in acceptance documents and JSON receipts refer
to their original snapshots. Retained files now have the same basename under
`tools/`; deleted `check-*`, `run-verified.py` and `summarize-hal-matrix.py` have no
replacement. The old CI README is preserved as [history](../docs/ci-history.md),
not a current execution guide. Historical hashes and results are not revalidated
by this relocation.
