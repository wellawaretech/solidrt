# srt mcp

{{ usage mcp }}

Exposes the running app to a coding agent over MCP: logs (source-mapped back
to your TSX), stats (and the on-screen overlay), the live render tree,
screenshots and texture readback, GPU resources, input injection, a
virtual-time transport (`step_frames`, `set_time_scale`), reload and load, a
mute on the user's own input while the agent measures or tests
(`mute_user_input`/`unmute_user_input`), a pause on reload-on-save while it
edits (`pause_watch`/`resume_watch`), and any debug commands the app itself
registers.
A scaffolded project ships an `.mcp.json`, so Claude Code attaches to your
running app with no setup. Other agents keep their server list in their own
file; point it at `bun node_modules/@solidrt/cli/bin/srt mcp`, run from the
project root (agents/debugging.md in this package lists the file per client).

That connection is why SolidRT keeps the app inspectable from outside: an
agent working on your app should be able to look at what it is actually
doing, not just at the source. Every tool is a thin wrapper over the dev
server's HTTP control API, so a shell script or a CI step can do the same
without MCP (agents/debugging.md).
