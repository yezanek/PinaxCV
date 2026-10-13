# PinaxCV

Privacy-first desktop CV editor. Write once, tailor variants per job, export ATS-friendly PDF and HTML, with optional AI help from the provider of your choice.

PinaxCV is a local-first desktop application written in Rust (Tauri 2 + Leptos, Typst for PDF rendering, SQLite for storage). It needs no PinaxCV account, server or subscription, and sends nothing over the network unless you configure an AI provider or opt into update checks.

> **Status:** planning complete, implementation starting. Nothing is usable yet.

## Product strategy

1. **Standalone desktop (v1):** Linux and Windows. Local SQLite data that you own.
2. **macOS:** only if user interest justifies the testing, signing and support cost.
3. **Hosted SaaS:** a possible later phase backed by PostgreSQL, only if demand and a business case justify it. Authentication, sync, multi-tenancy and billing are intentionally absent from the desktop product.

**Architecture principle:** design for replacement, not simultaneous implementation. Domain and application-service boundaries are clean enough that SQLite can later be replaced by PostgreSQL, and Tauri can coexist with a hosted API, without forcing SaaS complexity into v1.

## Documentation

Read in this order:

1. [`pinaxcv-spec.md`](pinaxcv-spec.md): authoritative product and implementation requirements.
2. [`AGENTS.md`](AGENTS.md): rules for coding agents, including the branch and PR workflow.
3. [`PLAN.md`](PLAN.md): milestone plan.
4. [`docs/DECISIONS.md`](docs/DECISIONS.md): decisions made so far.
5. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): system architecture and dependency boundaries.
6. [`docs/DOMAIN.md`](docs/DOMAIN.md) and [`docs/DATA_MODEL.md`](docs/DATA_MODEL.md): domain semantics and persistence model.
7. [`docs/APPLICATION_API.md`](docs/APPLICATION_API.md): application-service boundary.
8. [`docs/RENDERING.md`](docs/RENDERING.md): PDF, preview and HTML export.
9. [`docs/AI_ARCHITECTURE.md`](docs/AI_ARCHITECTURE.md): AI pipeline, privacy and verification.
10. [`docs/PRODUCT_EVOLUTION.md`](docs/PRODUCT_EVOLUTION.md) and [`docs/SAAS_MIGRATION.md`](docs/SAAS_MIGRATION.md): future phases.
11. [`PROMPTS.md`](PROMPTS.md): agent prompts by milestone.

Do not implement future SaaS infrastructure merely because the architecture anticipates it.

## License

[Apache-2.0](LICENSE).
