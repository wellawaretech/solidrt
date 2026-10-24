import { file } from "flux:fs";
import { Marked, Renderer } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import json from "highlight.js/lib/languages/json";
import css from "highlight.js/lib/languages/css";
import rust from "highlight.js/lib/languages/rust";
import ini from "highlight.js/lib/languages/ini";

hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("json", json);
hljs.registerLanguage("css", css);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("ini", ini); // its grammar carries the "toml" alias
hljs.registerAliases(["jsx"], { languageName: "javascript" });
hljs.registerAliases(["tsx"], { languageName: "typescript" });

function applyTemplate(
  template: string,
  vars: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_, key) => String(vars[key] ?? ""));
}

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// An identifier in inline code is kept whole: `d-texture` never splits at the
// hyphen, `getBoundingBoxViewport` never mid-word. Other tokens (paths, `::`
// chains) keep their break points, so a long one cannot widen a phone page.
const IDENTIFIER = /^[\w$-]+$/;

function unbrokenTokens(html: string): string {
  return html
    .split(" ")
    .map((token) => (IDENTIFIER.test(token) ? `<span class="nobr">${token}</span>` : token))
    .join(" ");
}

function buildRenderer(
  elements: Record<string, string | undefined>,
): Partial<Renderer> {
  let r: Partial<Renderer> = {};

  let heading = elements.heading;
  if (heading)
    r.heading = function ({ tokens, depth, text: raw }) {
      let text = this.parser!.parseInline(tokens);
      return applyTemplate(heading, {
        text,
        depth,
        slug: raw.toLowerCase().replace(/\s+/g, "-"),
      });
    };

  let paragraph = elements.paragraph;
  if (paragraph)
    r.paragraph = function ({ tokens }) {
      return applyTemplate(paragraph, { text: this.parser!.parseInline(tokens) });
    };

  let link = elements.link;
  if (link)
    r.link = function ({ href, title, tokens }) {
      return applyTemplate(link, {
        href,
        title: title ?? "",
        text: this.parser!.parseInline(tokens),
      })
    };

  let image = elements.image;
  if (image)
    r.image = ({ href, title, text }) =>
      applyTemplate(image, { href, title: title ?? "", text });

  let code = elements.code;
  if (code)
    r.code = ({ text, lang, escaped }) =>
      applyTemplate(code, {
        text: escaped ? text : escapeHtml(text),
        lang: lang ?? "",
      });

  let codespan = elements.codespan;
  if (codespan)
    r.codespan = ({ text }) =>
      applyTemplate(codespan, { text: unbrokenTokens(escapeHtml(text)) });

  // A table renders as marked's own, inside the template's wrapper.
  let table = elements.table;
  if (table)
    r.table = function (token) {
      return applyTemplate(table, { text: Renderer.prototype.table.call(this, token) });
    };

  let blockquote = elements.blockquote;
  if (blockquote)
    r.blockquote = function ({ tokens }) {
      return applyTemplate(blockquote, { text: this.parser!.parse(tokens) });
    };

  let strong = elements.strong;
  if (strong)
    r.strong = function ({ tokens }) {
      return applyTemplate(strong, { text: this.parser!.parseInline(tokens) });
    };

  let em = elements.em;
  if (em)
    r.em = function ({ tokens }) {
      return applyTemplate(em, { text: this.parser!.parseInline(tokens) });
    };

  return r;
}

export type Rules = {
  elements: Record<string, string | undefined>;
  page?: {
    template?: string;
    navItem?: string;
    sidebar?: string;
    sidebarItem?: string;
    sidebarGroup?: string;
    sidebarGroupPlain?: string;
  };
} | null;

/** What the page template needs around the content. */
export type PageShell = { title?: string; nav?: string; sidebar?: string };

// A page as the sidebar sees it: its URL path (directory, no trailing slash;
// "" for the site root) and its title.
export type PageEntry = { path: string; title: string };

