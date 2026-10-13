# PinaxCV Rendering and Export

## Canonical pipeline

```text
Cv
 ↓
Variant
 ↓
resolve()
 ↓
ResolvedCv
 ├── Typst → PDF
 ├── Typst → PNG preview
 └── HTML exporter → single HTML file
```

## PDF

PDF output is optimized for ATS-safe parsing, printing, and deterministic testing.

## HTML

The HTML exporter is not a PDF-to-HTML converter. It directly consumes `ResolvedCv`.

Requirements:

- exactly one HTML file;
- UTF-8;
- embedded CSS;
- no external stylesheets;
- no external fonts;
- no external images;
- no JavaScript required;
- semantic HTML;
- responsive layout;
- accessible headings and lists;
- clickable `https:` and `mailto:` URLs only;
- optional email and phone;
- system fonts, automatic dark mode, print styles;
- escaped user content;
- no PinaxCV runtime dependency.

The HTML design should be web-friendly rather than pixel-identical to the PDF. Both formats must represent the same resolved CV content.
