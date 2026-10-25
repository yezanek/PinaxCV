-- Migration 0002: optional profile photo per CV (ADR-083). Kept beside the CV
-- JSON so snapshots, AI payloads and JSON exports never carry image bytes.

CREATE TABLE photos (
  cv_id         TEXT PRIMARY KEY REFERENCES cvs(id) ON DELETE CASCADE,
  original      BLOB NOT NULL,              -- JPEG, metadata removed, at most 2000 px a side
  image         BLOB NOT NULL,              -- JPEG of the crop, at most 600 px a side
  crop          TEXT NOT NULL,              -- JSON of the crop, in original pixels
  updated_at    TEXT NOT NULL
);
