---
title: A bare string argument to call_debug arrives JSON-quoted
description: Calling a debug command with a bare string argument delivers it to the handler with literal quote characters, so a membership guard rejects it and the command silently no-ops while still returning a plausible payload; the object form is unaffected, and reading the transport end to end does not show where the second encoding is added, so the first move is a live repro.
created: 2026-09-03
completed: 2026-09-03
---

# A bare string argument to call_debug arrives JSON-quoted

## Symptom

A debug command registered with `registerDebug` from `srt:dev` and called
with a bare string:

```
call_debug <name> "arcana"   ->  handler receives  "\"arcana\""
```

The value reaches the handler carrying literal quote characters. A
handler guarding its input (`if (!(name in SETS)) return ...`) rejects
it, does nothing, and still returns a well-formed result, so the caller
sees a successful call that had no effect. A handler without a guard is
worse: it acts on a value that is not the one that was sent.

Passing the same value in object form (`{"name": "arcana"}`) is
unaffected, which is what makes the shape of the bug specific rather than
general argument corruption.

## What reading the code does NOT explain

The transport round-trips a bare string correctly at every hop that can
be checked statically, so the defect is not visible in the source:

- the tool's `args` is `z.any()` and is forwarded untouched
  (`packages/cli/src/mcp/main.ts`, the `call_debug` case);
- the control client serialises it once with `JSON.stringify` into the
  POST body (`control()` in the same file);
- the endpoint parses it once with `await req.json()` and passes the
  result through as `args`
  (`packages/cli/src/server/control.ts`, `/__control__/debug`);
- the runtime's `debug_call` handling parses JSON once more from the
  wire message.

A string that is stringified once and parsed once comes back as itself.
So one of the hops adds an encoding that is not in the path above: the
most likely candidates are the MCP client serialising the argument before
the tool ever sees it (in which case the tool receives an
already-encoded string and is behaving correctly), and the websocket
message assembly on the way to the client.

## Shape

Reproduce first, then fix at the hop that owns it - this is not a
change-the-obvious-line item, and guessing at the fix would put a
compensating unquote somewhere that then breaks a legitimately quoted
string.

Repro is cheap: a scaffolded app registering one debug command that logs
`typeof` and the raw value, called three ways - through the MCP tool,
through `curl -X POST '.../__control__/debug?name=<n>' -d '"arcana"'`,
and with the object form. The two transports disagreeing localises it to
the MCP layer; both agreeing localises it below the control API.

If it turns out the argument is already encoded when the tool receives
it, the fix is on the describing end: `call_debug`'s `args` description
should say the value is passed as JSON and show the bare-string form, so
a caller does not encode it a second time by hand.

## Done looks like

A debug command taking a bare string receives that string, verified
through both the MCP tool and a direct control-API POST, with the object
form still working.

## Findings

Reproduced 2026-09-03 with `probes/debug-args-probe.tsx`, an `echo` command
returning `typeof` and `JSON.stringify` of its argument, served in file mode
on port 34896 and called two ways: a direct control-API POST, and the bridge
(`srt mcp --port 34896`) driven over stdio with the same `tools/call`
message an agent client sends.

| caller | sent | handler received |
|---|---|---|
| POST, body `"arcana"` | JSON string | `string` `arcana` |
| POST, body `{"name":"arcana"}` | JSON object | `object` |
| POST, body `"\"arcana\""` | JSON string with quotes inside | `string` `"arcana"`, quotes kept |
| bridge, `args: "arcana"` | string | `string` `arcana` |
| bridge, `args: "\"arcana\""` | pre-quoted string | `string` `"arcana"`, quotes kept |
| bridge, `args: {name: "arcana"}` | object | `object` |

Every hop is faithful: the bridge, the control endpoint and the runtime each
encode once and decode once, and a string that already carries quote
characters arrives with them, as it must. The second encoding is added by the
caller, above the bridge, before the tool sees the value. What invites it is
the schema: `args` is `z.any()`, which the MCP SDK publishes as a JSON schema
with no `type` at all, so a caller has nothing but the description to go on,
and "any JSON value" reads as "hand over JSON text". On the wire a
pre-encoded string is identical to a legitimately quoted one, so no hop can
unquote it without breaking the latter; the transport stays as it is.

Fix: the `call_debug` tool description and the `args` schema description now
say the value itself is passed and the bridge serializes it, with the
bare-string form spelled out (`arcana`, not `"arcana"`). Verified by reading
`tools/list` from a fresh bridge and calling `echo` with a bare string
through it. The session that did this had no `solidrt` MCP server wired, so
the agent-client leg was covered by the stdio driver rather than by a live
tool call.