// The in-section sidebar for the page at `current`: every page under the same
// top-level section, in `pages` order. Pages directly in the section come
// first (its index leading); each subdirectory becomes a group headed by its
// index page (or its name), holding the pages beneath it. Several groups are
// collapsible, open only when the current page is in one; a lone group has
// nothing to collapse against, so it renders plain and always open. Empty
// when the section has one page.
export function buildSidebar(current: string, pages: PageEntry[], rules: Rules): string {
  let page = rules?.page;
  if (!page?.sidebar || !page.sidebarItem || !page.sidebarGroup) return "";
  let section = current.split("/")[1];
  if (!section) return "";
  let prefix = "/" + section;
  let inSection = pages.filter((p) => p.path === prefix || p.path.startsWith(prefix + "/"));
  if (inSection.length < 2) return "";
  let item = (p: PageEntry) =>
    applyTemplate(page.sidebarItem!, {
      href: p.path + "/",
      text: escapeHtml(p.title),
      current: p.path === current ? ' aria-current="page"' : "",
    });
  let top: PageEntry[] = [];
  let groups = new Map<string, PageEntry[]>();
  for (let p of inSection) {
    let rest = p.path.slice(prefix.length + 1);
    let slash = rest.indexOf("/");
    if (slash < 0) {
      if (rest === "") top.unshift(p);
      else top.push(p);
    } else {
      let dir = rest.slice(0, slash);
      let group = groups.get(dir);
      if (!group) groups.set(dir, (group = []));
      group.push(p);
    }
  }
  let out: string[] = [];
  for (let p of top) {
    let dir = p.path.slice(prefix.length + 1);
    if (dir && groups.has(dir)) continue;
    out.push(item(p));
  }
  let groupTemplate =
    groups.size === 1 && page.sidebarGroupPlain ? page.sidebarGroupPlain : page.sidebarGroup;
  for (let [dir, members] of groups) {
    let head = top.find((p) => p.path === `${prefix}/${dir}`);
    out.push(
      applyTemplate(groupTemplate, {
        text: head ? escapeHtml(head.title) : dir,
        href: head ? head.path + "/" : "",
        current: head?.path === current ? ' aria-current="page"' : "",
        // Collapsed unless the reader is somewhere inside it.
        open:
          head?.path === current || members.some((m) => m.path === current)
            ? " open"
            : "",
        items: members.map(item).join("\n"),
      }),
    );
  }
  return applyTemplate(page.sidebar, { items: out.join("\n") });
}

/** The site nav: one item per section, in `sections` order, the section
 * holding the page at `current` marked current. */
export function buildNav(current: string, sections: PageEntry[], rules: Rules): string {
  let navItem = rules?.page?.navItem;
  if (!navItem) return "";
  return sections
    .map((s) =>
      applyTemplate(navItem, {
        href: s.path + "/",
        text: escapeHtml(s.title),
        current: current === s.path || current.startsWith(s.path + "/") ? ' aria-current="true"' : "",
      }),
    )
    .join("\n          ");
}

export function highlight(code: string, lang: string | undefined): string {
  if (lang && hljs.getLanguage(lang))
    return hljs.highlight(code, { language: lang }).value;
  return escapeHtml(code);
}

// The configured parser. Each configureMarked builds a fresh one, so a dev
// rebuild with edited rules does not stack extensions on the old ones.
let marked = new Marked();

export function configureMarked(rules: Rules) {
  marked = new Marked();
  if (!rules) return;
  marked.use(markedHighlight({ highlight }));
  marked.use({ renderer: buildRenderer(rules.elements) });
}

async function resolveTemplate(template: string): Promise<string> {
  if (template.startsWith("file:")) {
    return file(template.slice(5)).text();
  }
  return template;
}

/** Wrap already-rendered page content in the page template (shell + nav). */
export async function renderPage(content: string, rules: Rules, page: PageShell = {}): Promise<string> {
  if (!rules?.page?.template) return content;
  let template = await resolveTemplate(rules.page.template);
  return applyTemplate(template, {
    content,
    title: escapeHtml(page.title ?? ""),
    nav: page.nav ?? "",
    sidebar: page.sidebar ?? "",
  });
}

/** Markdown to an HTML fragment, no page shell. */
export async function renderMarkdown(md: string): Promise<string> {
  return marked.parse(md);
}

export async function markdownToHtml(md: string, rules: Rules, page: PageShell = {}): Promise<string> {
  return renderPage(await marked.parse(md), rules, page);
}
