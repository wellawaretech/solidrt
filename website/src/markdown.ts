import { file } from "flux:fs";
import { Renderer, marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import bash from "highlight.js/lib/languages/bash";
import json from "highlight.js/lib/languages/json";
import css from "highlight.js/lib/languages/css";

hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("json", json);
hljs.registerLanguage("css", css);
hljs.registerAliases(["jsx"], { languageName: "javascript" });
hljs.registerAliases(["tsx"], { languageName: "typescript" });
hljs.registerAliases(["sh", "shell"], { languageName: "bash" });

function applyTemplate(
  template: string,
  vars: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_, key) => String(vars[key] ?? ""));
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
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
      applyTemplate(codespan, { text: escapeHtml(text) });

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
    nav?: { text: string; href: string }[];
  };
} | null;

function buildNav(
  nav: { text: string; href: string }[] | undefined,
  navItem: string | undefined,
): string {
  if (!nav || !navItem) return "";
  return nav
    .map((item) => applyTemplate(navItem, item))
    .join("\n          ");
}

export function configureMarked(rules: Rules) {
  if (!rules) return;
  marked.use(
    markedHighlight({
      highlight(code, lang) {
        if (lang && hljs.getLanguage(lang))
          return hljs.highlight(code, { language: lang }).value;
        return escapeHtml(code);
      },
    }),
  );
  marked.use({ renderer: buildRenderer(rules.elements) });
}

async function resolveTemplate(template: string): Promise<string> {
  if (template.startsWith("file:")) {
    return file(template.slice(5)).text();
  }
  return template;
}

/** Wrap already-rendered page content in the page template (shell + nav). */
export async function renderPage(content: string, rules: Rules, title?: string): Promise<string> {
  if (!rules?.page?.template) return content;
  let template = await resolveTemplate(rules.page.template);
  return applyTemplate(template, {
    content,
    title: title ?? "",
    nav: buildNav(rules.page.nav, rules.page.navItem),
  });
}

export async function markdownToHtml(md: string, rules: Rules, title?: string): Promise<string> {
  let content = await marked(md);
  return renderPage(content, rules, title);
}
