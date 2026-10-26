# AI evals

Six synthetic (CV, job) pairs for spec section 11.15, checked by `crates/cv-ai/tests/evals.rs` (ADR-077).

| Fixture | Case |
| --- | --- |
| `role_mismatch` | Backend CV with an earlier design job, for a backend job |
| `career_changer` | Teacher moving into data analysis |
| `long_cv` | 61 entries, scored in two requests |
| `german_cv` | German CV text, English job: rewrites must stay German |
| `injection` | Job that tells the model to score everything 100, and a clean copy (`job_clean.txt`) |
| `thin_cv` | One short job and a degree: mostly gap questions |

Each folder has `cv.json` (a PinaxCV CV), `job.txt`, an optional `expect.json` (`relevant` and `irrelevant` entry IDs, `clean_job`, `min_relevance_requests`) and `recorded.json` (the model's answers per job file, per schema name, in request order).

## In CI

`cargo test -p cv-ai --test evals` replays the recordings.

## Live, before a release

```sh
PINAXCV_EVAL_PROVIDER=anthropic PINAXCV_EVAL_MODEL=<model> PINAXCV_EVAL_KEY=<key> \
    cargo test -p cv-ai --test evals -- --ignored --nocapture
```

`PINAXCV_EVAL_PROVIDER` is `anthropic`, `openai`, `gemini` or `compatible` (set `PINAXCV_EVAL_BASE_URL`; the key is then optional, as for Ollama). Add `PINAXCV_EVAL_RECORD=1` to replace `recorded.json` with the live answers, then commit them when the checks pass. Keep the key out of shell history, for example with `read -s PINAXCV_EVAL_KEY; export PINAXCV_EVAL_KEY`.

Never put real CVs here: the repository is public.
