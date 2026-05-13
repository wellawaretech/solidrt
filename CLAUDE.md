# Code style
ASCII characters only. No em-dashes.

## JavaScript and TypeScript
Prefer `let` over `const`. Use `const` only for "real" constants of a single value referred to in ALL_CAPS.

## Rust
Never only use `.unwrap()`; use `.expect(..)` or `.unwrap_or(..)` or something similar to explicitly handle the scenario where the result is not Ok.

# Dependencies
## SDL
SDL is accessed through the sdl3 Rust crate, which does not expose all SDL functionality. If something is not available in the sdl3 crate, check if it's available in SDL directly, and if so, add a wrapper function in `alloy/src/sdl_utils.rs`.

# Projects
## Rust
- `alloy` combines SDL, Impeller, wgpu
- `crystal` adds structure to Alloy, providing a render tree with Taffy integration
- `flux` embeds a JavaScript runtime built on QuickJS
- `packages/core/lattice` combines Alloy, Crystal and Flux, providing commands to access rendering from JavaScript

## JavaScript
- `packages/core` SolidRT core, linking SolidJS and Lattice
- `packages/cli` SolidRT command-line developer tooling 

# General
If you get a prompt which asks to implement something, but there's a non-trivial reason why that is not easy, then point this out and ask for feedback how to continue.

Always ask for user confirmation of your plan before starting to implement.

If you get a question without asking for an implementation, then just answer the question instead of implementing anything.

If you do not know something, or if instructions are ambiguous, then explicitly say so and ask for feedback.