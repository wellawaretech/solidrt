# Ideas

One line each. No frontmatter, no ceremony. Shaped ideas graduate to a file in
`backlog/`; the line stays here with a link so the trail survives.

- Evaluate `changesets` for release management: version bumps, changelogs and dist-tag selection across `@solidrt/*`, replacing the hand-rolled bash in release.yml. Only worth it once release cadence picks up or version drift hurts.
- `srt:dev` registerDebug: an async command's Promise JSON-encodes as `{}` with no warning (async is known-unsupported, okf/done/mcp-debug-commands.md) - either await the promise or reject async commands loudly at registration.
- `instances` on an isolate handle: N instances of one module behind one handle, calls spread over them; shape undecided, a pool is userland today (N `isolate()` calls). Left open when okf/done/isolate-follow-ups.md closed.
- viewBox as layout space: let a viewBox view lay its flex children out at the design size (available space = viewBox), not the real box, so a subtree scales into a smaller box without reflowing; today apps fake it with a fixed inner box (apps/console/SPEC.md)
- Error containment visibility: a native diagnostic flag on a node whose prop or child expression is contained (kept its last value after a throw), so /tree and the inspector show it, plus a dev-only outline. Only if the `Contained error` log line proves insufficient (okf/plans/reactivity-halt-containment.md).
- Focus under `display: "none"`: browsers blur a focused element when an ancestor hides; today a TextInput in a hidden pane keeps focus and keeps receiving keys. Decide whether hiding blurs (and whether re-showing restores), then wire it in the focus session. Left open when okf/done/display-none-subtree.md closed.
