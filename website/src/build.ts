// Static site build: renders ../docs into dist/site/ (see okf/plans/website.md),
// or with `--dev [port]` serves it from memory and rebuilds on every change.
//
// docs/ is markdown only and its tree is the site: a directory is a section
// (and a sidebar group), a file is a page, and a name starting with "_" is not
// published. Siblings are ordered by their directory's index.md: an `order:`
// line in its frontmatter naming them, else the order it first links them,
// else by name. The top nav is the top-level directories, in that same order.
// Everything else the site needs (css, icon) lives in assets/ and is copied
// byte-for-byte. Generated pages (the References) join the same page list, so
// the in-section sidebar lists hand-written and generated pages alike.
// Only the landing page and the PUBLISHED sections are written; the rest of
// the tree is still read and checked, and links into it lose their link.

import { file, dir } from "flux:fs";
import { serve } from "flux:http";
import { argv, exit } from "flux:process";
import {
  configureMarked,
  markdownToHtml,
  renderPage,
  buildNav,
  buildSidebar,
  type PageEntry,
  type Rules,
} from "./markdown.ts";
import { clearSourceCache, resolveDirectives, undocumentedCommands, unpulled } from "./directives.ts";
import { referencePages } from "./reference.ts";
import { tokensCss } from "./tokens.ts";

// Source whose whole exported surface belongs on the site, checked after the
// build: pages name what they show, so nothing lists a new type by itself.
const COVERED = "packages/core/src/types.d.ts";

const DOCS_DIR = "../docs";
// Sections whose pages live with the package they document, mounted into
// the tree at the given section path: a docs/ folder shipped as is, or a
// composed section whose index is the package README and whose pages are
// the `docs.md` of each folder under `pages` (one per sol command), with
// their relative `<name>/docs.md` links rewritten to section URLs.
type Mount = { dir: string } | { index: string; pages: string; label: string };
const MOUNTS: Record<string, Mount> = {
  "/core": { dir: "../packages/core/docs" },
  "/tools": { index: "../packages/cli/README.md", pages: "../packages/cli/src", label: "Tools" },
};
// Top-level sections put on the site, beside the landing page. The others are
// not published yet.
const PUBLISHED = ["guides"];
const ASSETS_DIR = "assets";
const OUT_DIR = "dist/site";
const TEMPLATE = "template.json";
const SITE_NAME = "SolidRT";

