# PinaxCV Import and Export

## Import

Importers produce `CvPartial`, warnings, and confidence/review information. They never write directly to storage.

Supported v1 sources:

- JSON Resume
- Europass JSON/XML (verified against real exports)
- LinkedIn ZIP (verified against a real export)
- pasted text
- text-bearing PDF via the AI-assisted importer, or a heuristic importer without AI (low confidence)

## Export

V1 exports:

1. ATS-compatible PDF.
2. Single self-contained HTML file.

The HTML export is intended for personal websites and should be directly uploadable without copying an asset directory. Email (default on) and phone (default off) are optional per export.

Future exporters such as DOCX/ODT remain possible through the exporter seam but are not v1 requirements.
