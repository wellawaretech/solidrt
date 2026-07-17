---
type: plan
title: Documentation website (structure, generator, content strategy)
status: active
timestamp: 2026-07-16T00:00:00Z
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

**Nav:** `Start - Core - Frameworks - Tools - Runtime - Architecture`

(News is deferred: blog + changelog get added as a top-level section later,
not in the initial site.)

- **Start** (new since the original nav): the one journey-shaped item in an
  otherwise layer-shaped nav. A single page, not a section: install,
  scaffold, run, change one line, see it update. Five minutes, one scroll.
  Ends with the fork: go deeper (Core Concepts) or go faster (Frameworks).
  Hold the line at "first successful edit"; everything past that belongs in
  section Guides.
- **Core**: the stable spine. Sub-structure: Concepts first (mental model),
  then Guides, then Examples, plus Reference. Concept-vs-Guide test: if
  removing the page costs transferable understanding it is a Concept; if it
  costs one specific task it is a Guide.
- **Frameworks**: things built on Core, siblings not a stack. Plain card
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
break, so it builds on the stable layer; with more frameworks coming, any
framework in Start reads as an endorsement, and teaching the shared
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
- Examples pages from `examples/` source. Upgrade path: `srt playback`
  deterministic frame capture + captureSnapshot can generate real
  screenshots/recordings of every example at build time.
- CLI reference from command definitions / help output.
- Changelog from releases (deferred with News).

**Existing `docs/` content is outdated** and gets rewritten fresh as website
content; `docs/internals/` may partially survive as the Architecture
section.

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
   workspace. All pages are stubs marked as such.
2. **Examples generator**: pages from `examples/` (cheapest generator,
   highest visibility).
3. **API reference generation** from types (its own project, especially
   type extraction).
4. **Screenshot/recording capture** for example pages via playback.

## Findings from stage 1

- The scaffold command is `srt init` (not `srt create`); templates are
  `default` (uses @solidrt/components, animated logo), `gallery`, `minimal`
  (core-only: window + text). So a bare-Core onboarding template already
  exists as `minimal`; the open question is only whether `default` should
  stay Components-flavored. The Start stub scaffolds `minimal`.
- Dogfooding gap, FIXED 2026-07-16: `flux:fs` had no mkdir, so the build
  could not create output directories. Added `dir().create()`
  (create_dir_all semantics) across forge/fs + the dir plugin + flux-types
  + docs/flux.md; the build script now mirrors directories itself and the
  Makefile find + mkdir workaround is gone. Also fixed a docs/flux.md
  defect on the way: it showed `dir().list()`, the real method is
  `entries()`.

Follow-up, separate: migrate the three `docs/flux-*-plan.md` docs into
`okf/plans/`.
