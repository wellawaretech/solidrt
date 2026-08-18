// Static site build: renders ../docs into dist/ (see okf/plans/website.md).
//
// docs/ is markdown only and its tree is the site: a directory is a section
// (and a sidebar group), a file is a page, an "NN-" name prefix orders it and
// is stripped from the URL, and a name starting with "_" is not published.
// The top nav is the top-level directories, in that same order. Everything
// else the site needs (css, icon) lives in assets/ and is copied byte-for-byte.
// Generated pages (the References) join the same page list, so the in-section
// sidebar lists hand-written and generated pages alike.

import { file, dir } from "flux:fs";
import {
  configureMarked,
  markdownToHtml,
  renderPage,
  buildNav,
  buildSidebar,
  type PageEntry,
  type Rules,
} from "./markdown.ts";
import { resolveDirectives, undocumentedCommands, unpulled } from "./directives.ts";
import { referencePages } from "./reference.ts";
import { tokensCss } from "./tokens.ts";

// Source whose whole exported surface belongs on the site, checked after the
// build: pages name what they show, so nothing lists a new type by itself.
const COVERED = "packages/core/src/types.d.ts";

const DOCS_DIR = "../docs";
const ASSETS_DIR = "assets";
const OUT_DIR = "dist";
const TEMPLATE = "template.json";
const SITE_NAME = "SolidRT";

let rules: Rules = await file(TEMPLATE).json();
configureMarked(rules);

// The page's own title: the first h1 of the markdown, or of a raw HTML block
// in it (the landing page is written as HTML).
function h1Of(source: string): string | undefined {
  return (source.match(/^#\s+(.+)$/m) ?? source.match(/<h1[^>]*>(.*?)<\/h1>/))?.[1]?.trim();
}

function pageTitle(own: string | undefined): string {
  return own && own !== SITE_NAME ? `${own} - ${SITE_NAME}` : SITE_NAME;
}

// Optional leading "---" frontmatter, as flat key/value lines. Only `nav` is
// read today: the label this page takes in the nav and sidebar when its h1 is
// not the right one there.
type Front = { nav?: string };
function frontmatter(source: string): { front: Front; body: string } {
  let end = source.startsWith("---\n") ? source.indexOf("\n---\n", 3) : -1;
  if (end < 0) return { front: {}, body: source };
  let front: Front = {};
  for (let line of source.slice(4, end).split("\n")) {
    let match = line.match(/^(\w+):\s*(.*)$/);
    if (match) front[match[1] as keyof Front] = match[2]!.trim();
  }
  return { front, body: source.slice(end + 5) };
}

// URL path (directory, no trailing slash) of a docs file: pages are
// <dir>/index.md for clean URLs, so the directory is the path, and every
// segment drops its ordering prefix.
function urlOf(rel: string): string {
  return rel
    .replace(/\/index\.md$/, "")
    .replace(/\.md$/, "")
    .replace(/\/\d+-/g, "/");
}

// Relative file paths (with leading "/") under docs/, recursively and in name
// order, which the "NN-" prefixes make the authored order.
async function walk(rel: string): Promise<string[]> {
  let files: string[] = [];
  let entries = await dir(DOCS_DIR + rel).entries();
  entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (let entry of entries) {
    if (entry.name.startsWith("_") || entry.name.startsWith(".")) continue;
    let path = rel + "/" + entry.name;
    if (entry.type === "directory") files.push(...(await walk(path)));
    else if (entry.name.endsWith(".md")) files.push(path);
  }
  return files;
}

// Copy a directory tree byte-for-byte into dist/.
async function copyTree(rel: string): Promise<number> {
  await dir(OUT_DIR + rel.slice(ASSETS_DIR.length)).create();
  let copied = 0;
  for (let entry of await dir(rel).entries()) {
    let path = rel + "/" + entry.name;
    if (entry.type === "directory") copied += await copyTree(path);
    else if (entry.type === "file") {
      await file(OUT_DIR + path.slice(ASSETS_DIR.length)).write(await file(path).bytes());
      copied++;
    }
  }
  return copied;
}

// Pass 1: collect every page (body, url, label) so each render knows its section.
type Source = { url: string; body: string; own?: string; label: string };
let sources: Source[] = [];
let authored: string[] = [];
for (let rel of await walk("")) {
  let { front, body } = frontmatter(await file(DOCS_DIR + rel).text());
  let url = urlOf(rel);
  let own = h1Of(body);
  authored.push(body);
  sources.push({
    url,
    body: await resolveDirectives(body, "docs" + rel),
    own,
    label: front.nav ?? own ?? url.slice(url.lastIndexOf("/") + 1),
  });
}
let generated = await referencePages();

let pages: PageEntry[] = [
  ...sources.map((s) => ({ path: s.url, title: s.label })),
  ...generated.map((g) => ({ path: g.path, title: g.title })),
];
// The nav is the section index pages: one path segment, in document order.
let nav = pages.filter((p) => p.path.length > 1 && !p.path.includes("/", 1));

// Pass 2: render.
async function writePage(url: string, html: string): Promise<void> {
  await dir(OUT_DIR + url).create();
  await file(OUT_DIR + url + "/index.html").write(html);
}
for (let s of sources) {
  let page = { title: pageTitle(s.own), nav: buildNav(nav, rules), sidebar: buildSidebar(s.url, pages, rules) };
  await writePage(s.url, await markdownToHtml(s.body, rules, page));
}
for (let g of generated) {
  let page = { title: pageTitle(g.title), nav: buildNav(nav, rules), sidebar: buildSidebar(g.path, pages, rules) };
  await writePage(g.path, await renderPage(g.html, rules, page));
}

let assets = await copyTree(ASSETS_DIR);
await file(OUT_DIR + "/css/tokens.css").write(tokensCss());

console.log(
  `Built ${sources.length} pages, generated ${generated.length}, copied ${assets} assets and wrote css/tokens.css into ${OUT_DIR}/`,
);

let missing = await unpulled(COVERED, authored);
if (missing.length > 0) console.log(`Not shown anywhere, from ${COVERED}: ${missing.join(", ")}`);
let missingCommands = await undocumentedCommands(authored);
if (missingCommands.length > 0) console.log(`Not shown anywhere, srt commands: ${missingCommands.join(", ")}`);
