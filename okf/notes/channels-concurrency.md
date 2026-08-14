---
title: Concurrency mechanisms
description: Survey of CSP, actor and shared-memory models plus TC39 status; the conclusion is isolates and ports for parallelism, with engine-free forge::Value as the durable first step.
created: 2026-07-15
---

# Concurrency mechanisms

JavaScript in flux runs on a single thread. The original question was whether
to add goroutine-style channels, ideally independent of what executes the
endpoints (JavaScript, Rust tasks, wasm guests, any future engine). The
conclusion after working it through: general CSP channels are not the
mechanism solidrt most needs. Recommendation below; survey kept for
reference.

## Recommendation

Match the mechanism to the three async needs a UI runtime actually has,
which are not a server's needs (Go channels shine with thousands of peer
tasks coordinating; a solidrt app does not look like that):

- **State that changes over time** (pose, sensor values, connection
  status): latest-value-wins with coalescing - a watch/signal, not a
  queue. Queues add lag for state-like data; the PointerMove coalescing
  fix was exactly removing queue semantics in favor of latest-value.
  Solid signals are already the app-level answer here.
- **Event/byte streams** (subprocess stdout, WS messages, mic buffers):
  async iteration with backpressure, which JS expresses natively and the
  flux modules already trend toward. A named Channel object adds nothing
  over `for await (let chunk of proc.stdout)`.
- **Compute off the JS thread**: the one real, unsolved gap - a big JSON
  parse or a game-logic tick stalls rendering. The minimal mechanism is
  **spawn + ports** (Dart isolates, actor mailboxes): each spawned
  QuickJS runtime gets an inbox, messages are neutral values copied
  across, shared-nothing. Ports need about three design decisions; CSP
  channels need eight, and `select` is the expensive one.

CSP channels generalize ports, so nothing is lost by starting narrower.
They stay on the table only if experience with ports proves a need for
N:M wiring and select.

**Hold implementation until a concrete consumer shows up** that genuinely
wants JS-level parallelism (a game tick loop, a protocol handler), and
let that consumer force the design. Caveats that temper the value:
QuickJS isolates un-block the UI thread but do not make compute fast
(QuickJS stays slow), and the runtime's existing answer to heavy compute
is "push it into Rust or wasm", which is a good answer.

## Durable assets (worthwhile regardless of mechanism)

- **A neutral value type.** An engine-free `forge::Value` (roughly the
  structured-clone set: null, bool, number, string, bytes, list, map) is
  required for ports, for wasm guests, for any future engine - and the
  marshalling toolkit keeps re-deriving that vocabulary ad hoc. If there
  is a first implementation step that cannot be wasted, it is this.
- **The event loop blends perfectly with message passing** as long as no
  data is shared. An awaited port/channel `recv` is Go's blocking receive
  with the state-machine transform done by the compiler; Go needs its own
  scheduler because it has preemption and shared memory, and JS gave up
  both. Precisely: across runtimes the neutral-value copy enforces no
  sharing (the blend is perfect); within one runtime tasks do share
  closures/module scope - data-race-free, but state can change across any
  `await` point (interleaving hazards remain), and a task that never
  awaits starves the loop. That last limit is the actual argument for
  isolates.

## Recorded pitfalls

- **`Promise.race` is not `select`.** If `recv()` returns a plain
  promise, racing looks like select for free but leaks values: the losing
  branches' `recv` calls already committed, and their messages resolve to
  nobody. Real select needs the channel to know it is being raced
  (poll/peek or native multi-recv). Do not "simplify" to Promise.race.
- **Queue semantics for state-like data** reintroduce the unbounded-lag
  bug class that PointerMove coalescing fixed.
- **Worker unpopularity is ergonomics, not the model.** Separate-file
  setup, untyped postMessage, no shared scope, heavy startup in browsers.
  None are inherent: Dart isolates are the same model with good
  ergonomics, and QuickJS contexts are cheap (a few hundred KB,
  milliseconds to start - why txiki.js and LLRT can afford workers), so
  isolate-per-task is viable in a way it is not in V8. Shared-nothing
  also sidesteps QuickJS's single-thread-affine runtime constraint.
  `spawn(scriptOrModule, port)` with typed ports fixes the soup.

## Survey: prior art in other languages

Three families:

- **CSP with green threads (Go, Clojure core.async, Kotlin).** Channels
  as first-class values plus `select`, M:N scheduling so blocking on a
  channel is cheap. Clojure core.async proves CSP works as a *library* on
  a host that never had it, by compiling `go` blocks into state machines;
  JS `async/await` already is that transform.
- **Shared-nothing message passing (Erlang/Elixir, Dart isolates).** Each
  unit of concurrency has its own heap; communication copies (or
  transfers) messages through ports/mailboxes. Dart isolates
  (SendPort/ReceivePort) are the closest analog to the recommendation
  above. The contract is the message format, not the memory model, which
  is what keeps it language-agnostic.
- **Shared memory with synchronization (Java, Rust, C++).** Threads plus
  locks/atomics. Java's virtual threads (Loom) add structured concurrency
  (task tree, parent scope cannot outlive children); Python's Trio
  ("nurseries") pioneered that. Worth stealing for spawn regardless of
  family: it fixes the leaked-background-task problem.

## Survey: where JavaScript stands

- The language is committed to the single-threaded event loop. async/await
  gives cooperative coroutines; channels-as-a-library exist (js-csp,
  Effection) and give concurrency without parallelism.
- **TC39** (as of early 2026): Shared Structs (stage 2) is the active
  effort - fixed-shape objects in shared memory plus Atomics.Mutex, i.e.
  shared-memory-with-locks, not channels. Module expressions/declarations
  aim to fix worker ergonomics. There is no goroutine/channel proposal
  and there will not be one: the committee position is that the event
  loop is the concurrency model and parallelism goes through
  workers/agents.
- **WHATWG Streams** (ReadableStream/WritableStream) are structurally
  async channels with backpressure and close semantics. Clunky for
  message passing; a cheap ReadableStream adapter on a receive end could
  cover interop with fetch/Response bodies if ports ever need it.
