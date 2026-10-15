//! LLM provider abstraction, prompts, redaction and verification. Receives explicit payloads; never reads the database.
//!
//! See `pinaxcv-spec.md` and `docs/ARCHITECTURE.md` for this crate's responsibilities.

#![forbid(unsafe_code)]
