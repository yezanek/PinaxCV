-- Migration 0003: optional signature image per CV for the closing block
-- (ADR-090). Kept beside the CV JSON like the photo, so snapshots, AI
-- payloads and JSON exports never carry image bytes.

CREATE TABLE signatures (
  cv_id         TEXT PRIMARY KEY REFERENCES cvs(id) ON DELETE CASCADE,
  image         BLOB NOT NULL,              -- PNG, grey ink on transparency, metadata removed
  updated_at    TEXT NOT NULL
);
