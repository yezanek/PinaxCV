# Privacy

PinaxCV is a desktop app. Your CVs stay on your computer, and PinaxCV has no
account, no server and no analytics. This page says what it stores, what it
sends, and how to remove it.

## What stays on your computer

- **Your CVs, variants and settings** are kept in one database file in the
  app's data folder. Settings → Your data → "Open folder" shows where it is.
- **Cached AI answers.** When you use AI features, PinaxCV keeps the answers
  in the same database so the same request is not sent twice. They contain
  text from your CV. Settings → Your data → "Clear AI cache" removes them.
- **A log file** (`pinaxcv.log`, at most about 2 MB) records which actions
  ran, how long they took and error codes, so you can attach it to a bug
  report yourself. It never contains CV text, job descriptions, AI prompts or
  API keys, and it is never uploaded.
- **Your AI provider's API key** is stored in your system's keychain (macOS
  Keychain, Windows Credential Manager, or the Secret Service on Linux). If
  no keychain is available, PinaxCV keeps the key in memory only and asks for
  it again next time. It is never written to the database, the log or a
  backup.

## What PinaxCV sends over the network

PinaxCV makes no network requests of its own except these:

1. **The AI provider you set up**, and only when you start an AI feature
   (tailoring a CV, reading pasted text). Without an AI provider, PinaxCV
   works fully offline. Before the first request to a provider, PinaxCV
   asks for your consent and names the host it will send to; changing the
   provider asks again. What is sent:
   - **Tailoring** sends your CV's text and the job description. Email,
     phone, links and street addresses are taken out first, and anything that
     looks like an email address, a link or a phone number is replaced by
     `[redacted]`. Your name is left out unless you turn on "Send my name".
     Locations are cut to city and country, and are left out entirely when
     you turn off "Send my city and country".
   - **Importing text with AI** shows you the exact text before anything is
     sent. Email addresses, phone numbers and links are taken out of it and
     found again on your computer; your name, headline and location are sent
     as written, because the model needs them to read the CV.

   What the provider does with that data is governed by its own privacy
   policy. A local provider such as Ollama keeps everything on your machine.
2. **HTTPS links you click.** Only `https://` web links (and `mailto:`
   email links) are made clickable in your exported PDF and HTML files.
   Clicking one opens it in your normal web browser; PinaxCV itself never
   opens a link or any other kind of address.

There is no telemetry, no crash upload and no tracking. Exported PDF and HTML
files load nothing from the internet: fonts and styles are built in.

A check for new versions on GitHub Releases is planned. When it comes,
PinaxCV will ask you about it on first run with "off" already selected, and
you can change your answer later in Settings. Until you turn it on, it makes
no request.

## Backups and exports

- **Back up…** writes a copy of all your CVs, settings and cached AI answers
  to a file you choose. It holds personal data, so keep it somewhere safe. It
  never contains your API key.
- **Exports** (PDF and HTML) are files you choose to save and share.
  PinaxCV only writes to the place you picked in the save dialog.

## Removing your data

- **Clear AI cache** removes every cached AI answer and keeps your CVs.
- **Delete all data…** (after you confirm) deletes every CV, variant,
  snapshot, setting and cached answer, compacts the database so the deleted text is not
  left in the file, removes the safety copies made by a restore, and deletes
  your API keys from the keychain. Files you saved elsewhere (exports and
  backups) and the log file are not touched; delete them yourself if you
  want.
- Uninstalling PinaxCV does not always remove the data folder. Delete it
  yourself (Settings → Your data → "Open folder") to remove everything.

## Questions

PinaxCV is open source: you can read exactly what it does at
<https://github.com/yezanek/pinaxcv>. Report privacy problems as an issue
there.
