# Counter migration consumer inventory

Decision: **POSTPONED**, 2026-09-26. The
[manifest](counter-migration.json) is authoritative. Review by 2026-10-31;
that date does not remove any series.

## Repository-owned references

The six legacy names are defined and tested in `src-tauri/src/lib.rs`. The
current scrape emits them alongside the six canonical `_total` names with
identical values. `CHANGELOG.md`, `engineering-log.archive.md`, and the
`openspec/changes/{prometheus-counter-convention,prometheus-counter-rename-2026-q3,otel-provider-metrics-contract,r114-k0-coverage-and-dual-emit-guard}/`
documents contain historical contract tables or illustrative PromQL. The
sample in `docs/audits/issue-3-runtime-evidence-2026-09-17.md` uses a canonical
`_total` name.

The repository has **zero active PromQL recording rules, alert rules, or
Grafana dashboards** referring to the legacy names in `.github/`,
`collector/`, `runtime/`, `scripts/`, or `src/`. The consistency gate checks
tracked repository-owned JSON/YAML/Prometheus configuration files in those locations
against the manifest's active-consumer inventory. Historical prose and test
fixtures are not deployed queries.

## External consumers

Status: **unknown**. No Prometheus, Grafana, or alert owner has supplied a
consumer inventory or cutover acceptance receipt in issue #11. No external
host, dashboard, alert, or arbitrary local file was scanned. The repository
owner must collect an explicit inventory and query/update receipts before
proposing a new removal date. Until then, dual emit remains the contract.

The extra compatibility cost is six metric families: two global series and
four per-provider families, or up to four extra per-provider series per scrape
when all four per-provider counters have samples. At 13 provider labels that
is up to 52 extra per-provider series, plus two global series.
