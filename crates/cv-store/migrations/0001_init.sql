-- Migration 0001: initial schema (spec section 5.2). Applied in one transaction;
-- the runner sets PRAGMA user_version afterwards.

CREATE TABLE cvs (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL,
  content       TEXT NOT NULL,              -- JSON of Cv (includes schema_version)
  revision      INTEGER NOT NULL DEFAULT 1, -- optimistic concurrency
  created_at    TEXT NOT NULL,              -- RFC 3339 UTC
  updated_at    TEXT NOT NULL,
  deleted_at    TEXT                        -- soft delete for undo; purged after 30 days
);

CREATE TABLE variants (
  id            TEXT PRIMARY KEY,
  cv_id         TEXT NOT NULL REFERENCES cvs(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  content       TEXT NOT NULL,              -- JSON of Variant
  revision      INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE INDEX idx_variants_cv ON variants(cv_id);

CREATE TABLE snapshots (                    -- automatic history of base CVs
  id            TEXT PRIMARY KEY,
  cv_id         TEXT NOT NULL REFERENCES cvs(id) ON DELETE CASCADE,
  content       TEXT NOT NULL,
  reason        TEXT NOT NULL,              -- 'export' | 'import' | 'manual' | 'ai_apply'
  created_at    TEXT NOT NULL
);
CREATE INDEX idx_snapshots_cv ON snapshots(cv_id, created_at DESC);

CREATE TABLE review_sessions (              -- latest AI review (section 11.11) per CV, made without a variant
  cv_id         TEXT PRIMARY KEY REFERENCES cvs(id) ON DELETE CASCADE,
  content       TEXT NOT NULL,              -- JSON: suggestions + notes
  created_at    TEXT NOT NULL
);

CREATE TABLE settings (
  key           TEXT PRIMARY KEY,
  value         TEXT NOT NULL
);

CREATE TABLE ai_cache (
  key           TEXT PRIMARY KEY,           -- sha256(kind + prompt_version + model + input)
  kind          TEXT NOT NULL,
  response      TEXT NOT NULL,
  created_at    TEXT NOT NULL
);
