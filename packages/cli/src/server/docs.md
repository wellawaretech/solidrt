# srt run and srt server

`run` is the everyday command: it starts the dev server and a local client
window together, and it is what `bun run dev` calls in a scaffolded project.

{{ usage run }}

Run from the project root to serve the project (the entry is
`solidrt.entry` in package.json, default `src/index.tsx`); `srt run <file>`
serves a single file outside a project. In a project root a file argument
is ambiguous, so it takes `--project` (the project, with this entry) or
`--file` (the file alone). Nothing searches upward for a package.json.

The server pushes the bundle to every connected client, so one server can
drive a desktop window and a phone at the same time. A save reaches them on
its own: the server watches every file the running bundle was built from
(the app's modules, the dependencies it bundles in, inlined files,
package.json and tsconfig.json) plus the `assets/` tree, so a file the app
does not import never triggers a rebuild, while an edit to a workspace
package it does import does. A coding agent pauses this while it edits
(`pause_watch`) and pushes with the MCP `reload` tool instead.

`server` is the same without the local client, for clients on other devices
(see [srt client](../client/docs.md)):

{{ usage server }}

One server per project or file. It keeps the port it had last time, else
takes the first free one from 34884 up, and prints it, so projects run side
by side without any numbering; `--port N` pins it. Loopback only unless
`--lan`, which is what phones and other devices need; `--tunnel` accepts
clients over a peer-to-peer connection instead, with no network setup.
`--proxy-http` routes the app's `fetch` calls through the dev server, cached
in `.srt-data/http-cache.db` in the project root (delete the file to
clear), for clients on other devices that need your machine's data.
`-- <args>` hands the app its own arguments (`flux:process` argv) on every
client.

Dev state lives in `~/.solidrt/`: `servers/<key hash>/` holds each server's
registry record, remembered port and tunnel key; `clients/client<N>/` the
data tree of a locally spawned client (`-c <N>` picks it, default 0;
storage is per app inside a tree, so two projects share client 0).
