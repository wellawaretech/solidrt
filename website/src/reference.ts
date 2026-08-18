// Generated Reference pages. Bodies are the declaration sources themselves,
// highlighted: flux-types and core's types.d.ts are hand-written with JSDoc on
// every member, so they are the documentation. A proper type extractor can
// replace the bodies later without moving the URLs.
//
//   /runtime/reference/<name>/   one page per flux-types declaration file
//   /core/reference/<element>/   one page per JSX intrinsic element, showing
//                                every prop interface it composes
//   /core/reference/types/       the shared aliases and event types
//   /tools/reference/<command>/  one page per srt command, from the CLI's
//                                own usage text
//   /extensions/components/...   the components README: its head, then one
//                                page per "### Widget" section
//   /extensions/3d/...           the 3d README, plus its export surface
import { file } from "flux:fs";
import { escapeHtml, highlight, renderMarkdown } from "./markdown.ts";

const FLUX_TYPES = "../packages/flux-types";
const CORE = "../packages/core";
const CLI_ARGS = "../packages/cli/src/args.ts";
const COMPONENTS = "../packages/components";
const THREE_D = "../packages/3d";

export type ReferencePage = {
  // URL path of the page directory, e.g. /runtime/reference/fs
  path: string;
  title: string;
  html: string;
};

function code(source: string): string {
  return `<pre><code class="hljs language-typescript">${highlight(source, "typescript")}</code></pre>\n`;
}

function declaredIn(rel: string): string {
  return `<p><small>Declared in <code>${escapeHtml(rel)}</code>.</small></p>\n`;
}

// -- Runtime: flux-types ---------------------------------------------------

// The leading line-comment block of a file, joined into one paragraph.
function intro(source: string): string {
  let lines: string[] = [];
  for (let line of source.split("\n")) {
    if (line.startsWith("//")) lines.push(line.replace(/^\/\/ ?/, ""));
    else break;
  }
  return lines.join(" ").trim();
}

