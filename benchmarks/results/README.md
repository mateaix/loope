# Benchmark results

Dated JSON snapshots produced by `../deliver.sh`. Each file is one configuration
(`<date>-<preset>[-singleshot][-dryrun].json`) and contains the aggregate metrics plus the
per-sample `records`:

| Field | Meaning |
| --- | --- |
| `resolve_rate` | fraction of samples whose `verify_cmd` passed on the final workspace |
| `convergence_rate` | fraction the loop reported converged |
| `catch_and_fix_rate` | fraction with a recorded `highlight` (reviewer caught a defect, a later iteration fixed it) — Loope's signature metric |
| `median_iterations` / `…_resolved` | cycles to stop, overall and for resolved samples |
| `total_tokens` / `tokens_per_resolved` | token economy |
| `wasted_token_ratio` | tokens on non-converged runs ÷ total |
| `median_wall_ms` | wall-clock per sample |

The committed `*-dryrun.json` snapshot is a **pipeline self-test** (stub agents — it does not
solve real tasks), not a delivery result. Real-agent snapshots are added as they are run.
