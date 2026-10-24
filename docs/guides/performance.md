# Performance

SolidRT is an engine written in Rust, and JavaScript is its scripting
language, as Lua is in a game engine. This is different from Electron and
React Native. There, the app is JavaScript, and the engine serves it.

The engine runs your JavaScript in an interpreter (QuickJS), not in a JIT
compiler. Each prop write goes from JavaScript into the engine. Thus,
JavaScript that runs in each frame is the slow path. The work in the engine
and on the GPU is almost free.

Use JavaScript to coordinate: it sets targets and responds to input. Put
the work of each frame into the engine. This guide shows where each type of
work belongs.

## Let the runtime animate

Do not move elements with a JavaScript loop that writes values in each
frame. Declare the motion with the `transition` prop. The runtime then
calculates each frame in Rust. JavaScript runs only when a target changes,
for all the elements that move.

[Animation](/guides/animation/) shows how to use `transition`, and which
props are fast to animate.

## Move without a reflow: detached elements

A change to a layout prop starts a layout pass. The layout engine
calculates the boxes of the element and its neighbors again. The detached
elements (`d-*`) prevent this. A detached element has no layout box, so a
change to its `x`, `y`, `w` or `h` changes only that element.

Use detached elements for content that moves often, and for shapes that you
draw at coordinates, for example a chart. Use layout elements for the
structure of the screen.

## Keep content that does not change

`repaintBoundary` on a `view` keeps the drawing commands of its subtree. The
runtime uses them again until something in the subtree changes. With
`repaintBoundary="snapshot"`, the runtime also keeps the pixels. Use it for
heavy content that does not change, next to content that changes in each
frame.

## Let the GPU draw

For a continuous effect, for example snow or an animated background, use a
fragment shader. The effect then costs one write in each frame (for
example, the time), however complex the effect is.

- `createShaderTexture` from `@solidrt/core/gpu` makes a texture from a
  shader. A `<texture>` shows it, and its `params` prop sets the uniforms.
- The `shader` prop of a `view` or of the `window` runs a shader over the
  pixels of the view or of the frame.

[Shaders](/core/shaders/) and [GPU](/core/gpu/) describe them.

## Many objects: the spatial core

Each element is a node in the render tree. Detached elements are
satisfactory up to some thousands of objects. For more objects, use the
[2D](/extensions/2d/) and [3D](/extensions/3d/) extensions.

These extensions keep their objects in the spatial core, a native transform
hierarchy in the runtime. A 2D layer draws thousands of sprites in one draw
call. The motion, the animation clips and the hit tests run in Rust.

## Heavy JavaScript: isolates

`flux:isolate` runs a module on its own thread. The work is not faster,
but the UI thread stays free, so the app continues to respond. Use it for
work that takes more than some milliseconds, for example to parse a large
file.

## Calculations: WebAssembly

`flux:wasm` runs WebAssembly modules. A C, C++ or Rust library that has a
WebAssembly build runs without changes, on all platforms.

There are two engines:

- An interpreter on all targets. On calculations, it is 4x to 88x faster
  than JavaScript.
- A compiler (the `wasm-native` feature) on most targets. On calculations,
  it is 18x to 960x faster than JavaScript.

## Your own engine code: Rust

Code that must run at full native speed belongs in Rust. A
[custom runtime](/guides/custom-runtime/) adds your own native module to the
engine. Your JavaScript imports the module and calls it directly. A native
module can also use a native library or a platform SDK.

## Measure

Measure before you change the code, and measure on the slowest device that
must run the app. A fast desktop computer does not show the problems of a
phone or a TV.

- `bun run dev --stats` shows an overlay with the frames per second, the
  memory and the frame times. `bun run client --stats` does the same for
  a second window.
- A coding agent that uses [sol mcp](/tools/) can read the same statistics
  from the running app.
