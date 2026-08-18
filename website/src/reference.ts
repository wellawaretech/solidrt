// Generated Reference pages, for the sections whose page bodies ARE an already
// hand-written source: a flux-types declaration file (JSDoc on every member) or
// a package README. Where a page needs to choose what it shows, it is authored
// markdown in docs/ pulling through the directives instead - that is how the
// Core and Tools references work.
//
//   /runtime/<group>/<module>/   one page per flux-types declaration file,
//                                grouped by its flux-types directory
//   /extensions/components/...   the components README: its head, then one
//                                page per "### Widget" section
//   /extensions/3d/...           the 3d README, plus its export surface
import { file } from "flux:fs";
import { escapeHtml, highlight, renderMarkdown } from "./markdown.ts";

const FLUX_TYPES = "../packages/flux-types";
const COMPONENTS = "../packages/components";
const THREE_D = "../packages/3d";

export type ReferencePage = {
  // URL path of the page directory, e.g. /runtime/modules/fs
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

// -- The declaration splitter, shared with the pull directives --------------

export type Declaration = { name: string; exported: boolean; extends: string[]; source: string };

// Top-level declarations of a TypeScript source, each with the comment block
// directly above it (no blank line between). `interface` / `type` /
// `class` run to the line that closes their top-level brace (or are a single
// line); `function` / `const` / `let` contribute their signature only, cut at
// the body's opening brace.
export function splitDeclarations(source: string): Map<string, Declaration> {
  let out = new Map<string, Declaration>();
  let lines = source.split("\n");
  let i = 0;
  while (i < lines.length) {
    let head = lines[i]!.match(
      /^(export )?(?:declare )?(?:async )?(interface|type|class|function|const|let) ([A-Za-z_][A-Za-z0-9_]*)\b(?: extends ([^{]+))?/,
    );
    if (!head) {
      i++;
      continue;
    }
    let kind = head[2]!;
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
    out.set(head[3]!, {
      name: head[3]!,
      exported: head[1] !== undefined,
      extends: head[4] ? head[4].split(",").map((s) => s.trim()).filter(Boolean) : [],
      source: comment + body,
    });
    i = end + 1;
  }
  return out;
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

// The declarations themselves are the reference here, so these pages stay
// generated: each is 1:1 with a hand-written .d.ts whose JSDoc is the
// documentation, and flux-types' own directories are the grouping. A group's
// blurb lives here; anything more to say about a module belongs in its .d.ts
// header comment, which becomes the page's lead.
const GROUP_BLURB: Record<string, string> = {
  modules: "The <code>flux:*</code> capability modules. Capabilities are named imports, never ambient globals.",
  standards: "The web-standard globals, with the names and shapes you already know.",
  gui: "The GUI capabilities: the render tree, devices, and the GPU surface.",
};

function groupTitle(dir: string): string {
  return dir === "gui" ? "GUI" : dir.charAt(0).toUpperCase() + dir.slice(1);
}

async function runtimePages(): Promise<ReferencePage[]> {
  const BASE = "/runtime";
  let index = await file(FLUX_TYPES + "/index.d.ts").text();
  let refs = [...index.matchAll(/\/\/\/ <reference path="\.\/([^"]+)\.d\.ts" \/>/g)].map((m) => m[1]!);
  let groups = new Map<string, ReferencePage[]>();
  for (let rel of refs) {
    let slash = rel.indexOf("/");
    let [dir, stem] = [rel.slice(0, slash), rel.slice(slash + 1)];
    let source = await file(`${FLUX_TYPES}/${rel}.d.ts`).text();
    let title = source.match(/^declare module "([^"]+)"/m)?.[1] ?? stem;
    let lead = intro(source);
    let group = groups.get(dir);
    if (!group) groups.set(dir, (group = []));
    group.push({
      path: `${BASE}/${dir}/${stem}`,
      title,
      html:
        `<h1>${escapeHtml(title)}</h1>\n` +
        (lead ? `<p>${escapeHtml(lead)}</p>\n` : "") +
        declaredIn(`packages/flux-types/${rel}.d.ts`) +
        code(source),
    });
  }
  let pages: ReferencePage[] = [];
  for (let [dir, members] of groups) {
    let list = members.map((p) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`).join("\n");
    pages.push(
      {
        path: `${BASE}/${dir}`,
        title: groupTitle(dir),
        html:
          `<h1>${groupTitle(dir)}</h1>\n` +
          `<p>${GROUP_BLURB[dir] ?? ""}</p>\n` +
          "<p>One page per declaration file, showing the declarations themselves: they carry a doc comment on every member, so they are the documentation.</p>\n" +
          `<ul>\n${list}\n</ul>\n`,
      },
      ...members,
    );
  }
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
  // Module stem -> its file extension and the names index.ts takes from it.
  let modules = new Map<string, { ext: string; names: string[] }>();
  for (let m of index.matchAll(/^export (?:type )?\{([^}]*)\} from "\.\/([a-z0-9-]+)\.(tsx?)"/gm)) {
    let names = m[1]!.split(",").map((n) => n.trim().replace(/^type /, "").replace(/ as .*$/, "")).filter(Boolean);
    let entry = modules.get(m[2]!) ?? { ext: m[3]!, names: [] };
    entry.names.push(...names);
    modules.set(m[2]!, entry);
  }
  let pages: ReferencePage[] = [];
  for (let [stem, { ext, names }] of modules) {
    let rel = `packages/3d/src/${stem}.${ext}`;
    let source = await file(`${THREE_D}/src/${stem}.${ext}`).text();
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
    ...(await componentsPages()),
    ...(await threeDPages()),
    ...(await runtimePages()),
  ];
}