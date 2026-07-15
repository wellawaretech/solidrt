---
type: research-note
title: Channels and lightweight concurrency (goroutine-style, engine-agnostic)
status: open
timestamp: 2026-07-15T00:00:00Z
---

# Channels and lightweight concurrency

JavaScript in flux runs on a single thread. We want a channel mechanism,
ideally goroutine-lightweight, and ideally independent of what executes the
endpoints: JavaScript, Rust tasks, wasm guests, or any future engine.

## Prior art in other languages

Three families:

- **CSP with green threads (Go, Clojure core.async, Kotlin).** Channels as
  first-class values plus `select`, M:N scheduling so blocking on a channel
  is cheap. Clojure core.async is the interesting datapoint: it proves CSP
  works as a *library* on a host that never had it, by compiling `go` blocks
  into state machines. JS `async/await` already is that transform, so a CSP
  layer over promises needs zero engine support.
- **Shared-nothing message passing (Erlang/Elixir, Dart isolates).** Each
  unit of concurrency has its own heap; communication is by copying (or
  transferring) messages through ports/mailboxes. Dart isolates
  (SendPort/ReceivePort) are the closest analog to what we would build. This
  model stays language-agnostic almost for free: the contract is the message
  format, not the memory model.
- **Shared memory with synchronization (Java, Rust, C++).** Threads plus
  locks/atomics. Java's recent contribution is virtual threads (Loom) plus
  structured concurrency (task tree, parent scope cannot outlive children);
  Python's Trio ("nurseries") pioneered that. Worth stealing regardless of
  family, because it fixes the leaked-background-task problem every channel
  system eventually has.

## Where JavaScript stands

- The language is committed to the single-threaded event loop. async/await
  gives cooperative coroutines; channels-as-a-library on top exist (js-csp,
  Effection) and give concurrency without parallelism.
- **Workers** are the official parallelism story and are unloved, but for
  fixable reasons: separate-file ergonomics, untyped postMessage, no shared
  scope, heavy startup in browsers. None are inherent to the model; Dart
  isolates are the same model with good ergonomics.
- **TC39** (as of early 2026): the main active effort is Shared Structs
  (stage 2) - fixed-shape objects in shared memory plus Atomics.Mutex and
  friends. That is shared-memory-with-locks, not channels. Module
  expressions/declarations aim to fix worker ergonomics
  (`new Worker(module { ... })`). Atomics.waitAsync and structuredClone
  shipped. There is no goroutine/channel proposal and there will not be one:
  the committee position is that the event loop is the concurrency model and
  parallelism goes through workers/agents.
- **WHATWG Streams** (ReadableStream/WritableStream) are structurally async
  channels with backpressure and close semantics, and already a web
  standard. Clunky for message passing, but a real precedent.

## Direction for solidrt

The architecture already points at shared-nothing message passing: forge is
the engine-free layer, plugins are thin marshalling, and we already have
non-JS executors (tokio tasks, flux:wasm guests, subprocesses, iroh
streams). Anything tied to the JS heap is ruled out by the
language-agnostic requirement.

**Stage 1: `forge::channel` core.** A bounded async MPSC/MPMC channel
carrying a neutral value type (roughly the structured-clone set: null,
bool, number, string, bytes, list, map - the marshalling toolkit already
has most of this vocabulary). Endpoints are plain Rust handles, so an end
can be held by:

- JS via a `flux:channel` plugin: `send` returns a promise when the buffer
  is full (backpressure), `recv` returns a promise, async iteration for
  free;
- a tokio task (audio, p2p, subprocess stdio could all become channels
  eventually);
- a wasm guest or any future engine.

Useful immediately even with JS still single-threaded, because the other
end does not have to be JS.

**Stage 2: the goroutine part.** Spawn additional QuickJS runtimes on
threads. QuickJS contexts are cheap (a few hundred KB, milliseconds to
start - why txiki.js and LLRT can afford workers), so isolate-per-task is
viable in a way it is not in V8. Worker unpopularity is an ergonomics
problem we control: `spawn(scriptOrModule, channel)` with typed channels
instead of postMessage soup. Shared-nothing also sidesteps QuickJS's
threading constraints (a runtime is single-thread-affine).

## Design decisions to settle early

- **Bounded by default.** Go's unbuffered default is the honest one;
  unbounded queues hide bugs.
- **Explicit `close()`** with well-defined recv-after-close semantics.
- **`select`**: the feature that separates "async queue" from CSP, and the
  hardest to retrofit. Decide even if deferred.
- **Structured spawning** (parent scope owns children): keep in view rather
  than bolting on later.
- **JS surface naming**: a focused `flux:channel` module fits the module
  direction better than contorting into WHATWG Streams or MessagePort; a
  cheap ReadableStream adapter on a receive end can cover interop with
  fetch/Response bodies.
