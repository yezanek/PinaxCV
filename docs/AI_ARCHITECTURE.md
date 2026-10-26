# PinaxCV AI Architecture

Summary of `pinaxcv-spec.md` section 11. The spec is authoritative.

## Principles

1. **AI proposes; the user approves.** Nothing changes a CV or variant until the user accepts a suggestion.
2. **No fabrication.** The AI may rephrase, reorder, select or ask questions. It never adds employers, dates, titles, tools, numbers or achievements.
3. **IDs identify targets.** Every suggestion references an ID from the input; unknown IDs are dropped.
4. **Untrusted input.** Job descriptions and imported text are data, never instructions.
5. **Minimum data.** Contact details are never sent, except by the text importer after explicit consent. The name is opt-in; location is city and country only, and can be disabled.
6. **Consent gate.** First use, and every provider change, shows exactly what is sent and to which host.

## Providers

`Llm` trait with `AnthropicLlm`, `OpenAiLlm`, `GeminiLlm`, `OpenAiCompatibleLlm` (any base URL, including local servers such as Ollama) and `FakeLlm` for tests. The user enters the model name; there is no default. Requests use `ureq` with rustls on a blocking thread (ADR-068), a 60 s timeout by default, up to two retries on network errors, 408, 429 and 5xx, no redirects (so a key header never reaches another host), and never log request or response bodies. An OpenAI model that refuses `temperature` gets one retry without it. Keys live in the OS keychain (memory-only fallback) and AI stays off until the user has configured a provider and accepted the consent for its host (ADR-069).

## Pipeline (`ai_tailor`)

| Stage | Kind | Output |
|---|---|---|
| A. Job parse | LLM, cached | `JobRequirements` |
| B. Relevance | LLM, batched | scores per entry, bullet, skill |
| C. Selection | deterministic | hide/collapse/order suggestions |
| D. Rewrite | LLM + mandatory verification | rewrite suggestions |
| E. Summary | LLM + verification | summary suggestion |
| F. Gaps | LLM | questions for the user, never content |
| G. Review (no job) | LLM | rewrites and advisory notes |
| Fit to length | deterministic | `Pending` length cuts |

Optional stages (D, E, F) can fail without discarding earlier results; the job still completes and lists them in `TailorResult.failed_stages`. Every stage answer is checked against its Rust type and retried once with the problem quoted back; after that the stage fails with `ai_invalid_output` (ADR-072). Suggestions land on the variant as pending and appear in the review list (ADR-073).

## Verification

Every rewrite is checked deterministically for new numbers, new proper nouns or technologies, terminology changes, length limits, language changes and unknown or locked targets. Errors drop the suggestion; warnings show a badge. Dropped suggestions are counted in the results (`TailorResult.discarded`) and listed under "discarded by safety checks" (ADR-074, ADR-075).

## Prompts and caching

Prompts are `const` strings with a `PROMPT_VERSION`, filled by a minimal placeholder renderer that escapes closing delimiter tags. Responses are cached in SQLite by `sha256(kind + prompt_version + model + input)`. Only answers that pass the type check are cached. The payload builder redacts contact details from free text too (ADR-070).

## Text import (`IMPORT_EXTRACT`)

"Import text…" shows the exact text that would be sent before anything is. Email addresses, URLs and phone numbers are removed from it first and found again locally; the name, headline and location are sent as written. Every extracted bullet is checked word for word against the sent text, and bullets that fail are flagged for review. "Read without AI" runs the heuristic importer instead (ADR-076).

## Testing

`FakeLlm` pipeline tests and a recording `Llm` that asserts no contact details leave in any stage except `IMPORT_EXTRACT`. Evals over six (CV, job) fixtures in `testdata/evals/` (`crates/cv-ai/tests/evals.rs`) replay recorded responses in CI and run live by hand before releases; see `testdata/evals/README.md` and ADR-077. The committed recordings are hand-written until a passing live run with `PINAXCV_EVAL_RECORD=1` replaces them (#115).
