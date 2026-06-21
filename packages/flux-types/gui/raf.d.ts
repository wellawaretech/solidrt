// Animation-frame scheduling globals (gui-enabled runtime only).

declare global {
  /**
   * Request that `callback` run before the next rendered frame, receiving the
   * frame timestamp in milliseconds. Returns an id for
   * {@link cancelAnimationFrame}. Available only on a gui-enabled runtime.
   */
  function requestAnimationFrame(callback: (timestamp: number) => void): number
  /** Cancel a frame callback scheduled with {@link requestAnimationFrame}. */
  function cancelAnimationFrame(id: number): void
}

export {}