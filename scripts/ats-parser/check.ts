// Parses every exported fixture PDF with OpenResume's resume parser and
// checks that it reads back the name, contacts, jobs and schools (ADR-061).
//
// Usage: npx tsx check.ts DIR, where DIR holds NAME.pdf and NAME.json pairs
// from `cargo run -p cv-render --example export_fixtures -- DIR`.
// The parser is cloned into ./open-resume by CI; it is AGPL-3.0 and is only
// run here, never copied into PinaxCV.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.js";
import { groupTextItemsIntoLines } from "lib/parse-resume-from-pdf/group-text-items-into-lines";
import { groupLinesIntoSections } from "lib/parse-resume-from-pdf/group-lines-into-sections";
import { extractResumeFromSections } from "lib/parse-resume-from-pdf/extract-resume-from-sections";
import type { TextItems } from "lib/parse-resume-from-pdf/types";

/** OpenResume's `readPdf`, reading bytes instead of a browser URL. */
async function readPdf(data: Uint8Array): Promise<TextItems> {
  const assets = join(__dirname, "node_modules/pdfjs-dist");
  const doc = await pdfjs.getDocument({
    data,
    useSystemFonts: false,
    cMapUrl: join(assets, "cmaps") + "/",
    cMapPacked: true,
    standardFontDataUrl: join(assets, "standard_fonts") + "/",
  }).promise;
  const items: TextItems = [];
  for (let i = 1; i <= doc.numPages; i++) {
    const page = await doc.getPage(i);
    const content = await page.getTextContent();
    await page.getOperatorList();
    for (const raw of content.items) {
      if (!("str" in raw)) continue;
      const { str, dir, transform, fontName, ...rest } = raw;
      let name = fontName;
      try {
        name = page.commonObjs.get(fontName).name;
      } catch {
        // Keep pdf.js's internal name if the font is not loaded.
      }
      items.push({ ...rest, fontName: name, text: str.replace(/-­‐/g, "-"), x: transform[4], y: transform[5] });
    }
  }
  return items;
}

type Entry = { title: string; org: string };
type Fixture = {
  basics: { full_name: string; email?: string | null; phone?: string | null };
  sections: { type: string; entries?: Entry[] }[];
};

type Field = "name" | "email" | "phone" | "job" | "school";

// Fields this parser cannot read back in some fixtures, and why. A miss in a
// listed field is reported but does not fail the check; a miss in any other
// field, or in any other fixture, does.
const KNOWN_LIMITS: Record<string, { fields: Field[]; reason: string }> = {
  cjk: {
    fields: ["name", "email", "phone", "job", "school"],
    reason: "Inter has no CJK glyphs (ADR-033), so the export itself warns",
  },
  long: { fields: ["job"], reason: "OpenResume ignores job titles that contain digits (\"Engineer level 6\")" },
  unicode: { fields: ["job"], reason: "OpenResume recognises job titles from an English word list only" },
  two_column_long: {
    fields: ["job"],
    reason:
      "two columns over two pages: the sidebar's text comes after page 1 of the main column, so page 2's jobs land under the sidebar's last heading; the one-column ATS copy is the remedy (ADR-084), and the titles contain digits as in \"long\"",
  },
  special_chars: {
    fields: ["name", "job"],
    reason: "OpenResume's name pattern allows letters, spaces and full stops only, and its job titles come from an English word list",
  },
};

const squash = (s: string | null | undefined) => (s ?? "").replace(/\s+/g, " ").trim().toLowerCase();
const digits = (s: string | null | undefined) => (s ?? "").replace(/\D/g, "");

async function main() {
const dir = process.argv[2];
if (!dir) throw new Error("usage: tsx check.ts DIR");
let failures = 0;
for (const file of readdirSync(dir).filter((f) => f.endsWith(".pdf")).sort()) {
  const name = file.slice(0, -4);
  const cv: Fixture = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
  const items = await readPdf(new Uint8Array(readFileSync(join(dir, file))));
  const lines = groupTextItemsIntoLines(items);
  if (process.env.SHOW_LINES === name) {
    for (const line of lines) console.log(JSON.stringify(line.map((i) => [i.text, i.fontName])));
  }
  const resume = extractResumeFromSections(groupLinesIntoSections(lines));
  const problems: { field: Field; text: string }[] = [];
  const expect = (field: Field, what: string, ok: boolean, got: unknown) => {
    if (!ok) problems.push({ field, text: `${what}: got ${JSON.stringify(got)}` });
  };
  const b = cv.basics;
  if (b.full_name) expect("name", "name", squash(resume.profile.name) === squash(b.full_name), resume.profile.name);
  if (b.email) expect("email", "email", squash(resume.profile.email) === squash(b.email), resume.profile.email);
  // OpenResume reads US-style ten-digit numbers only.
  if (b.phone && /^1?\d{10}$/.test(digits(b.phone))) {
    const ok = digits(resume.profile.phone).endsWith(digits(b.phone).slice(-10));
    expect("phone", "phone", ok, resume.profile.phone);
  }
  const entries = (type: string) => cv.sections.filter((s) => s.type === type).flatMap((s) => s.entries ?? []);
  // Each field is compared on its own, so a title read as the company (or
  // the other way round) is still a miss.
  for (const e of entries("experience")) {
    const found = resume.workExperiences.some(
      (w) => squash(w.jobTitle).includes(squash(e.title)) && squash(w.company).includes(squash(e.org)),
    );
    const got = resume.workExperiences.map((w) => ({ title: w.jobTitle, company: w.company }));
    expect("job", `job "${e.title}" at "${e.org}"`, found, got);
  }
  for (const e of entries("education")) {
    const found = resume.educations.some((s) => squash(s.school).includes(squash(e.org)));
    expect("school", `school "${e.org}"`, found, resume.educations.map((s) => s.school));
  }
  const known = KNOWN_LIMITS[name];
  const excused = (field: Field) => known?.fields.includes(field) ?? false;
  const unexpected = problems.filter((p) => !excused(p.field));
  const status = problems.length === 0 ? "ok  " : unexpected.length === 0 ? "known" : "FAIL";
  console.log(`${status} ${name}${problems.length && known ? ` (${known.reason})` : ""}`);
  for (const p of problems) console.log(`     ${excused(p.field) ? "known: " : ""}${p.text}`);
  failures += unexpected.length ? 1 : 0;
}
if (failures) {
  console.error(`${failures} fixture(s) did not parse back`);
  process.exit(1);
}
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
