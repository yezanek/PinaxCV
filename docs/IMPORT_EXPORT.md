# PinaxCV Import and Export

## Import

Importers produce `CvPartial`, warnings, and confidence/review information. They never write directly to storage.

Supported v1 sources (`cv_import::import_any` picks the importer from the file's content):

- Europass CV: the XML file, or a Europass PDF with the XML embedded (ADR-043). Tried first for every PDF.
- Any other text PDF, LinkedIn "Save to PDF" included: heading and date heuristics, with LinkedIn's sidebar detected (ADR-047). Results carry warnings to check.
- JSON Resume (ADR-059).
- LinkedIn data export ZIP, read in memory with per-entry and total size limits and path-traversal checks (ADR-060).
- Pasted text, or a PDF or `.txt` file, through the AI text importer, or through the same heuristics without AI (ADR-076).

Every import goes through the import wizard (ADR-058): a preview with warnings, then a new CV or a merge into an existing one, where the user picks per contact field (keep mine or use imported) and per entry (skip, add or replace). A merge takes a snapshot first. Inputs over 20 MB are refused before parsing. Tests use synthetic fixtures only; real CVs never go into the repository.

## Export

V1 exports:

1. ATS-compatible PDF.
2. Single self-contained HTML file.

The HTML export is intended for personal websites and should be directly uploadable without copying an asset directory. Email (default on) and phone (default off) are optional per export.

Future exporters such as DOCX/ODT remain possible through the exporter seam but are not v1 requirements.

## Style files

The Style tab's **Save style…** and **Load style…** write and read a style file (`.pinaxstyle.xml`, ADR-084): the CV's template, paper, options and layout, never its content. Sections are named by kind, and custom sections also by title, so a style fits any CV. Saving writes the CV's current settings; loading is one edit, so Undo puts the previous look back.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<pinaxcv-style xmlns="urn:pinaxcv:style:1" version="1">
  <template id="classic"/>
  <paper size="a4"/>
  <options font-size-pt="10" line-spacing="1.1" margin-mm="18" date-style="month_name_year" accent="#2B5CAB"/>
  <layout columns="two" sidebar-width-pct="32">
    <block kind="name" column="sidebar"/>
    <block kind="contact" column="sidebar"/>
    <block kind="summary" column="main"/>
    <block kind="experience" column="main"/>
    <block kind="education" column="main"/>
    <block kind="languages" column="sidebar"/>
    <block kind="custom" title="Volunteering" column="main"/>
  </layout>
</pinaxcv-style>
```

- `paper size`: `a4` or `letter`. `date-style`: `month_name_year`, `numeric_month_year` or `year_only`. `accent` is optional (`#RRGGBB`; none means black).
- `columns`: `one` or `two`; `sidebar-width-pct` 25 to 40.
- `<block>`: `kind` is `name`, `contact`, `summary`, `experience`, `education`, `projects`, `skills`, `languages`, `certifications` or `custom` (with `title`); `column` is `sidebar` or `main` (the default). The blocks are one list: the top blocks in their order, then the sections in the CV's order. One list rather than a `<sidebar>` and a `<main>` list keeps the order across columns, which a one-column page shows. Placements are written in one-column mode too, as the layout keeps them.
- The name's column is drawn on the left, so there is no side setting.

Reading:

- Files over 256 KB are refused. The XML is parsed with `roxmltree`, which refuses DTDs, so entities cannot expand. The root must be `pinaxcv-style` in the `urn:pinaxcv:style:1` namespace with `version="1"`; a higher version is refused with a message to update PinaxCV.
- Unknown elements and attributes are ignored, so later additions to version 1 still load. Unknown block kinds are skipped, a repeated top block is dropped, and out-of-range or unknown values are clamped or replaced by defaults; each is reported in the summary. Missing parts use the defaults.
- A template this build does not have is reported, and the CV keeps its own.
- Matching: custom blocks first take a custom section with the same title (ignoring case and outer spaces), then every remaining block takes the next unused section of its kind, in order. Matched sections take the style's order and column; the CV's other sections follow in their own order, in the main column. The summary counts blocks with no section and sections the style did not name.
