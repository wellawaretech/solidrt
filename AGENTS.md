# Code style
ASCII characters only. No em-dashes.

## JavaScript and TypeScript
Prefer let over const. Use const only for "real" constants of a single value referred to in ALL_CAPS.

# Dependencies
## SDL
SDL is accessed through the sdl3 Rust crate, which does not expose all SDL functionality. If something is not available in the sdl3 crate, check if it's available in SDL directly, and if so, add a wrapper function in `alloy/src/sdl_utils.rs`.

