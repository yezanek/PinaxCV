# PinaxCV ATS Rules

The ATS checks apply primarily to PDF output. They are best-practice checks, not vendor-published specifications.

The HTML export is intended for personal websites and is not the primary ATS submission artifact.

Users should still verify the final document through the employer's application flow.

## Template rules (enforced by the golden ATS suite)

1. Single column by default; no side-by-side layout. Exceptions: the optional profile photo (ADR-083) may sit beside the contact block, as a PDF artifact with no text, so extraction and reading order are the same with and without it; and a CV may choose two columns (ADR-084). With two columns the column holding the name is drawn on the left, because PDF text is read left column first; each column must read in order on its own, bullets and descriptions never break across pages, and on a CV longer than a page the right column's text comes after page 1 of the left column. Some ATS read straight across the page, so the ATS check warns and export offers a one-column ATS copy.
2. Reading order equals source order; no floating or absolutely positioned content.
3. Standard headings, printed as written (not upper-cased).
4. Contact block is body text at the top of page 1, with no icons. Email and phone should appear within the first five extracted lines; lines six to eight still pass, and the check fails only if either appears after line eight.
5. No headers or footers with contact info; a "Page 1 of 2" footer only if it does not interleave with body text.
6. Ligatures off; kerning on.
7. Real lists with a standard bullet glyph.
8. No images, backgrounds, skill bars or decorative glyphs. Exceptions: the user's profile photo (ADR-083) and the user's signature image in the closing block (ADR-090). Both are off by default, shown only when switched on for the CV or variant, and drawn as PDF artifacts with no text, so extracted text is the same with and without them.
9. Text is real text, never outlined shapes.
10. Inter only; document language set from the CV locale.
11. Empty fields and sections leave no stray separators or headings.
12. No orphan headings; an entry's first two lines stay together.

13. An entry's title (bold) and period (right-aligned) share the first line; the organisation and location start the second, as separate text runs (ADR-061).

## Open-source parser check

CI parses every exported fixture with OpenResume's resume parser and requires it to read back the name, contacts, jobs and schools (ADR-061). Fixtures it cannot read for documented reasons outside the template are reported but do not fail. PDFs are tagged and declare PDF/UA-1 whenever Typst can meet it.

## ATS report checks

Name within the first 3 non-empty lines; email and phone within the first 8 and unbroken; headings present and in order; each entry's title, org and period within a 4-line window; no private-use, replacement or ligature codepoints; every bullet recoverable; heading and entry order matches the CV. The score is labelled "as read by PinaxCV" if the custom extractor fallback is used.

The score is the share of these checks that pass, from 0 to 100. Text that cannot be read from the PDF scores 0. CJK text is a known limitation in v1: the bundled Inter font has no CJK glyphs, so the export warns about missing glyphs and the check fails (ADR-033).