// The page's own title: the first h1 of the markdown, or of a raw HTML block
// in it (the landing page is written as HTML).
function h1Of(source: string): string | undefined {
  return (source.match(/^#\s+(.+)$/m) ?? source.match(/<h1[^>]*>(.*?)<\/h1>/))?.[1]?.trim();
}

function pageTitle(own: string | undefined): string {
  return own && own !== SITE_NAME ? `${own} - ${SITE_NAME}` : SITE_NAME;
}

// Optional leading "---" frontmatter, as flat key/value lines: `nav` is the
// label this page takes in the nav and sidebar when its h1 is not the right
// one there, and `order` (index pages only) names the sibling pages in order.
type Front = { nav?: string; order?: string };
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
// <dir>/index.md for clean URLs, so the directory is the path.
function urlOf(rel: string): string {
  return rel.replace(/\/index\.md$/, "").replace(/\.md$/, "");
}

// The order a directory's index.md gives its children, by URL segment: the
// frontmatter `order` list, else the order of its first links to them.
async function orderOf(rel: string): Promise<string[]> {
  let source = await file(sourceFile(rel + "/index.md")).text().catch(() => "");
  let { front, body } = frontmatter(rewriteLinks(rel + "/index.md", source));
  if (front.order) return front.order.split(/\s+/);
  let here = rel + "/";
  let names: string[] = [];
  for (let match of body.matchAll(/(?:\]\(|href=")(\/[^)"#?]*)/g)) {
    let url = match[1]!.replace(/\/$/, "");
    if (!url.startsWith(here) || url.includes("/", here.length)) continue;
    let name = url.slice(here.length);
    if (!names.includes(name)) names.push(name);
  }
  return names;
}

// The mount a tree-relative path (with leading "/") falls under, if any.
function mountOf(rel: string): [string, Mount] | undefined {
  return Object.entries(MOUNTS).find(([section]) => rel === section || rel.startsWith(section + "/"));
}

// The file a page path is read from: a mounted section resolves into its
// package (a composed one: the README for its index, `<name>/docs.md` under
// `pages` for `<name>.md`), everything else into docs/.
function sourceFile(rel: string): string {
  let mount = mountOf(rel);
  if (!mount) return DOCS_DIR + rel;
  let [section, m] = mount;
  let inner = rel.slice(section.length);
  if ("dir" in m) return m.dir + inner;
  return inner === "/index.md" ? m.index : m.pages + inner.replace(/\.md$/, "/docs.md");
}

// A directory's entries as the tree sees them: a composed section lists its
// index and one page per folder under `pages` that carries a docs.md.
async function entriesOf(rel: string): Promise<{ name: string; type: string }[]> {
  let mount = mountOf(rel);
  if (!mount) return dir(DOCS_DIR + rel).entries();
  let [section, m] = mount;
  if ("dir" in m) return dir(m.dir + rel.slice(section.length)).entries();
  if (rel !== section) return [];
  let entries = [{ name: "index.md", type: "file" }];
  for (let e of await dir(m.pages).entries()) {
    if (e.type === "directory" && (await file(`${m.pages}/${e.name}/docs.md`).exists())) {
      entries.push({ name: e.name + ".md", type: "file" });
    }
  }
  return entries;
}

// In a composed section the sources link each other as files
// (`src/<name>/docs.md` from the README, `../<name>/docs.md` between pages)
// so they read on GitHub and npm too; on the site those are the pages.
function rewriteLinks(rel: string, body: string): string {
  let mount = mountOf(rel);
  if (!mount || "dir" in mount[1]) return body;
  return body.replace(/\]\((?:\.\.?\/)?(?:src\/)?([\w-]+)\/docs\.md\)/g, `](${mount[0]}/$1/)`);
}

// Relative file paths (with leading "/") of the page tree, recursively, each
// directory's children in its index.md order (see orderOf) and the rest by
// name. The mounted sections join the top level like any directory.
async function walk(rel: string): Promise<string[]> {
  let files: string[] = [];
  let entries = await entriesOf(rel);
  let names = entries.filter((e) => e.type === "directory" || e.name.endsWith(".md")).map((e) => e.name);
  if (rel === "") names.push(...Object.keys(MOUNTS).map((s) => s.slice(1)));
  let order = await orderOf(rel);
  let rank = (name: string) => {
    let i = order.indexOf(name.replace(/\.md$/, ""));
    return i < 0 ? order.length : i;
  };
  names.sort((a, b) => rank(a) - rank(b) || (a < b ? -1 : a > b ? 1 : 0));
  for (let name of names) {
    if (name.startsWith("_") || name.startsWith(".")) continue;
    let path = rel + "/" + name;
    let isDir = Object.keys(MOUNTS).includes(path) || entries.some((e) => e.name === name && e.type === "directory");
    if (isDir) files.push(...(await walk(path)));
    else files.push(path);
  }
  return files;
}

// Whether a page URL path is on the site: the landing page, or in a PUBLISHED
// section.
function published(url: string): boolean {
  return url === "" || PUBLISHED.includes(url.split("/")[1]!);
}

// A link into an unpublished page keeps its text and loses the link: the
// sources link the whole tree, the site is only part of it.
function unlinkUnpublished(html: string, unpublished: Set<string>): string {
  return html.replace(/<a\b[^>]*?\bhref="(\/[^"#?]*)[^"]*"[^>]*>([\s\S]*?)<\/a>/g, (link, path: string, text: string) =>
    unpublished.has(path.replace(/\/$/, "")) ? text : link,
  );
}

// The site: each output file's URL path ("/guides/index.html") and contents.
type Site = Map<string, string | Uint8Array>;

// Add a directory tree to the site byte-for-byte, at its path below assets/.
async function readTree(rel: string, site: Site): Promise<void> {
  for (let entry of await dir(rel).entries()) {
    let path = rel + "/" + entry.name;
    if (entry.type === "directory") await readTree(path, site);
    else if (entry.type === "file") site.set(path.slice(ASSETS_DIR.length), await file(path).bytes());
  }
}

// Render the whole site from its sources, reading them all afresh.
async function render(): Promise<Site> {
  clearSourceCache();
  let rules: Rules = await file(TEMPLATE).json();
  configureMarked(rules);
  let site: Site = new Map();

  // Pass 1: collect every page (body, url, label) so each render knows its section.
  type Source = { url: string; body: string; own?: string; label: string };
  let sources: Source[] = [];
  let authored: string[] = [];
  for (let rel of await walk("")) {
    let { front, body } = frontmatter(rewriteLinks(rel, await file(sourceFile(rel)).text()));
    let url = urlOf(rel);
    let own = h1Of(body);
    authored.push(body);
    // A composed section's index is a package README; the section label is
    // the mount's, not the README's h1.
    let mount = mountOf(rel);
    let sectionLabel = mount && !("dir" in mount[1]) && rel === mount[0] + "/index.md" ? mount[1].label : undefined;
    sources.push({
      url,
      body: await resolveDirectives(body, "docs" + rel),
      own,
      label: front.nav ?? sectionLabel ?? own ?? url.slice(url.lastIndexOf("/") + 1),
    });
  }
  let allGenerated = await referencePages();
  let unpublished = new Set(
    [...sources.map((s) => s.url), ...allGenerated.map((g) => g.path)].filter((url) => !published(url)),
  );
  let shown = sources.filter((s) => published(s.url));
  let generated = allGenerated.filter((g) => published(g.path));

  let pages: PageEntry[] = [
    ...shown.map((s) => ({ path: s.url, title: s.label })),
    ...generated.map((g) => ({ path: g.path, title: g.title })),
  ];
  // The nav is the section index pages: one path segment, in document order.
  let nav = pages.filter((p) => p.path.length > 1 && !p.path.includes("/", 1));

  // Pass 2: render.
  for (let s of shown) {
    let page = { title: pageTitle(s.own), nav: buildNav(s.url, nav, rules), sidebar: buildSidebar(s.url, pages, rules) };
    site.set(s.url + "/index.html", unlinkUnpublished(await markdownToHtml(s.body, rules, page), unpublished));
  }
  for (let g of generated) {
    let page = { title: pageTitle(g.title), nav: buildNav(g.path, nav, rules), sidebar: buildSidebar(g.path, pages, rules) };
    site.set(g.path + "/index.html", unlinkUnpublished(await renderPage(g.html, rules, page), unpublished));
  }

  let before = site.size;
  await readTree(ASSETS_DIR, site);
  let assets = site.size - before;
  site.set("/css/tokens.css", tokensCss());

  console.log(
    `Built ${shown.length} pages, generated ${generated.length}, with ${assets} assets and css/tokens.css` +
      ` (${unpublished.size} pages not published)`,
  );

  let missing = await unpulled(COVERED, authored);
  if (missing.length > 0) console.log(`Not shown anywhere, from ${COVERED}: ${missing.join(", ")}`);
  let missingCommands = await undocumentedCommands(authored);
  if (missingCommands.length > 0) console.log(`Not shown anywhere, sol commands: ${missingCommands.join(", ")}`);
  return site;
}

// Write the site into OUT_DIR, which the Makefile has emptied first.
async function write(site: Site): Promise<void> {
  for (let [path, contents] of site) {
    await dir(OUT_DIR + path.slice(0, path.lastIndexOf("/"))).create();
    await file(OUT_DIR + path).write(contents);
  }
  console.log(`Wrote ${site.size} files into ${OUT_DIR}/`);
}

// -- Dev server ----------------------------------------------------------------

// Exit status asking `make dev` to re-bundle this script and start it again: a
// change to the generator itself cannot be picked up in-process. The Makefile's
// RESTART_EXIT is the same number.
const RESTART_EXIT = 75;
// How long the sources stay quiet before a rebuild starts: one save arrives as
// several watch events.
const REBUILD_DEBOUNCE_MS = 100;
// How long the reload script waits before reconnecting to a restarting server.
const RELOAD_RETRY_MS = 500;
const RELOAD_PATH = "/__reload";
const RELOAD_TOPIC = "reload";
// Repo sources the pages read (docs, READMEs, declarations, package.json),
// beside the docs tree itself.
const PACKAGES_DIR = "../packages";
const PACKAGE_SOURCE = /\.(md|tsx?|json)$/;
const NOT_SOURCE = /[\\/](node_modules|dist)[\\/]/;

const CONTENT_TYPES: Record<string, string> = {
  html: "text/html; charset=utf-8",
  css: "text/css; charset=utf-8",
  js: "text/javascript; charset=utf-8",
  json: "application/json",
  svg: "image/svg+xml",
  png: "image/png",
  ico: "image/x-icon",
  woff2: "font/woff2",
};

// Added to every served page: reload on a rebuild, and after the server comes
// back from a restart.
const RELOAD_SCRIPT = `<script>
(function connect(reconnecting) {
  let socket = new WebSocket("ws://" + location.host + "${RELOAD_PATH}");
  socket.onopen = () => reconnecting && location.reload();
  socket.onmessage = () => location.reload();
  socket.onclose = () => setTimeout(() => connect(true), ${RELOAD_RETRY_MS});
})(false);
</script>`;

// Serve the site from memory on `port` (0: any free one), rebuilding it when
// a source changes and reloading the open pages. A failed rebuild keeps the
// last good site.
async function dev(port: number): Promise<void> {
  let site: Site = new Map();
  let server = serve({
    port,
    host: "127.0.0.1",
    fetch(req, server) {
      // The path of the absolute request URL, without query or fragment.
      let path = decodeURIComponent(req.url.replace(/^\w+:\/\/[^/]*/, "").replace(/[?#].*$/, ""));
      if (path === RELOAD_PATH) {
        if (server.upgrade(req)) return;
        return new Response("Expected a websocket", { status: 400 });
      }
      if (path.endsWith("/")) path += "index.html";
      let contents = site.get(path);
      if (contents === undefined) {
        if (site.has(path + "/index.html")) return new Response(null, { status: 308, headers: { location: path + "/" } });
        return new Response("Not found", { status: 404 });
      }
      let ext = path.slice(path.lastIndexOf(".") + 1);
      if (ext === "html") contents = (contents as string).replace("</body>", RELOAD_SCRIPT + "</body>");
      return new Response(contents, { headers: { "content-type": CONTENT_TYPES[ext] ?? "application/octet-stream" } });
    },
    websocket: {
      open(ws) {
        ws.subscribe(RELOAD_TOPIC);
      },
    },
  });

  let building = false;
  let stale = false;
  let restart = false;
  async function rebuild(): Promise<void> {
    if (building) {
      stale = true;
      return;
    }
    building = true;
    do {
      if (restart) exit(RESTART_EXIT);
      stale = false;
      try {
        site = await render();
        server.publish(RELOAD_TOPIC, "reload");
      } catch (error) {
        console.error("Build failed, still serving the last good site:", error);
      }
    } while (stale || restart);
    building = false;
  }

  let pending: ReturnType<typeof setTimeout> | undefined;
  function changed(generator = false): void {
    restart ||= generator;
    clearTimeout(pending);
    pending = setTimeout(rebuild, REBUILD_DEBOUNCE_MS);
  }
  dir("src").watch((e) => /\.tsx?$/.test(e.path) && changed(true), { recursive: true });
  dir(".").watch((e) => /[\\/]template\.[^\\/]+$/.test(e.path) && changed());
  dir(ASSETS_DIR).watch(() => changed(), { recursive: true });
  dir(DOCS_DIR).watch((e) => e.path.endsWith(".md") && changed(), { recursive: true });
  dir(PACKAGES_DIR).watch((e) => PACKAGE_SOURCE.test(e.path) && !NOT_SOURCE.test(e.path) && changed(), {
    recursive: true,
  });

  await rebuild();
  console.log(`Serving the site at http://127.0.0.1:${server.port}/, rebuilding on change`);
}

if (argv[0] === "--dev") await dev(Number(argv[1] ?? 0));
else await write(await render());
