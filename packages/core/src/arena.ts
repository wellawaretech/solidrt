// The gesture arena: per-pointer ownership arbitration between recognizers.
// Raw pointer events keep bubbling along the frozen down path regardless; the
// arena only decides which recognizer's gesture a pointer belongs to. It lives
// in core beside that routing (window.ts) because arbitration only works with
// exactly ONE claims map per app: recognizers from every package (components'
// press and pan, 3d's orbit transform, future runtime-level recognizers) must
// see each other's claims, and two arenas cannot arbitrate against each other.
//
// Two claim strengths mirror how gestures resolve. A press claims its pointer
// provisionally on the down: it is the presumed winner (innermost-wins falls
// out of leaf-to-root dispatch, first claim sticks) but a later recognizer
// with positive evidence of a different gesture (a pan crossing its movement
// slop) may steal the pointer, cancelling the press. A steal resolves the
// arena: the pointer is won outright and cannot be stolen again, so e.g. the
// outer axis of two nested scrollers cannot take a drag the inner one already
// owns. Plain state on purpose: claims must be visible across recognizers
// within one synchronous bubble dispatch, before any signal flush.

export type ArenaOwner = {
  /** Retract the gesture without firing; invoked when the pointer is stolen. */
  cancel(): void
}

type Claim = { owner: ArenaOwner; resolved: boolean }

let claims = new Map<number, Claim>()

export let arena = {
  /**
   * Provisionally claim an unowned pointer. Returns false (claim refused) when
   * any recognizer already owns it. The claim is stealable until the owner
   * releases it or a steal resolves the arena.
   */
  claim(pointerId: number, owner: ArenaOwner): boolean {
    if (claims.has(pointerId)) return false
    claims.set(pointerId, { owner, resolved: false })
    return true
  },

  /**
   * Take the pointer on positive evidence of a gesture, cancelling the current
   * provisional owner (if any), and resolve the arena: the resulting claim
   * cannot be stolen. Returns false when the arena is already resolved, in
   * which case the caller lost and must stand down.
   */
  steal(pointerId: number, owner: ArenaOwner): boolean {
    let current = claims.get(pointerId)
    if (current) {
      if (current.resolved) return false
      current.owner.cancel()
    }
    claims.set(pointerId, { owner, resolved: true })
    return true
  },

  /** Release the claim on a pointer, if `owner` still holds it. */
  release(pointerId: number, owner: ArenaOwner): void {
    if (claims.get(pointerId)?.owner === owner) claims.delete(pointerId)
  },
}
