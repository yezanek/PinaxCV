# PinaxCV ATS Rules

The ATS checks apply primarily to PDF output. They are best-practice checks, not vendor-published specifications.

The HTML export is intended for personal websites and is not the primary ATS submission artifact.

Users should still verify the final document through the employer's application flow.

## Template rules (enforced by the golden ATS suite)

1. Single column; no side-by-side layout. Exception: the optional profile photo (ADR-083) may sit beside the contact block, as a PDF artifact with no text, so extraction and reading order are the same with and without it.
2. Reading order equals source order; no floating or absolutely positioned content.
3. Standard headings, printed as written (not upper-cased).
4. Contact block is body text at the top of page 1, with no icons. Email and phone should appear within the first five extracted lines; lines six to eight still pass, and the check fails only if either appears after line eight.
5. No headers or footers with contact info; a "Page 1 of 2" footer only if it does not interleave with body text.
6. Ligatures off; kerning on.
7. Real lists with a standard bullet glyph.
8. No images, backgrounds, skill bars or decorative glyphs. Exception: the user's profile photo, off by default and shown only when switched on for the CV or variant (ADR-083).
9. Text is real text, never outlined shapes.
10. Inter only; document language set from the CV locale.
11. Empty fields and sections leave no stray separators or headings.
12. No orphan headings; an entry's first two lines stay together.

## ATS report checks

Name within the first 3 non-empty lines; email and phone within the first 8 and unbroken; headings present and in order; each entry's title, org and period within a 4-line window; no private-use, replacement or ligature codepoints; every bullet recoverable; heading and entry order matches the CV. The score is labelled "as read by PinaxCV" if the custom extractor fallback is used.

The score is the share of these checks that pass, from 0 to 100. Text that cannot be read from the PDF scores 0. CJK text is a known limitation in v1: the bundled Inter font has no CJK glyphs, so the export warns about missing glyphs and the check fails (ADR-033).
