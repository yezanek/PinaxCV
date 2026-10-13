# PinaxCV Planning Update

## 2026-09-29

Integrated the latest product decisions:

- Product renamed from the generic CV Builder to **PinaxCV**.
- Initial release explicitly targets Linux and Windows.
- macOS is deferred until user interest justifies platform support.
- Hosted SaaS is deferred until product demand and a business case justify infrastructure costs.
- Future SaaS is expected to use PostgreSQL.
- Added explicit persistence/application-service boundaries to make SQLite replaceable later.
- Added M0 Architecture & Contracts before implementation milestones.
- Added standalone single-file HTML export for personal websites.
- Added HTML exporter architecture, UI flow, tests, and acceptance criteria.
- Added product evolution, SaaS migration, storage, platform, rendering, application API, security, and release documentation.
