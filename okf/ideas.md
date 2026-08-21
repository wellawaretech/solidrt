# Ideas

One line each. No frontmatter, no ceremony. Shaped ideas graduate to a file in
`backlog/`; the line stays here with a link so the trail survives.

- Evaluate `changesets` for release management: version bumps, changelogs and dist-tag selection across `@solidrt/*`, replacing the hand-rolled bash in release.yml. Only worth it once release cadence picks up or version drift hurts.
- `srt:dev` registerDebug: an async command's Promise JSON-encodes as `{}` with no warning (async is known-unsupported, okf/done/mcp-debug-commands.md) - either await the promise or reject async commands loudly at registration.
- `instances` on an isolate handle: N instances of one module behind one handle, calls spread over them; shape undecided, a pool is userland today (N `isolate()` calls). Left open when okf/done/isolate-follow-ups.md closed.
