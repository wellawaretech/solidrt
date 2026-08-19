---
title: Documentation website
description: "A monorepo website/ generated statically by a flux script: section nav, a Core-first Start page, and generate-what-we-can content (API reference from types, examples from the repo)."
created: 2026-07-16
---

# Documentation website

Build the public SolidRT documentation website. Structure was settled in IA
discussions (2026-07-11 through 2026-07-16); this plan records the decisions
and the staging to get there.

## Branding (settled, keep)

Project description, to keep as-is:

> SolidRT lets you build cross-platform native apps with familiar web
> technologies. Built from the ground up for developers and AI agents to
> work together.

Tagline: **"the pieces fit"** - the logo symbolizes this (geometric pieces
composing a square; see the SVG in the `~/solidrt/docs` `template.html`
header and `lattice/assets/icon-puzzle.svg`). Landing hero = description +
tagline.

## Decisions

**Lives in the monorepo**, as `website/`. Two structure decisions depend on
proximity to the code: example pages are pulled from `examples/` so they
cannot drift, and the API reference is generated from the types in
`packages/core` (and `packages/flux-types` for the Runtime section). The
standalone experiment at `~/solidrt/docs` gets ported here and retired.

**Static generation, markdown source.** The generator is a flux script
(dogfooding the runtime): the `~/solidrt/docs` converter wraps `marked` +
`highlight.js` with a JSON rules file of per-element HTML templates
(`template.json`) plus a page shell (`template.html`). It currently serves
pages on request via `flux:http`; production renders everything to static
output instead. Dynamic serving is not needed once source is markdown.

