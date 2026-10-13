#!/usr/bin/env bash
# Enforces the dependency boundaries from AGENTS.md and pinaxcv-spec.md section 3.
# Fails if a crate depends (directly or transitively) on something it must not.
set -euo pipefail

fail=0

# deps_of CRATE: names of all normal (non-dev, non-build) dependencies of CRATE.
deps_of() {
  cargo tree --locked -p "$1" -e normal --prefix none --format '{p}' \
    | awk '{print $1}' | sort -u
}

# forbid CRATE PATTERN REASON: fail if any dependency of CRATE matches PATTERN.
forbid() {
  local crate="$1" pattern="$2" reason="$3" hits
  hits=$(deps_of "$crate" | grep -E "^(${pattern})$" || true)
  if [[ -n "$hits" ]]; then
    echo "error: $crate must not depend on: $(echo "$hits" | tr '\n' ' ')($reason)"
    fail=1
  fi
}

# cv-core is pure: no platform, database, network, async runtime or filesystem crates.
forbid cv-core 'tauri.*|rusqlite|libsqlite3-sys|sqlx.*|postgres.*|tokio.*|reqwest|hyper.*|keyring|directories|dirs.*|cv-store|cv-render|cv-import|cv-ai|cv-api' \
  "cv-core must stay pure and WASM-compatible"

# cv-api is shared with the WASM UI: no storage, Tauri or network crates.
forbid cv-api 'tauri.*|rusqlite|libsqlite3-sys|reqwest|hyper.*|cv-store|cv-render|cv-import|cv-ai' \
  "cv-api types must stay transport-neutral"

# Only cv-store may use SQLite.
for crate in cv-render cv-import cv-ai; do
  forbid "$crate" 'rusqlite|libsqlite3-sys|cv-store' "only cv-store owns persistence"
done

# No PostgreSQL or SaaS server code anywhere in v1.
for crate in cv-core cv-api cv-store cv-render cv-import cv-ai; do
  forbid "$crate" 'postgres.*|tokio-postgres|sqlx.*|axum.*|actix.*' "no PostgreSQL or HTTP server in v1"
done

if [[ $fail -ne 0 ]]; then
  exit 1
fi
echo "dependency boundaries ok"
