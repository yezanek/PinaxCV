# PinaxCV SaaS Migration Strategy

This document describes a future direction, not a v1 implementation plan.

## Intended architecture

```text
Client
  ↓ HTTPS
PinaxCV API
  ↓
Application services
  ├── cv-core
  ├── cv-render
  └── cv-ai
  ↓
PostgreSQL
```

## Desktop continuity

The standalone application remains:

```text
Tauri → cv-api → SQLite
```

The domain semantics should remain shared.

## Data migration

The desktop backup/export format should contain enough canonical CV data to support a future migration/import into SaaS. Secrets must never be part of this data migration.

A future migration tool can therefore follow:

```text
SQLite backup
   ↓
validated canonical data
   ↓
server import
   ↓
PostgreSQL
```

## Deferred decisions

Do not decide in v1:

- synchronization conflict semantics;
- tenant schema details;
- authentication provider;
- subscription model;
- public API versioning;
- server-side versus client-side rendering;
- cloud AI routing.

These depend on actual SaaS requirements.
