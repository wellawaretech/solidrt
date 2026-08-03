# Upstream issues

Bugs in our dependencies that we track here: the write-up doubles as the
draft report, and the frontmatter records where the upstream conversation
stands. One markdown file per issue.

## Status lifecycle

- `unfiled` - written up here, nothing reported upstream yet
- `filed` - issue opened upstream; link in frontmatter
- `acknowledged` - upstream confirmed the bug
- `fixed-upstream` - fix merged/released upstream, not yet in our tree
- `resolved` - fix arrived in our dependency tree; local workarounds can come out
- `expected` - upstream closed it as working-as-intended; workaround is permanent
- `wont-fix` - upstream declined; workaround is permanent

When an issue reaches `resolved`, `expected`, or `wont-fix`, note the
outcome in the file and update any code comments that reference it.

## Issues

- [rquickjs: external ArrayBuffer callbacks double-invoked on detach](rquickjs-detach-double-free.md)
  [unfiled] - `JS_DetachArrayBuffer` runs the free callback but leaves it
  set, the finalizer runs it again; rquickjs's shims consume their opaque
  unconditionally, so safe `from_source` + `detach()` (or pure JS
  `transfer(0)` on any Rust-minted buffer) is a double free.
- [quickjs-ng: ArrayBuffer.prototype.transfer mishandles external buffers](quickjs-ng-transfer-external-buffers.md)
  [unfiled] - length-changing `transfer` calls `js_realloc` on a pointer the
  JS allocator does not own (heap corruption); same-length `transfer`
  re-homes the pointer with a NULL opaque, breaking the free-callback
  contract and escaping embedder invalidation. `resize` has the external
  guard; `transfer` forgot it.
