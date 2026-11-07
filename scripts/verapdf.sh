#!/usr/bin/env bash
# Validates every exported PDF that declares PDF/UA-1 with veraPDF, the PDF
# Association's open-source validator (issue #138, ADR-061). A PDF that
# declares the standard must pass it; one that dropped the claim (ADR-061)
# is listed but not checked. CI only: veraPDF is Java and is never bundled.
#
# Usage: scripts/verapdf.sh DIR, where DIR holds the PDFs from
# `cargo run -p cv-render --example export_fixtures -- DIR`.
set -euo pipefail

dir=$(cd "${1:?usage: scripts/verapdf.sh DIR}" && pwd)
image="verapdf/cli:v1.30.2"

claimed=()
for pdf in "$dir"/*.pdf; do
  name=$(basename "$pdf")
  if grep -q --binary-files=text 'pdfuaid' "$pdf"; then
    claimed+=("/data/$name")
  else
    echo "SKIP $name (no PDF/UA-1 claim)"
  fi
done
if [ "${#claimed[@]}" -eq 0 ]; then
  echo "No PDF declares PDF/UA-1; the export should for most fixtures." >&2
  exit 1
fi

report=$(docker run --rm -v "$dir:/data:ro" "$image" --flavour ua1 --format text "${claimed[@]}") || true
echo "$report"
if [ "$(grep -c '^PASS ' <<<"$report" || true)" -ne "${#claimed[@]}" ]; then
  echo "A PDF that declares PDF/UA-1 does not pass veraPDF. Details:" >&2
  for pdf in "${claimed[@]}"; do
    if ! grep -q "^PASS $pdf" <<<"$report"; then
      docker run --rm -v "$dir:/data:ro" "$image" --flavour ua1 --format text --verbose "$pdf" >&2 || true
    fi
  done
  exit 1
fi
echo "All ${#claimed[@]} PDFs that declare PDF/UA-1 pass veraPDF."
