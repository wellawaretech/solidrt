// Generated Reference pages, for the sections whose page bodies ARE an already
// hand-written source: a flux-types declaration file (JSDoc on every member),
// a package's docs/ files, or a package README. Where a page needs to choose
// what it shows, it is authored markdown in docs/ pulling through the
// directives instead - that is how the Core and Tools references work.
//
//   /runtime/<group>/<module>/   one page per flux-types declaration file,
//                                grouped by its flux-types directory
//   /extensions/components/...   one page per components module: its docs/
//                                file, then the declarations index.ts exports
//   /extensions/2d/...           the 2d README, plus its export surface
//   /extensions/3d/...           the 3d README, plus its export surface
import { file } from "flux:fs";
import { escapeHtml, highlight, renderMarkdown } from "./markdown.ts";

const FLUX_TYPES = "../packages/flux-types";
const COMPONENTS = "../packages/components";
// README-fronted extensions: the overview page is the package README, the API
// pages are its export surface by module.
const README_EXTENSIONS = [
  { pkg: "2d", dir: "../packages/2d" },
  { pkg: "3d", dir: "../packages/3d" },
];

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

// -- Extensions: components docs/, and the 2d/3d READMEs ---------------------

// Concept modules (the system the components assume) lead the listing, in
// this order; every other module is a component. The prose is the module's
// docs/ file, the API its exported declarations pulled from the typed source,
// so a page cannot disagree with either.
// SYNC: scripts/build-components-docs.ts (the README groups by the same list,
// and its coverage check is the hard gate: every module has a docs/ file).
const COMPONENT_CONCEPTS = ["theme", "policy", "types", "typography", "spacing"];

async function componentsPages(): Promise<ReferencePage[]> {
  const BASE = "/extensions/components";
  // Modules in src/index.ts order, with the names it re-exports from each.
  let index = await file(COMPONENTS + "/src/index.ts").text();
  let modules = new Map<string, string[]>();
  for (let m of index.matchAll(/^export (?:type )?\{([^}]*)\} from "\.\/([a-z0-9-]+)"/gm)) {
    let names = m[1]!.split(",").map((n) => n.trim().replace(/^type /, "")).filter(Boolean);
    modules.set(m[2]!, [...(modules.get(m[2]!) ?? []), ...names]);
  }
  let pages: ReferencePage[] = [];
  for (let [stem, names] of modules) {
    let doc = await file(`${COMPONENTS}/docs/${stem}.md`).text();
    let m = doc.match(/^# (.+)\n+([\s\S]*)$/);
    if (!m) throw new Error(`packages/components/docs/${stem}.md must start with an h1 title`);
    let [, title, body] = m;
    let ext = (await file(`${COMPONENTS}/src/${stem}.tsx`).exists()) ? "tsx" : "ts";
    let rel = `packages/components/src/${stem}.${ext}`;
    let source = await file(`${COMPONENTS}/src/${stem}.${ext}`).text();
    let decls = splitDeclarations(source);
    // A module may re-export from an internal sibling (Pressable's PressState
    // lives in press.ts); pull those declarations too, one level deep.
    for (let m of source.matchAll(/^export (?:type )?\{([^}]*)\} from "\.\/([a-z0-9-]+)"/gm)) {
      let inner = splitDeclarations(await file(`${COMPONENTS}/src/${m[2]}.ts`).text());
      for (let name of m[1]!.split(",").map((n) => n.trim().replace(/^type /, "")).filter(Boolean)) {
        let d = inner.get(name);
        if (d && !decls.has(name)) decls.set(name, d);
      }
    }
    let missing = names.filter((n) => !decls.has(n));
    if (missing.length > 0) console.log(`Not shown on ${BASE}/${stem}, from ${rel}: ${missing.join(", ")}`);
    let blocks = names
      .map((n) => decls.get(n))
      .filter((d): d is Declaration => d !== undefined)
      .map((d) => `<h3 id="${d.name}">${escapeHtml(d.name)}</h3>\n` + code(d.source))
      .join("");
    pages.push({
      path: `${BASE}/${stem}`,
      title: title!.trim(),
      html:
        `<h1>${escapeHtml(title!.trim())}</h1>\n` +
        (await renderMarkdown(body!.trim())) +
        `<h2>API</h2>\n` +
        declaredIn(rel) +
        blocks,
    });
  }
  let item = (p: ReferencePage) => `<li><a href="${p.path}/"><code>${escapeHtml(p.title)}</code></a></li>`;
  let isConcept = (p: ReferencePage) => COMPONENT_CONCEPTS.includes(p.path.slice(BASE.length + 1));
  let concepts = pages.filter(isConcept);
  let components = pages.filter((p) => !isConcept(p));
  let head = await file(COMPONENTS + "/docs/index.md").text();
  let indexPage: ReferencePage = {
    path: BASE,
    title: "@solidrt/components",
    html:
      `<h1>@solidrt/components</h1>\n` +
      (await renderMarkdown(head.replace(/^# .+\n/, ""))) +
      `<h2>Concepts</h2>\n<ul>\n${concepts.map(item).join("\n")}\n</ul>\n` +
      `<h2>Components</h2>\n<ul>\n${components.map(item).join("\n")}\n</ul>\n`,
  };
  return [indexPage, ...concepts, ...components];
}

// One page per source module index.ts re-exports from, in index.ts order,
// showing only the exports index.ts names (the public surface): each with its
// JSDoc and signature (functions) or full declaration (types).
async function readmeExtensionPages(pkg: string, dir: string): Promise<ReferencePage[]> {
  const BASE = `/extensions/${pkg}`;
  let readme = await file(dir + "/README.md").text();
  let index = await file(dir + "/src/index.ts").text();
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
    let rel = `packages/${pkg}/src/${stem}.${ext}`;
    let source = await file(`${dir}/src/${stem}.${ext}`).text();
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
    title: `@solidrt/${pkg}`,
    html: (await renderMarkdown(readme)) + `<h2>API</h2>\n<p>By module, as <code>src/index.ts</code> exports it:</p>\n<ul>\n${list}\n</ul>\n`,
  });
  return pages;
}

export async function referencePages(): Promise<ReferencePage[]> {
  return [
    ...(await componentsPages()),
    ...(await Promise.all(README_EXTENSIONS.map((e) => readmeExtensionPages(e.pkg, e.dir)))).flat(),
    ...(await runtimePages()),
  ];
}