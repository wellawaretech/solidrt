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
//
// One relation beyond claims: a recognizer that may still take a gesture
// AFTER the pointer lifts (a double-tap waiting for its second tap) pends
// a decision at the down, and a recognizer that resolved at the lift
// defers its firing on that decision (a press beside a double-tap must
// not fire on the first tap, or double-tap-to-zoom also single-taps).
// UIKit's require(toFail:), RNGH's waitFor, Flutter's delayed tap. A node
// with nothing pending fires at once: the wait costs nothing where no
// double-tap is registered.

export type ArenaOwner = {
  /** Retract the gesture without firing; invoked when the pointer is stolen. */
  cancel(): void
}

type Claim = { owner: ArenaOwner; resolved: boolean }
type Pending = { owners: Set<ArenaOwner>; fires: (() => void)[] }

let claims = new Map<number, Claim>()
let pending = new Map<number, Pending>()

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

  /**
   * Register that `owner` may still take the pointer's gesture after its
   * lift, so firings resolved at the lift wait (defer) until `decide`.
   */
  pend(pointerId: number, owner: ArenaOwner): void {
    let p = pending.get(pointerId)
    if (!p) {
      p = { owners: new Set(), fires: [] }
      pending.set(pointerId, p)
    }
    p.owners.add(owner)
  },

  /**
   * Settle a pending decision: `won` drops every deferred firing (the
   * gesture was the pending recognizer's), lost runs them once no other
   * decision is pending on the pointer. Idempotent for an owner that is
   * not pending.
   */
  decide(pointerId: number, owner: ArenaOwner, won: boolean): void {
    let p = pending.get(pointerId)
    if (!p || !p.owners.has(owner)) return
    if (won) {
      pending.delete(pointerId)
      return
    }
    p.owners.delete(owner)
    if (p.owners.size > 0) return
    pending.delete(pointerId)
    for (let fire of p.fires) fire()
  },

  /**
   * Hold a firing until every decision pending on the pointer is settled:
   * run when the last one is lost, dropped when one is won. Returns false,
   * storing nothing, when no decision is pending: the caller fires now.
   */
  defer(pointerId: number, fire: () => void): boolean {
    let p = pending.get(pointerId)
    if (!p) return false
    p.fires.push(fire)
    return true
  },
}
