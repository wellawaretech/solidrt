---
title: Prepared text as columns, breakLines on the text breaker, a byte-bounded scan-resistant word cache
description: Preparing and breaking 600k characters took 1.0 s plus 0.13 s on a Galaxy Tab A7, split between marshalling one JS object per wrap unit and shaping a vocabulary the 8192-word cache could not hold; PreparedText is now typed-array columns, breakLines runs the <text> breaker natively for every line at one width, the word cache is a 16 MiB segmented LRU keyed without paint, and the <text> line box is max ascent plus max descent. An async prepareText was built and dropped.
created: 2026-11-03
completed: 2026-11-03
---

# Prepared text as columns, breakLines, a byte-bounded word cache

## Symptom

App-side line breaking of a long text (600k characters, about 100k wrap
units) on a Galaxy Tab A7 (SM-T500), frame-sliced: `prepareText` about
1.0 s, the `layoutNextLine` loop 0.27 s, and an app-side regex reflow
0.75 s (5.8 ms under bun on a desktop). Nothing said in advance that plain
JS runs at interpreter speed on a phone.

## What was built

- `PreparedText` is columns: `length`, `start`/`end` (`Uint32Array`, JS
  string offsets), `advance`/`width`/`ascent`/`descent` (`Float32Array`),
  `hardBreak`/`glue` (`Uint8Array`), `run` (`Int32Array`, with `runs`
  only), and `carets` as one table (`first`/`offset`/`x`). The per-unit
  `text` copy is gone (`text.slice(start, end)`). flux:font's prepared text
  adds a glyph table the same way. Built in alloy
  (`PreparedColumns::from_units`), marshalled as one typed array per column.
- `breakLines(prepared, width, { textWrap })`: alloy's `break_lines`, the
  `<text>` breaker on a plain column (Balance and Pretty included), over the
  JS columns read in place; lines back as columns with each line's top `y`
  (summed in f64) and `totalHeight`. `layoutNextLine` stays for a width per
  line; both follow one greedy rule (pinned by
  packages/core/tests/text-prepare.test.tsx).
- The shared word cache is a segmented LRU bounded by bytes: a word enters
  probation (12 MiB) and moves to protected (4 MiB) on its first hit;
  protected evicts back to probation. A pass over new words never reaches
  protected, whatever its length. Its key is the shaping half of the run
  style, so paint (a color, a color transition) no longer splits or
  re-shapes entries. A protected hit is one lookup, and the layout path
  reads the metrics in place (`WordCache::metrics`).
- The breaker's line box (`<text>` included) is the highest ascent plus the
  deepest descent. It was the tallest single run, so an inline atom (all
  ascent) beside descending text gave a line box shorter than its content
  and the descenders ran into the next line. `layoutNextLine` already used
  the CSS rule.
- Docs: core AGENTS.md and docs/text.md (columns, breakLines, long texts
  prepared per paragraph); performance.md rule 10 and a new "Where JS work
  stops being cheap" section with the interpreter and device factors.

## Findings

Galaxy Tab A7 (SM-T500), cadence hold off, thermal status 0. Before is the
published Player 0.0.83 (`release-opt`, fat LTO), after a Player.dev of this
change (`release`). 600k characters of cargo-registry license and README
text, 92k units, 14.7k distinct words:

| | 0.0.83 | After |
|---|---|---|
| `prepareText`, first call | 918 ms | 500 ms |
| `prepareText`, again | 741 ms | 97 ms |
| every line at 600 px | 127 ms (`layoutNextLine` loop over objects) | 16 ms (`breakLines`) |

A repetitive text (140 distinct words, 129k units), every word a cache hit
after the first call, so prepareText is the native lookups plus the bridge:

| | 0.0.83 | After |
|---|---|---|
| native only (`measureText`) | 122 ms | 123 ms |
| `prepareText` | 511 ms | 90 ms |

Desktop, release client, power profile `performance`: the license text
244 ms first and 219 ms again before, 96 ms and 24 ms after; breakLines
2.4 ms against a 33 ms loop. The reported 1.0 s matches the license
corpus's 918 ms.

- The bridge was always one call: the cost was inside it, about a million
  property writes building 92k unit objects (some 390 ms of the 511 on the
  tablet's repetitive text). Columns are ten typed arrays.
- A word cache entry costs 650 to 760 bytes (resident growth over 2000
  entries, and the structures summed: a 288-byte lru node of which the key
  was 224 bytes, mostly the paint, 9.5 glyphs of 32 bytes, the strings and
  allocator headers). 8192 entries were some 6 MB; 16 MiB holds about 23k
  words, more with the smaller key.
- The segment ratio decides whether a long text's second prepare hits.
  With a small probation (4 MiB) a word the text uses once fell out before
  the next prepare reached it (tablet 307 ms again); probation is how long
  a word seen once survives, so it is the large segment, and protection
  comes from the split, not from probation being small (pinned by
  alloy/src/tests/text_prepare.rs `a_long_texts_second_prepare_shapes_nothing`).
- The first segmented build measured 22% slower on the native side (the
  same profile on the desktop, 22.0 to 27.1 ms): a protected hit hashed its
  key twice and the layout path cloned the word to read its metrics.
- `layoutNextLine` over columns first measured slower than over objects:
  `unitInk` destructured four properties per unit. Calling it only when the
  next unit is glued fixed it.
- The same plain JS, bun against the runtime's QuickJS on one desktop:
  objects 13x, regexes 22x, a tight arithmetic loop 72x. The tablet is about
  6x the desktop again (the reported reflow, 130x in all).

## Dropped: an async prepareText

A `prepareTextAsync` shaped on tokio's blocking pool over a shared,
thread-safe font set (`Arc<FontSet>`, a `Mutex` in `Face`), through a
private per-call word table, with an in-flight hold and a rejection on a
font reset. It moved the first-call stall (500 ms on the tablet) off the JS
thread, at the price of all that machinery and a second public call that
looked like the same operation and was not: no shared cache, so every call
re-shaped every word, which no editor or re-flowing text can use. For a
prepare an app runs about once (a licenses screen, a document), the
per-paragraph pattern in docs/text.md avoids the long call instead.
