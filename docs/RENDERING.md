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

PDF output is optimized for ATS-safe parsing, printing, and deterministic testing. Exports are tagged and declare PDF/UA-1 when Typst can meet it, falling back to a tagged PDF without the claim (ADR-061). CI parses every fixture export with OpenResume's parser.

The `classic` template (embedded, with Inter) takes the CV's style options: paper size, date style, font size, line spacing, margins and accent colour, within the ranges in its manifest. The optional profile photo sits beside the contact block as an artifact, so the extracted text is the same with or without it (ADR-083).

## Preview

The editor shows PNG pages rendered from the same Typst document and served over the `preview` URI scheme (`preview://localhost`; WebView2 on Windows reaches it as `http://preview.localhost`), with a strip of template diagnostics (such as missing glyphs) from the newest render.

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
- the profile photo and the signature, when shown, embedded as `data:` URLs with an empty `alt`;
- no PinaxCV runtime dependency.

The HTML design should be web-friendly rather than pixel-identical to the PDF. Both formats must represent the same resolved CV content.
