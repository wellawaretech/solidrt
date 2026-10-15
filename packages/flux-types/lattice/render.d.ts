// Frame draw (lattice runner), present in every build. renderFrame()
// synchronously renders the current frame: layout, the postLayout hook, paint
// and hover refresh, then builds and submits the display list. To schedule a
// future frame instead, use requestFrame() from "flux:rendertree". The
// tree-building surface itself is "flux:rendertree" (gui/rendertree.d.ts).
declare module "sol:render" {
  export function renderFrame(): void
}
