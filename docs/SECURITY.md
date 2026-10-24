# PinaxCV Security and Privacy Architecture

## Desktop v1

- Offline by default.
- No telemetry or crash upload.
- Network requests only for AI calls the user has configured and consented to, optional update checks, and user-clicked HTTPS links.
- API keys stored in the OS keychain or memory-only fallback; never in SQLite, logs, or backups.
- Logs contain no CV content, job descriptions, prompts, or secrets.
- The local log is `pinaxcv.log` in the app's log folder: `~/.local/share/dev.pinaxcv.app/logs` on Linux, `%LOCALAPPDATA%\dev.pinaxcv.app\logs` on Windows, and `~/Library/Logs/dev.pinaxcv.app` on macOS. It records startup, each command's name, duration and error code, and panics (location and a shortened message). It never records command arguments or error messages, which can quote CV content. A log over 1 MB is moved to `pinaxcv.1.log` at startup. Users open it with any text editor and attach it to an issue themselves; nothing is uploaded.
- User data deletion removes database/WAL data and stored keychain credentials after confirmation.
- Embedded templates/fonts only; no user-supplied executable templates in v1.
- Only `https:` and `mailto:` links are clickable in exports.
- The backend owns every file dialog; the UI never passes file paths.
- Contact details are never sent to AI providers, except by the text importer after consent that says so.
- "Clear AI cache" and "Delete all data" cover cached AI responses, which contain CV text.

## Future SaaS

The hosted phase requires a separate threat model covering authentication, authorization, tenant isolation, network security, database backups, secrets management, abuse controls, logging, incident response, and data deletion.

Do not retrofit SaaS security assumptions into the standalone application in ways that add unnecessary infrastructure.
