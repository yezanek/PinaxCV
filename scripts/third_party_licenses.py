#!/usr/bin/env python3
"""Writes THIRD_PARTY_LICENSES.md from `cargo metadata` and checks that every
dependency shipped in the app has a licence compatible with Apache-2.0.

Shipped means reachable from the desktop app (`pinaxcv`) or the UI
(`pinaxcv-ui`) through normal dependencies, on any platform. Build and
dev dependencies are not shipped and are left out.

    python3 scripts/third_party_licenses.py           # rewrite the file
    python3 scripts/third_party_licenses.py --check   # fail if it is stale
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD_PARTY_LICENSES.md"
APPS = ("pinaxcv", "pinaxcv-ui")

# Licences that may be combined with Apache-2.0 in a distributed binary.
ALLOWED = {
    "0BSD", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause",
    "BSL-1.0", "CC0-1.0", "CDLA-Permissive-2.0", "ISC", "MIT", "MIT-0", "MPL-2.0",
    "OFL-1.1", "Unicode-3.0", "Unicode-DFS-2016", "Unlicense", "Zlib",
}

# Fonts are not crates: listed by hand from their licence files.
FONTS = [
    ("Inter", "crates/cv-render/fonts/LICENSE.txt", "OFL-1.1", "https://github.com/rsms/inter"),
]

# Crates that ship no licence file: the text from their upstream repository,
# kept in licenses/. Used only when the crate has no file of its own.
UPSTREAM_TEXTS = {
    "alloc-stdlib": "licenses/alloc-stdlib-BSD-3-Clause.txt",
    "roman-numerals-rs": "licenses/roman-numerals-rs-0BSD.txt",
    "selectors": "licenses/selectors-MPL-2.0.txt",
}

LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE)", re.I)
COPYRIGHT = re.compile(r"^\s*(copyright\s*(\(c\)|©|\d{4})|\(c\)\s*\d{4}|©)", re.I)


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def shipped(meta):
    """Packages reachable from the apps through normal dependencies."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    packages = {p["id"]: p for p in meta["packages"]}
    members = set(meta["workspace_members"])
    roots = [p["id"] for p in meta["packages"] if p["name"] in APPS and p["id"] in members]
    seen, stack = set(), list(roots)
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    return sorted(
        (packages[p] for p in seen if p not in members),
        key=lambda p: (p["name"], p["version"]),
    )


def alternatives(expr):
    """An SPDX expression as a list of AND-groups, any one of which suffices.

    AND binds tighter than OR and parentheses group, as SPDX defines, so
    `GPL-3.0-only AND (MIT OR Apache-2.0)` gives two groups that both need
    GPL-3.0-only. `WITH` stays part of its licence; `/` is the old OR."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def take():
        nonlocal pos
        pos += 1
        return tokens[pos - 1]

    def term():
        if peek() == "(":
            take()
            groups = either()
            if peek() != ")":
                raise ValueError(f"unbalanced parentheses in licence {expr!r}")
            take()
            return groups
        if peek() in (None, ")", "AND", "OR", "WITH"):
            raise ValueError(f"malformed licence {expr!r}")
        name = take()
        if peek() == "WITH":
            take()
            if peek() in (None, "(", ")", "AND", "OR", "WITH"):
                raise ValueError(f"malformed licence {expr!r}")
            name += " WITH " + take()
        return [[name]]

    def both():
        groups = term()
        while peek() == "AND":
            take()
            right = term()
            groups = [a + b for a in groups for b in right]
        return groups

    def either():
        groups = both()
        while peek() == "OR":
            take()
            groups += both()
        return groups

    if not tokens:
        return []
    groups = either()
    if pos != len(tokens):
        raise ValueError(f"malformed licence {expr!r}")
    return groups


def chosen(expr):
    """The licences PinaxCV uses a crate under: Apache-2.0 or MIT where the
    crate offers them, otherwise the first allowed choice. None if no
    choice is allowed."""
    try:
        options = alternatives(expr)
    except ValueError:
        return None
    groups = [g for g in options if all(p in ALLOWED for p in g)]
    preferred = [g for g in groups if set(g) <= {"Apache-2.0", "MIT"}]
    return (preferred or groups or [None])[0]


def notices(pkg):
    """Every copyright line from the package's licence files, without repeats."""
    lines = []
    for f in licence_files(pkg):
        try:
            text = f.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            line = " ".join(line.split())
            if COPYRIGHT.match(line) and "<" not in line[:20] and "[yyyy]" not in line and "{" not in line:
                if line not in lines:
                    lines.append(line)
    return lines


