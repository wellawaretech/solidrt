---
title: Reactive SQLite queries
description: A SQLite query as a reactive value - it re-runs when any table it reads is written on this connection, with SQLite's own authorizer and update hook reporting both sides of the dependency graph; forge/flux carry the framework-neutral primitives and @solidrt/core/data is the thin Solid binding.
created: 2026-08-19
completed: 2026-08-19
---

# Reactive SQLite queries

Done (2026-08-19). `forge::sqlite::{read_set, subscribe_writes}`, `flux:sqlite`
`stmt.tables()` / `db.onWrite(cb)`, and `@solidrt/core/data` with
`createQuery` / `createQueryRow` over a plain `flux:sqlite` `Database`.

The contract, in one sentence: **a query re-runs when any table it reads is
written on this connection.** No dependency declarations, no SQL parsing, no
"writes must go through our wrapper" discipline.

## Why SQLite can carry the whole design

Two native hooks make the reactivity automatic and correct:

- **Read-set via the authorizer.** When a statement is compiled, SQLite
  invokes the authorizer for every table (and column) it touches, including
  tables reached through views and subqueries. Capturing names during an
  uncached prepare gives each statement its exact read-set.
- **Write-set via the update hook.** `sqlite3_update_hook` fires on every
  INSERT/UPDATE/DELETE on the connection, with the table name - including
  rows changed by triggers and foreign-key cascades, which wrapper-level
  bookkeeping would miss. The actor flushes the coalesced set after each
  completed command as one "these tables changed" event.

Join the two sets and invalidation is exact at table granularity.

## Layering

Flux is independent from SolidJS and the feature splits along that line.
The actual feature is two framework-neutral primitives on `flux:sqlite`
(`Statement.tables()`, `Database.onWrite(cb)`); any framework, or none,
could build reactivity on them. `@solidrt/core/data` is only the thin Solid
binding: a lazily created version signal per table (module-private WeakMap
keyed by the plain `Database`), an async memo per query. Nothing
Solid-specific leaks downward.

It landed in core as a subpath export (`@solidrt/core/data`, like camera and
sound), not as a separate `@solidrt/data` package: core is already the layer
of reactive bindings over flux runtime modules, and a workspace package was
pure overhead for one file.

```tsx
import { createQuery } from "@solidrt/core/data"

let todos = createQuery(db, "SELECT * FROM todos WHERE done = ?", () => [showDone()])
<For each={todos() ?? []}>{(row) => <Todo row={row} />}</For>

await db.run("INSERT INTO todos (title) VALUES (?)", [title])
// -> update hook reports `todos`, the query re-runs, the UI updates.
```

## Findings from implementation

- **Truncate optimization hole, found by the tests.** SQLite skips the
  update hook for a full-table `DELETE FROM t` (the truncate optimization).
  Fixed at the connection: a permanent authorizer returns Ignore for DELETE,
  which per SQLite's documented contract forces row-by-row deletion so the
  hook fires. The same authorizer doubles as the read-set collector (armed
  only during a ReadSet compile) - exactly one authorizer with two jobs.
- **Read-set is a dedicated command** (`stmt.tables()`, async), compiling
  uncached so the authorizer always sees a compile; the statement cache is
  untouched. `data.ts` memoizes the result - it never changes for a
  statement.
- **onWrite dispatch holds a plain `Function<'js>`, not `Persistent`.** A
  `Persistent` is a permanent GC root; a listener still registered at
  shutdown aborted the runtime (`gc_obj_list` assertion). Each `onWrite`
  gets its own forge subscription and dispatch task holding a refcounted
  `Function` (the serve/websocket pattern); unsubscribe flips a flag and the
  task unwinds on the next event or on close.
- **No `createAsync`.** Solid 2.0 has no such API; async is
  `createMemo(() => promise)` with suspend-on-read semantics. `createQuery`
  returns such a memo: reads surface as pending until the first result
  (wrap in `<Loading>` or default with `?? []`).
- Because the query memo reads the read-set memo before running, the first
  execution already subscribes to its tables - no write can slip between
  subscription and execution.
- Cargo: rusqlite gains the `hooks` feature (authorizer and update hook are
  gated behind it).

## Documented limitations (accepted, not open work)

All carried as doc comments on `subscribe_writes` / `onWrite` and the
module header of `data.ts`:

- **Other connections and other processes are not seen** - the update hook
  is connection-level. Fine for a single-app runtime; it is the documented
  boundary.
- **Table granularity, not row granularity.** Re-running a handful of
  SELECTs per write is cheap; predicate matching is a rabbit hole
  deliberately not entered.
- **WITHOUT ROWID tables do not report** - SQLite's update hook does not
  fire for them at all. Revisit with the preupdate hook (needs a compile
  flag) only if it ever bites.
- **Rollback may falsely invalidate**: the hook fires per row change during
  a transaction, including one that rolls back. A false invalidation is a
  spurious re-read, never a stale one.

## Non-goals unless pulled by real use

Row/predicate invalidation, cross-connection invalidation (polling
`PRAGMA data_version` could cover it), write batching/debounce, a
migrations helper, typed row schemas, optimistic updates.

## Verification

5 forge tests (`forge/src/tests/sqlite.rs`, release): read-set incl. views
and invalid SQL, per-command write coalescing, trigger writes. Plus two
smoke scripts on the standalone flux binary - primitives (7 checks) and the
full Solid layer (5 checks: initial resolve, insert re-run, unrelated-write
no-op, reactive params, full-table delete). A JS-level regression script
lives at `sandbox/smoke-data.ts` (gitignored):
`bun build sandbox/smoke-data.ts --format=esm --external "flux:*" --outfile <out>.js && target/release/flux <out>.js`.

## Why this was worth doing

Highest value-per-line of the candidate extensions: ~300 lines across three
layers for a headline capability, and it demonstrates the runtime's thesis
(reactivity all the way down to storage) better than any UI widget can.
