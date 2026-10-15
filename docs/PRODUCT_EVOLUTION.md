# PinaxCV Product Evolution

## Stage 1 — Standalone desktop

**Linux + Windows.**

Local SQLite, no accounts, no PinaxCV server, no cloud synchronization, no telemetry, no subscription infrastructure.

The goal is to validate whether the product is useful without incurring hosting costs.

## Stage 2 — macOS

Release only if demand justifies platform support. Reuse the existing Rust domain, application services, renderers, importers, and AI abstractions.

Expected new work is platform integration, CI, signing/notarization, packaging, and testing.

## Stage 3 — Hosted SaaS

Release only if real product interest and a viable business case justify infrastructure costs.

Expected additions:

- authentication;
- authorization and tenant isolation;
- PostgreSQL;
- HTTP API;
- cloud persistence;
- synchronization;
- hosted backups;
- billing/subscriptions if needed;
- server deployment/observability.

These are intentionally absent from v1.

## Product validation before investment

Useful evidence includes actual usage, repeat usage, feedback, feature requests, willingness-to-pay signals, and explicit requests for macOS or synchronization.

The architecture should enable future work without requiring it today.
