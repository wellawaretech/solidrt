---
title: srt run exits immediately when stdin is not a terminal
description: The repl bound readline close to full shutdown, so any non-interactive launch (background shell, supervisor, CI) tore down the server, the client and the registry record within a second; startRepl now returns early when stdin is not a tty, and the piped-sleep workaround is gone.
created: 2026-08-20
completed: 2026-08-20
---

# srt run exits immediately when stdin is not a terminal

## Symptom

`srt run app.tsx &` - or any launch where stdin is a closed pipe rather than
a terminal - brings up the dev server, prints the usual banner, and is gone
about a second later. `~/.solidrt/servers/34884/live.json` is removed on the
way out, so a moment later nothing distinguishes this from "no server was
ever started". Nothing in the output says why.

The workaround was to hold stdin open artificially:

```
(sleep 100000 | bunx srt run app.tsx > run.log 2>&1 &)
```

which was documented in the root `CLAUDE.md` and carried its own trap - the
matching `pkill -f "sleep 100000"` matches the shell that launched it.

## Mechanism

`startRepl()` (`packages/cli/src/repl.ts`) is called unconditionally by both
`run` and `server`, and does:

```ts
state.rl = createInterface({ input: process.stdin, output: process.stdout, completer })
state.rl.on("close", shutdown)
```

A closed or EOF stdin fires `close` immediately. `shutdown()`
(`packages/cli/src/util.ts`) kills the spawned client, kills the flux server
process and calls `process.exit(0)`; the `process.on("exit")` hook in
`dev-server.ts` removes the live record. So one EOF takes down the whole
session.

The binding is right for an interactive terminal - ctrl-d should quit - and
wrong for every other launch. The bug is that there is no other launch mode.

## Who keeps hitting it

Any launcher that is not a person at a prompt: a background shell, a
supervisor process, a CI step, an editor task, a coding agent driving the
app to verify a change. A GUI console that starts and supervises several
dev servers cannot exist without an answer here: it would have to hold a
pipe open per server, forever.

## Fix

`startRepl()` returns immediately when `process.stdin.isTTY` is false, after
logging "No terminal on stdin, running without the repl". Nothing else was
needed: the spawned server process, its piped stdout reader and the fs
watchers keep the event loop alive on their own, `print`/`printErr` and
`pipeAbovePrompt` already guarded the prompt redraw with `state.rl?.`, and
SIGINT/SIGTERM were already wired to `shutdown()`. Interactive behaviour is
untouched - ctrl-d at an `srt>` prompt still quits.

Verified 2026-08-20 on Linux: `srt server -s 9` and
`srt run examples/hello-world/src/index.tsx -s 9`, both backgrounded with
stdin on /dev/null and no sleep, come up, keep their live record, answer
`/__control__/clients` (client attached in the `run` case) and are still
serving well past the one-second mark that used to kill them. `kill` on the
srt pid tears down server and client and removes the live record.

The `CLAUDE.md` run recipe lost its `sleep 100000` holder.

## Deliberately not done

- **A `--no-repl` flag** for a supervisor that does have a terminal. No
  consumer asked for it; `isTTY` covers every case that was actually
  hurting.
- **Detached mode** - srt starting a server and exiting, leaving it for the
  registry to rediscover. A separate feature, and it costs something:
  rebuild is server-owned (`packages/cli/server/rebuild.ts`, reachable over
  `/__control__/reload`), but the file watcher and the bundle-on-change path
  live in the bun process (`packages/cli/src/watcher.ts`), so a detached
  server would have on-demand reload and no auto-reload unless watching
  moves server-side. Whatever wants to supervise several servers from one
  place is what should force this decision.

## Related

- `okf/backlog/cli-flux-migration.md` and `okf/backlog/stdin-tty-support.md`
  both touch the repl's hosting; a repl that is skipped when absent is
  strictly easier to port than one that is always constructed.
