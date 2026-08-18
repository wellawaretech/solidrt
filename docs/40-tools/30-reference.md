# Reference

Every `srt` command, in the order you meet them. Running `srt` with no
arguments prints the same text this page is built from.

Inside a project that depends on `@solidrt/cli`, invoke it as
`bunx srt <command>` (a scaffolded project's scripts do). Bun is a
development prerequisite only: SolidRT apps run on the bundled `flux`
runtime, not on Bun.

## Starting a project

{{ usage init }}

The picker offers extensions on an interactive terminal. The public entry point
is `bun create solidrt <dir>`, which forwards here and installs dependencies
for you.

## Developing

`run` is the everyday command: it starts the dev server and a local client
together, and it is what `bun run dev` calls in a scaffolded project.

{{ usage run }}

The two halves are separately available, which is how you drive a phone and a
desktop window from one server, or attach a client to a server started
elsewhere.

{{ usage server }}

{{ usage client }}

A session number is the whole multi-instance story: `-s 1` moves the dev
server to port 34885 and gives the client its own data tree, so several
projects run side by side.

## Checking and building

`check` is the gate worth wiring into CI: it builds and typechecks the app
without producing build output.

{{ usage check }}

{{ usage bundle }}

{{ usage pack }}

## Rendering and agents

`render` replays an optional recorded script and writes frames, which is how
an app produces video or deterministic screenshots with no display attached.

{{ usage render }}

`mcp` exposes a running dev server to a coding agent over stdio: its logs,
render tree, live snapshots and input injection.

{{ usage mcp }}
