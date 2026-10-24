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

// Fixtures this parser cannot read back, and why. They are reported but do
// not fail the check; every other fixture must parse back completely.
const KNOWN_LIMITS: Record<string, string> = {
  cjk: "Inter has no CJK glyphs (ADR-033), so the export itself warns",
  long: "OpenResume ignores job titles that contain digits (\"Engineer level 6\")",
  unicode: "OpenResume recognises job titles from an English word list only",
  special_chars: "OpenResume's name pattern allows letters, spaces and full stops only",
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
  const problems: string[] = [];
  const expect = (what: string, ok: boolean, got: unknown) => {
    if (!ok) problems.push(`${what}: got ${JSON.stringify(got)}`);
  };
  const b = cv.basics;
  if (b.full_name) expect("name", squash(resume.profile.name) === squash(b.full_name), resume.profile.name);
  if (b.email) expect("email", squash(resume.profile.email) === squash(b.email), resume.profile.email);
  // OpenResume reads US-style ten-digit numbers only.
  if (b.phone && /^1?\d{10}$/.test(digits(b.phone))) {
    expect("phone", digits(resume.profile.phone).endsWith(digits(b.phone).slice(-10)), resume.profile.phone);
  }
  const entries = (type: string) => cv.sections.filter((s) => s.type === type).flatMap((s) => s.entries ?? []);
  const jobs = resume.workExperiences.map((w) => `${squash(w.jobTitle)} | ${squash(w.company)}`);
  for (const e of entries("experience")) {
    const found = jobs.some((j) => j.includes(squash(e.title)) && j.includes(squash(e.org)));
    expect(`job "${e.title}" at "${e.org}"`, found, jobs);
  }
  const schools = resume.educations.map((e) => `${squash(e.school)} | ${squash(e.degree)}`);
  for (const e of entries("education")) {
    expect(`school "${e.org}"`, schools.some((s) => s.includes(squash(e.org))), schools);
  }
  const known = KNOWN_LIMITS[name];
  const status = problems.length === 0 ? "ok  " : known ? "known" : "FAIL";
  console.log(`${status} ${name}${problems.length && known ? ` (${known})` : ""}`);
  for (const p of problems) console.log(`     ${p}`);
  failures += problems.length && !known ? 1 : 0;
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
