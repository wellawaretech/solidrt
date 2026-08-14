---
title: Client build info in list_clients
description: Git hash, version and profile per connected client in list_clients, so "does this binary have my engine fix" is checkable; build timestamp and HEAD staleness still deferred.
created: 2026-07-15
completed: 2026-07-27
---

Implemented 2026-07-15: lattice/Makefile git describe gained --dirty (so
uncommitted engine changes show as a -dirty version suffix) and the client
info message now carries a `profile` field (debug/release via
cfg!(debug_assertions)), surfaced through list_clients. Still deferred from
the wishlist below: build timestamp (a date env var would invalidate the
cargo cache on every make) and automatic staleness comparison against engine
HEAD (the MCP bridge runs in the app project, not the engine checkout).

# Client build info in list_clients

Motivation (doom, 2026-07-15): an engine fix (alloy update_texture re-render)
was made while a client was already running. There was no way to tell whether
the connected client binary contained the fix - the agent had to describe
symptoms ("if the door only moves while you move, rebuild") instead of
checking. As engine and app iterate together, "which binary is this" comes
up constantly.

Update: `list_clients` already reports a git-describe version per client
(e.g. "v0.0.26-11-g6472da0"), which covers the identity half. Remaining gaps:

- a dirty flag (uncommitted engine changes are exactly the "did my fix make
  it in" case - a clean describe string looks authoritative when it is not)
  and build profile/timestamp.
- staleness surfacing: the MCP layer (or list_clients itself) comparing the
  client's hash against the checked-out engine HEAD and saying so, instead
  of leaving the comparison to whoever reads the string.
