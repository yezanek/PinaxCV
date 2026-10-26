# PinaxCV UI Specification Addendum

## Export

The export action exposes:

- PDF
- HTML

The HTML option explains that it creates a single self-contained file suitable for a personal website, and offers "Include email" (default on) and "Include phone" (default off) checkboxes.

## Screens as built

- **CV list:** cards with title, last edit, pages and variant count; New, Duplicate, Delete with an 8-second Undo; Import… and Import text…; Settings.
- **Editor:** Content, Style, Checks and Variants tabs beside the live preview (zoom: fit width or 100%). Undo/redo (`Ctrl+Z`, `Ctrl+Shift+Z`/`Ctrl+Y`), save (`Ctrl+S`) and export PDF (`Ctrl+E`); Cmd on macOS.
- **Variants tab:** version switcher, show/hide, one-line, reorder and per-variant text, length target with fit cuts, the tailor panel and the suggestion review list.
- **Checks tab:** lint findings with Show (jumps to the field), the ATS check with extracted text, and Review with AI.
- **Settings:** AI provider and consent, defaults for new CVs, Your data (folder, backup, restore, clear cache, delete all), About (version, licences).

All UI text lives in `ui/src/i18n.rs` (ADR-050); English is the only locale in v1.

## Future platform behavior

The v1 UI does not expose account, sync, subscription, or cloud settings because those features do not exist in the standalone product.
