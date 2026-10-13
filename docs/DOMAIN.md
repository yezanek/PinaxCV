# PinaxCV Domain

The canonical object is the Base CV. Variants are overlays and never silently mutate the base.

```text
Cv → Variant → resolve() → ResolvedCv
```

`ResolvedCv` is the input to renderers and exporters.

Entity IDs are stable ULIDs. IDs must survive edits, imports, variants, backups, and future migrations.

The domain contains no account, tenant, subscription, or database identity. Those concerns belong to deployment/persistence layers and will be introduced only if SaaS is activated.
