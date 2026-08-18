// Pull directives: `{{ provider path/to/source.ts Symbol }}` alone on a line in
// a docs page, replaced by generated HTML before the markdown is parsed.
//
// The page decides what to show and says where it comes from; the source file
// stays the single copy. A directive that cannot be resolved (unknown provider,
// unreadable file, missing symbol) fails the build: it names a declaration by
// hand, so a rename in the source must not silently blank a page.
//
// Paths are repo-relative, and readable as-is by someone browsing docs/ raw,
// which is the whole reason the path is in the directive rather than implied.
// One caveat, worth the simplicity: a directive is replaced wherever it appears,
// including inside a fenced code block, so a page cannot show one literally.

import { file } from "flux:fs";
import { escapeHtml, highlight } from "./markdown.ts";
import { splitDeclarations, type Declaration } from "./reference.ts";

const ROOT = "..";

// Declarations per source file, parsed once however many symbols a page pulls.
let parsed = new Map<string, Map<string, Declaration>>();
async function declarationsOf(path: string): Promise<Map<string, Declaration>> {
  let cached = parsed.get(path);
  if (!cached) parsed.set(path, (cached = splitDeclarations(await file(ROOT + "/" + path).text())));
  return cached;
}

function code(source: string): string {
  return `<pre><code class="hljs language-typescript">${highlight(source, "typescript")}</code></pre>\n`;
}

// -- The CLI's usage text ---------------------------------------------------

// The CLI keeps its whole usage text in printUsage() (args.ts): a "Commands:"
// table, then "<name>[/<name>] options:" blocks. That text is the source, so a
// command documents itself the moment it is added there.
const CLI_ARGS = "packages/cli/src/args.ts";

type Command = { name: string; args: string; summary: string; options: { heading: string; text: string }[] };

let commands: Map<string, Command> | undefined;
async function commandsOf(): Promise<Map<string, Command>> {
  if (commands) return commands;
  let source = await file(ROOT + "/" + CLI_ARGS).text();
  let usage = source.match(/console\.error\(`Usage: srt[^`]*`\)/)?.[0].slice("console.error(`".length, -2) ?? "";
  let [, commandsText = "", optionsText = ""] = usage.match(/Commands:\n([\s\S]*?)\n\n([\s\S]*)/) ?? [];
  let blocks = optionsText
    .split("\n\n")
    .map((block) => block.match(/^(\S+) options:\n([\s\S]*)$/))
    .filter((m): m is RegExpMatchArray => m !== null)
    .map((m) => ({ names: m[1]!.split("/"), heading: `${m[1]} options`, text: m[2]! }));
  commands = new Map();
  for (let line of commandsText.split("\n")) {
    let m = line.match(/^\s+(\S+)(.*?)\s{2,}(.+)$/);
    if (!m) continue;
    commands.set(m[1]!, {
      name: m[1]!,
      args: m[2]!.trim(),
      summary: m[3]!.trim(),
      options: blocks.filter((b) => b.names.includes(m![1]!)).map(({ heading, text }) => ({ heading, text })),
    });
  }
  return commands;
}

type Provider = (args: string[]) => Promise<string>;

let providers: Record<string, Provider> = {
  // {{ decl <path> <Symbol> }} - one declaration with its doc comment.
  async decl([path, name]) {
    if (!path || !name) throw new Error("decl takes <path> <Symbol>");
    let declaration = (await declarationsOf(path)).get(name);
    if (!declaration) throw new Error(`no declaration ${name} in ${path}`);
    return code(declaration.source);
  },

  // {{ intrinsics <path> }} - the JSX element vocabulary as a table, from the
  // IntrinsicElements map. `ElementRef` (the `ref` callback every element
  // takes) is dropped from each composition as noise.
  async intrinsics([path]) {
    if (!path) throw new Error("intrinsics takes <path>");
    let source = await file(ROOT + "/" + path!).text();
    let block = source.match(/interface IntrinsicElements \{([\s\S]*?)\n  \}/)?.[1];
    if (!block) throw new Error(`no IntrinsicElements map in ${path}`);
    let rows = [...block.matchAll(/^\s+"?([a-z-]+)"?: (.+)$/gm)].map((m) => {
      let composition = m[2]!
        .split("&")
        .map((s) => s.trim())
        .filter((s) => s !== "ElementRef")
        .join(" &amp; ");
      return `<tr><td><code>&lt;${m[1]}&gt;</code></td><td><code>${composition}</code></td></tr>`;
    });
    if (rows.length === 0) throw new Error(`no elements in the IntrinsicElements map of ${path}`);
    return (
      "<table>\n<thead><tr><th>Element</th><th>Props</th></tr></thead>\n<tbody>\n" +
      rows.join("\n") +
      "\n</tbody>\n</table>\n"
    );
  },

  // {{ usage <command> }} - one srt command: its synopsis, its summary, and
  // every option block of the usage text that names it.
  async usage([name]) {
    if (!name) throw new Error("usage takes <command>");
    let command = (await commandsOf()).get(name);
    if (!command) throw new Error(`no command ${name} in ${CLI_ARGS}`);
    let pre = (text: string) => `<pre><code>${escapeHtml(text)}</code></pre>\n`;
    return (
      pre(`srt ${command.name}${command.args ? " " + command.args : ""}`) +
      `<p>${escapeHtml(command.summary)}.</p>\n` +
      command.options.map((o) => `<p><small>${escapeHtml(o.heading)}</small></p>\n` + pre(o.text)).join("")
    );
  },
};

/** Resolve every directive in a page body. `doc` names the page in errors. */
export async function resolveDirectives(body: string, doc: string): Promise<string> {
  let out = "";
  let at = 0;
  for (let match of body.matchAll(/^\{\{ *(.+?) *\}\}$/gm)) {
    let [name, ...args] = match[1]!.split(/\s+/);
    let provider = providers[name ?? ""];
    if (!provider) throw new Error(`${doc}: unknown pull directive "${name}"`);
    let html: string;
    try {
      html = await provider(args);
    } catch (error) {
      throw new Error(`${doc}: {{ ${match[1]} }}: ${(error as Error).message}`);
    }
    out += body.slice(at, match.index) + html;
    at = match.index! + match[0].length;
  }
  return out + body.slice(at);
}

// What the pages pulled, per provider: the second word of each directive.
function pulledBy(provider: string, bodies: string[], where?: string): Set<string> {
  let out = new Set<string>();
  let pattern = new RegExp(`^\\{\\{ *${provider} +(\\S+)(?: +(\\S+))? *\\}\\}$`, "gm");
  for (let body of bodies)
    for (let match of body.matchAll(pattern))
      if (where === undefined) out.add(match[1]!);
      else if (match[1] === where && match[2]) out.add(match[2]);
  return out;
}

/**
 * Exported declarations of `path` that no page pulled, for the build to report:
 * with pages naming what they show, a new type is otherwise documented nowhere.
 */
export async function unpulled(path: string, bodies: string[]): Promise<string[]> {
  let pulled = pulledBy("decl", bodies, path);
  let all = await declarationsOf(path);
  return [...all.values()].filter((d) => d.exported && !pulled.has(d.name)).map((d) => d.name);
}

/** Likewise for srt commands: one the CLI offers and no page shows. */
export async function undocumentedCommands(bodies: string[]): Promise<string[]> {
  let pulled = pulledBy("usage", bodies);
  return [...(await commandsOf()).keys()].filter((name) => !pulled.has(name));
}
