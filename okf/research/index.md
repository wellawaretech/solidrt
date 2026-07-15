---
type: bundle-index
title: Research
description: Open design research, one file per topic; surveys and direction notes that precede a backlog item or plan.
timestamp: 2026-07-15T00:00:00Z
---

# Research

- [Concurrency mechanisms](channels-concurrency.md) - survey of
  CSP/actor/shared-memory models + JS/TC39 status; conclusion: isolates +
  ports (not general CSP channels) for parallelism, watch/signal for state,
  async iterators for streams; engine-free `forge::Value` is the durable
  first step; hold implementation until a real consumer forces the design.
  Status: open.