**Nav:** `Start - Core - Extensions - Tools - Runtime - Architecture`
(the section was "Frameworks" until 2026-08-18; renamed to match the
scaffolder's `srt init --with` vocabulary; URLs are `/extensions/...`)

(News is deferred: blog + changelog get added as a top-level section later,
not in the initial site.)

- **Start** (new since the original nav): the one journey-shaped item in an
  otherwise layer-shaped nav. A single page, not a section: install,
  scaffold, run, change one line, see it update. Five minutes, one scroll.
  Ends with the fork: go deeper (Core Concepts) or go faster (Extensions).
  Hold the line at "first successful edit"; everything past that belongs in
  section Guides.
- **Core**: the stable spine. Sub-structure: Concepts first (mental model),
  then Guides, then Examples, plus Reference. Concept-vs-Guide test: if
  removing the page costs transferable understanding it is a Concept; if it
  costs one specific task it is a Guide.
- **Extensions** (was Frameworks): things built on Core, siblings not a stack. Plain card
  list until a second framework exists. Explicit maturity labels in
  frontmatter (stable / evolving / experimental) so Components can honestly
  read "evolving" without the caveat leaking into every page.
- **Tools**: CLI now, planned GUI later.
- **Runtime**: Flux as a general-purpose JS runtime, deliberately not hero
  billing (page title still says "Flux").
- **Architecture**: native internals only (forge/alloy/flux/lattice), not
  the JS-facing layering story.

**Per-section sub-shape:** every section gets an Examples subsection; every
section with an API surface also gets a Reference subsection (generated):
Core Reference from `packages/core` types, Runtime Reference from
`packages/flux-types` (one page per module), Tools Reference from CLI
command definitions, Components Reference later once type extraction
handles it. URLs like `/core/reference/view`, `/runtime/reference/http`,
`/tools/reference/create`. "Persistent sidebar" means: within a section,
the sidebar always links into its Reference regardless of the page being
read. Changelog is NOT per-section; it arrives with News later (per-section
changelogs would need per-package release tagging - over-engineered now).

**Core-first onboarding** (overturns the earlier Components-first
assumption). Reasons: the Start page is the most-read page and must not
break, so it builds on the stable layer; with more extensions coming, any
extension in Start reads as an endorsement, and teaching the shared
substrate keeps the fork honest; onboarding that works with Core alone
demonstrates the "stable spine" story better than prose. Requires the
five-minute Core experience to be good (signal + View + Text + animation
is), and `srt create` scaffolding a bare-Core default template with
framework templates as named options (check what it currently produces).

**No landing switcher** (overturns the earlier two-button Core/Components
"choose your altitude" live example). The altitude fork lives at the end of
Start; on the landing page it front-runs the story and doubles example
maintenance, half of it on an unstable API. Landing page is: hero, one live
Core example, Get started CTA, deploy teaser. A "same app at two altitudes"
showcase can return deeper in the site once Components stabilizes or a
second framework ships.

**Generate what we can.** Reference-shaped content is generated at site
build by the same flux script; understanding-shaped content is hand-written
and stays small (landing, Start, Concepts, Guides, Architecture - roughly
fifteen pages of prose). Generated:

- API reference from `packages/core` types (+ `packages/flux-types` for
  Runtime); this is also the agent-facing surface (feeds `llms.txt`).
- Examples pages from `examples/` source. Scope per
  okf/plans/examples-rescope.md (2026-07-25): the corpus is the flat set
  of realistic, scaffold-shaped apps in root `examples/<app>/` - the
  human-facing tier. `packages/*/examples` are agent-facing concept
  isolations and are NOT harvested for the site. Section attribution
  (Core vs a framework's section) is derived from each app's
  package.json dependencies, not from folder structure. Upgrade path:
  `srt playback` deterministic frame capture + captureSnapshot can
  generate real screenshots/recordings of every example at build time.
- CLI reference from command definitions / help output.
- Changelog from releases (deferred with News).

**Existing `docs/` content is outdated** and gets rewritten fresh as website
content; `docs/internals/` may partially survive as the Architecture
section.

## Content rework (decided 2026-08-18)

The first generation pass came out push-shaped: `referencePages()` walked the
sources and invented a page tree, so the site's shape was a side effect of how
the declarations happened to be organised (seven `d-*` intrinsics in
`jsx-runtime.d.ts` produced seven `d-*` pages nobody asked for). 88 of 94
pages were generated, and the hand-written Core page is the quality bar none
of them met. Inverted:

**`docs/` is the content root**, markdown only, and its tree is the site: a
directory is a section and a sidebar group, a file is a page, an `NN-` name
prefix sets the order and is stripped from the URL, and a name starting with
`_` is not published. The top nav is the top-level directories in that same
order, so nav and sidebar have one definition and there is no sidebar or nav
config anywhere. Optional `nav:` frontmatter overrides the label a page takes
in navigation when its h1 is not the right one there (`/runtime/` is the case:
h1 "Flux", nav "Runtime"). Two levels of sidebar, hard limit: a section
wanting a third level wants to be its own section (Extensions is the
precedent). Everything the site needs that is not markdown (css, icon) lives
in `website/assets/` and is copied byte-for-byte, so `website/content/` is
gone.

**Pull, not push.** Pages are hand-written and name the generated data they
want, inline, as `{{ provider path/to/source.ts Symbol }}` on its own line.
Browsing `docs/` raw then shows a marker that names the exact file and symbol
(considered and rejected: a fenced block, which renders as a code box and so
reads as a snippet to copy; a link-wrapped directive, clickable on GitHub but
paying `../../../` noise). An unresolved directive **fails the build** - the
directive hard-codes a declaration name, and a rename in the source must not
silently blank a page. Providers: `props` (a core prop interface), `decl` (any
declaration from any TS source), `dts` (a whole flux-types declaration file),
`usage` (one srt command), `source` (a file from `examples/`).

**Reference by topic, not by symbol.** Core Reference becomes Elements,
Detached elements (one generic page: what detached means, the shared
positioning contract, when to reach for them, then the prop delta - not one
page per `d-*`), Layout, Transforms, Input, Types. The Runtime reference is
the one place a full dump is honest (flux-types is hand-written JSDoc), so
those pages stay, but as small markdown files holding one `{{ dts ... }}`
each, visible and reorderable. Expected page count drops from 94 to ~45.

## Staging

1. **Skeleton**: `website/` with the generator ported from `~/solidrt/docs`
   (static output mode), the new nav, hand-written landing + Start, ported /
   rewritten section landing pages. DONE 2026-07-16: `make build` in
   `website/` (own Makefile, not the root one) renders `content/` to
   `dist/` (7 pages + css) via `srt bundle --flux` + the host
   `dist/<triple>/flux` binary. Rules: `*.md`
   through the converter, `*.html` wrapped as pre-rendered fragments (the
   landing page), everything else copied. Titles from the first h1. Content
   pages are `<dir>/index.md` for clean URLs. `website` joined the bun
   workspace.
   Content filled 2026-07-26: landing plus all six top-level pages are real
   prose, no stubs, every command and API name verified against the shipped
   CLI and packages. Sub-pages (Concepts, Guides, per-crate Architecture)
   are still to write, and they need in-section navigation first: the
   generator has only the top nav, so a sidebar (cheapest version: derived
   from the directory tree, title from each page's first h1) is a
   prerequisite for any section with more than one page.
   Content and styling are separate tracks from here on; the sidebar belongs
   to the styling track, so section indexes link their sub-pages in prose
   until it lands.
2. **Examples generator**: pages from `examples/` (cheapest generator,
   highest visibility). PARKED 2026-08-18: the examples corpus is not in a
   state to generate from, and getting it right is a larger job than the
   generator. If the generator is built before the corpus is cleaned up,
   develop it against a fixture example, not `examples/`.
3. **API reference generation** from types (its own project, especially
   type extraction).
4. **Screenshot/recording capture** for example pages via playback.

Content rework, staged on top (2026-08-18):

5. **Move only.** DONE 2026-08-18: `website/content/*/index.md` moved to
   `docs/NN-<section>/index.md`, the landing HTML fragment to `docs/index.md`
   (raw HTML blocks pass through marked, so the folder is markdown only and
   the build's `*.html` page kind is gone), css + icon to `website/assets/`.
   The build reads `../docs`, strips `NN-` prefixes, skips `_` entries,
   derives the nav from the tree (the hand-kept `nav` array in
   `template.json` deleted), and copies `assets/` separately. Output
   unchanged: 7 pages, 87 generated, same URLs. The stale `docs/*` is staged
   at `docs/_old/` to be mined by stages 6-7 and deleted with them; the four
   in-repo pointers into it (cli AGENTS.md, dev-server.ts, server/main.ts)
   point at `_old` meanwhile, and `flux/CLAUDE.md`'s "update docs/flux.md"
   rule now says flux-types alone is the documentation.
6. **Directives + Core Reference.** DONE 2026-08-18: `src/directives.ts`
   resolves `{{ provider ... }}` before the markdown is parsed, with two
   providers so far - `decl <path> <Symbol>` (one declaration with its doc
   comment) and `intrinsics <path>` (the IntrinsicElements map as a table).
   `splitDeclarations` moved to a shared section of `reference.ts` and gained
   an `exported` flag; `corePages()` and `compose()` are gone. The Core
   Reference is now ten authored pages (index, Elements, Drawing, Text,
   Detached elements, Layout, Transforms, Input, Shaders, Types) against the
   previous eighteen generated ones, and all 30 exported declarations of
   `types.d.ts` are pulled by one of them. Two guards, both verified: an
   unresolved directive throws naming the page and the directive, and the
   build reports any exported declaration of `types.d.ts` no page pulls
   (`unpulled()`), which is what keeps "show less" from becoming "silently
   document less". Site total: 94 pages to 86.
   Known gap, needs a decision: `flux script.js` exits 0 even on an uncaught
   top-level throw (verified for sync throw, post-await throw, and an
   unhandled rejection - `eval_source` returns `()` and only logs through
   `report_error`/`report_rejection`), so the failing directive prints its
   error and `make build` still succeeds. The guard is only advisory until
   the `flux` binary can exit non-zero; parked as
   okf/backlog/flux-script-exit-code.md.
7. **The rest.** DONE 2026-08-18, and it revised the stage-6 assumption that
   every generated page should become authored markdown. The test that
   emerged: **authored markdown where a page must choose what it shows,
   generated where the page body IS an already hand-written source.** By that
   test the sections split rather than converting wholesale.
   Tools converted: nine command pages plus an index became one authored page,
   `/tools/reference/`, grouping the commands by purpose (start, develop,
   check and build, render and agents) with a `{{ usage <command> }}` pull
   each. A command page was three lines of synopsis, so a page apiece was the
   `d-*` problem in miniature. `toolsPages()` deleted; a coverage check
   reports any `srt` command no page pulls (verified).
   Runtime stayed generated, but grouped. Each page is 1:1 with a flux-types
   declaration file carrying JSDoc on every member, and those files total 2742
   lines (gpu.d.ts alone is 834), so per-module pages are right and converting
   them to 27 stub files holding one pull each would add ceremony plus a drift
   risk: a new module would need a docs file or vanish silently, where today
   adding the `.d.ts` is enough. What was wrong was the flat 28-item sidebar,
   so `runtimePages()` now groups by flux-types' own directories:
   `/runtime/modules/<name>`, `/runtime/standards/<name>`, `/runtime/gui/<name>`,
   each group getting an index page and a sidebar group (the URLs lost the
   `/reference/` level - for Runtime the section IS the reference). Group
   labels derive from the directory names, so a flux-types reorganization
   flows through with at most a label-map edit.
   Extensions stayed generated for the same reason: the package READMEs are
   hand-written prose that already chooses what it shows, and the widgets are
   distinct APIs rather than near-duplicates. The recorded README gap (21
   widgets documented, 29 exported) is a components-package job, not a website
   one.
   Site total: 94 pages before the rework, 79 now (18 authored, 61 generated).
   `docs/_old/` stays for now, by decision - it is still the raw material for
   the section pages that are not written yet.

Open, not decided here: the three `docs/_old/flux-*-plan.md` documents need an
okf home, and the directory is their state (dev server and wasm shipped; mdns
did not), so it is three state calls rather than one move.

## Styling track (decided 2026-07-26, not yet built)

The site must look exactly like an app built with `@solidrt/components`, the
launcher being the reference. Its visual signature: borderless filled
surfaces (card fill on window fill, no rules or underlines anywhere), radius
12 / padding 20 / gap 16, one text color with muted for secondary rows,
headings separated by size and weight rather than tone, flat accent buttons
(primary fill, white label, hover tint, press scale), NotoSans.

Decisions:

- **Pico goes.** DONE 2026-08-18, see stage 2 (a) below. Colors mapped onto it cleanly (`--pico-*` overrides are its
  documented path, and `content/css/theme.css` does that today), but every
  geometry and typography item above is a Pico default we would override
  rather than use, and we ship 71 KB to use a tenth of it. Replace with our
  own classless stylesheet, roughly 200 lines, over what the markdown
  actually emits.
- **One token source, no hand-copying.** DONE 2026-08-18. `theme.ts` now
  imports `createStore` from `@solidjs/signals` and `mixColors` from a new
  `@solidrt/core/color` subpath export (that file only imports colord), so
  it no longer reaches core's index (which pulls `flux:rendertree`, absent
  on the plain flux binary) and is importable as `@solidrt/components/theme`
  from the website build. `website/src/tokens.ts` emits `dist/css/tokens.css`
  (both color presets, plus spacing, radius, borderWidth and the type scale
  on `:root`). An earlier idea of extracting a separate `tokens.ts` data
  file was dropped as unnecessary.
- **Type scale: structure matched, relative sizes kept, body one step up**
  for long-form reading. The framework's 14px body is right for app UI and
  small for prose; the ratios between caption/label/body/title/heading stay
  as they are. Verified 2026-08-18: "one step up" is not the next framework
  step (title, 18px, is too big for prose); it means a 16px prose body with
  the ratios preserved (about 12.6/13.7/16/20.6/25). Express it as one
  visible factor in the stylesheet (`--srt-prose-scale: calc(16 / 14)`)
  applied over the emitted `--srt-*-size` tokens, so tokens.css stays
  identical to the framework. Decided 2026-08-18: 16px.
- **Font parity costs something.** `NotoSans.ttf` is 2 MB, too heavy to
  serve raw. Preferred fix is a self-hosted woff2 subset, which needs
  fonttools installed; alternatives are a Google Fonts link (external
  dependency on a page we otherwise control) or a lookalike system stack.
  Verified 2026-08-18: fonttools is not installed on the build machine, and
  the same decision applies to mono (`NotoSansMono.ttf`, 1.7 MB, the
  runtime's `mono` stack; code blocks are a large share of a docs site).
  Decision: system stacks first (zero cost, no external dependency), font
  parity as a follow-up once subset tooling is settled; the stylesheet does
  not wait for it.

Stage 2 of this track (Pico replacement) is two things, kept separate:

- (a) The classless stylesheet. DONE 2026-08-18: `content/css/site.css`
  (about 350 lines) over the known element inventory from `template.json`
  (h1-h6, p, a, img, `pre > code.hljs`, code, blockquote, strong, em), marked
  defaults for what the rules do not cover (ul/ol/li, table, hr) and the
  landing fragment (section, hgroup, header/nav/main). It sits on a vendored
  `modern-normalize` (3 KB, MIT) for the cross-browser baseline and reads
  only `--srt-*` tokens; the prose factor is `--srt-prose-scale: calc(16 /
  14)`. Landing sections render as cards (surface fill, radius lg, padding
  xl), nav links as ghost buttons, `a[role=button]` as Button primary,
  tables and code on surface fills, hr is spacing only. tokens.css collapsed
  to plain `:root` / `@media (prefers-color-scheme: dark)` /
  `[data-theme=dark]` and Pico plus the mapping-only `theme.css` are gone.
  Verified in headless Chromium in both schemes. Known limit: `hljs.min.css`
  switches on the OS preference only, so a forced `data-theme` does not
  retint code tokens (the site never forces one today).
- (b) The in-section sidebar. DONE 2026-08-18, together with the first
  generated Reference (staging item 3, first cut). `build.ts` now runs two
  passes: collect every page (content `*.md`/`*.html` plus generated pages)
  with its URL and h1 title, then render each with `buildSidebar()`
  (markdown.ts): all pages under the same top-level section, index first,
  each subdirectory a group headed by its index page. A section with a
  single page gets no sidebar, so the other sections look unchanged.
  Templates: `sidebar` / `sidebarItem` / `sidebarGroup` rules in
  template.json, a `{sidebar}` slot in template.html; the layout is
  `<main><aside/><article/></main>`, sticky aside at 14rem, content first
  and the list after it under 40rem.
  Runtime Reference (`src/reference.ts`): one page per flux-types
  declaration file in index.d.ts order (`/runtime/reference/<stem>/`, title
  from `declare module` or the file stem, intro from the file's leading
  comment, body = the .d.ts highlighted; JSDoc is the documentation), plus a
  `/runtime/reference/` index. 27 pages today. A real type extractor can
  replace the bodies later without moving URLs; note the repo's TypeScript
  is `^7` (the Go port), whose programmatic API is not the classic compiler
  API, so ts-morph-style extraction is not a given.
  Core Reference, DONE 2026-08-18, element-centric: `jsx-runtime.d.ts` maps
  each of the 16 intrinsic elements to a composition of prop interfaces, so
  `/core/reference/<element>/` shows the composition line and then every
  interface it composes, transitively through `extends`, each as its own
  highlighted block with its JSDoc (own props first, then transforms,
  pointer, layout, flexbox, grid). Declarations reached by no element (the
  aliases, event types, shader props) go to `/core/reference/types/`. The
  splitter (`splitDeclarations` in reference.ts) is a line scanner over
  top-level `interface`/`type` with the comment block directly above; no
  TypeScript API involved.
  Tools Reference, DONE 2026-08-18: the CLI has no structured command
  descriptor; its single source is the usage text in `printUsage()`
  (`packages/cli/src/args.ts`), so `toolsPages()` reads that template
  literal, takes the "Commands:" table for `/tools/reference/` and gives
  each command a page with its table row and every "<a>/<b> options:" block
  whose heading names it. If the CLI ever grows a command descriptor, the
  generator should read that instead. Components Reference stays deferred.
  Extensions, DONE 2026-08-18: two extensions now (`@solidrt/components`,
  `@solidrt/3d`), each a sidebar group under Extensions. Their source is
  the package READMEs, not types: `/extensions/components/` is the README
  head and `/extensions/components/<widget>/` each `### Widget` section
  (props tables and examples already live there); `/extensions/3d/` is the
  README, then one page per source module `src/index.ts` re-exports from
  (scene, geometry, profile, sweep, material, components, orbit, math),
  showing only the exports index.ts names: JSDoc plus signature for
  functions, full declaration for types. `splitDeclarations` grew
  function/const/let support (signature cut at the body's opening brace)
  for this and is shared with the Core Reference. The Extensions page got a 3D section. Known gap:
  the components README documents 21 widgets while `index.ts` exports 29
  (Select, SegmentedControl, ContextMenu, NavShell, SplitView, Modal,
  Tooltip, Portal are missing from the README), and its relative
  `./AGENTS.md` link does not resolve on the site. Naming settled the same
  day: the section is "Extensions", as in the scaffolder.
  Item 3's first cut is complete: 87 generated pages (Core 18, Extensions
  31, Runtime 28, Tools 10).
  Sidebar layout, revised the same day: the content column and header never
  move for a sidebar; at >= 80rem the aside hangs in the left gutter
  (negative margin, sticky), below that it follows the article.

## Findings from stage 1

- The scaffold command is `srt init` (not `srt create`); templates are
  `default` (uses @solidrt/components, animated logo), `gallery`, `minimal`
  (core-only: window + text). So a bare-Core onboarding template already
  exists as `minimal`; the open question is only whether `default` should
  stay Components-flavored. The Start stub scaffolds `minimal`.
  Superseded 2026-07-26 while writing the content: the public entry point is
  `bun create solidrt <dir>` (create-solidrt forwards to `srt init`), which
  installs dependencies itself, and the run command is `bun run dev`
  (`srt run <entry>`) - bare `srt` only prints usage. Templates are now
  `default` (core level, animated logo), `minimal` (core, blank),
  `components` (blank) and `gallery` (widget tour), so `default` already
  went core-first and that open question is closed.
- Dogfooding gap, FIXED 2026-07-16: `flux:fs` had no mkdir, so the build
  could not create output directories. Added `dir().create()`
  (create_dir_all semantics) across forge/fs + the dir plugin + flux-types
  + docs/flux.md; the build script now mirrors directories itself and the
  Makefile find + mkdir workaround is gone. Also fixed a docs/flux.md
  defect on the way: it showed `dir().list()`, the real method is
  `entries()`.

Follow-up, separate: migrate the three `docs/flux-*-plan.md` docs into
`okf/plans/`.

## Components reference rework (2026-08-19)

The Components pages no longer split the package README: the sources are now
`packages/components/docs/<module>.md` (one prose file per module src/index.ts
re-exports from) plus the typed, commented interfaces in src/, shown as
declarations the flux-types way. `componentsPages()` renders doc + exported
declarations per module (one level of sibling re-export followed), grouped as
Concepts (theme, policy, types, typography, spacing) then Components; page
URLs are the module stems. The package README and the AGENTS.md exports list
are GENERATED from the same doc files by `scripts/build-components-docs.ts`,
whose coverage check (module without doc, doc without module, staleness via
--check) runs in CI and exits non-zero - the hard gate the flux build cannot
be (flux-script-exit-code). The previously recorded README gap (21 widgets
documented, 29 exported) is closed structurally by that check.
