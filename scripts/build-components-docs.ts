#!/usr/bin/env bun
//
// Generate packages/components/README.md and the AGENTS.md "Exports" section
// from docs/*.md, and enforce doc coverage of the public surface.
//
// One file per public module lives in packages/components/docs/, named after
// the module src/index.ts re-exports from (docs/button.md for "./button").
// Prose lives there and nowhere else; the props are the typed, commented
// interfaces in src/ (the package ships its source). This script only
// assembles:
//
//   README.md            generated whole: docs/index.md, then the concept
//                        docs, then every component doc as a section, each
//                        with an API line naming the module's exports.
//   AGENTS.md            only the block between the GENERATED markers: one
//                        bullet per module, its doc's first paragraph.
//
// Coverage is the point: a module without a doc file, or a doc file without
// a module, fails the build (exit 1). --check regenerates in memory and
// fails on drift instead of writing, so CI can hold the line.

import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

let PKG = join(dirname(fileURLToPath(import.meta.url)), "..", "packages", "components");
let DOCS = join(PKG, "docs");
let RUN = "bun scripts/build-components-docs.ts";

// Concept modules lead the README (they explain the system the components
// assume), in this order; every other module is a component section.
// SYNC: website/src/reference.ts (componentsPages groups by the same list).
const CONCEPTS = ["theme", "policy", "types", "typography", "spacing"];

let check = process.argv.includes("--check");
let failed = false;
function fail(message: string): void {
  console.error(message);
  failed = true;
}

// -- The public surface: src/index.ts re-export blocks, in order ------------

type Module = { stem: string; names: string[] };

let index = readFileSync(join(PKG, "src", "index.ts"), "utf8");
let modules: Module[] = [];
for (let m of index.matchAll(/^export (?:type )?\{([^}]*)\} from "\.\/([a-z0-9-]+)"/gm)) {
  let names = m[1]!
    .split(",")
    .map((n) => n.trim().replace(/^type /, ""))
    .filter(Boolean);
  let existing = modules.find((mod) => mod.stem === m[2]);
  if (existing) existing.names.push(...names);
  else modules.push({ stem: m[2]!, names });
}

function srcFile(stem: string): string {
  return existsSync(join(PKG, "src", stem + ".tsx")) ? `src/${stem}.tsx` : `src/${stem}.ts`;
}

// -- Coverage: docs/ and index.ts must name the same modules -----------------

let docFiles = readdirSync(DOCS)
  .filter((f) => f.endsWith(".md") && f !== "index.md")
  .map((f) => f.slice(0, -3));
for (let { stem } of modules)
  if (!docFiles.includes(stem)) fail(`No doc: src/index.ts exports from "./${stem}" but docs/${stem}.md is missing`);
for (let stem of docFiles)
  if (!modules.some((m) => m.stem === stem)) fail(`Orphan doc: docs/${stem}.md matches no export in src/index.ts`);
for (let stem of CONCEPTS)
  if (!modules.some((m) => m.stem === stem)) fail(`CONCEPTS names "${stem}", which src/index.ts does not export from`);
if (failed) process.exit(1);

// -- Doc parsing -------------------------------------------------------------

type Doc = { title: string; body: string; firstParagraph: string };

function docOf(stem: string): Doc {
  let source = readFileSync(join(DOCS, stem + ".md"), "utf8").trim();
  let m = source.match(/^# (.+)\n+([\s\S]*)$/);
  if (!m) {
    fail(`docs/${stem}.md must start with an h1 title`);
    return { title: stem, body: source, firstParagraph: "" };
  }
  let body = m[2]!.trim();
  let firstParagraph = (body.split(/\n\s*\n/)[0] ?? "").split("\n").map((l) => l.trim()).join(" ");
  return { title: m[1]!.trim(), body, firstParagraph };
}

// Demote every heading in a doc body by `by` levels, skipping fenced code.
function demoteHeadings(body: string, by: number): string {
  let fence = false;
  return body
    .split("\n")
    .map((line) => {
      if (line.startsWith("```")) fence = !fence;
      return !fence && /^#{1,4} /.test(line) ? "#".repeat(by) + line : line;
    })
    .join("\n");
}

function apiLine(module: Module): string {
  let names = module.names.map((n) => `\`${n}\``).join(", ");
  let file = srcFile(module.stem);
  return `API: ${names} - typed and commented in [${file}](./${file}).`;
}

// -- README.md ---------------------------------------------------------------

function section(module: Module, level: number): string {
  let doc = docOf(module.stem);
  return (
    `${"#".repeat(level)} ${doc.title}\n\n` +
    demoteHeadings(doc.body, level - 1) +
    `\n\n${apiLine(module)}\n`
  );
}

let readme =
  `<!-- GENERATED FILE, do not edit: edit docs/*.md and the interfaces in src/, then run \`${RUN}\`. -->\n\n` +
  readFileSync(join(DOCS, "index.md"), "utf8").trim() +
  "\n\n" +
  CONCEPTS.map((stem) => section(modules.find((m) => m.stem === stem)!, 2)).join("\n") +
  "\n## Components\n\n" +
  modules
    .filter((m) => !CONCEPTS.includes(m.stem))
    .map((m) => section(m, 3))
    .join("\n") +
  "\n## License\n\nMIT. Copyright (c) 2026 Antoine van Wel.\n";

// -- AGENTS.md: the block between the markers --------------------------------

const BEGIN = `<!-- BEGIN GENERATED: exports (${RUN}) -->`;
const END = "<!-- END GENERATED: exports -->";

let agentsPath = join(PKG, "AGENTS.md");
let agents = readFileSync(agentsPath, "utf8");
let begin = agents.indexOf(BEGIN);
let end = agents.indexOf(END);
if (begin < 0 || end < 0 || end < begin) {
  console.error(`AGENTS.md is missing the "${BEGIN}" / "${END}" markers`);
  process.exit(1);
}

let bullets = modules.map((m) => {
  let doc = docOf(m.stem);
  return `- \`${doc.title}\` - ${doc.firstParagraph}`;
});
agents = agents.slice(0, begin + BEGIN.length) + "\n" + bullets.join("\n") + "\n" + agents.slice(end);

if (failed) process.exit(1);

// -- Write or check ----------------------------------------------------------

function emit(path: string, content: string): void {
  let current = existsSync(path) ? readFileSync(path, "utf8") : "";
  if (current === content) return;
  if (check) fail(`${path} is stale: run \`${RUN}\``);
  else {
    writeFileSync(path, content);
    console.log(`Wrote ${path}`);
  }
}

emit(join(PKG, "README.md"), readme);
emit(agentsPath, agents);
if (failed) process.exit(1);
if (check) console.log("Components docs are up to date");
