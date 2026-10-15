// UI event bus (lattice), provided by the runtime as a builtin module, present
// in every lattice build. on/once return an unsubscribe function. Notable
// events: the routed pointer stream ("pointerMove"/"pointerDown"/... consumed
// by core's window.ts), "pointerFrame" (the move-batch terminator: fires after
// all of a frame's pointer moves have dispatched, every pointer the same age -
// multi-pointer recognizers measure there; its `timeStamp` is theirs), and
// "render" (the per-frame signal).
declare module "sol:events" {
  export function on(event: string, callback: (data: any) => void): () => void
  export function once(event: string, callback: (data: any) => void): () => void
}
