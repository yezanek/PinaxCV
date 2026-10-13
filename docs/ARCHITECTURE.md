# PinaxCV Architecture

## 1. Runtime architecture

```text
Leptos UI
   │
   ▼
Tauri commands
   │
   ▼
cv-api application services
   │
   ├── cv-core
   ├── cv-store ── SQLite (v1)
   ├── cv-render ── Typst / HTML
   ├── cv-import
   └── cv-ai
```

`cv-core` is the semantic center. It must not know about the desktop shell, database, network, or operating system.

## 2. Future deployment models

Desktop:

```text
Leptos → Tauri → cv-api → SQLite
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

`cv-api` defines operations as explicit request/response contracts. Tauri commands adapt these contracts to the UI. Future HTTP transport can expose equivalent operations without changing domain semantics.

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
