# PinaxCV Architecture

## 1. Runtime architecture

```text
Leptos UI
   │
   ▼
Tauri commands (src-tauri)      ◄── cv-api DTOs ──►  Leptos UI (WASM)
   │
   ▼
cv-app application services
   │
   ├── cv-core
   ├── cv-store ── SQLite (v1)
   ├── cv-render ── Typst / HTML
   ├── cv-import
   └── cv-ai
```

`cv-core` is the semantic center. It must not know about the desktop shell, database, network, or operating system.

`cv-api` holds only the request/response DTOs and `ApiError`. It is compiled into the WASM UI as well as the backend, so it cannot depend on storage, rendering or network crates. The application services that orchestrate a request (load, resolve, render, save) live in `cv-app`, a native-only crate that `src-tauri` calls and a future HTTP server could reuse (ADR-021).

## 2. Future deployment models

Desktop:

```text
Leptos → Tauri → cv-app → SQLite
```

Future hosted service:

```text
Web/Desktop client → HTTPS API → application services → PostgreSQL
```

The latter is deliberately not implemented in v1.

## 3. Boundaries

### Domain

`cv-core` owns CV semantics, variants, resolution, validation, linting, and fit/prune behavior.

### Persistence

`cv-store` owns SQLite. Repository/application interfaces must not expose `rusqlite` types.

### Application API

`cv-api` defines operations as explicit request/response contracts. `cv-app` implements them; Tauri commands adapt them to the UI. Future HTTP transport can expose equivalent operations through `cv-app` without changing domain semantics.

### Rendering

`cv-render` consumes a `ResolvedCv`. PDF and HTML are independent output formats from the same semantic input.

### Imports

`cv-import` transforms external formats into `CvPartial`; persistence happens only after user review.

### AI

`cv-ai` receives explicit redacted payloads. It does not access the database directly.

### Platform

Filesystem paths, keychain, update integration, packaging, and native behavior must be isolated from domain code.

## 4. What not to abstract prematurely

Do not implement a generic distributed storage system, database-agnostic ORM, HTTP API, or SaaS authentication layer merely because future SaaS is anticipated. A clean interface and one good SQLite implementation are sufficient for v1.
