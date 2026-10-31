# PinaxCV Import and Export

## Import

Importers produce `CvPartial`, warnings, and confidence/review information. They never write directly to storage.

Supported v1 sources (`cv_import::import_any` picks the importer from the file's content):

- Europass CV: the XML file, or a Europass PDF with the XML embedded (ADR-043). Tried first for every PDF. An embedded profile picture (JPEG or PNG, at most 5 MB) is imported too, cleaned like a chosen photo and left hidden (ADR-086).
- Any other text PDF, LinkedIn "Save to PDF" included: heading and date heuristics, with LinkedIn's sidebar detected (ADR-047). Results carry warnings to check.
- JSON Resume (ADR-059).
- LinkedIn data export ZIP, read in memory with per-entry and total size limits and path-traversal checks (ADR-060).
- Pasted text, or a PDF or `.txt` file, through the AI text importer, or through the same heuristics without AI (ADR-076).

Every import goes through the import wizard (ADR-058): a preview with warnings, then a new CV or a merge into an existing one, where the user picks per contact field (keep mine or use imported), for the photo when both have one, and per entry (skip, add or replace). A merge takes a snapshot first. Inputs over 20 MB are refused before parsing. Tests use synthetic fixtures only; real CVs never go into the repository.

## Export

V1 exports:

1. ATS-compatible PDF.
2. Single self-contained HTML file.

The HTML export is intended for personal websites and should be directly uploadable without copying an asset directory. Email (default on) and phone (default off) are optional per export.

Future exporters such as DOCX/ODT remain possible through the exporter seam but are not v1 requirements.