async function runtimePages(): Promise<ReferencePage[]> {
  const BASE = "/runtime/reference";
  let index = await file(FLUX_TYPES + "/index.d.ts").text();
  let refs = [...index.matchAll(/\/\/\/ <reference path="\.\/([^"]+)\.d\.ts" \/>/g)].map((m) => m[1]!);
  let pages: ReferencePage[] = [];
  for (let rel of refs) {
    let stem = rel.slice(rel.lastIndexOf("/") + 1);
    let source = await file(`${FLUX_TYPES}/${rel}.d.ts`).text();
    let title = source.match(/^declare module "([^"]+)"/m)?.[1] ?? stem;
    let lead = intro(source);
    let html =
      `<h1>${escapeHtml(title)}</h1>\n` +
      (lead ? `<p>${escapeHtml(lead)}</p>\n` : "") +
      declaredIn(`packages/flux-types/${rel}.d.ts`) +
      code(source);
    pages.push({ path: `${BASE}/${stem}`, title, html });
  }
  let list = pages.map((p) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`).join("\n");
  pages.unshift({
    path: BASE,
    title: "Reference",
    html:
      "<h1>Reference</h1>\n" +
      "<p>The typed surface of the runtime: every <code>flux:*</code> module, the web-standard globals, and the GUI capabilities, one page per declaration file.</p>\n" +
      `<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

// -- Core: JSX elements from types.d.ts ------------------------------------

type Declaration = { name: string; extends: string[]; source: string };

// Top-level declarations of a TypeScript source, each with the comment block
// directly above it (no blank line between). `interface` / `type` /
// `class` run to the line that closes their top-level brace (or are a single
// line); `function` / `const` / `let` contribute their signature only, cut at
// the body's opening brace.
function splitDeclarations(source: string): Map<string, Declaration> {
  let out = new Map<string, Declaration>();
  let lines = source.split("\n");
  let i = 0;
  while (i < lines.length) {
    let head = lines[i]!.match(
      /^(?:export )?(?:declare )?(?:async )?(interface|type|class|function|const|let) ([A-Za-z_][A-Za-z0-9_]*)\b(?: extends ([^{]+))?/,
    );
    if (!head) {
      i++;
      continue;
    }
    let kind = head[1]!;
    let start = i;
    while (start > 0 && /^(\/\/|\/\*| \*)/.test(lines[start - 1]!)) start--;
    let end = i;
    let body: string;
    if (kind === "function" || kind === "const" || kind === "let") {
      // Signature: lines continue while they end in "(" or "," (an open
      // parameter list); the line that ends in the body's or value's opening
      // brace, or in nothing special, closes it. The opener is cut off.
      while (end < lines.length - 1 && /[(,]\s*$/.test(lines[end]!)) end++;
      body = lines.slice(i, end + 1).join("\n").replace(/\s*(=\s*)?[{[(]\s*$/, "");
    } else {
      let depth = 0;
      for (; end < lines.length; end++) {
        let line = lines[end]!;
        for (let ch of line) {
          if (ch === "{") depth++;
          else if (ch === "}") depth--;
        }
        if (depth <= 0 && (line.includes("}") || !line.includes("{"))) break;
      }
      body = lines.slice(i, end + 1).join("\n");
    }
    let comment = start < i ? lines.slice(start, i).join("\n") + "\n" : "";
    out.set(head[2]!, {
      name: head[2]!,
      extends: head[3] ? head[3].split(",").map((s) => s.trim()).filter(Boolean) : [],
      source: comment + body,
    });
    i = end + 1;
  }
  return out;
}

// The interfaces an element composes, in reading order: each named interface
// followed by what it extends, depth-first, deduplicated.
function compose(names: string[], decls: Map<string, Declaration>, seen = new Set<string>()): Declaration[] {
  let out: Declaration[] = [];
  for (let name of names) {
    let d = decls.get(name);
    if (!d || seen.has(name)) continue;
    seen.add(name);
    out.push(d, ...compose(d.extends, decls, seen));
  }
  return out;
}

async function corePages(): Promise<ReferencePage[]> {
  const BASE = "/core/reference";
  let typesRel = "packages/core/src/types.d.ts";
  let decls = splitDeclarations(await file(`${CORE}/src/types.d.ts`).text());
  let jsx = await file(`${CORE}/jsx-runtime.d.ts`).text();
  let elements = [...jsx.matchAll(/^\s+"?([a-z-]+)"?: ([A-Za-z &]+)$/gm)].map((m) => ({
    name: m[1]!,
    composition: m[2]!,
    props: m[2]!.split("&").map((s) => s.trim()).filter((s) => s !== "ElementRef"),
  }));
  let used = new Set<string>();
  let pages: ReferencePage[] = [];
  for (let el of elements) {
    let parts = compose(el.props, decls);
    for (let d of parts) used.add(d.name);
    let html =
      `<h1>&lt;${escapeHtml(el.name)}&gt;</h1>\n` +
      `<p>Props: <code>${escapeHtml(el.composition)}</code>. <code>ElementRef</code> is the <code>ref</code> callback every element accepts.</p>\n` +
      declaredIn(typesRel) +
      parts.map((d) => `<h2 id="${d.name}">${d.name}</h2>\n` + code(d.source)).join("");
    pages.push({ path: `${BASE}/${el.name}`, title: `<${el.name}>`, html });
  }
  // JSX plumbing (ElementChildrenAttribute, Children) is not a reader-facing type.
  const PLUMBING = new Set(["ElementChildrenAttribute", "Children"]);
  let rest = [...decls.values()].filter((d) => !used.has(d.name) && !PLUMBING.has(d.name));
  pages.push({
    path: `${BASE}/types`,
    title: "Types",
    html:
      "<h1>Types</h1>\n" +
      "<p>The shared aliases and event types the element props refer to.</p>\n" +
      declaredIn(typesRel) +
      rest.map((d) => `<h2 id="${d.name}">${d.name}</h2>\n` + code(d.source)).join(""),
  });
  let list = pages.map((p) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`).join("\n");
  pages.unshift({
    path: BASE,
    title: "Reference",
    html:
      "<h1>Reference</h1>\n" +
      "<p>The JSX element vocabulary: one page per intrinsic element with every prop interface it composes, and the shared types.</p>\n" +
      `<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

// -- Tools: the srt CLI ----------------------------------------------------

// The CLI keeps its whole usage text in printUsage() (args.ts): a "Commands:"
// table, then "<name>[/<name>] options:" blocks. That text is the source here:
// the index page carries the table, each command page its table row and every
// option block whose heading names it.
async function toolsPages(): Promise<ReferencePage[]> {
  const BASE = "/tools/reference";
  let args = await file(CLI_ARGS).text();
  let usage = args.match(/console\.error\(`Usage: srt[^`]*`\)/)?.[0].slice("console.error(`".length, -2) ?? "";
  let [, commandsText = "", optionsText = ""] = usage.match(/Commands:\n([\s\S]*?)\n\n([\s\S]*)/) ?? [];
  let commands = commandsText
    .split("\n")
    .map((line) => line.match(/^\s+(\S+)(.*?)\s{2,}(.+)$/))
    .filter((m): m is RegExpMatchArray => m !== null)
    .map((m) => ({ name: m[1]!, args: m[2]!.trim(), summary: m[3]!.trim(), row: m[0]!.trim() }));
  let blocks = optionsText
    .split("\n\n")
    .map((block) => block.match(/^(\S+) options:\n([\s\S]*)$/))
    .filter((m): m is RegExpMatchArray => m !== null)
    .map((m) => ({ commands: m[1]!.split("/"), heading: `${m[1]} options`, text: m[2]! }));
  let pre = (text: string) => `<pre><code>${escapeHtml(text)}</code></pre>\n`;
  let pages: ReferencePage[] = commands.map((c) => ({
    path: `${BASE}/${c.name}`,
    title: `srt ${c.name}`,
    html:
      `<h1>srt ${escapeHtml(c.name)}</h1>\n` +
      `<p>${escapeHtml(c.summary)}.</p>\n` +
      pre(`srt ${c.name}${c.args ? " " + c.args : ""}`) +
      blocks
        .filter((b) => b.commands.includes(c.name))
        .map((b) => `<h2>${escapeHtml(b.heading)}</h2>\n` + pre(b.text))
        .join(""),
  }));
  let list = commands
    .map((c) => `<li><a href="${BASE}/${c.name}/"><code>srt ${escapeHtml(c.name)}</code></a> ${escapeHtml(c.summary)}</li>`)
    .join("\n");
  pages.unshift({
    path: BASE,
    title: "Reference",
    html:
      "<h1>Reference</h1>\n" +
      "<p>Every <code>srt</code> command and its options, from the CLI's own usage text (<code>srt</code> with no arguments prints the same).</p>\n" +
      `<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

// -- Extensions: the package READMEs ---------------------------------------

// The README's h1 text, and the README with that h1 stripped so a page's
// intro follows the generated heading.
function slug(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

async function componentsPages(): Promise<ReferencePage[]> {
  const BASE = "/extensions/components";
  let readme = await file(COMPONENTS + "/README.md").text();
  // Head: everything before "## Components"; widgets: each "### X" under it,
  // up to the next h2 (License). Section headings become page h1s.
  let at = readme.indexOf("\n## Components\n");
  let head = at < 0 ? readme : readme.slice(0, at);
  let rest = at < 0 ? "" : readme.slice(at + "\n## Components\n".length);
  let nextH2 = rest.search(/^## /m);
  let widgets = (nextH2 < 0 ? rest : rest.slice(0, nextH2)).split(/^### /m).slice(1);
  let pages: ReferencePage[] = [];
  for (let section of widgets) {
    let nl = section.indexOf("\n");
    let title = section.slice(0, nl).trim();
    let body = section.slice(nl + 1).replace(/^####? /gm, (m) => "#".repeat(m.length - 2) + " ");
    pages.push({
      path: `${BASE}/${slug(title)}`,
      title,
      html: `<h1>${escapeHtml(title)}</h1>\n` + (await renderMarkdown(body)),
    });
  }
  let list = pages.map((p) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`).join("\n");
  pages.unshift({
    path: BASE,
    title: "@solidrt/components",
    html: (await renderMarkdown(head)) + `<h2>Components</h2>\n<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

// One page per source module index.ts re-exports from, in index.ts order,
// showing only the exports index.ts names (the public surface): each with its
// JSDoc and signature (functions) or full declaration (types).
async function threeDPages(): Promise<ReferencePage[]> {
  const BASE = "/extensions/3d";
  let readme = await file(THREE_D + "/README.md").text();
  let index = await file(THREE_D + "/src/index.ts").text();
  let modules = new Map<string, string[]>();
  for (let m of index.matchAll(/^export (?:type )?\{([^}]*)\} from "\.\/([a-z]+)\.tsx?"/gm)) {
    let names = m[1]!.split(",").map((n) => n.trim().replace(/^type /, "").replace(/ as .*$/, "")).filter(Boolean);
    let file = m[2]!;
    modules.set(file, [...(modules.get(file) ?? []), ...names]);
  }
  let pages: ReferencePage[] = [];
  for (let [stem, names] of modules) {
    let rel = `packages/3d/src/${stem}.${stem === "components" ? "tsx" : "ts"}`;
    let source = await file(`${THREE_D}/src/${stem}.${stem === "components" ? "tsx" : "ts"}`).text();
    let decls = splitDeclarations(source);
    let lead = intro(source);
    let blocks = names
      .map((n) => decls.get(n))
      .filter((d): d is Declaration => d !== undefined)
      .map((d) => `<h2 id="${d.name}">${escapeHtml(d.name)}</h2>\n` + code(d.source))
      .join("");
    pages.push({
      path: `${BASE}/${stem}`,
      title: stem,
      html: `<h1>${escapeHtml(stem)}</h1>\n` + (lead ? `<p>${escapeHtml(lead)}</p>\n` : "") + declaredIn(rel) + blocks,
    });
  }
  let list = pages.map((p) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`).join("\n");
  pages.unshift({
    path: BASE,
    title: "@solidrt/3d",
    html: (await renderMarkdown(readme)) + `<h2>API</h2>\n<p>By module, as <code>src/index.ts</code> exports it:</p>\n<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

export async function referencePages(): Promise<ReferencePage[]> {
  return [
    ...(await corePages()),
    ...(await componentsPages()),
    ...(await threeDPages()),
    ...(await runtimePages()),
    ...(await toolsPages()),
  ];
}