def licence_files(pkg):
    root = Path(pkg["manifest_path"]).parent
    files = sorted(f for f in root.iterdir() if f.is_file() and LICENSE_FILE.match(f.name))
    if pkg.get("license_file"):
        files.append(root / pkg["license_file"])
    if not files and pkg["name"] in UPSTREAM_TEXTS:
        files.append(ROOT / UPSTREAM_TEXTS[pkg["name"]])
    return files


def mit_text(pkgs):
    """The MIT licence's terms, from the first MIT-only package that ships them."""
    for p in pkgs:
        root = Path(p["manifest_path"]).parent
        for name in ("LICENSE-MIT", "LICENSE-MIT.md", "LICENSE"):
            f = root / name
            if f.is_file():
                text = f.read_text(encoding="utf-8", errors="replace")
                start = text.find("Permission is hereby granted")
                if start >= 0:
                    return text[start:].strip()
    return None


def render(pkgs):
    by_licence = {}
    for p in pkgs:
        by_licence.setdefault(p.get("license") or "(none)", []).append(p)
    out = [
        "# Third-party licences",
        "",
        "PinaxCV is licensed under Apache-2.0 (see `LICENSE`). It ships the components below.",
        "This file is generated by `scripts/third_party_licenses.py` from `Cargo.lock`; do not edit it by hand.",
        "",
        "## Fonts",
        "",
    ]
    for name, path, licence, url in FONTS:
        text = (ROOT / path).read_text(encoding="utf-8")
        notice = next((l.strip() for l in text.splitlines() if COPYRIGHT.match(l)), "")
        out.append(f"- **{name}** ({url}), {licence}. {notice}")
    out += ["", "## Rust crates", ""]
    out.append(f"{len(pkgs)} crates, by licence:")
    out.append("")
    out.append("| Licence | Crates |")
    out.append("| --- | --- |")
    for licence in sorted(by_licence):
        out.append(f"| {licence} | {len(by_licence[licence])} |")
    for licence in sorted(by_licence):
        out += ["", f"### {licence}", ""]
        for p in by_licence[licence]:
            repo = p.get("repository") or ""
            line = f"- `{p['name']}` {p['version']}"
            if repo:
                line += f" ({repo})"
            notes = notices(p)
            if notes:
                line += ": " + "; ".join(notes)
            out.append(line)
    out += ["", "## Licence texts", ""]
    out.append("Each crate's copyright notice is listed with it above.")
    out += ["", "### Apache-2.0", "", "```text", (ROOT / "LICENSE").read_text(encoding="utf-8").strip(), "```"]
    mit = mit_text([p for p in pkgs if (p.get("license") or "").startswith("MIT")])
    if mit:
        out += ["", "### MIT", "", "```text", mit, "```"]
    for name, path, licence, _ in FONTS:
        out += ["", f"### {licence} ({name})", "", "```text", (ROOT / path).read_text(encoding="utf-8").strip(), "```"]
    # Other licences ask for their text to ship too: each crate's own files.
    texts = []
    missing = []
    for p in pkgs:
        if set(chosen(p.get("license") or "")) <= {"Apache-2.0", "MIT"}:
            continue
        found = False
        for f in licence_files(p):
            if re.search(r"APACHE|MIT", f.name, re.I):
                continue
            try:
                text = f.read_text(encoding="utf-8", errors="replace").strip()
            except OSError:
                continue
            if not text:
                continue
            found = True
            if text not in texts:
                texts.append(text)
                licence = " AND ".join(chosen(p["license"]))
                out += ["", f"### {licence} (`{p['name']}`)", "", "```text", text, "```"]
        if not found:
            missing.append(p)
    return "\n".join(out) + "\n", missing


def main():
    pkgs = shipped(metadata())
    bad = [p for p in pkgs if chosen(p.get("license") or "") is None]
    if bad:
        for p in bad:
            print(f"not compatible with Apache-2.0 or unknown: {p['name']} {p['version']}: {p.get('license')}", file=sys.stderr)
        sys.exit(1)
    text, missing = render(pkgs)
    if missing:
        for p in missing:
            licence = " AND ".join(chosen(p["license"]))
            print(f"no {licence} licence text found for {p['name']} {p['version']}", file=sys.stderr)
        sys.exit(1)
    if "--check" in sys.argv[1:]:
        if not OUT.is_file() or OUT.read_text(encoding="utf-8") != text:
            print("THIRD_PARTY_LICENSES.md is out of date: run python3 scripts/third_party_licenses.py", file=sys.stderr)
            sys.exit(1)
        print(f"THIRD_PARTY_LICENSES.md is up to date ({len(pkgs)} crates).")
        return
    OUT.write_text(text, encoding="utf-8")
    print(f"Wrote {OUT.name}: {len(pkgs)} crates.")


if __name__ == "__main__":
    main()
