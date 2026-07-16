// Static site build: renders content/ into dist/ (see okf/plans/website.md).
//
// Rules, per file under content/:
//   *.md    rendered through the markdown converter + page template
//   *.html  treated as a pre-rendered fragment, wrapped in the page template
//   rest    copied byte-for-byte

import { file, dir } from "flux:fs";
import { configureMarked, markdownToHtml, renderPage, type Rules } from "./markdown.ts";

const CONTENT_DIR = "content";
const OUT_DIR = "dist";
const TEMPLATE = "template.json";
const SITE_NAME = "SolidRT";

let rules: Rules = await file(TEMPLATE).json();
configureMarked(rules);

// Page title from the first markdown h1, suffixed with the site name.
function titleOf(md: string): string {
  let h1 = md.match(/^#\s+(.+)$/m)?.[1]?.trim();
  return h1 ? `${h1} - ${SITE_NAME}` : SITE_NAME;
}

// Relative file paths (with leading "/") under content/, recursively,
// mirroring each directory into dist/ along the way.
async function walk(rel: string): Promise<string[]> {
  await dir(OUT_DIR + rel).create();
  let files: string[] = [];
  for (let entry of await dir(CONTENT_DIR + rel).entries()) {
    let path = rel + "/" + entry.name;
    if (entry.type === "directory") files.push(...(await walk(path)));
    else if (entry.type === "file") files.push(path);
  }
  return files;
}

let pages = 0;
let assets = 0;
for (let path of await walk("")) {
  let source = file(CONTENT_DIR + path);
  if (path.endsWith(".md")) {
    let md = await source.text();
    let out = OUT_DIR + path.replace(/\.md$/, ".html");
    await file(out).write(await markdownToHtml(md, rules, titleOf(md)));
    pages++;
  } else if (path.endsWith(".html")) {
    let fragment = await source.text();
    await file(OUT_DIR + path).write(await renderPage(fragment, rules, SITE_NAME));
    pages++;
  } else {
    await file(OUT_DIR + path).write(await source.bytes());
    assets++;
  }
}

console.log(`Built ${pages} pages, copied ${assets} assets into ${OUT_DIR}/`);
