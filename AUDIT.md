# Audit

What to check when auditing solidrt. One section per rule: the rule, where it
is stated, and how to check it. This file holds the checks only. Findings are
work: file them in `okf/` (`tiny.md` or `backlog/`), not here.

## Flux plugins marshal only

Rule: CLAUDE.md "Plugins should be thin FFI layers", `flux/CLAUDE.md` "Plugin
layering". A plugin decodes JS arguments into native types, calls the owning
crate (forge, ore, alloy), and encodes the result back. Domain logic belongs
in the owning crate, exposed as a method the plugin forwards to.

Scope: `flux/src/standards_plugins/`, `forge_plugins/`, `ore_plugins/`,
`alloy_plugins/`.

How to check:

- Read the diff of the plugin layers since the last audit, then the largest
  plugin files (`wc -l flux/src/*_plugins/*.rs | sort -n`). Logic grows in
  the large files.
- Flag code that computes rather than converts: geometry or pixel math,
  thread loops and pumps, state machines, queues, caches, retry or timing
  policy, tuning constants that are not about the JS surface.
- For each flag, ask: could the owning crate expose this as a method that
  takes and returns native types? If yes, it is a finding.

Not a finding:

- Argument validation and option decoding (defaults, exclusive options,
  error messages naming the API).
- Engine resource bookkeeping: tracking created ids so engine teardown
  releases them, `Persistent` registries, `PendingOps` holds.
- Logic of the JavaScript engine itself, which no other crate can own
  (forge names no engine types): timer and idle-callback scheduling, child
  runtimes for `flux:isolate`.
- One-line adapters from one crate's trait to another crate's handle, where
  flux is the only place both crates meet.
