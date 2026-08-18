// Static site build: renders content/ into dist/ (see okf/plans/website.md).
//
// Rules, per file under content/:
//   *.md    rendered through the markdown converter + page template
//   *.html  treated as a pre-rendered fragment, wrapped in the page template
//   rest    copied byte-for-byte
// Generated pages (the Runtime Reference) join the same page list, so the
// in-section sidebar lists hand-written and generated pages alike.

import { file, dir } from "flux:fs";
import {
  configureMarked,
  markdownToHtml,
  renderPage,
  buildSidebar,
  type PageEntry,
  type Rules,
} from "./markdown.ts";
import { referencePages } from "./reference.ts";
import { tokensCss } from "./tokens.ts";

const CONTENT_DIR = "content";
const OUT_DIR = "dist";
const TEMPLATE = "template.json";
const SITE_NAME = "SolidRT";

let rules: Rules = await file(TEMPLATE).json();
configureMarked(rules);

// The page's own title: the first h1 of markdown or of a pre-rendered fragment.
function h1Of(source: string): string | undefined {
  return (source.match(/^#\s+(.+)$/m) ?? source.match(/<h1[^>]*>(.*?)<\/h1>/))?.[1]?.trim();
}

function pageTitle(own: string | undefined): string {
  return own && own !== SITE_NAME ? `${own} - ${SITE_NAME}` : SITE_NAME;
}

// URL path (directory, no trailing slash) of a content file: content pages are
// <dir>/index.* for clean URLs, so the directory is the path.
function urlOf(rel: string): string {
  return rel.replace(/\/index\.(md|html)$/, "").replace(/\.(md|html)$/, "");
}

// Relative file paths (with leading "/") under content/, recursively and in
// name order, mirroring each directory into dist/ along the way.
async function walk(rel: string): Promise<string[]> {
  await dir(OUT_DIR + rel).create();
  let files: string[] = [];
  let entries = await dir(CONTENT_DIR + rel).entries();
  entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (let entry of entries) {
    let path = rel + "/" + entry.name;
    if (entry.type === "directory") files.push(...(await walk(path)));
    else if (entry.type === "file") files.push(path);
  }
  return files;
}

// Pass 1: collect every page (source, url, title) so each render knows its section.
type Source = { rel: string; url: string; kind: "md" | "html"; text: string; own?: string };
let sources: Source[] = [];
let assets = 0;
for (let rel of await walk("")) {
  if (rel.endsWith(".md") || rel.endsWith(".html")) {
    let text = await file(CONTENT_DIR + rel).text();
    sources.push({ rel, url: urlOf(rel), kind: rel.endsWith(".md") ? "md" : "html", text, own: h1Of(text) });
  } else {
    await file(OUT_DIR + rel).write(await file(CONTENT_DIR + rel).bytes());
    assets++;
  }
}
let generated = await referencePages();

let pages: PageEntry[] = [
  ...sources.map((s) => ({ path: s.url, title: s.own ?? s.url.slice(s.url.lastIndexOf("/") + 1) })),
  ...generated.map((g) => ({ path: g.path, title: g.title })),
];

// Pass 2: render.
for (let s of sources) {
  let sidebar = buildSidebar(s.url, pages, rules);
  let html =
    s.kind === "md"
      ? await markdownToHtml(s.text, rules, pageTitle(s.own), sidebar)
      : await renderPage(s.text, rules, pageTitle(s.own), sidebar);
  await file(OUT_DIR + s.rel.replace(/\.md$/, ".html")).write(html);
}
for (let g of generated) {
  await dir(OUT_DIR + g.path).create();
  let sidebar = buildSidebar(g.path, pages, rules);
  await file(OUT_DIR + g.path + "/index.html").write(await renderPage(g.html, rules, pageTitle(g.title), sidebar));
}

await file(OUT_DIR + "/css/tokens.css").write(tokensCss());

console.log(
  `Built ${sources.length} pages, generated ${generated.length}, copied ${assets} assets and wrote css/tokens.css into ${OUT_DIR}/`,
);