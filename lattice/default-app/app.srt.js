// node_modules/.bun/@solidjs+signals@2.0.0-beta.17/node_modules/@solidjs/signals/dist/prod.js
class NotReadyError extends Error {
  source;
  constructor(e) {
    super();
    this.source = e;
  }
}

class StatusError extends Error {
  source;
  constructor(e, t) {
    super(t instanceof Error ? t.message : String(t), { cause: t });
    this.source = e;
  }
}
function unwrapStatusError(e) {
  return e instanceof StatusError ? e.cause : e;
}
var REACTIVE_NONE = 0;
var REACTIVE_CHECK = 1 << 0;
var REACTIVE_DIRTY = 1 << 1;
var REACTIVE_RECOMPUTING_DEPS = 1 << 2;
var REACTIVE_IN_HEAP = 1 << 3;
var REACTIVE_IN_HEAP_HEIGHT = 1 << 4;
var REACTIVE_ZOMBIE = 1 << 5;
var REACTIVE_DISPOSED = 1 << 6;
var REACTIVE_OPTIMISTIC_DIRTY = 1 << 7;
var REACTIVE_SNAPSHOT_STALE = 1 << 8;
var REACTIVE_LAZY = 1 << 9;
var REACTIVE_MANUAL_WRITE = 1 << 10;
var CONFIG_OWNED_WRITE = 1 << 0;
var CONFIG_NO_SNAPSHOT = 1 << 1;
var CONFIG_TRANSPARENT = 1 << 2;
var CONFIG_IN_SNAPSHOT_SCOPE = 1 << 3;
var CONFIG_CHILDREN_FORBIDDEN = 1 << 4;
var CONFIG_AUTO_DISPOSE = 1 << 5;
var CONFIG_SYNC = 1 << 6;
var STATUS_PENDING = 1 << 0;
var STATUS_ERROR = 1 << 1;
var STATUS_UNINITIALIZED = 1 << 2;
var EFFECT_RENDER = 1;
var EFFECT_USER = 2;
var EFFECT_TRACKED = 3;
var NOT_PENDING = {};
var NO_SNAPSHOT = {};
var SUPPORTS_PROXY = typeof Proxy === "function";
var defaultContext = {};
var $REFRESH = Symbol("refresh");
typeof globalThis !== "undefined" && globalThis.process?.env?.COMPANION_CENSUS;
var signalLanes = new WeakMap;
var activeLanes = new Set;
function getOrCreateLane(e) {
  let t = signalLanes.get(e);
  if (t) {
    return findLane(t);
  }
  const n = e.t;
  const i = n?.i ? findLane(n.i) : null;
  t = { o: e, u: new Set, l: [[], []], T: null, S: activeTransition, _: i };
  signalLanes.set(e, t);
  activeLanes.add(t);
  return t;
}
function findLane(e) {
  while (e.T)
    e = e.T;
  return e;
}
function mergeLanes(e, t) {
  e = findLane(e);
  t = findLane(t);
  if (e === t)
    return e;
  t.T = e;
  for (const n of t.u)
    e.u.add(n);
  t.u.clear();
  e.l[0].push(...t.l[0]);
  e.l[1].push(...t.l[1]);
  t.l[0].length = 0;
  t.l[1].length = 0;
  return e;
}
function resolveLane(e) {
  const t = e.i;
  if (!t)
    return;
  const n = findLane(t);
  if (activeLanes.has(n))
    return n;
  e.i = undefined;
  return;
}
function resolveTransition(e) {
  return resolveLane(e)?.S ?? e.S;
}
function hasActiveOverride(e) {
  return !!(e.O !== undefined && e.O !== NOT_PENDING);
}
function assignOrMergeLane(e, t) {
  const n = findLane(t);
  const i = e.i;
  if (i) {
    if (i.T) {
      e.i = t;
      return;
    }
    const r = findLane(i);
    if (activeLanes.has(r)) {
      if (r !== n && !hasActiveOverride(e)) {
        if (n._ && findLane(n._) === r) {
          e.i = t;
        } else if (r._ && findLane(r._) === n)
          ;
        else
          mergeLanes(n, r);
      }
      return;
    }
  }
  e.i = t;
}
var transitions = new Set;
var dirtyQueue = { R: new Array(2000).fill(undefined), p: false, I: 0, h: 0 };
var zombieQueue = { R: new Array(2000).fill(undefined), p: false, I: 0, h: 0 };
var clock = 0;
var activeTransition = null;
var scheduled = false;
var halted = false;
var haltNotified = false;
var syncDepth = 0;
var projectionWriteActive = false;
var stashedOptimisticReads = null;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(e) {
  return transitions.size === 0 && activeLanes.size === 0 && e.A.length === 0 && e.N.length === 0 && e.C.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.P !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.D !== NOT_PENDING)
      continue;
    if (e.O !== undefined && e.O !== NOT_PENDING)
      continue;
    transientStoreNodes.delete(e);
    e.m?.();
  }
}
function shouldReadStashedOptimisticValue(e) {
  return !!stashedOptimisticReads?.has(e);
}
function runLaneEffects(e) {
  for (const t of activeLanes) {
    if (t.T || t.u.size > 0)
      continue;
    const n = t.l[e - 1];
    if (n.length) {
      t.l[e - 1] = [];
      runQueue(n, e);
    }
  }
}
function queueStashedOptimisticEffects(e) {
  for (let t = e.P;t !== null; t = t.L) {
    const e2 = t.U;
    if (!e2.V)
      continue;
    if (e2.V === EFFECT_TRACKED) {
      if (!e2.G) {
        e2.G = true;
        e2.F.enqueue(EFFECT_USER, e2.k);
      }
      continue;
    }
    const n = e2.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (n.I > e2.H)
      n.I = e2.H;
    insertIntoHeap(e2, n);
  }
}
function mergeTransitionState(e, t) {
  t.M = e;
  e.j.push(...t.j);
  for (const n of activeLanes)
    if (n.S === t)
      n.S = e;
  e.N.push(...t.N);
  for (const n of t.C)
    e.C.add(n);
  for (const [n, i] of t.$) {
    let t2 = e.$.get(n);
    if (!t2)
      e.$.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
  for (const n of t.K)
    e.K.add(n);
}
function resolveOptimisticNodes(e) {
  const t = e.length;
  for (let n = 0;n < t; n++) {
    const t2 = e[n];
    t2.i = undefined;
    if (!(t2.Y & STATUS_PENDING))
      t2.Y &= ~STATUS_UNINITIALIZED;
    const i = t2.O;
    t2.O = NOT_PENDING;
    if (i !== NOT_PENDING && t2.Z !== i)
      insertSubs(t2, true);
    t2.S = null;
  }
  for (let n = 0;n < t; n++) {
    const t2 = e[n];
    if (t2.B || t2.q)
      snapCompanionsToState(t2);
    const i = t2.t;
    if (i && (i.B === t2 || i.q === t2))
      snapCompanionsToState(i);
  }
  e.splice(0, t);
}
function cleanupCompletedLanes(e) {
  for (const t of activeLanes) {
    const n = e ? t.S === e : !t.S;
    if (!n)
      continue;
    if (!t.T) {
      if (t.l[0].length)
        runQueue(t.l[0], EFFECT_RENDER);
      if (t.l[1].length)
        runQueue(t.l[1], EFFECT_USER);
    }
    if (t.o.i === t)
      t.o.i = undefined;
    t.u.clear();
    t.l[0].length = 0;
    t.l[1].length = 0;
    activeLanes.delete(t);
    signalLanes.delete(t.o);
  }
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.X && !projectionWriteActive)
    queueMicrotask(flush);
}
function haltReactivity() {
  if (halted)
    return;
  halted = true;
  let e = "[REACTIVITY_HALTED] An uncaught error halted the reactive system.";
  console.error(e);
}
function notifyHalted() {
  if (haltNotified)
    return;
  haltNotified = true;
  console.error("[REACTIVITY_HALTED] Update ignored.");
}
class Queue {
  J = null;
  ee = [[], []];
  A = [];
  created = clock;
  addChild(e) {
    this.A.push(e);
    e.J = this;
  }
  removeChild(e) {
    const t = this.A.indexOf(e);
    if (t >= 0) {
      this.A.splice(t, 1);
      e.J = null;
    }
  }
  notify(e, t, n, i) {
    if (this.J)
      return this.J.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.ee[e - 1].length) {
      const t = this.ee[e - 1];
      this.ee[e - 1] = [];
      runQueue(t, e);
    }
    for (let t = 0;t < this.A.length; t++)
      this.A[t].run?.(e);
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const n = findLane(currentOptimisticLane);
        n.l[e - 1].push(t);
      } else {
        this.ee[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.ee[0].push(...this.ee[0]);
    e.ee[1].push(...this.ee[1]);
    this.ee = [[], []];
    for (let t = 0;t < this.A.length; t++) {
      let n = this.A[t];
      let i = e.A[t];
      if (!i) {
        i = { ee: [[], []], A: [] };
        e.A[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.ee[0].push(...e.ee[0]);
    this.ee[1].push(...e.ee[1]);
    for (let t = 0;t < e.A.length; t++) {
      const n = e.A[t];
      let i = this.A[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue extends Queue {
  X = false;
  te = null;
  ne = [];
  N = [];
  C = new Set;
  static ie;
  static re;
  static oe;
  static se = null;
  flush() {
    if (this.X)
      return;
    this.X = true;
    try {
      if (false)
        ;
      runHeap(dirtyQueue, GlobalQueue.ie);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, GlobalQueue.ie);
          this.te = null;
          this.ne = [];
          this.N = [];
          this.C = new Set;
          runLaneEffects(EFFECT_RENDER);
          runLaneEffects(EFFECT_USER);
          this.stashQueues(e2.ue);
          clock++;
          scheduled = dirtyQueue.h >= dirtyQueue.I;
          reassignPendingTransition(e2.ne);
          activeTransition = null;
          if (!e2.j.length && !e2.$.size && e2.N.length) {
            stashedOptimisticReads = new Set;
            for (let t2 = 0;t2 < e2.N.length; t2++) {
              const n = e2.N[t2];
              if (n.ce || n.le & CONFIG_OWNED_WRITE)
                continue;
              stashedOptimisticReads.add(n);
              queueStashedOptimisticEffects(n);
            }
          }
          try {
            finalizePureQueue(null, true);
          } finally {
            stashedOptimisticReads = null;
          }
          return;
        }
        this.ne !== activeTransition.ne && this.ne.push(...activeTransition.ne);
        this.restoreQueues(activeTransition.ue);
        transitions.delete(activeTransition);
        const t = activeTransition;
        activeTransition = null;
        reassignPendingTransition(this.ne);
        finalizePureQueue(t);
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.h >= dirtyQueue.I) {
            runHeap(dirtyQueue, GlobalQueue.ie);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.ie);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.h >= dirtyQueue.I;
      activeLanes.size && runLaneEffects(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && runLaneEffects(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
      if (false)
        ;
      if (false)
        ;
    } finally {
      this.X = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING) {
      if (n & STATUS_PENDING) {
        const t2 = i !== undefined ? i : e.ae;
        if (activeTransition && t2) {
          const n2 = t2.source;
          let i2 = activeTransition.$.get(n2);
          if (!i2)
            activeTransition.$.set(n2, i2 = new Set);
          const r = i2.size;
          i2.add(e);
          if (i2.size !== r)
            schedule();
        }
      }
      return true;
    }
    return false;
  }
  initTransition(e) {
    if (e)
      e = currentTransition(e);
    if (e && e === activeTransition)
      return;
    if (!e && activeTransition && activeTransition.fe === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? {
        fe: clock,
        ne: [],
        $: new Map,
        N: [],
        C: new Set,
        j: [],
        ue: { ee: [[], []], A: [] },
        M: false,
        K: new Set
      };
    } else if (e) {
      const t = activeTransition;
      mergeTransitionState(e, t);
      transitions.delete(t);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.fe = clock;
    if (this.te !== null) {
      this.te.S = activeTransition;
      activeTransition.ne.push(this.te);
      this.te = null;
    }
    if (this.ne !== activeTransition.ne) {
      for (let e2 = 0;e2 < this.ne.length; e2++) {
        const t = this.ne[e2];
        t.S = activeTransition;
        activeTransition.ne.push(t);
      }
      this.ne = activeTransition.ne;
    }
    if (this.N !== activeTransition.N) {
      for (let e2 = 0;e2 < this.N.length; e2++) {
        const t = this.N[e2];
        t.S = activeTransition;
        activeTransition.N.push(t);
      }
      this.N = activeTransition.N;
    }
    for (const e2 of activeLanes) {
      if (!e2.S)
        e2.S = activeTransition;
    }
    if (this.C !== activeTransition.C) {
      for (const e2 of this.C)
        activeTransition.C.add(e2);
      this.C = activeTransition.C;
    }
  }
}
function queuePendingNode(e) {
  if (activeTransition) {
    globalQueue.ne.push(e);
    return;
  }
  if (globalQueue.te === null && globalQueue.ne.length === 0) {
    globalQueue.te = e;
    return;
  }
  if (globalQueue.te !== null) {
    globalQueue.ne.push(globalQueue.te);
    globalQueue.te = null;
  }
  globalQueue.ne.push(e);
}
function insertSubs(e, t = false) {
  const n = e.i || currentOptimisticLane;
  const i = e.Ee !== undefined;
  for (let r = e.P;r !== null; r = r.L) {
    if (i && r.U.le & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.U.W |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && n) {
      r.U.W |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.U, n);
    } else if (t) {
      r.U.W |= REACTIVE_OPTIMISTIC_DIRTY;
      r.U.i = undefined;
    }
    const e2 = r.U;
    if (e2.V === EFFECT_TRACKED) {
      if (!e2.G) {
        e2.G = true;
        e2.F.enqueue(EFFECT_USER, e2.k);
      }
      continue;
    }
    const o = r.U.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (o.I > r.U.H)
      o.I = r.U.H;
    insertIntoHeap(r.U, o);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.ce) {
    if (e.D !== NOT_PENDING) {
      e.Z = e.D;
      e.D = NOT_PENDING;
    }
    if (e.B || e.q)
      snapCompanionsToState(e);
    return;
  }
  if (e.D !== NOT_PENDING) {
    e.Z = e.D;
    e.D = NOT_PENDING;
    if (e.V && e.V !== EFFECT_TRACKED)
      e.G = true;
  }
  t.W &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.Y & STATUS_PENDING))
    t.Y &= ~STATUS_UNINITIALIZED;
  if (t.de !== null || t.Te !== null)
    GlobalQueue.re(t, false, true);
  if (e.B || e.q)
    snapCompanionsToState(e);
}
function commitPendingNodes() {
  if (globalQueue.te !== null) {
    commitPendingNode(globalQueue.te);
    globalQueue.te = null;
  }
  const e = globalQueue.ne;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
}
function finalizePureQueue(e = null, t = false) {
  const n = !t;
  if (n)
    commitPendingNodes();
  if (!t && globalQueue.A.length)
    checkBoundaryChildren(globalQueue);
  const i = dirtyQueue.h >= dirtyQueue.I;
  if (i)
    runHeap(dirtyQueue, GlobalQueue.ie);
  if (n) {
    if (i)
      commitPendingNodes();
    resolveOptimisticNodes(e ? e.N : globalQueue.N);
    if (e && e.K.size) {
      for (const t3 of e.K) {
        if (t3.W & REACTIVE_DISPOSED)
          continue;
        if (t3.V === EFFECT_TRACKED) {
          if (!t3.G) {
            t3.G = true;
            t3.F.enqueue(EFFECT_USER, t3.k);
          }
          continue;
        }
        const e2 = t3.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
        if (e2.I > t3.H)
          e2.I = t3.H;
        insertIntoHeap(t3, e2);
      }
      e.K.clear();
    }
    const t2 = e ? e.C : globalQueue.C;
    if (GlobalQueue.se && t2.size) {
      for (const e2 of t2) {
        GlobalQueue.se(e2);
      }
      t2.clear();
      schedule();
    }
    sweepTransientStoreNodes();
    cleanupCompletedLanes(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.A) {
    t.checkSources?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].S = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
function flush(e) {
  if (e) {
    syncDepth++;
    try {
      return e();
    } finally {
      try {
        flush();
      } finally {
        syncDepth--;
      }
    }
  }
  if (globalQueue.X) {
    return;
  }
  if (halted)
    return;
  while (scheduled || activeTransition) {
    globalQueue.flush();
  }
}
function runQueue(e, t) {
  for (let n = 0;n < e.length; n++)
    e[n](t);
}
function reporterBlocksSource(e, t) {
  if (e.W & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.Se === t || e._e?.has(t))
    return true;
  for (let n = e.Oe;n; n = n.Re) {
    let e2 = n.pe;
    while (e2) {
      if (e2 === t || e2.Ie === t)
        return true;
      e2 = e2.t;
    }
  }
  return !!(e.Y & STATUS_PENDING && e.ae instanceof NotReadyError && e.ae.source === t);
}
function transitionComplete(e) {
  if (e.M)
    return true;
  if (e.j.length)
    return false;
  let t = true;
  for (const [n, i] of e.$) {
    let r = false;
    for (const e2 of i) {
      if (reporterBlocksSource(e2, n)) {
        r = true;
        break;
      }
      i.delete(e2);
    }
    if (!r)
      e.$.delete(n);
    else if (n.Y & STATUS_PENDING && n.ae?.source === n) {
      t = false;
      break;
    }
  }
  if (t) {
    for (let n = 0;n < e.N.length; n++) {
      const i = e.N[n];
      if (hasActiveOverride(i) && "Y" in i && i.Y & STATUS_PENDING && i.ae instanceof NotReadyError) {
        t = false;
        break;
      }
    }
  }
  t && (e.M = true);
  return t;
}
function currentTransition(e) {
  while (e.M && typeof e.M === "object")
    e = e.M;
  return e;
}
function runInTransition(e, t) {
  const n = activeTransition;
  try {
    activeTransition = currentTransition(e);
    return t();
  } finally {
    activeTransition = n;
  }
}
function actualInsertIntoHeap(e, t) {
  const n = (e.J?.he ? e.J.Ae?.H : e.J?.H) ?? -1;
  if (n >= e.H)
    e.H = n + 1;
  const i = e.H;
  const r = t.R[i];
  if (r === undefined)
    t.R[i] = e;
  else {
    const t2 = r.Ne;
    t2.Ce = e;
    e.Ne = t2;
    r.Ne = e;
  }
  if (i > t.h)
    t.h = i;
}
function insertIntoHeap(e, t) {
  let n = e.W;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (n & REACTIVE_CHECK) {
    e.W = n & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else
    e.W = n | REACTIVE_IN_HEAP;
  if (!(n & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, t);
}
function insertIntoHeapHeight(e, t) {
  let n = e.W;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.W = n | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, t);
}
function deleteFromHeap(e, t) {
  const n = e.W;
  if (!(n & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.W = n & -25;
  const i = e.H;
  if (e.Ne === e)
    t.R[i] = undefined;
  else {
    const n2 = e.Ce;
    const r = t.R[i];
    const o = n2 ?? r;
    if (e === r)
      t.R[i] = n2;
    else
      e.Ne.Ce = n2;
    o.Ne = e.Ne;
  }
  e.Ne = e;
  e.Ce = undefined;
}
function markHeap(e) {
  if (e.p)
    return;
  e.p = true;
  for (let t = 0;t <= e.h; t++) {
    for (let n = e.R[t];n !== undefined; n = n.Ce) {
      if (n.W & REACTIVE_IN_HEAP)
        markNode(n);
    }
  }
}
function markNode(e, t = REACTIVE_DIRTY) {
  const n = e.W;
  if ((n & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= t)
    return;
  e.W = n & -4 | t;
  for (let t2 = e.P;t2 !== null; t2 = t2.L) {
    markNode(t2.U, REACTIVE_CHECK);
  }
  if (e.ye !== null) {
    for (let t2 = e.ye;t2 !== null; t2 = t2.Pe) {
      for (let e2 = t2.P;e2 !== null; e2 = e2.L) {
        markNode(e2.U, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, t) {
  e.p = false;
  for (e.I = 0;e.I <= e.h; e.I++) {
    let n = e.R[e.I];
    while (n !== undefined) {
      if (n.W & REACTIVE_IN_HEAP)
        t(n);
      else
        adjustHeight(n, e);
      n = e.R[e.I];
    }
  }
  e.h = 0;
}
function adjustHeight(e, t) {
  deleteFromHeap(e, t);
  let n = e.H;
  for (let t2 = e.Oe;t2; t2 = t2.Re) {
    const e2 = t2.pe;
    const i = e2.Ie || e2;
    if (i.ce && i.H >= n)
      n = i.H + 1;
  }
  if (e.H !== n) {
    e.H = n;
    for (let t2 = e.P;t2 !== null; t2 = t2.L) {
      insertIntoHeapHeight(t2.U, t2.U.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    }
  }
}
function markDisposal(e) {
  let t = e.ge;
  while (t) {
    const e2 = t.W;
    t.W = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(t, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(t, zombieQueue);
      else
        insertIntoHeapHeight(t, zombieQueue);
    }
    markDisposal(t);
    t = t.De;
  }
}
function disposeChildren(e, t = false, n) {
  const i = e.W;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t) {
    e.W = i | REACTIVE_DISPOSED;
    const t2 = e;
    if (t2.B || t2.q)
      snapCompanionsToState(t2);
  }
  if (t && e.ce)
    e.be = null;
  let r = n ? e.de : e.ge;
  while (r) {
    const e2 = r.De;
    if (r.Oe) {
      const e3 = r;
      deleteFromHeap(e3, e3.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      let t2 = e3.Oe;
      do {
        t2 = unlinkSubs(t2);
      } while (t2 !== null);
      e3.Oe = null;
      e3.me = null;
    }
    disposeChildren(r, true);
    r = e2;
  }
  if (n) {
    e.de = null;
  } else {
    e.ge = null;
    e.ve = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e.J !== null && !(e.J.W & REACTIVE_DISPOSED)) {
    const t2 = e.we;
    const n2 = e.De;
    if (t2 !== null)
      t2.De = n2;
    else
      e.J.ge = n2;
    if (n2 !== null)
      n2.we = t2;
    e.we = null;
  }
  runDisposal(e, n);
  if (t && e.Le) {
    const t2 = e.Le;
    e.Le = undefined;
    t2();
  }
}
function runDisposal(e, t) {
  let n = t ? e.Te : e.Ue;
  if (!n)
    return;
  if (Array.isArray(n)) {
    for (let e2 = 0;e2 < n.length; e2++) {
      const t2 = n[e2];
      t2.call(t2);
    }
  } else {
    n.call(n);
  }
  t ? e.Te = null : e.Ue = null;
}
function childId(e, t) {
  let n = e;
  while (n.le & CONFIG_TRANSPARENT && n.J)
    n = n.J;
  if (n.id != null)
    return formatId(n.id, t ? n.ve++ : n.ve);
  throw new Error("Cannot get child id from owner without an id");
}
function getNextChildId(e) {
  return childId(e, true);
}
function formatId(e, t) {
  const n = t.toString(36), i = n.length - 1;
  return e + (i ? String.fromCharCode(64 + i) : "") + n;
}
function getOwner() {
  return context;
}
function cleanup(e) {
  if (!context)
    return e;
  if (!context.Ue)
    context.Ue = e;
  else if (Array.isArray(context.Ue))
    context.Ue.push(e);
  else
    context.Ue = [context.Ue, e];
  return e;
}
function disposeRootSelf(e = true) {
  disposeChildren(this, e);
}
function createOwner(e) {
  const t = context;
  const n = e?.transparent ?? false;
  const i = {
    id: e?.id ?? (n ? t?.id : t?.id != null ? getNextChildId(t) : undefined),
    le: n ? CONFIG_TRANSPARENT : 0,
    he: true,
    Ae: t?.he ? t.Ae : t,
    ge: null,
    De: null,
    we: null,
    Ue: null,
    F: t?.F ?? globalQueue,
    Ve: t?.Ve || defaultContext,
    ve: 0,
    Te: null,
    de: null,
    J: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.ge;
    if (e2 === null) {
      t.ge = i;
    } else {
      i.De = e2;
      e2.we = i;
      t.ge = i;
    }
  }
  return i;
}
function createRoot(e, t) {
  const n = createOwner(t);
  return runWithOwner(n, () => e(() => n.dispose()));
}
function unlinkSubs(e) {
  const t = e.pe;
  const n = e.Re;
  const i = e.L;
  const r = e.Ge;
  if (i !== null)
    i.Ge = r;
  else
    t.Fe = r;
  if (r !== null)
    r.L = i;
  else {
    t.P = i;
    if (i === null) {
      t.m?.();
      const e2 = t;
      e2.ce && e2.le & CONFIG_AUTO_DISPOSE && !(e2.W & REACTIVE_ZOMBIE) && unobserved(e2);
    }
  }
  return n;
}
function trimStaleDeps(e) {
  const t = e.me;
  let n = t !== null ? t.Re : e.Oe;
  if (n !== null) {
    do {
      n = unlinkSubs(n);
    } while (n !== null);
    if (t !== null)
      t.Re = null;
    else
      e.Oe = null;
  }
}
function unobserved(e) {
  deleteFromHeap(e, e.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  let t = e.Oe;
  while (t !== null) {
    t = unlinkSubs(t);
  }
  e.Oe = null;
  e.me = null;
  disposeChildren(e, true);
}
function link(e, t, n = false) {
  const i = t.me;
  if (i !== null && i.pe === e) {
    i.ke = n;
    return;
  }
  let r = null;
  const o = t.W & REACTIVE_RECOMPUTING_DEPS;
  if (o) {
    r = i !== null ? i.Re : t.Oe;
    if (r !== null && r.pe === e) {
      r.We = t.He;
      t.me = r;
      r.ke = n;
      return;
    }
  }
  const s = e.Fe;
  if (s !== null && s.U === t && (!o || s.We === t.He)) {
    s.ke = n;
    return;
  }
  const u = t.me = e.Fe = { pe: e, U: t, Re: r, Ge: s, L: null, We: t.He, ke: n };
  if (i !== null)
    i.Re = u;
  else
    t.Oe = u;
  if (s !== null)
    s.L = u;
  else
    e.P = u;
}
function addPendingSource(e, t) {
  if (e.Se === t || e._e?.has(t))
    return false;
  if (!e.Se) {
    e.Se = t;
    return true;
  }
  if (!e._e) {
    e._e = new Set([e.Se, t]);
  } else {
    e._e.add(t);
  }
  e.Se = undefined;
  return true;
}
function removePendingSource(e, t) {
  if (e.Se) {
    if (e.Se !== t)
      return false;
    e.Se = undefined;
    return true;
  }
  if (!e._e?.delete(t))
    return false;
  if (e._e.size === 1) {
    e.Se = e._e.values().next().value;
    e._e = undefined;
  } else if (e._e.size === 0) {
    e._e = undefined;
  }
  return true;
}
function clearPendingSources(e) {
  e.Se = undefined;
  e._e?.clear();
  e._e = undefined;
}
function setPendingError(e, t, n) {
  if (!t) {
    e.ae = null;
    return;
  }
  if (n instanceof NotReadyError && n.source === t) {
    e.ae = n;
    return;
  }
  const i = e.ae;
  if (!(i instanceof NotReadyError) || i.source !== t) {
    e.ae = new NotReadyError(t);
  }
}
function forEachDependent(e, t) {
  for (let n = e.P;n !== null; n = n.L)
    t(n.U, n);
  for (let n = e.ye;n !== null; n = n.Pe) {
    for (let e2 = n.P;e2 !== null; e2 = e2.L)
      t(e2.U, e2);
  }
}
function enqueueForRerun(e) {
  if (e.V === EFFECT_TRACKED) {
    const t = e;
    if (!t.G) {
      t.G = true;
      t.F.enqueue(EFFECT_USER, t.k);
    }
  } else {
    const t = e.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (t.I > e.H)
      t.I = e.H;
    insertIntoHeap(e, t);
  }
}
function settlePendingSource(e) {
  let t = false;
  const n = new Set;
  const settle = (i) => {
    if (n.has(i) || !removePendingSource(i, e))
      return;
    n.add(i);
    i.fe = clock;
    const r = i.Se ?? i._e?.values().next().value;
    if (r) {
      setPendingError(i, r);
      updatePendingSignal(i);
    } else {
      i.Y &= ~STATUS_PENDING;
      setPendingError(i);
      updatePendingSignal(i);
      if (i.Me) {
        enqueueForRerun(i);
        t = true;
      }
      i.Me = false;
    }
    forEachDependent(i, settle);
  };
  forEachDependent(e, settle);
  if (t)
    schedule();
}
function isThenable(e) {
  return e != null && typeof e === "object" && typeof e.then === "function";
}
function handleAsync(e, t, n) {
  let i = false;
  let r = false;
  if (typeof t === "object" && t !== null) {
    untrack(() => {
      i = t[Symbol.asyncIterator];
      r = !i && isThenable(t);
    });
  }
  if (!r && !i) {
    e.be = null;
    return t;
  }
  e.be = t;
  let o;
  const handleError = (n2) => {
    if (e.be !== t)
      return;
    globalQueue.initTransition(resolveTransition(e));
    notifyStatus(e, n2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, n2);
    e.fe = clock;
  };
  const asyncWrite = (i2, r2) => {
    if (e.be !== t)
      return;
    if (e.W & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    globalQueue.initTransition(resolveTransition(e));
    const o2 = !!(e.Y & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const s = resolveLane(e);
    if (s)
      s.u.delete(e);
    if (n) {
      n(i2);
      if (o2)
        clearStatus(e, true);
    } else if (e.O !== undefined) {
      if (e.D === NOT_PENDING)
        queuePendingNode(e);
      e.D = i2;
      syncCompanions(e, i2);
      if (!hasActiveOverride(e))
        insertSubs(e);
      e.fe = clock;
    } else if (s) {
      const t2 = e.V;
      const n2 = e.Z;
      const r3 = e.xe;
      try {
        if (!t2 && o2 || !r3 || !r3(i2, n2)) {
          e.Z = i2;
          e.fe = clock;
          syncCompanions(e, i2);
          insertSubs(e, true);
        }
      } catch (t3) {
        notifyStatus(e, STATUS_ERROR, t3);
      }
    } else {
      try {
        setSignal(e, () => i2);
      } catch (t2) {
        notifyStatus(e, STATUS_ERROR, t2);
      }
    }
    settlePendingSource(e);
    schedule();
    flush();
    r2?.();
  };
  if (r) {
    let n2 = false, i2 = false, r2, s = true;
    t.then((e2) => {
      if (s) {
        o = e2;
        n2 = true;
      } else
        asyncWrite(e2);
    }, (e2) => {
      if (s) {
        r2 = e2;
        i2 = true;
      } else
        handleError(e2);
    });
    s = false;
    if (i2) {
      handleError(r2);
      throw r2;
    } else if (!n2) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  if (i) {
    const n2 = t[Symbol.asyncIterator]();
    let i2 = false;
    let r2 = false;
    let s = true;
    cleanup(() => {
      if (r2)
        return;
      r2 = true;
      try {
        const e2 = n2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    });
    const iterate = () => {
      let u2, c, l = false, a = false, f = true;
      n2.next().then((n3) => {
        if (f) {
          u2 = n3;
          l = true;
          if (n3.done)
            r2 = true;
        } else if (e.be !== t) {
          return;
        } else if (!n3.done) {
          i2 = true;
          asyncWrite(n3.value, iterate);
        } else {
          r2 = true;
          if (i2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
        }
      }, (n3) => {
        if (f) {
          c = n3;
          a = true;
        } else if (e.be === t) {
          r2 = true;
          handleError(n3);
        }
      });
      f = false;
      if (a) {
        r2 = true;
        handleError(c);
        if (s)
          throw c;
        return true;
      }
      if (l && !u2.done) {
        o = u2.value;
        i2 = true;
        return iterate();
      }
      return l && u2.done;
    };
    const u = iterate();
    s = false;
    if (!i2 && !u) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  return o;
}
function clearStatus(e, t = false) {
  if (e.Se || e._e)
    clearPendingSources(e);
  if (e.Me)
    e.Me = false;
  e.Y = t ? 0 : e.Y & STATUS_UNINITIALIZED;
  if (e.ae)
    setPendingError(e);
  if (e.B || e.q)
    updatePendingSignal(e);
  if (e.ye)
    updateChildCompanions(e);
  if (e.Qe)
    e.Qe();
}
function notifyStatus(e, t, n, i, r) {
  if (t === STATUS_ERROR && !(n instanceof StatusError) && !(n instanceof NotReadyError))
    n = new StatusError(e, n);
  const o = t === STATUS_PENDING && n instanceof NotReadyError ? n.source : undefined;
  const s = o === e;
  const u = t === STATUS_PENDING && e.O !== undefined && !s;
  const c = u && hasActiveOverride(e);
  if (!i) {
    if (t === STATUS_PENDING && o) {
      addPendingSource(e, o);
      e.Y = STATUS_PENDING | e.Y & STATUS_UNINITIALIZED;
      setPendingError(e, o, n);
    } else {
      clearPendingSources(e);
      e.Y = t | (t !== STATUS_ERROR ? e.Y & STATUS_UNINITIALIZED : 0);
      e.ae = n;
    }
    updatePendingSignal(e);
    if (e.ye)
      updateChildCompanions(e);
  }
  if (r && !i) {
    assignOrMergeLane(e, r);
  }
  const l = i || c;
  const a = i || u ? undefined : r;
  if (e.Qe) {
    if (i && t === STATUS_PENDING) {
      return;
    }
    if (l) {
      e.Qe(t, n);
    } else {
      e.Qe();
    }
    return;
  }
  forEachDependent(e, (e2, i2) => {
    e2.fe = clock;
    if (t === STATUS_PENDING && o && e2.Se !== o && !e2._e?.has(o) || t !== STATUS_PENDING && (e2.ae !== n || e2.Se || e2._e)) {
      if (i2.ke && t !== STATUS_PENDING && !(n instanceof NotReadyError)) {
        enqueueForRerun(e2);
        schedule();
        return;
      }
      if (!l && !e2.S)
        queuePendingNode(e2);
      notifyStatus(e2, t, n, l, a);
    }
  });
}
var externalSourceConfig = null;
GlobalQueue.ie = recompute;
GlobalQueue.re = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var pendingProbe = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.je)
      return true;
    e = e.J;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.V;
  if (!t) {
    if (e.S && (!n || activeTransition) && activeTransition !== e.S)
      globalQueue.initTransition(e.S);
    deleteFromHeap(e, e.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    e.be = null;
    if (e.S || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ge !== null || e.Ue !== null) {
      markDisposal(e);
      e.Te = e.Ue;
      e.de = e.ge;
      e.Ue = null;
      e.ge = null;
      e.ve = 0;
    }
  }
  let i = !!(e.W & REACTIVE_OPTIMISTIC_DIRTY);
  const r = e.O !== undefined && e.O !== NOT_PENDING;
  e.Y & STATUS_PENDING;
  const o = !!(e.Y & STATUS_UNINITIALIZED);
  const s = context;
  context = e;
  e.me = null;
  e.He++;
  e.W = REACTIVE_RECOMPUTING_DEPS;
  e.fe = clock;
  let u = e.D === NOT_PENDING ? e.Z : e.D;
  let c = e.H;
  let l = tracking;
  let a = currentOptimisticLane;
  tracking = true;
  if (i) {
    const t2 = resolveLane(e);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.N.length) {
    for (let t2 = e.Oe;t2; t2 = t2.Re) {
      const n2 = t2.pe;
      if (n2.W & REACTIVE_OPTIMISTIC_DIRTY) {
        const t3 = resolveLane(n2);
        if (t3) {
          i = true;
          currentOptimisticLane = t3;
          e.W |= REACTIVE_OPTIMISTIC_DIRTY;
          assignOrMergeLane(e, t3);
          break;
        }
      }
    }
  }
  const f = n && n !== EFFECT_USER;
  const E = stale;
  if (f)
    stale = true;
  try {
    if (e.le & CONFIG_SYNC) {
      u = e.ce(u);
      e.be = null;
    } else {
      const t2 = e.be;
      const n2 = e.ce(u);
      const i2 = typeof n2 === "object" && n2 !== null;
      const r2 = e.be !== t2;
      u = r2 || !i2 ? n2 : handleAsync(e, n2);
      if (!r2 && !i2)
        e.be = null;
    }
    clearStatus(e, t);
    if (e.i) {
      const t2 = resolveLane(e);
      if (t2) {
        t2.u.delete(e);
        updatePendingSignal(t2.o);
      }
    }
  } catch (t2) {
    if (t2 instanceof NotReadyError && currentOptimisticLane) {
      const t3 = findLane(currentOptimisticLane);
      if (t3.o !== e) {
        t3.u.add(e);
        e.i = t3;
        updatePendingSignal(t3.o);
      }
    }
    if (t2 instanceof NotReadyError)
      e.Me = true;
    notifyStatus(e, t2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, t2, undefined, t2 instanceof NotReadyError ? e.i : undefined);
  } finally {
    tracking = l;
    if (f)
      stale = E;
    e.W = REACTIVE_NONE | (t ? e.W & REACTIVE_SNAPSHOT_STALE : 0);
    context = s;
  }
  if (!e.ae) {
    trimStaleDeps(e);
    const s2 = r ? e.O : e.D === NOT_PENDING ? e.Z : e.D;
    let l2 = false;
    try {
      l2 = !n && o || !e.xe || !e.xe(s2, u);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && l2) {
      e.G = !e.ae;
      if (!t)
        e.F.enqueue(n, e.$e ??= GlobalQueue.oe.bind(null, e));
    }
    if (e.ae)
      ;
    else if (l2) {
      const o2 = r ? e.O : undefined;
      if (t || n && activeTransition !== e.S || i) {
        e.Z = u;
        if (r && i) {
          e.O = u;
          e.D = NOT_PENDING;
        }
      } else {
        e.D = u;
        if (activeTransition || e.S)
          syncCompanions(e, u);
      }
      if (!r || i || e.O !== o2)
        insertSubs(e, i || r);
    } else if (r) {
      if (e.D === NOT_PENDING)
        queuePendingNode(e);
      e.D = u;
    } else if (e.H != c) {
      for (let t2 = e.P;t2 !== null; t2 = t2.L) {
        insertIntoHeapHeight(t2.U, t2.U.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      }
    }
  }
  currentOptimisticLane = a;
  const d = e.D !== NOT_PENDING || e.de !== null || e.Te !== null || !!(e.Y & (STATUS_PENDING | STATUS_UNINITIALIZED));
  d && (!t || e.Y & STATUS_PENDING) && (!e.S || r) && queuePendingNode(e);
  e.S && n && activeTransition !== e.S && runInTransition(e.S, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.W & REACTIVE_CHECK) {
    for (let t = e.Oe;t; t = t.Re) {
      const n = t.pe;
      const i = n.Ie || n;
      if (i.ce) {
        updateIfNecessary(i);
      }
      if (e.W & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.W & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.ae && e.fe < clock && !e.be) {
    recompute(e);
  }
  e.W = e.W & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = {
    id: t?.id ?? (n ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    le: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.Ke ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    xe: t?.equals != null ? t.equals : isEqual,
    m: t?.unobserved,
    Ue: null,
    F: context?.F ?? globalQueue,
    Ve: context?.Ve ?? defaultContext,
    ve: 0,
    ce: e,
    Z: undefined,
    H: 0,
    ye: null,
    Ce: undefined,
    Ne: null,
    Oe: null,
    me: null,
    He: 0,
    P: null,
    Fe: null,
    J: context,
    De: null,
    we: null,
    ge: null,
    W: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    Y: STATUS_UNINITIALIZED,
    fe: clock,
    D: NOT_PENDING,
    Te: null,
    de: null,
    be: null,
    S: null
  };
  setupComputedNode(i, t);
  return i;
}
function createEffectNode(e, t, n, i, r, o) {
  const s = o?.transparent ?? false;
  const u = {
    id: o?.id ?? (s ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    le: (s ? CONFIG_TRANSPARENT : 0) | (o?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (o?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    xe: false,
    m: o?.unobserved,
    Ue: null,
    F: context?.F ?? globalQueue,
    Ve: context?.Ve ?? defaultContext,
    ve: 0,
    ce: e,
    Z: undefined,
    H: 0,
    ye: null,
    Ce: undefined,
    Ne: null,
    Oe: null,
    me: null,
    He: 0,
    P: null,
    Fe: null,
    J: context,
    De: null,
    we: null,
    ge: null,
    W: REACTIVE_LAZY,
    Y: STATUS_UNINITIALIZED,
    fe: clock,
    D: NOT_PENDING,
    Te: null,
    de: null,
    be: null,
    S: null,
    G: false,
    Ye: undefined,
    Ze: t,
    Be: n,
    Le: undefined,
    V: i,
    Qe: r
  };
  setupComputedNode(u, lazyOptions);
  return u;
}
var lazyOptions = { lazy: true };
function setupComputedNode(e, t) {
  e.Ne = e;
  const n = context?.he ? context.Ae : context;
  if (context) {
    const t2 = context.ge;
    if (t2 === null) {
      context.ge = e;
    } else {
      e.De = t2;
      t2.we = e;
      context.ge = e;
    }
  }
  if (n)
    e.H = n.H + 1;
  if (externalSourceConfig) {
    const t2 = signal(undefined, { equals: false, ownedWrite: true });
    const n2 = externalSourceConfig.factory(e.ce, () => {
      setSignal(t2, undefined);
    });
    cleanup(() => n2.dispose());
    e.ce = (e2) => {
      read(t2);
      return n2.track(e2);
    };
  }
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.Y & STATUS_PENDING) && !(e.le & CONFIG_NO_SNAPSHOT)) {
      e.Ee = e.Z === undefined ? NO_SNAPSHOT : e.Z;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    xe: t?.equals != null ? t.equals : isEqual,
    le: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.Ke ? CONFIG_NO_SNAPSHOT : 0),
    m: t?.unobserved,
    Z: e,
    P: null,
    Fe: null,
    fe: clock,
    Ie: n,
    Pe: n?.ye || null,
    D: NOT_PENDING
  };
  n && (n.ye = i);
  if (snapshotCaptureActive && !(i.le & CONFIG_NO_SNAPSHOT) && !((n?.Y ?? 0) & STATUS_PENDING)) {
    i.Ee = e === undefined ? NO_SNAPSHOT : e;
    snapshotSources.add(i);
  }
  return i;
}
function optimisticComputed(e, t) {
  const n = computed(e, t);
  n.O = NOT_PENDING;
  return n;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (!externalSourceConfig && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (externalSourceConfig)
      return externalSourceConfig.untrack(e);
    return e();
  } finally {
    tracking = n;
  }
}
function prepareComputed(e, t) {
  if (e.W & REACTIVE_LAZY) {
    e.W &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.W & REACTIVE_DISPOSED) {
    recompute(e, true);
  } else if (t) {
    updateIfNecessary(e);
  }
}
function read(e) {
  if (latestReadActive) {
    const t2 = getLatestValueComputed(e);
    const n2 = latestReadActive;
    latestReadActive = false;
    const i2 = e.O !== undefined && e.O !== NOT_PENDING ? e.O : e.Z;
    let r2;
    try {
      r2 = read(t2);
    } catch (t3) {
      if (t3 instanceof NotReadyError && (!context || !(e.Y & STATUS_UNINITIALIZED)))
        return i2;
      throw t3;
    } finally {
      latestReadActive = n2;
    }
    if (t2.Y & STATUS_PENDING)
      return i2;
    if (stale && currentOptimisticLane && t2.i) {
      const e2 = findLane(t2.i);
      const n3 = findLane(currentOptimisticLane);
      if (e2 !== n3 && e2.u.size > 0) {
        return i2;
      }
    }
    return r2;
  }
  let t = context;
  if (t?.he)
    t = t.Ae;
  const n = e;
  const i = e.Ie;
  const r = i || e;
  if (pendingCheckActive) {
    pendingCheckActive = false;
    if (typeof n.ce === "function")
      prepareComputed(e, true);
    if (t && r.Y & STATUS_PENDING && r.Y & STATUS_UNINITIALIZED) {
      if (tracking && e !== t)
        link(e, t);
      pendingCheckActive = true;
      throw r.ae;
    }
    collectPendingSources(e);
    if (i)
      collectPendingSources(i);
    pendingCheckActive = true;
  } else if (typeof n.ce === "function") {
    prepareComputed(e, false);
  }
  if (!n.ce && r === e && e.O === undefined && e.Ee === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.D === NOT_PENDING ? e.Z : e.D;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (r.ce) {
      const n2 = e.W & REACTIVE_ZOMBIE;
      if (r.H >= (n2 ? zombieQueue.I : dirtyQueue.I)) {
        markNode(t);
        markHeap(n2 ? zombieQueue : dirtyQueue);
        updateIfNecessary(r);
      }
      const i2 = r.H;
      if (i2 >= t.H && e.J !== t) {
        t.H = i2 + 1;
      }
    }
  }
  if (r.Y & STATUS_PENDING) {
    if (t && !(stale && r.S && activeTransition !== r.S)) {
      if (currentOptimisticLane) {
        const n2 = r.i;
        const i2 = findLane(currentOptimisticLane);
        if (n2 && findLane(n2) === i2 && !hasActiveOverride(r)) {
          if (!tracking && e !== t)
            link(e, t);
          throw r.ae;
        }
      } else {
        if (!tracking && e !== t)
          link(e, t);
        throw r.ae;
      }
    } else if (t && r !== e && r.Y & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw r.ae;
    } else if (!t && r.Y & STATUS_UNINITIALIZED) {
      throw r.ae;
    }
  }
  if (e.ce && e.Y & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && e.fe < clock) {
      recompute(e);
      return read(e);
    } else
      throw e.ae;
  }
  if (snapshotCaptureActive && t && t.le & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.Ee;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const r2 = e.D !== NOT_PENDING ? e.D : e.Z;
      if (r2 !== i2)
        t.W |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.O !== undefined && e.O !== NOT_PENDING) {
    if (t && stale && shouldReadStashedOptimisticValue(e))
      return e.Z;
    return e.O;
  }
  if (activeTransition !== null && currentOptimisticLane !== null && !latestReadActive && e.D !== NOT_PENDING && (r === e || !!(r.W & REACTIVE_MANUAL_WRITE)) && !e.ce && t) {
    activeTransition.K.add(t);
    return e.Z;
  }
  const o = !t || currentOptimisticLane !== null && (e.O !== undefined || e.i || r === e && stale && t.t !== e || !!(r.Y & STATUS_PENDING)) || e.D === NOT_PENDING || stale && e.S && activeTransition !== e.S ? e.Z : e.D;
  if (pendingCheckActive && pendingProbe !== null && e.D !== NOT_PENDING && o === e.D)
    pendingProbe.freshReads.add(e);
  if (!t && r === e && typeof n.ce === "function" && e.le & CONFIG_AUTO_DISPOSE && !(r.Y & STATUS_PENDING) && !e.P) {
    unobserved(e);
  }
  return o;
}
function setSignal(e, t) {
  if (e.S && activeTransition !== e.S)
    globalQueue.initTransition(e.S);
  const n = e.O !== undefined && !projectionWriteActive;
  const i = e.O !== undefined && e.O !== NOT_PENDING;
  const r = n ? i ? e.O : e.Z : e.D === NOT_PENDING ? e.Z : e.D;
  if (typeof t === "function")
    t = t(r);
  const o = !!(e.Y & STATUS_UNINITIALIZED) || !e.xe || !e.xe(r, t);
  if (!o) {
    if (n && i) {
      const t2 = resolveTransition(e);
      if (t2 && activeTransition !== t2)
        globalQueue.initTransition(t2);
    }
    return t;
  }
  if (n) {
    const n2 = e.O === NOT_PENDING;
    if (!n2)
      globalQueue.initTransition(resolveTransition(e));
    if (n2)
      globalQueue.N.push(e);
    const i2 = getOrCreateLane(e);
    e.i = i2;
    e.O = t;
  } else {
    if (e.D === NOT_PENDING)
      queuePendingNode(e);
    e.D = t;
  }
  syncCompanions(e, t);
  e.fe = clock;
  insertSubs(e, n);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, e.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  if (!(e.W & REACTIVE_MANUAL_WRITE) && e.D === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.W = e.W & -4 | REACTIVE_MANUAL_WRITE;
}
function setMemo(e, t) {
  const n = setSignal(e, t);
  suppressComputedRecompute(e);
  return n;
}
function runWithOwner(e, t) {
  const n = context;
  const i = tracking;
  context = e;
  tracking = false;
  try {
    return t();
  } finally {
    context = n;
    tracking = i;
  }
}
function collectPendingSources(e) {
  if (!pendingProbe)
    return;
  pendingProbe.sources.add(e);
  const t = e.Ie || e;
  if (t !== e)
    pendingProbe.sources.add(t);
}
function computePendingState(e) {
  const t = e;
  if (t.W & REACTIVE_DISPOSED)
    return false;
  const n = e.Ie;
  if ((n || t).qe)
    return false;
  if (e.t) {
    const t2 = e.t;
    if (hasActiveOverride(t2))
      return false;
    const n2 = t2.Ie || t2;
    if (n2.qe)
      return false;
    return !!(n2.Y & STATUS_PENDING && !(n2.Y & STATUS_UNINITIALIZED));
  }
  if (hasActiveOverride(e))
    return false;
  if (n && e.D !== NOT_PENDING) {
    return !!(n.W & REACTIVE_MANUAL_WRITE) || !n.be && !(n.Y & STATUS_PENDING);
  }
  if (e.D !== NOT_PENDING && !(t.Y & STATUS_UNINITIALIZED))
    return true;
  return !!(t.Y & STATUS_PENDING && !(t.Y & STATUS_UNINITIALIZED));
}
function syncCompanions(e, t) {
  if (e.B)
    updatePendingSignal(e);
  if (e.q)
    setSignal(e.q, t);
}
function updatePendingSignal(e) {
  if (e.B) {
    setSignal(e.B, computePendingState(e));
  }
  if (e.q)
    updatePendingSignal(e.q);
}
function updateChildCompanions(e) {
  for (let t = e.ye;t !== null; t = t.Pe) {
    if (t.B || t.q)
      updatePendingSignal(t);
  }
}
function snapCompanionsToState(e) {
  const t = e.B;
  if (t && (t.O === undefined || t.O === NOT_PENDING)) {
    const n2 = computePendingState(e);
    if (t.Z !== n2 || t.D !== NOT_PENDING) {
      t.Z = n2;
      t.D = NOT_PENDING;
      t.fe = clock;
      insertSubs(t);
      schedule();
    }
  }
  const n = e.q;
  if (n && !(n.W & REACTIVE_DISPOSED)) {
    if ((n.O === undefined || n.O === NOT_PENDING) && n.D === NOT_PENDING && !Object.is(n.Z, e.Z) && !(n.W & (REACTIVE_DIRTY | REACTIVE_CHECK))) {
      n.W |= REACTIVE_DIRTY;
      insertIntoHeap(n, n.W & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      insertSubs(n);
      schedule();
    }
    snapCompanionsToState(n);
  }
}
function getLatestValueComputed(e) {
  if (!e.q) {
    const t = latestReadActive;
    latestReadActive = false;
    const n = pendingCheckActive;
    pendingCheckActive = false;
    const i = context;
    context = null;
    e.q = optimisticComputed(() => read(e));
    e.q.t = e;
    context = i;
    pendingCheckActive = n;
    latestReadActive = t;
  }
  return e.q;
}
function staleValues(e, t = true) {
  const n = stale;
  stale = t;
  try {
    return e();
  } finally {
    stale = n;
  }
}
function createContext(e, t) {
  return { id: Symbol(t), defaultValue: e };
}
function effect(e, t, n, i) {
  const r = !!i?.user;
  const o = createEffectNode(e, t, n, r ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, i);
  recompute(o, true);
  !i?.defer && (o.V === EFFECT_USER || i?.schedule ? o.F.enqueue(o.V, runEffect.bind(null, o)) : runEffect(o));
}
function notifyEffectStatus(e, t) {
  const n = e !== undefined ? e : this.Y;
  const i = t !== undefined ? t : this.ae;
  if (n & STATUS_ERROR) {
    this.F.notify(this, STATUS_PENDING, 0);
    if (this.V === EFFECT_USER) {
      if (this.Y & STATUS_ERROR) {
        this.G = true;
        this.F.enqueue(this.V, this.$e ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.F.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity();
      throw i;
    }
  } else if (this.V === EFFECT_RENDER) {
    this.F.notify(this, STATUS_PENDING | STATUS_ERROR, n, i);
  }
}
function runEffect(e) {
  if (!e.G || e.W & REACTIVE_DISPOSED)
    return;
  if (e.Y & STATUS_ERROR && e.V === EFFECT_USER) {
    const t2 = unwrapStatusError(e.ae);
    e.Ye = e.Z;
    e.G = false;
    try {
      e.Be ? e.Be(t2, () => {
        const t3 = e.Le;
        e.Le = undefined;
        t3?.();
      }) : console.error(t2);
    } catch (t3) {
      if (!e.F.notify(e, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity();
        throw t3;
      }
    }
    return;
  }
  const t = e.Le;
  e.Le = undefined;
  try {
    t?.();
    const n = e.Ze(e.Z, e.Ye);
    if (false)
      ;
    e.Le = n;
  } catch (t2) {
    e.ae = new StatusError(e, t2);
    e.Y |= STATUS_ERROR;
    if (!e.F.notify(e, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity();
      throw t2;
    }
  } finally {
    e.Ye = e.Z;
    e.G = false;
  }
}
GlobalQueue.oe = runEffect;
function trackedEffect(e, t) {
  const run = () => {
    if (!n.G || n.W & REACTIVE_DISPOSED)
      return;
    try {
      n.G = false;
      recompute(n);
    } finally {}
  };
  const n = computed(() => {
    const t2 = n.Le;
    n.Le = undefined;
    t2?.();
    const i = staleValues(e);
    n.Le = i;
  }, { ...t, lazy: true });
  n.Le = undefined;
  n.le = n.le & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  n.G = true;
  n.V = EFFECT_TRACKED;
  n.Qe = (e2, t2) => {
    const i = e2 !== undefined ? e2 : n.Y;
    if (i & STATUS_ERROR) {
      n.F.notify(n, STATUS_PENDING, 0);
      const e3 = t2 !== undefined ? t2 : n.ae;
      if (!n.F.notify(n, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity();
        throw e3;
      }
    }
  };
  n.k = run;
  n.F.enqueue(EFFECT_USER, run);
}
function onCleanup(e) {
  return cleanup(e);
}
function accessor(e) {
  const t = read.bind(null, e);
  t[$REFRESH] = e;
  return t;
}
function createSignal(e, t) {
  if (typeof e === "function") {
    const n2 = computed(e, t);
    n2.le &= ~CONFIG_AUTO_DISPOSE;
    return [accessor(n2), setMemo.bind(null, n2)];
  }
  const n = signal(e, t);
  return [accessor(n), setSignal.bind(null, n)];
}
function createMemo(e, t) {
  return accessor(computed(e, t));
}
function createEffect(e, t, n) {
  effect(e, t.effect || t, t.error, { user: true, ...n });
}
function createRenderEffect(e, t, n) {
  effect(e, t, undefined, n);
}
function createTrackedEffect(e, t) {
  trackedEffect(e, t);
}
function onSettled(e) {
  const t = getOwner();
  t && !(t.le & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    e();
  });
}
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var STORE_SELF_PENDING = Symbol(0);
var storeLookup = new WeakMap;
var symbolKeyedRecords = new WeakSet;
function isWrappable(e) {
  if (e == null || typeof e !== "object" || Object.isFrozen(e))
    return false;
  return typeof Node === "undefined" || !(e instanceof Node);
}
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
var DELETE = Symbol(0);
function isPrototypePollutionKey(e) {
  return e === "__proto__" || e === "constructor" || e === "prototype";
}
function updatePath(e, t, n = 0) {
  let i, r = e;
  if (n < t.length - 1) {
    i = t[n];
    const o2 = typeof i;
    const s = Array.isArray(e);
    if (o2 === "string" && isPrototypePollutionKey(i))
      return;
    if (Array.isArray(i)) {
      for (let r2 = 0;r2 < i.length; r2++) {
        t[n] = i[r2];
        updatePath(e, t, n);
      }
      t[n] = i;
      return;
    } else if (s && o2 === "function") {
      for (let r2 = 0;r2 < e.length; r2++) {
        if (i(e[r2], r2)) {
          t[n] = r2;
          updatePath(e, t, n);
        }
      }
      t[n] = i;
      return;
    } else if (s && o2 === "object") {
      const { from: r2 = 0, to: o3 = e.length - 1, by: s2 = 1 } = i;
      for (let i2 = r2;i2 <= o3; i2 += s2) {
        t[n] = i2;
        updatePath(e, t, n);
      }
      t[n] = i;
      return;
    } else if (n < t.length - 2) {
      updatePath(e[i], t, n + 1);
      return;
    }
    r = e[i];
  }
  let o = t[t.length - 1];
  if (typeof o === "function") {
    o = o(r);
    if (o === r)
      return;
  }
  if (i === undefined && o == undefined)
    return;
  if (o === DELETE) {
    delete e[i];
  } else if (i === undefined || isWrappable(r) && isWrappable(o) && !Array.isArray(o)) {
    const t2 = i !== undefined ? e[i] : e;
    const n2 = ownEnumerableKeys(o);
    for (let e2 = 0;e2 < n2.length; e2++) {
      const i2 = n2[e2];
      if (typeof i2 === "string" && isPrototypePollutionKey(i2))
        continue;
      const r2 = Object.getOwnPropertyDescriptor(o, i2);
      if (r2.get || r2.set)
        Object.defineProperty(t2, i2, r2);
      else
        t2[i2] = r2.value;
    }
  } else {
    e[i] = o;
  }
}
var storePath = Object.assign(function storePath2(...e) {
  return (t) => {
    updatePath(t, e);
  };
}, { DELETE });
function trueFn() {
  return true;
}
var propTraps = {
  get(e, t, n) {
    if (t === $PROXY)
      return n;
    return e.get(t);
  },
  has(e, t) {
    if (t === $PROXY)
      return true;
    return e.has(t);
  },
  set: trueFn,
  deleteProperty: trueFn,
  getOwnPropertyDescriptor(e, t) {
    return {
      configurable: true,
      enumerable: true,
      get() {
        return e.get(t);
      },
      set: trueFn,
      deleteProperty: trueFn
    };
  },
  ownKeys(e) {
    return e.keys();
  }
};
function resolveSource(e) {
  return !(e = typeof e === "function" ? e() : e) ? {} : e;
}
var $SOURCES = Symbol(0);
function merge(...e) {
  if (e.length === 1 && typeof e[0] !== "function")
    return e[0];
  let t = false;
  const n = [];
  for (let i2 = 0;i2 < e.length; i2++) {
    const r2 = e[i2];
    t = t || !!r2 && $PROXY in r2;
    const o2 = !!r2 && r2[$SOURCES];
    if (o2) {
      for (let e2 = 0;e2 < o2.length; e2++)
        n.push(o2[e2]);
    } else
      n.push(typeof r2 === "function" ? (t = true, createMemo(r2)) : r2);
  }
  if (SUPPORTS_PROXY && t) {
    return new Proxy({
      get(e2) {
        if (e2 === $SOURCES)
          return n;
        for (let t2 = n.length - 1;t2 >= 0; t2--) {
          const i2 = resolveSource(n[t2]);
          if (e2 in i2)
            return i2[e2];
        }
      },
      has(e2) {
        for (let t2 = n.length - 1;t2 >= 0; t2--) {
          if (e2 in resolveSource(n[t2]))
            return true;
        }
        return false;
      },
      keys() {
        const e2 = new Set;
        for (let t2 = 0;t2 < n.length; t2++) {
          const i2 = ownEnumerableKeys(resolveSource(n[t2]));
          for (let t3 = 0;t3 < i2.length; t3++)
            e2.add(i2[t3]);
        }
        return [...e2];
      }
    }, propTraps);
  }
  const i = Object.create(null);
  let r = false;
  let o = n.length - 1;
  for (let e2 = o;e2 >= 0; e2--) {
    const t2 = n[e2];
    if (!t2) {
      e2 === o && o--;
      continue;
    }
    const s2 = Object.getOwnPropertyNames(t2);
    for (let n2 = s2.length - 1;n2 >= 0; n2--) {
      const u2 = s2[n2];
      if (u2 === "__proto__" || u2 === "constructor")
        continue;
      if (!i[u2]) {
        r = r || e2 !== o;
        const n3 = Object.getOwnPropertyDescriptor(t2, u2);
        i[u2] = n3.get ? { enumerable: true, configurable: true, get: n3.get.bind(t2) } : n3;
      }
    }
  }
  if (!r)
    return n[o];
  const s = {};
  const u = Object.keys(i);
  for (let e2 = u.length - 1;e2 >= 0; e2--) {
    const t2 = u[e2], n2 = i[t2];
    if (n2.get)
      Object.defineProperty(s, t2, n2);
    else
      s[t2] = n2.value;
  }
  s[$SOURCES] = n;
  return s;
}
function mapArray(e, t, n) {
  const i = typeof n?.keyed === "function" ? n.keyed : undefined;
  const r = t.length > 1;
  const o = t;
  const s = {
    ze: createOwner(),
    Xe: 0,
    Je: e,
    et: [],
    tt: o,
    nt: [],
    it: [],
    rt: i,
    ot: i || n?.keyed === false ? [] : undefined,
    st: r && n?.keyed !== false ? [] : undefined,
    ut: n?.keyed === false,
    ct: n?.fallback
  };
  const u = computed(updateKeyedMap.bind(s));
  s.ze.Ae = u;
  u.le &= ~CONFIG_AUTO_DISPOSE;
  return accessor(u);
}
var pureOptions = { ownedWrite: true };
function updateKeyedMap() {
  const e = this.Je() || [], t = e.length;
  e[$TRACK];
  runWithOwner(this.ze, () => {
    let n, i, r = this.ot ? this.ut ? () => {
      this.ot[i] = signal(e[i], pureOptions);
      return this.tt(accessor(this.ot[i]), i);
    } : () => {
      this.ot[i] = signal(e[i], pureOptions);
      this.st && (this.st[i] = signal(i, pureOptions));
      return this.tt(accessor(this.ot[i]), this.st ? accessor(this.st[i]) : undefined);
    } : this.st ? () => {
      const t2 = e[i];
      this.st[i] = signal(i, pureOptions);
      return this.tt(t2, accessor(this.st[i]));
    } : () => {
      const t2 = e[i];
      return this.tt(t2);
    };
    if (t === 0) {
      if (this.Xe !== 0) {
        this.ze.dispose(false);
        this.it = [];
        this.et = [];
        this.nt = [];
        this.Xe = 0;
        this.ot && (this.ot = []);
        this.st && (this.st = []);
      }
      if (this.ct && !this.nt[0]) {
        this.nt[0] = runWithOwner(this.it[0] = createOwner(), this.ct);
      }
    } else if (this.Xe === 0) {
      if (this.it[0])
        this.it[0].dispose();
      this.nt = new Array(t);
      for (i = 0;i < t; i++) {
        this.et[i] = e[i];
        this.nt[i] = runWithOwner(this.it[i] = createOwner(), r);
      }
      this.Xe = t;
    } else {
      let o, s, u, c, l, a, f, E = new Array(t), d = new Array(t), T = this.ot ? new Array(t) : undefined, S = this.st ? new Array(t) : undefined;
      for (o = 0, s = Math.min(this.Xe, t);o < s && (this.et[o] === e[o] || this.ot && compare(this.rt, this.et[o], e[o])); o++) {
        if (this.ot)
          setSignal(this.ot[o], e[o]);
      }
      for (s = this.Xe - 1, u = t - 1;s >= o && u >= o && (this.et[s] === e[u] || this.ot && compare(this.rt, this.et[s], e[u])); s--, u--) {
        E[u] = this.nt[s];
        d[u] = this.it[s];
        T && (T[u] = this.ot[s]);
        S && (S[u] = this.st[s]);
      }
      a = new Map;
      f = new Array(u + 1);
      for (i = u;i >= o; i--) {
        c = e[i];
        l = this.rt ? this.rt(c) : c;
        n = a.get(l);
        f[i] = n === undefined ? -1 : n;
        a.set(l, i);
      }
      for (n = o;n <= s; n++) {
        c = this.et[n];
        l = this.rt ? this.rt(c) : c;
        i = a.get(l);
        if (i !== undefined && i !== -1) {
          E[i] = this.nt[n];
          d[i] = this.it[n];
          T && (T[i] = this.ot[n]);
          S && (S[i] = this.st[n]);
          i = f[i];
          a.set(l, i);
        } else
          this.it[n].dispose();
      }
      for (i = o;i < t; i++) {
        if (i in E) {
          this.nt[i] = E[i];
          this.it[i] = d[i];
          if (T) {
            this.ot[i] = T[i];
            setSignal(this.ot[i], e[i]);
          }
          if (S) {
            this.st[i] = S[i];
            setSignal(this.st[i], i);
          }
        } else {
          this.nt[i] = runWithOwner(this.it[i] = createOwner(), r);
        }
      }
      this.nt = this.nt.slice(0, this.Xe = t);
      this.et = e.slice(0);
    }
  });
  return this.nt;
}
function compare(e, t, n) {
  return e ? e(t) === e(n) : true;
}
var ON_INIT = Symbol();
var RevealControllerContext = createContext(null);
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.isReady() : e.Tt.size === 0 && !e.St;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.isMinimallyReady() : isSlotReady(e);
}
function setSlotState(e, t, n, i) {
  setSignal(e._t, n);
  setSignal(e.Ot, i);
  if (isRevealController(e)) {
    if (!n && e.Rt === t)
      e.Rt = undefined;
    return e.evaluate(n, i);
  }
  if (!n && e.It === t && e.ht)
    e.It = undefined;
}

class RevealController {
  At;
  Nt;
  Ct = [];
  Rt;
  _t = signal(false, { ownedWrite: true, Ke: true });
  Ot = signal(false, { ownedWrite: true, Ke: true });
  yt = true;
  Pt = true;
  gt = false;
  constructor(e, t) {
    this.At = e;
    this.Nt = t;
  }
  Dt(e) {
    for (let t = 0;t < this.Ct.length; t++) {
      const n = this.Ct[t];
      if ((isRevealController(n) ? n.Rt : n.It) !== this)
        continue;
      if (e(n) === false)
        return false;
    }
    return true;
  }
  isReady() {
    return this.Dt(isSlotReady);
  }
  isMinimallyReady() {
    const e = untrack(this.At);
    if (e === "together")
      return this.isReady();
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.Dt((n) => {
        e2 = true;
        if (isSlotMinimallyReady(n)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.Dt((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  register(e) {
    if (this.Ct.includes(e))
      return;
    this.Ct.push(e);
    const t = untrack(this.At);
    setSignal(e._t, true), setSignal(e.Ot, t === "sequential" ? !!untrack(this.Nt) : false);
    untrack(() => this.evaluate());
  }
  unregister(e) {
    const t = this.Ct.indexOf(e);
    if (t >= 0)
      this.Ct.splice(t, 1);
    untrack(() => this.evaluate());
  }
  evaluate(e, t) {
    if (this.gt)
      return;
    this.gt = true;
    const n = this.yt;
    const i = this.Pt;
    try {
      const n2 = e ?? read(this._t), i2 = untrack(this.At), r = i2 === "sequential" && !!untrack(this.Nt), o = t ?? r;
      if (n2) {
        this.Dt((e2) => setSlotState(e2, this, true, o));
      } else if (i2 === "natural") {
        this.Dt((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.Ot, false);
            setSignal(e2._t, false);
            e2.evaluate(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (i2 === "together") {
        const e2 = this.Dt(isSlotMinimallyReady);
        this.Dt((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.Dt((t2) => {
          if (e2)
            return setSlotState(t2, this, true, r);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.Ot, false);
            setSignal(t2._t, false);
            t2.evaluate(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.yt = this.isReady();
      this.Pt = this.isMinimallyReady();
      this.gt = false;
    }
    if (this.Rt && (n !== this.yt || i !== this.Pt))
      this.Rt.evaluate();
  }
}

class CollectionQueue extends Queue {
  bt;
  Tt = new Set;
  vt;
  St = true;
  _t = signal(false, { ownedWrite: true, Ke: true });
  ae;
  Ot = signal(false, { ownedWrite: true, Ke: true });
  It;
  ht = false;
  wt;
  Lt = ON_INIT;
  constructor(e) {
    super();
    this.bt = e;
  }
  run(e) {
    if (!e || read(this._t) && (!_revealUsed || read(this.Ot)))
      return;
    return super.run(e);
  }
  notify(e, t, n, i) {
    if (!(t & this.bt))
      return super.notify(e, t, n, i);
    if (this.ht && this.wt) {
      const e2 = untrack(() => {
        try {
          return this.wt();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.Lt) {
        this.Lt = e2;
        this.ht = false;
        this.Tt.clear();
      }
    }
    if (this.bt & STATUS_PENDING && this.ht)
      return super.notify(e, t, n, i);
    if (n & this.bt) {
      this.St = true;
      const t2 = i?.source || e.ae?.source;
      if (t2) {
        const e2 = this.Tt.size === 0;
        this.Tt.add(t2);
        if (e2)
          setSignal(this._t, true);
        if (this.bt & STATUS_ERROR) {
          setSignal(this.ae, unwrapStatusError(t2.ae));
        }
      }
    }
    t &= ~this.bt;
    return t ? super.notify(e, t, n, i) : true;
  }
  checkSources() {
    for (const e of this.Tt) {
      if (e.W & REACTIVE_DISPOSED || !(e.Y & this.bt) && !(this.bt & STATUS_ERROR && e.Y & STATUS_PENDING))
        this.Tt.delete(e);
    }
    if (!this.Tt.size) {
      if (this.bt & STATUS_PENDING && this.St && !this.ht && this.vt) {
        this.St = !!(this.vt.Y & this.bt);
      } else {
        this.St = false;
      }
      if (!this.St) {
        setSignal(this._t, false);
        if (this.wt) {
          try {
            this.Lt = untrack(() => this.wt());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this.It?.evaluate();
  }
}
function flatten(e, t) {
  if (typeof e === "function" && !e.length) {
    if (t?.doNotUnwrap)
      return e;
    do {
      e = e();
    } while (typeof e === "function" && !e.length);
  }
  if (t?.skipNonRendered && (e == null || e === true || e === false || e === ""))
    return;
  if (Array.isArray(e)) {
    let n = [];
    if (flattenArray(e, n, t)) {
      return () => {
        let e2 = [];
        flattenArray(n, e2, { ...t, doNotUnwrap: false });
        return e2;
      };
    }
    return n;
  }
  return e;
}
function flattenArray(e, t = [], n) {
  let i = null;
  let r = false;
  for (let o = 0;o < e.length; o++) {
    try {
      let i2 = e[o];
      if (typeof i2 === "function" && !i2.length) {
        if (n?.doNotUnwrap) {
          t.push(i2);
          r = true;
          continue;
        }
        do {
          i2 = i2();
        } while (typeof i2 === "function" && !i2.length);
      }
      if (Array.isArray(i2)) {
        r = flattenArray(i2, t, n);
      } else if (n?.skipNonRendered && (i2 == null || i2 === true || i2 === false || i2 === "")) {} else
        t.push(i2);
    } catch (e2) {
      if (!(e2 instanceof NotReadyError))
        throw e2;
      i = e2;
    }
  }
  if (i)
    throw i;
  return r;
}

// node_modules/.bun/solid-js@2.0.0-beta.17/node_modules/solid-js/dist/solid.js
var IS_DEV = false;
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createMemo;
var _createRenderEffect;
class MockPromise {
  static {
    for (const k of ["all", "allSettled", "any", "race", "reject", "resolve"]) {
      MockPromise[k] = () => new MockPromise;
    }
  }
  catch() {
    return new MockPromise;
  }
  then() {
    return new MockPromise;
  }
  finally() {
    return new MockPromise;
  }
}
var NO_HYDRATED_VALUE = Symbol("NO_HYDRATED_VALUE");
var createMemo2 = (...args) => (_createMemo || createMemo)(...args);
var createRenderEffect2 = (...args) => (_createRenderEffect || createRenderEffect)(...args);
function createComponent(Comp, props) {
  return untrack(() => Comp(props || {}));
}
var narrowedError = (name) => `Stale read from <${name}>.`;
function For(props) {
  const options = "fallback" in props ? {
    keyed: props.keyed,
    fallback: () => props.fallback
  } : {
    keyed: props.keyed
  };
  return mapArray(() => props.each, props.children, options);
}
function Show(props) {
  const keyed = props.keyed;
  const conditionValue = createMemo(() => props.when, undefined);
  const condition = keyed ? conditionValue : createMemo(conditionValue, {
    equals: (a, b) => !a === !b,
    sync: true
  });
  return createMemo(() => {
    const c = condition();
    if (c) {
      const child = props.children;
      const fn = typeof child === "function" && child.length > 0;
      return fn ? keyed ? untrack(() => child(c), IS_DEV) : untrack(() => child(() => {
        if (!untrack(condition))
          throw narrowedError("Show");
        return conditionValue();
      }), IS_DEV) : child;
    }
    return props.fallback;
  }, {
    sync: true
  });
}

// node_modules/.bun/@solidjs+universal@2.0.0-beta.17+2405e6a685c2448a/node_modules/@solidjs/universal/dist/universal.js
var transparentOptions = {
  transparent: true,
  sync: true
};
var syncOptions = {
  sync: true
};
var effect2 = (fn, effectFn, options) => createRenderEffect2(fn, effectFn, options ? {
  sync: true,
  ...options,
  transparent: !options.scope
} : transparentOptions);
var memo = (fn) => createMemo2(() => fn(), syncOptions);
var INNER_OWNED = {};
function createRenderer$1({
  createElement,
  createTextNode,
  createSentinel = () => createTextNode(""),
  isTextNode,
  replaceText,
  insertNode,
  removeNode,
  cleanupNodes,
  setProperty,
  getParentNode,
  getFirstChild,
  getNextSibling
}) {
  function insert(parent, accessor2, marker, initial, options) {
    const onUpdate = options && options.onUpdate;
    let effectOptions = options;
    if (onUpdate) {
      const {
        onUpdate: onUpdate2,
        ...rest
      } = options;
      effectOptions = rest;
    }
    const multi = marker !== undefined;
    if (multi && !initial)
      initial = [];
    if (typeof accessor2 !== "function") {
      accessor2 = normalize(accessor2, multi, true);
      if (typeof accessor2 !== "function") {
        insertExpression(parent, accessor2, initial, marker);
        onUpdate && onUpdate(accessor2);
        return;
      }
    }
    if (multi && initial.length === 0) {
      const sentinel = createSentinel();
      insertNode(parent, sentinel, marker);
      initial = [sentinel];
    }
    let current = initial;
    effect2((prev) => {
      const value = normalize(accessor2(), multi, true);
      if (typeof value !== "function")
        return value;
      effect2(() => normalize(value, multi), (inner) => {
        insertExpression(parent, inner, current, marker);
        current = inner;
        onUpdate && onUpdate(current);
      }, prev !== undefined && !(options && options.schedule) ? {
        ...effectOptions,
        schedule: true
      } : effectOptions);
      return INNER_OWNED;
    }, (value) => {
      if (value === INNER_OWNED)
        return;
      insertExpression(parent, value, current, marker);
      current = value;
      onUpdate && onUpdate(current);
    }, effectOptions);
  }
  function insertExpression(parent, value, current, marker) {
    if (value === current)
      return;
    const t = typeof value, multi = marker !== undefined;
    if (t === "string" || t === "number") {
      const tc = typeof current;
      if (tc === "string" || tc === "number") {
        replaceText(getFirstChild(parent), value);
      } else {
        cleanChildren(parent, current, marker, createTextNode(value));
      }
    } else if (value == null) {
      cleanChildren(parent, current, marker);
    } else if (Array.isArray(value)) {
      if (value.length === 0) {
        cleanChildren(parent, current, marker);
      } else {
        if (Array.isArray(current)) {
          if (current.length === 0) {
            appendNodes(parent, value, marker);
          } else
            reconcileArrays(parent, current, value);
        } else if (current == null) {
          appendNodes(parent, value);
        } else {
          reconcileArrays(parent, multi && current || [getFirstChild(parent)], value);
        }
      }
    } else {
      if (Array.isArray(current)) {
        cleanChildren(parent, current, multi ? marker : null, value);
      } else if (current == null || !getFirstChild(parent)) {
        insertNode(parent, value);
      } else
        replaceNode(parent, value, getFirstChild(parent));
    }
  }
  function normalize(value, multi, doNotUnwrap) {
    value = flatten(value, {
      skipNonRendered: true,
      doNotUnwrap
    });
    if (doNotUnwrap && typeof value === "function")
      return value;
    if (multi && !Array.isArray(value))
      value = [value != null ? value : ""];
    if (Array.isArray(value)) {
      for (let i = 0, len = value.length;i < len; i++) {
        const item = value[i], t = typeof item;
        if (t === "string" || t === "number")
          value[i] = createTextNode(item);
      }
    }
    return value;
  }
  function reconcileArrays(parentNode, a, b) {
    let bLength = b.length, aEnd = a.length, bEnd = bLength, aStart = 0, bStart = 0, after = getNextSibling(a[aEnd - 1]), map = null;
    while (aStart < aEnd || bStart < bEnd) {
      if (a[aStart] === b[bStart]) {
        aStart++;
        bStart++;
        continue;
      }
      while (a[aEnd - 1] === b[bEnd - 1]) {
        aEnd--;
        bEnd--;
      }
      if (aEnd === aStart) {
        const node = bEnd < bLength ? bStart ? getNextSibling(b[bStart - 1]) : b[bEnd - bStart] : after;
        while (bStart < bEnd)
          insertNode(parentNode, b[bStart++], node);
      } else if (bEnd === bStart) {
        while (aStart < aEnd) {
          if (!map || !map.has(a[aStart]))
            removeNode(parentNode, a[aStart]);
          aStart++;
        }
      } else if (a[aStart] === b[bEnd - 1] && b[bStart] === a[aEnd - 1]) {
        const anchor = a[aStart];
        do {
          insertNode(parentNode, a[--aEnd], anchor);
          bStart++;
          if (aStart >= aEnd - 1 || bStart >= bEnd)
            break;
        } while (a[aStart] === b[bEnd - 1] && b[bStart] === a[aEnd - 1]);
      } else {
        if (!map) {
          map = new Map;
          let i = bStart;
          while (i < bEnd)
            map.set(b[i], i++);
        }
        const index = map.get(a[aStart]);
        if (index != null) {
          if (bStart < index && index < bEnd) {
            let i = aStart, sequence = 1, t;
            while (++i < aEnd && i < bEnd) {
              if ((t = map.get(a[i])) == null || t !== index + sequence)
                break;
              sequence++;
            }
            if (sequence > index - bStart) {
              const node = a[aStart];
              while (bStart < index)
                insertNode(parentNode, b[bStart++], node);
            } else
              replaceNode(parentNode, b[bStart++], a[aStart++]);
          } else
            aStart++;
        } else
          removeNode(parentNode, a[aStart++]);
      }
    }
  }
  function cleanChildren(parent, current, marker, replacement) {
    if (marker === undefined) {
      let removed;
      while (removed = getFirstChild(parent))
        removeNode(parent, removed);
      replacement && insertNode(parent, replacement);
      return "";
    }
    if (current.length) {
      let inserted = false;
      for (let i = current.length - 1;i >= 0; i--) {
        const el = current[i];
        if (replacement !== el) {
          const isParent = getParentNode(el) === parent;
          if (replacement && !inserted && !i)
            isParent ? replaceNode(parent, replacement, el) : insertNode(parent, replacement, marker);
          else
            isParent && removeNode(parent, el);
        } else
          inserted = true;
      }
    } else if (replacement)
      insertNode(parent, replacement, marker);
  }
  function appendNodes(parent, array, marker) {
    for (let i = 0, len = array.length;i < len; i++)
      insertNode(parent, array[i], marker);
  }
  function replaceNode(parent, newNode, oldNode) {
    insertNode(parent, newNode, oldNode);
    removeNode(parent, oldNode);
  }
  function collectNodes(value, nodes) {
    if (Array.isArray(value)) {
      for (let i = 0, len = value.length;i < len; i++)
        collectNodes(value[i], nodes);
    } else if (value != null && typeof value !== "string" && typeof value !== "number") {
      nodes.push(value);
    }
    return nodes;
  }
  function collectMounted(parent, value) {
    const nodes = collectNodes(value, []);
    if (!nodes.length && (typeof value === "string" || typeof value === "number")) {
      const node = getFirstChild(parent);
      if (node)
        nodes.push(node);
    }
    return nodes;
  }
  function defaultCleanupNodes(parent, nodes) {
    for (let i = 0, len = nodes.length;i < len; i++) {
      const node = nodes[i];
      if (getParentNode(node) === parent)
        removeNode(parent, node);
    }
  }
  function spread(node, props, skipChildren) {
    const prevProps = {};
    props || (props = {});
    if (!skipChildren)
      insert(node, () => props.children);
    effect2(() => {
      const r = props.ref;
      (typeof r === "function" || Array.isArray(r)) && ref(() => r, node);
    }, () => {});
    effect2(() => {
      const newProps = {};
      for (const prop in props) {
        if (prop === "children" || prop === "ref")
          continue;
        newProps[prop] = props[prop];
      }
      return newProps;
    }, (props2) => {
      for (const prop in prevProps) {
        if (!(prop in props2)) {
          setProperty(node, prop, undefined, prevProps[prop]);
          delete prevProps[prop];
        }
      }
      for (const prop in props2) {
        const value = props2[prop];
        if (value === prevProps[prop])
          continue;
        setProperty(node, prop, value, prevProps[prop]);
        prevProps[prop] = value;
      }
    });
    return prevProps;
  }
  function applyRef(r, element) {
    Array.isArray(r) ? r.flat(Infinity).forEach((f) => f && f(element)) : r(element);
  }
  function ref(fn, element) {
    const resolved = untrack(fn);
    runWithOwner(null, () => applyRef(resolved, element));
  }
  return {
    render(code, element) {
      let disposer, disposed = false, mounted = [];
      const cleanup2 = cleanupNodes || defaultCleanupNodes;
      try {
        createRoot((dispose) => {
          disposer = dispose;
          insert(element, code(), undefined, undefined, {
            onUpdate(value) {
              mounted = collectMounted(element, value);
            }
          });
        });
      } catch (err) {
        if (disposer)
          disposer();
        cleanup2(element, mounted);
        throw err;
      }
      return () => {
        if (disposed)
          return;
        disposed = true;
        disposer();
        cleanup2(element, mounted);
        mounted = [];
      };
    },
    insert,
    spread,
    createElement,
    createTextNode,
    insertNode,
    setProp(node, name, value, prev) {
      setProperty(node, name, value, prev);
      return value;
    },
    mergeProps: merge,
    effect: effect2,
    memo,
    createComponent,
    applyRef,
    ref
  };
}
function createRenderer(options) {
  const base = createRenderer$1(options);
  const baseInsert = base.insert;
  return {
    ...base,
    render(code, element) {
      let dispose;
      createRoot((d) => {
        dispose = d;
        const tree = code();
        baseInsert(element, () => tree, undefined, undefined, {
          schedule: true
        });
      });
      flush();
      return dispose;
    }
  };
}

// packages/core/src/renderer.ts
import * as tree2 from "flux:rendertree";

// packages/core/src/window.ts
import { requestFrame } from "flux:rendertree";
import { renderFrame } from "srt:render";
import { on, once } from "srt:events";

// packages/core/src/core.ts
import * as tree from "flux:rendertree";
var handlers = new Map;
function setEventHandler(nodeId, name, fn) {
  if (fn == null) {
    handlers.get(nodeId)?.delete(name);
    return;
  }
  let nodeHandlers = handlers.get(nodeId);
  if (!nodeHandlers) {
    nodeHandlers = new Map;
    handlers.set(nodeId, nodeHandlers);
  }
  nodeHandlers.set(name, fn);
}
function getEventHandler(nodeId, name) {
  return handlers.get(nodeId)?.get(name);
}
function cleanupNodeHandlers(nodeId) {
  handlers.delete(nodeId);
}
var focusedNodeId = null;
var textInputActive = false;
function setFocus(nodeId) {
  if (nodeId === focusedNodeId)
    return;
  let oldId = focusedNodeId;
  focusedNodeId = nodeId;
  if (oldId != null) {
    getEventHandler(oldId, "onBlur")?.();
  }
  if (nodeId != null) {
    getEventHandler(nodeId, "onFocus")?.();
  }
  let wantActive = nodeId != null && getEventHandler(nodeId, "onTextInput") != null;
  if (wantActive !== textInputActive) {
    textInputActive = wantActive;
    tree.setTextInputActive(wantActive);
  }
}
function getFocusedNodeId() {
  return focusedNodeId;
}

// packages/core/src/window.ts
var pointerCaptures = new Map;
var nextFrameId = 1;
var animationFrames = new Map;
var refreshRate = 60;
function onFrame(fn) {
  let frameId = null;
  let extendedFn = (tick, frame, rate) => {
    fn(tick, frame, rate);
    frameId = nextFrameId++;
    animationFrames.set(frameId, extendedFn);
    requestFrame();
  };
  frameId = nextFrameId++;
  animationFrames.set(frameId, extendedFn);
  requestFrame();
  let cleanup2 = () => animationFrames.delete(frameId);
  onCleanup(cleanup2);
  return cleanup2;
}
var sizeAccessor;
var safeAreaAccessor;
var displayScaleAccessor;
function ensureResizeState() {
  if (sizeAccessor)
    return;
  let [size, setSize] = createSignal({ width: 0, height: 0 }, { ownedWrite: true });
  let [safe, setSafe] = createSignal({ top: 0, left: 0, right: 0, bottom: 0 }, { ownedWrite: true });
  let [scale, setScale] = createSignal(1, { ownedWrite: true });
  on("resize", (e) => {
    setSize({ width: e.width, height: e.height });
    setSafe(e.safeArea);
    setScale(e.displayScale);
  });
  sizeAccessor = size;
  safeAreaAccessor = safe;
  displayScaleAccessor = scale;
}
function windowSize() {
  ensureResizeState();
  return sizeAccessor();
}
function attachWindow(_nodeId) {
  let unsubscribe = null;
  let unsubDown = null;
  let unsubUp = null;
  let unsubMove = null;
  let unsubEnter = null;
  let unsubLeave = null;
  let unsubWheel = null;
  let unsubKeyDown = null;
  let unsubKeyUp = null;
  let unsubTextInput = null;
  let unsubKeyboardVisibility = null;
  let unsubRefreshRate = null;
  let unsubFirstResize = null;
  function runFrame(t, frame) {
    if (animationFrames.size > 0) {
      let frames = animationFrames;
      animationFrames = new Map;
      for (let fn of frames.values())
        fn(t, frame, refreshRate);
    }
    flush();
    renderFrame();
  }
  onSettled(() => {
    unsubRefreshRate = on("displayRefreshRate", ({ hz }) => {
      if (hz > 0)
        refreshRate = hz;
    });
    unsubscribe = on("render", ({ time, frame }) => {
      runFrame(time * 1000, frame);
    });
    let bubble = (targets, handler, e) => {
      let stopped = false;
      e.stopPropagation = () => {
        stopped = true;
      };
      for (let i = targets.length - 1;i >= 0; i--) {
        getEventHandler(targets[i], handler)?.(e);
        if (stopped)
          break;
      }
    };
    unsubDown = on("pointerDown", ({ targets, ...e }) => {
      bubble(targets, "onPointerDown", e);
      let focused = getFocusedNodeId();
      if (focused != null && !targets.includes(focused)) {
        setFocus(null);
      }
    });
    unsubUp = on("pointerUp", ({ targets, ...e }) => {
      let captured = pointerCaptures.get(e.pointerId);
      if (captured != null) {
        e.stopPropagation = () => {};
        getEventHandler(captured, "onPointerUp")?.(e);
        pointerCaptures.delete(e.pointerId);
        return;
      }
      bubble(targets, "onPointerUp", e);
    });
    unsubMove = on("pointerMove", ({ targets, ...e }) => {
      let captured = pointerCaptures.get(e.pointerId);
      if (captured != null) {
        e.stopPropagation = () => {};
        getEventHandler(captured, "onPointerMove")?.(e);
        return;
      }
      bubble(targets, "onPointerMove", e);
    });
    let dispatchOrdered = (targets, handler, e) => {
      let stopped = false;
      e.stopPropagation = () => {
        stopped = true;
      };
      for (let nodeId of targets) {
        getEventHandler(nodeId, handler)?.(e);
        if (stopped)
          break;
      }
    };
    unsubEnter = on("pointerEnter", ({ targets, ...e }) => {
      dispatchOrdered(targets, "onPointerEnter", e);
    });
    unsubLeave = on("pointerLeave", ({ targets, ...e }) => {
      dispatchOrdered(targets, "onPointerLeave", e);
    });
    unsubWheel = on("wheel", ({ targets, ...e }) => {
      bubble(targets, "onWheel", e);
    });
    unsubKeyDown = on("keydown", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onKeyDown")?.(e);
      }
    });
    unsubKeyUp = on("keyup", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onKeyUp")?.(e);
      }
    });
    unsubTextInput = on("textInput", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e);
      }
    });
    unsubKeyboardVisibility = on("keyboardVisibility", ({ shown }) => {
      if (!shown)
        setFocus(null);
    });
    unsubFirstResize = once("resize", () => {
      queueMicrotask(() => runFrame(0, 0));
    });
  });
  onCleanup(() => {
    if (unsubscribe)
      unsubscribe();
    if (unsubDown)
      unsubDown();
    if (unsubUp)
      unsubUp();
    if (unsubMove)
      unsubMove();
    if (unsubEnter)
      unsubEnter();
    if (unsubLeave)
      unsubLeave();
    if (unsubWheel)
      unsubWheel();
    if (unsubKeyDown)
      unsubKeyDown();
    if (unsubKeyUp)
      unsubKeyUp();
    if (unsubTextInput)
      unsubTextInput();
    if (unsubKeyboardVisibility)
      unsubKeyboardVisibility();
    if (unsubRefreshRate)
      unsubRefreshRate();
    if (unsubFirstResize)
      unsubFirstResize();
  });
}

// node_modules/.bun/colord@2.9.3/node_modules/colord/index.mjs
var r = { grad: 0.9, turn: 360, rad: 360 / (2 * Math.PI) };
var t = function(r2) {
  return typeof r2 == "string" ? r2.length > 0 : typeof r2 == "number";
};
var n = function(r2, t2, n2) {
  return t2 === undefined && (t2 = 0), n2 === undefined && (n2 = Math.pow(10, t2)), Math.round(n2 * r2) / n2 + 0;
};
var e = function(r2, t2, n2) {
  return t2 === undefined && (t2 = 0), n2 === undefined && (n2 = 1), r2 > n2 ? n2 : r2 > t2 ? r2 : t2;
};
var u = function(r2) {
  return (r2 = isFinite(r2) ? r2 % 360 : 0) > 0 ? r2 : r2 + 360;
};
var a = function(r2) {
  return { r: e(r2.r, 0, 255), g: e(r2.g, 0, 255), b: e(r2.b, 0, 255), a: e(r2.a) };
};
var o = function(r2) {
  return { r: n(r2.r), g: n(r2.g), b: n(r2.b), a: n(r2.a, 3) };
};
var i = /^#([0-9a-f]{3,8})$/i;
var s = function(r2) {
  var t2 = r2.toString(16);
  return t2.length < 2 ? "0" + t2 : t2;
};
var h = function(r2) {
  var { r: t2, g: n2, b: e2, a: u2 } = r2, a2 = Math.max(t2, n2, e2), o2 = a2 - Math.min(t2, n2, e2), i2 = o2 ? a2 === t2 ? (n2 - e2) / o2 : a2 === n2 ? 2 + (e2 - t2) / o2 : 4 + (t2 - n2) / o2 : 0;
  return { h: 60 * (i2 < 0 ? i2 + 6 : i2), s: a2 ? o2 / a2 * 100 : 0, v: a2 / 255 * 100, a: u2 };
};
var b = function(r2) {
  var { h: t2, s: n2, v: e2, a: u2 } = r2;
  t2 = t2 / 360 * 6, n2 /= 100, e2 /= 100;
  var a2 = Math.floor(t2), o2 = e2 * (1 - n2), i2 = e2 * (1 - (t2 - a2) * n2), s2 = e2 * (1 - (1 - t2 + a2) * n2), h2 = a2 % 6;
  return { r: 255 * [e2, i2, o2, o2, s2, e2][h2], g: 255 * [s2, e2, e2, i2, o2, o2][h2], b: 255 * [o2, o2, s2, e2, e2, i2][h2], a: u2 };
};
var g = function(r2) {
  return { h: u(r2.h), s: e(r2.s, 0, 100), l: e(r2.l, 0, 100), a: e(r2.a) };
};
var d = function(r2) {
  return { h: n(r2.h), s: n(r2.s), l: n(r2.l), a: n(r2.a, 3) };
};
var f = function(r2) {
  return b((n2 = (t2 = r2).s, { h: t2.h, s: (n2 *= ((e2 = t2.l) < 50 ? e2 : 100 - e2) / 100) > 0 ? 2 * n2 / (e2 + n2) * 100 : 0, v: e2 + n2, a: t2.a }));
  var t2, n2, e2;
};
var c = function(r2) {
  return { h: (t2 = h(r2)).h, s: (u2 = (200 - (n2 = t2.s)) * (e2 = t2.v) / 100) > 0 && u2 < 200 ? n2 * e2 / 100 / (u2 <= 100 ? u2 : 200 - u2) * 100 : 0, l: u2 / 2, a: t2.a };
  var t2, n2, e2, u2;
};
var l = /^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s*,\s*([+-]?\d*\.?\d+)%\s*,\s*([+-]?\d*\.?\d+)%\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i;
var p = /^hsla?\(\s*([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s+([+-]?\d*\.?\d+)%\s+([+-]?\d*\.?\d+)%\s*(?:\/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i;
var v = /^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*,\s*([+-]?\d*\.?\d+)(%)?\s*(?:,\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i;
var m = /^rgba?\(\s*([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s+([+-]?\d*\.?\d+)(%)?\s*(?:\/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i;
var y = { string: [[function(r2) {
  var t2 = i.exec(r2);
  return t2 ? (r2 = t2[1]).length <= 4 ? { r: parseInt(r2[0] + r2[0], 16), g: parseInt(r2[1] + r2[1], 16), b: parseInt(r2[2] + r2[2], 16), a: r2.length === 4 ? n(parseInt(r2[3] + r2[3], 16) / 255, 2) : 1 } : r2.length === 6 || r2.length === 8 ? { r: parseInt(r2.substr(0, 2), 16), g: parseInt(r2.substr(2, 2), 16), b: parseInt(r2.substr(4, 2), 16), a: r2.length === 8 ? n(parseInt(r2.substr(6, 2), 16) / 255, 2) : 1 } : null : null;
}, "hex"], [function(r2) {
  var t2 = v.exec(r2) || m.exec(r2);
  return t2 ? t2[2] !== t2[4] || t2[4] !== t2[6] ? null : a({ r: Number(t2[1]) / (t2[2] ? 100 / 255 : 1), g: Number(t2[3]) / (t2[4] ? 100 / 255 : 1), b: Number(t2[5]) / (t2[6] ? 100 / 255 : 1), a: t2[7] === undefined ? 1 : Number(t2[7]) / (t2[8] ? 100 : 1) }) : null;
}, "rgb"], [function(t2) {
  var n2 = l.exec(t2) || p.exec(t2);
  if (!n2)
    return null;
  var e2, u2, a2 = g({ h: (e2 = n2[1], u2 = n2[2], u2 === undefined && (u2 = "deg"), Number(e2) * (r[u2] || 1)), s: Number(n2[3]), l: Number(n2[4]), a: n2[5] === undefined ? 1 : Number(n2[5]) / (n2[6] ? 100 : 1) });
  return f(a2);
}, "hsl"]], object: [[function(r2) {
  var { r: n2, g: e2, b: u2, a: o2 } = r2, i2 = o2 === undefined ? 1 : o2;
  return t(n2) && t(e2) && t(u2) ? a({ r: Number(n2), g: Number(e2), b: Number(u2), a: Number(i2) }) : null;
}, "rgb"], [function(r2) {
  var { h: n2, s: e2, l: u2, a: a2 } = r2, o2 = a2 === undefined ? 1 : a2;
  if (!t(n2) || !t(e2) || !t(u2))
    return null;
  var i2 = g({ h: Number(n2), s: Number(e2), l: Number(u2), a: Number(o2) });
  return f(i2);
}, "hsl"], [function(r2) {
  var { h: n2, s: a2, v: o2, a: i2 } = r2, s2 = i2 === undefined ? 1 : i2;
  if (!t(n2) || !t(a2) || !t(o2))
    return null;
  var h2 = function(r3) {
    return { h: u(r3.h), s: e(r3.s, 0, 100), v: e(r3.v, 0, 100), a: e(r3.a) };
  }({ h: Number(n2), s: Number(a2), v: Number(o2), a: Number(s2) });
  return b(h2);
}, "hsv"]] };
var N = function(r2, t2) {
  for (var n2 = 0;n2 < t2.length; n2++) {
    var e2 = t2[n2][0](r2);
    if (e2)
      return [e2, t2[n2][1]];
  }
  return [null, undefined];
};
var x = function(r2) {
  return typeof r2 == "string" ? N(r2.trim(), y.string) : typeof r2 == "object" && r2 !== null ? N(r2, y.object) : [null, undefined];
};
var M = function(r2, t2) {
  var n2 = c(r2);
  return { h: n2.h, s: e(n2.s + 100 * t2, 0, 100), l: n2.l, a: n2.a };
};
var H = function(r2) {
  return (299 * r2.r + 587 * r2.g + 114 * r2.b) / 1000 / 255;
};
var $ = function(r2, t2) {
  var n2 = c(r2);
  return { h: n2.h, s: n2.s, l: e(n2.l + 100 * t2, 0, 100), a: n2.a };
};
var j = function() {
  function r2(r3) {
    this.parsed = x(r3)[0], this.rgba = this.parsed || { r: 0, g: 0, b: 0, a: 1 };
  }
  return r2.prototype.isValid = function() {
    return this.parsed !== null;
  }, r2.prototype.brightness = function() {
    return n(H(this.rgba), 2);
  }, r2.prototype.isDark = function() {
    return H(this.rgba) < 0.5;
  }, r2.prototype.isLight = function() {
    return H(this.rgba) >= 0.5;
  }, r2.prototype.toHex = function() {
    return r3 = o(this.rgba), t2 = r3.r, e2 = r3.g, u2 = r3.b, i2 = (a2 = r3.a) < 1 ? s(n(255 * a2)) : "", "#" + s(t2) + s(e2) + s(u2) + i2;
    var r3, t2, e2, u2, a2, i2;
  }, r2.prototype.toRgb = function() {
    return o(this.rgba);
  }, r2.prototype.toRgbString = function() {
    return r3 = o(this.rgba), t2 = r3.r, n2 = r3.g, e2 = r3.b, (u2 = r3.a) < 1 ? "rgba(" + t2 + ", " + n2 + ", " + e2 + ", " + u2 + ")" : "rgb(" + t2 + ", " + n2 + ", " + e2 + ")";
    var r3, t2, n2, e2, u2;
  }, r2.prototype.toHsl = function() {
    return d(c(this.rgba));
  }, r2.prototype.toHslString = function() {
    return r3 = d(c(this.rgba)), t2 = r3.h, n2 = r3.s, e2 = r3.l, (u2 = r3.a) < 1 ? "hsla(" + t2 + ", " + n2 + "%, " + e2 + "%, " + u2 + ")" : "hsl(" + t2 + ", " + n2 + "%, " + e2 + "%)";
    var r3, t2, n2, e2, u2;
  }, r2.prototype.toHsv = function() {
    return r3 = h(this.rgba), { h: n(r3.h), s: n(r3.s), v: n(r3.v), a: n(r3.a, 3) };
    var r3;
  }, r2.prototype.invert = function() {
    return w({ r: 255 - (r3 = this.rgba).r, g: 255 - r3.g, b: 255 - r3.b, a: r3.a });
    var r3;
  }, r2.prototype.saturate = function(r3) {
    return r3 === undefined && (r3 = 0.1), w(M(this.rgba, r3));
  }, r2.prototype.desaturate = function(r3) {
    return r3 === undefined && (r3 = 0.1), w(M(this.rgba, -r3));
  }, r2.prototype.grayscale = function() {
    return w(M(this.rgba, -1));
  }, r2.prototype.lighten = function(r3) {
    return r3 === undefined && (r3 = 0.1), w($(this.rgba, r3));
  }, r2.prototype.darken = function(r3) {
    return r3 === undefined && (r3 = 0.1), w($(this.rgba, -r3));
  }, r2.prototype.rotate = function(r3) {
    return r3 === undefined && (r3 = 15), this.hue(this.hue() + r3);
  }, r2.prototype.alpha = function(r3) {
    return typeof r3 == "number" ? w({ r: (t2 = this.rgba).r, g: t2.g, b: t2.b, a: r3 }) : n(this.rgba.a, 3);
    var t2;
  }, r2.prototype.hue = function(r3) {
    var t2 = c(this.rgba);
    return typeof r3 == "number" ? w({ h: r3, s: t2.s, l: t2.l, a: t2.a }) : n(t2.h);
  }, r2.prototype.isEqual = function(r3) {
    return this.toHex() === w(r3).toHex();
  }, r2;
}();
var w = function(r2) {
  return r2 instanceof j ? r2 : new j(r2);
};
var S = [];
var k = function(r2) {
  r2.forEach(function(r3) {
    S.indexOf(r3) < 0 && (r3(j, y), S.push(r3));
  });
};

// node_modules/.bun/colord@2.9.3/node_modules/colord/plugins/names.mjs
function names_default(e2, f2) {
  var a2 = { white: "#ffffff", bisque: "#ffe4c4", blue: "#0000ff", cadetblue: "#5f9ea0", chartreuse: "#7fff00", chocolate: "#d2691e", coral: "#ff7f50", antiquewhite: "#faebd7", aqua: "#00ffff", azure: "#f0ffff", whitesmoke: "#f5f5f5", papayawhip: "#ffefd5", plum: "#dda0dd", blanchedalmond: "#ffebcd", black: "#000000", gold: "#ffd700", goldenrod: "#daa520", gainsboro: "#dcdcdc", cornsilk: "#fff8dc", cornflowerblue: "#6495ed", burlywood: "#deb887", aquamarine: "#7fffd4", beige: "#f5f5dc", crimson: "#dc143c", cyan: "#00ffff", darkblue: "#00008b", darkcyan: "#008b8b", darkgoldenrod: "#b8860b", darkkhaki: "#bdb76b", darkgray: "#a9a9a9", darkgreen: "#006400", darkgrey: "#a9a9a9", peachpuff: "#ffdab9", darkmagenta: "#8b008b", darkred: "#8b0000", darkorchid: "#9932cc", darkorange: "#ff8c00", darkslateblue: "#483d8b", gray: "#808080", darkslategray: "#2f4f4f", darkslategrey: "#2f4f4f", deeppink: "#ff1493", deepskyblue: "#00bfff", wheat: "#f5deb3", firebrick: "#b22222", floralwhite: "#fffaf0", ghostwhite: "#f8f8ff", darkviolet: "#9400d3", magenta: "#ff00ff", green: "#008000", dodgerblue: "#1e90ff", grey: "#808080", honeydew: "#f0fff0", hotpink: "#ff69b4", blueviolet: "#8a2be2", forestgreen: "#228b22", lawngreen: "#7cfc00", indianred: "#cd5c5c", indigo: "#4b0082", fuchsia: "#ff00ff", brown: "#a52a2a", maroon: "#800000", mediumblue: "#0000cd", lightcoral: "#f08080", darkturquoise: "#00ced1", lightcyan: "#e0ffff", ivory: "#fffff0", lightyellow: "#ffffe0", lightsalmon: "#ffa07a", lightseagreen: "#20b2aa", linen: "#faf0e6", mediumaquamarine: "#66cdaa", lemonchiffon: "#fffacd", lime: "#00ff00", khaki: "#f0e68c", mediumseagreen: "#3cb371", limegreen: "#32cd32", mediumspringgreen: "#00fa9a", lightskyblue: "#87cefa", lightblue: "#add8e6", midnightblue: "#191970", lightpink: "#ffb6c1", mistyrose: "#ffe4e1", moccasin: "#ffe4b5", mintcream: "#f5fffa", lightslategray: "#778899", lightslategrey: "#778899", navajowhite: "#ffdead", navy: "#000080", mediumvioletred: "#c71585", powderblue: "#b0e0e6", palegoldenrod: "#eee8aa", oldlace: "#fdf5e6", paleturquoise: "#afeeee", mediumturquoise: "#48d1cc", mediumorchid: "#ba55d3", rebeccapurple: "#663399", lightsteelblue: "#b0c4de", mediumslateblue: "#7b68ee", thistle: "#d8bfd8", tan: "#d2b48c", orchid: "#da70d6", mediumpurple: "#9370db", purple: "#800080", pink: "#ffc0cb", skyblue: "#87ceeb", springgreen: "#00ff7f", palegreen: "#98fb98", red: "#ff0000", yellow: "#ffff00", slateblue: "#6a5acd", lavenderblush: "#fff0f5", peru: "#cd853f", palevioletred: "#db7093", violet: "#ee82ee", teal: "#008080", slategray: "#708090", slategrey: "#708090", aliceblue: "#f0f8ff", darkseagreen: "#8fbc8f", darkolivegreen: "#556b2f", greenyellow: "#adff2f", seagreen: "#2e8b57", seashell: "#fff5ee", tomato: "#ff6347", silver: "#c0c0c0", sienna: "#a0522d", lavender: "#e6e6fa", lightgreen: "#90ee90", orange: "#ffa500", orangered: "#ff4500", steelblue: "#4682b4", royalblue: "#4169e1", turquoise: "#40e0d0", yellowgreen: "#9acd32", salmon: "#fa8072", saddlebrown: "#8b4513", sandybrown: "#f4a460", rosybrown: "#bc8f8f", darksalmon: "#e9967a", lightgoldenrodyellow: "#fafad2", snow: "#fffafa", lightgrey: "#d3d3d3", lightgray: "#d3d3d3", dimgray: "#696969", dimgrey: "#696969", olivedrab: "#6b8e23", olive: "#808000" }, r2 = {};
  for (var d2 in a2)
    r2[a2[d2]] = d2;
  var l2 = {};
  e2.prototype.toName = function(f3) {
    if (!(this.rgba.a || this.rgba.r || this.rgba.g || this.rgba.b))
      return "transparent";
    var d3, i2, n2 = r2[this.toHex()];
    if (n2)
      return n2;
    if (f3 == null ? undefined : f3.closest) {
      var o2 = this.toRgb(), t2 = 1 / 0, b2 = "black";
      if (!l2.length)
        for (var c2 in a2)
          l2[c2] = new e2(a2[c2]).toRgb();
      for (var g2 in a2) {
        var u2 = (d3 = o2, i2 = l2[g2], Math.pow(d3.r - i2.r, 2) + Math.pow(d3.g - i2.g, 2) + Math.pow(d3.b - i2.b, 2));
        u2 < t2 && (t2 = u2, b2 = g2);
      }
      return b2;
    }
  };
  f2.string.push([function(f3) {
    var r3 = f3.toLowerCase(), d3 = r3 === "transparent" ? "#0000" : a2[r3];
    return d3 ? new e2(d3).toRgb() : null;
  }, "name"]);
}

// node_modules/.bun/colord@2.9.3/node_modules/colord/plugins/mix.mjs
var t2 = function(t3, a2, n2) {
  return a2 === undefined && (a2 = 0), n2 === undefined && (n2 = 1), t3 > n2 ? n2 : t3 > a2 ? t3 : a2;
};
var a2 = function(t3) {
  var a3 = t3 / 255;
  return a3 < 0.04045 ? a3 / 12.92 : Math.pow((a3 + 0.055) / 1.055, 2.4);
};
var n2 = function(t3) {
  return 255 * (t3 > 0.0031308 ? 1.055 * Math.pow(t3, 1 / 2.4) - 0.055 : 12.92 * t3);
};
var r2 = 96.422;
var o2 = 100;
var u2 = 82.521;
var e2 = function(a3) {
  var r3, o3, u3 = { x: 0.9555766 * (r3 = a3).x + -0.0230393 * r3.y + 0.0631636 * r3.z, y: -0.0282895 * r3.x + 1.0099416 * r3.y + 0.0210077 * r3.z, z: 0.0122982 * r3.x + -0.020483 * r3.y + 1.3299098 * r3.z };
  return o3 = { r: n2(0.032404542 * u3.x - 0.015371385 * u3.y - 0.004985314 * u3.z), g: n2(-0.00969266 * u3.x + 0.018760108 * u3.y + 0.00041556 * u3.z), b: n2(0.000556434 * u3.x - 0.002040259 * u3.y + 0.010572252 * u3.z), a: a3.a }, { r: t2(o3.r, 0, 255), g: t2(o3.g, 0, 255), b: t2(o3.b, 0, 255), a: t2(o3.a) };
};
var i2 = function(n3) {
  var e3 = a2(n3.r), i3 = a2(n3.g), p2 = a2(n3.b);
  return function(a3) {
    return { x: t2(a3.x, 0, r2), y: t2(a3.y, 0, o2), z: t2(a3.z, 0, u2), a: t2(a3.a) };
  }(function(t3) {
    return { x: 1.0478112 * t3.x + 0.0228866 * t3.y + -0.050127 * t3.z, y: 0.0295424 * t3.x + 0.9904844 * t3.y + -0.0170491 * t3.z, z: -0.0092345 * t3.x + 0.0150436 * t3.y + 0.7521316 * t3.z, a: t3.a };
  }({ x: 100 * (0.4124564 * e3 + 0.3575761 * i3 + 0.1804375 * p2), y: 100 * (0.2126729 * e3 + 0.7151522 * i3 + 0.072175 * p2), z: 100 * (0.0193339 * e3 + 0.119192 * i3 + 0.9503041 * p2), a: n3.a }));
};
var p2 = 216 / 24389;
var h2 = 24389 / 27;
var f2 = function(t3) {
  var a3 = i2(t3), n3 = a3.x / r2, e3 = a3.y / o2, f3 = a3.z / u2;
  return n3 = n3 > p2 ? Math.cbrt(n3) : (h2 * n3 + 16) / 116, { l: 116 * (e3 = e3 > p2 ? Math.cbrt(e3) : (h2 * e3 + 16) / 116) - 16, a: 500 * (n3 - e3), b: 200 * (e3 - (f3 = f3 > p2 ? Math.cbrt(f3) : (h2 * f3 + 16) / 116)), alpha: a3.a };
};
var c2 = function(a3, n3, i3) {
  var c3, y2 = f2(a3), x2 = f2(n3);
  return function(t3) {
    var a4 = (t3.l + 16) / 116, n4 = t3.a / 500 + a4, i4 = a4 - t3.b / 200;
    return e2({ x: (Math.pow(n4, 3) > p2 ? Math.pow(n4, 3) : (116 * n4 - 16) / h2) * r2, y: (t3.l > 8 ? Math.pow((t3.l + 16) / 116, 3) : t3.l / h2) * o2, z: (Math.pow(i4, 3) > p2 ? Math.pow(i4, 3) : (116 * i4 - 16) / h2) * u2, a: t3.alpha });
  }({ l: t2((c3 = { l: y2.l * (1 - i3) + x2.l * i3, a: y2.a * (1 - i3) + x2.a * i3, b: y2.b * (1 - i3) + x2.b * i3, alpha: y2.alpha * (1 - i3) + x2.alpha * i3 }).l, 0, 400), a: c3.a, b: c3.b, alpha: t2(c3.alpha) });
};
function mix_default(t3) {
  function a3(t4, a4, n3) {
    n3 === undefined && (n3 = 5);
    for (var r3 = [], o3 = 1 / (n3 - 1), u3 = 0;u3 <= n3 - 1; u3++)
      r3.push(t4.mix(a4, o3 * u3));
    return r3;
  }
  t3.prototype.mix = function(a4, n3) {
    n3 === undefined && (n3 = 0.5);
    var r3 = a4 instanceof t3 ? a4 : new t3(a4), o3 = c2(this.toRgb(), r3.toRgb(), n3);
    return new t3(o3);
  }, t3.prototype.tints = function(t4) {
    return a3(this, "#fff", t4);
  }, t3.prototype.shades = function(t4) {
    return a3(this, "#000", t4);
  }, t3.prototype.tones = function(t4) {
    return a3(this, "#808080", t4);
  };
}

// packages/core/src/color.ts
k([names_default, mix_default]);
function parseColor(color) {
  let { r: r3, g: g2, b: b2, a: a3 } = w(color).toRgb();
  return ((r3 & 255) << 24 | (g2 & 255) << 16 | (b2 & 255) << 8 | a3 * 255 & 255) >>> 0;
}
function isGradient(value) {
  return typeof value === "object" && value !== null && "__gradient" in value;
}

// packages/core/src/renderer.ts
var nodes = new Map;
var id = 1;
function createProxyNode(elementType) {
  let node = { id, elementType, children: [] };
  nodes.set(id, node);
  id += 1;
  return node;
}
var pendingDestroy = new Map;
var destroyScheduled = false;
function destroyNode2(node) {
  tree2.destroyNode(node.id);
  let cleanup2 = (n3) => {
    for (let child of n3.children)
      if (child.parent === n3)
        cleanup2(child);
    if (n3.id === getFocusedNodeId())
      setFocus(null);
    nodes.delete(n3.id);
    cleanupNodeHandlers(n3.id);
  };
  cleanup2(node);
}
function flushDestroy() {
  destroyScheduled = false;
  let batch = pendingDestroy;
  pendingDestroy = new Map;
  for (let node of batch.values()) {
    if (node.parent === undefined)
      destroyNode2(node);
  }
}
function removeNode(parent, node) {
  if (!node || !parent)
    return;
  let index = parent.children.indexOf(node);
  if (index !== -1) {
    parent.children.splice(index, 1);
  }
  node.parent = undefined;
  tree2.detachNode(parent.id, node.id);
  pendingDestroy.set(node.id, node);
  if (!destroyScheduled) {
    destroyScheduled = true;
    Promise.resolve().then(flushDestroy);
  }
}
function applyProp(node, name, value) {
  if (!node)
    return;
  if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
    setEventHandler(node.id, name, value);
    return;
  }
  if (name === "color" && isGradient(value)) {
    tree2.setProperty(node.id, name, value);
    return;
  }
  if (name === "color" && typeof value === "string") {
    tree2.setProperty(node.id, name, parseColor(value));
    return;
  }
  tree2.setProperty(node.id, name, value);
}
var {
  effect: effect3,
  memo: memo2,
  createComponent: createComponent2,
  createElement,
  createTextNode,
  insertNode: insertNode2,
  insert,
  spread,
  setProp,
  mergeProps,
  applyRef,
  ref
} = createRenderer({
  createElement: (elementType, props) => {
    let proxy = createProxyNode(elementType);
    if (elementType === "window")
      tree2.createRoot(proxy.id);
    else
      tree2.createNode(proxy.id, elementType);
    if (props) {
      for (let name in props) {
        if (name === "children" || name === "ref")
          continue;
        applyProp(proxy, name, props[name]);
      }
    }
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode("d-span");
    tree2.createNode(proxy.id, "d-span");
    tree2.setProperty(proxy.id, "text", "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    tree2.setProperty(node.id, "text", "" + value);
  },
  isTextNode: (node) => node?.elementType === "d-span",
  setProperty: (node, name, value) => {
    applyProp(node, name, value);
  },
  insertNode: (parent, node, anchor) => {
    if (!node)
      return;
    pendingDestroy.delete(node.id);
    if (parent) {
      node.parent = parent;
      if (!anchor) {
        parent.children.push(node);
      } else {
        let index = parent.children.indexOf(anchor);
        if (index === -1) {
          parent.children.push(node);
        } else {
          parent.children.splice(index, 0, node);
        }
      }
      if (anchor)
        tree2.insertNode(parent.id, node.id, anchor.id);
      else
        tree2.insertNode(parent.id, node.id);
    }
  },
  removeNode,
  getParentNode: (node) => node?.parent,
  getFirstChild: (node) => node?.children[0],
  getNextSibling: (node) => {
    let parent = node?.parent;
    if (!parent)
      return;
    let index = parent.children.indexOf(node);
    if (index === -1)
      return;
    return parent.children[index + 1];
  }
});
var windowRoot;
function render(code) {
  createRoot(() => {
    let root = code();
    if (!root || root.elementType !== "window") {
      throw new Error("render() root must be a <window> element");
    }
    windowRoot = root;
    attachWindow(root.id);
    insert(null, root);
  });
}
// packages/core/src/environment.ts
import { on as on2 } from "srt:events";
// packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { destroyTexture as destroyTexture2, setShaderParams, uploadTexture } from "flux:gpu";
import { captureSnapshot, readTexture } from "flux:gpu";
// packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on3 } from "srt:events";
var devicesAccessor;
function cameraDevices() {
  if (!devicesAccessor) {
    let [devices, setDevices] = createSignal(listCameras());
    on3("cameraDeviceChange", () => setDevices(listCameras()));
    devicesAccessor = devices;
  }
  return devicesAccessor();
}
function createCamera(options = {}) {
  let [texture, setTexture] = createSignal(undefined);
  let [width, setWidth] = createSignal(undefined);
  let [height, setHeight] = createSignal(undefined);
  let [barcode, setBarcode] = createSignal(undefined);
  let [error, setError] = createSignal(undefined);
  let session;
  let disposed = false;
  open(options).then((cam) => {
    if (disposed) {
      cam.close();
      return;
    }
    session = cam;
    if (options.scan)
      cam.onBarcode((result) => setBarcode(result));
    setTexture(cam.texture);
    setWidth(cam.width);
    setHeight(cam.height);
  }).catch((e3) => setError(e3 instanceof Error ? e3 : new Error(String(e3))));
  onCleanup(() => {
    disposed = true;
    if (session) {
      session.close();
      session = undefined;
    }
  });
  return { texture, width, height, barcode, error };
}

// lattice/default-app/app.tsx
import { on as on4 } from "srt:events";
import { available as devAvailable, canDiscover, connect, discover, stop, recents as initialRecents, launchAddress } from "srt:dev";

// lattice/default-app/logo.tsx
var EXPLODE_DIST = 3;
var STAGGER_DELAY = 100;
var ANIM_DURATION = 600;
var HOLD_ASSEMBLED = 5000;
var HOLD_EXPLODED = 0;
var SOLID_COLORS = {
  dark: "rgba(26,51,128)",
  mid: "rgba(51,102,179)",
  light: "rgba(102,153,230)"
};
var RT_COLORS = {
  dark: "rgba(100,100,100)",
  mid: "rgba(140,140,140)",
  light: "rgba(180,180,180)"
};
var M2 = 25;
var R = M2 * Math.SQRT2;
var T = -0.5 * R;
var sq = [[0, 0], [2 * M2, 0], [2 * M2, 2 * M2], [0, 2 * M2]];
var tri1 = [[0, 0], [2 * M2, 0], [0, 2 * M2]];
var tri2 = [[0, 0], [2 * R, 0], [0, 2 * R]];
var tri3 = [[0, 0], [4 * M2, 0], [0, 4 * M2]];
var par1 = [[0, 0], [2 * M2, 0], [4 * M2, 2 * M2], [2 * M2, 2 * M2]];
var par2 = [[2 * M2, 0], [4 * M2, 0], [2 * M2, 2 * M2], [0, 2 * M2]];
function shapeCenter(shape, rotate) {
  let radians = rotate * Math.PI / 4;
  let cos = Math.cos(radians);
  let sin = Math.sin(radians);
  let pts = shape.map(([x2, y2]) => [x2 * cos - y2 * sin, x2 * sin + y2 * cos]);
  let minX = Math.min(...pts.map(([x2]) => x2));
  let minY = Math.min(...pts.map(([, y2]) => y2));
  pts = pts.map(([x2, y2]) => [x2 - minX, y2 - minY]);
  let area = 0;
  let cx = 0;
  let cy = 0;
  for (let i3 = 0;i3 < pts.length; i3++) {
    let [x0, y0] = pts[i3];
    let [x1, y1] = pts[(i3 + 1) % pts.length];
    let cross = x0 * y1 - x1 * y0;
    area += cross;
    cx += (x0 + x1) * cross;
    cy += (y0 + y1) * cross;
  }
  area *= 0.5;
  cx /= 6 * area;
  cy /= 6 * area;
  return [cx, cy];
}
function path(shape, rotate) {
  let radians = rotate * Math.PI / 4;
  let cos = Math.cos(radians);
  let sin = Math.sin(radians);
  let rotated = shape.map(([x2, y2]) => [x2 * cos - y2 * sin, x2 * sin + y2 * cos]);
  let minX = Math.min(...rotated.map(([x2]) => x2));
  let minY = Math.min(...rotated.map(([, y2]) => y2));
  let d2 = "M" + rotated.map(([x2, y2]) => `${x2 - minX} ${y2 - minY}`).join("L") + "Z";
  return d2;
}
var letters = [
  {
    width: 5 * R - 0.5 * R,
    height: 6 * R,
    pieces: [{
      shape: tri1,
      x: R,
      y: 5 * R,
      rot: 1,
      shade: "light"
    }, {
      shape: sq,
      x: 0,
      y: 4 * R,
      rot: 1,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R,
      y: 4 * R,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: 3 * R,
      y: 2 * R,
      rot: 3,
      shade: "mid"
    }, {
      shape: tri3,
      x: R,
      y: 0,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri2,
      x: 3 * R,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: par2,
      x: 5 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "light"
    }]
  },
  {
    width: 4 * R + 2 * M2 - 0.5 * R,
    height: 2 * M2 + 4 * R,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: sq,
      x: 2 * R,
      y: 4 * R,
      rot: 0,
      shade: "light"
    }, {
      shape: tri3,
      x: 2 * R + 2 * M2,
      y: 0,
      rot: 3,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R - 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R,
      y: 0,
      rot: 2,
      shade: "dark"
    }, {
      shape: par1,
      x: 2 * R + 2 * M2,
      y: 4 * R - 2 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri2,
      x: 2 * R - 2 * M2,
      y: 0,
      rot: 1,
      shade: "light"
    }]
  },
  {
    width: 4 * M2 + 2 * R,
    height: 4 * M2 + 4 * R,
    pieces: [{
      shape: sq,
      x: 2 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "light"
    }, {
      shape: tri1,
      x: 2 * R - 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: 2 * R - 2 * M2,
      y: 4 * R,
      rot: -2,
      shade: "mid"
    }, {
      shape: par1,
      x: 2 * R,
      y: 4 * R + 2 * M2,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri2,
      x: 4 * M2,
      y: 2 * R + 4 * M2,
      rot: 2,
      shade: "mid"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: R + 4 * M2,
      rot: 1,
      shade: "light"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: sq,
      x: 4 * M2,
      y: 0,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "light"
    }, {
      shape: par2,
      x: 2 * M2,
      y: 2 * M2,
      rot: -2,
      shade: "light"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 0,
      rot: -1,
      shade: "mid"
    }, {
      shape: tri3,
      x: 2 * M2,
      y: 4 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri1,
      x: 0,
      y: 6 * M2,
      rot: 4,
      shade: "mid"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 6 * M2,
      rot: 2,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 4 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri1,
      x: 2 * M2,
      y: 0,
      rot: 4,
      shade: "dark"
    }, {
      shape: par2,
      x: 4 * M2,
      y: 0,
      rot: 2,
      shade: "light"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 2 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: sq,
      x: 4 * M2,
      y: 4 * M2,
      rot: 0,
      shade: "light"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 6 * M2,
      rot: -3,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 4 * M2,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 0,
      rot: 1,
      shade: "dark"
    }, {
      shape: sq,
      x: 4 * M2 - R,
      y: 4 * M2,
      rot: 1,
      shade: "light"
    }, {
      shape: tri1,
      x: 0,
      y: 6 * M2,
      rot: 4,
      shade: "light"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 4 * M2 + R,
      rot: -1,
      shade: "mid"
    }, {
      shape: par2,
      x: 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 4 * M2 + 4 * R,
    pieces: [{
      shape: par1,
      x: T + 2 * R - 2 * M2,
      y: 0,
      rot: -2,
      shade: "light"
    }, {
      shape: tri1,
      x: T + 2 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: T + 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: T + 2 * R - 2 * M2,
      y: 4 * R,
      rot: -2,
      shade: "mid"
    }, {
      shape: tri2,
      x: T + 2 * R,
      y: 2 * M2,
      rot: -3,
      shade: "light"
    }, {
      shape: tri1,
      x: T + 2 * R,
      y: 2 * M2,
      rot: -2,
      shade: "mid"
    }, {
      shape: sq,
      x: T + 2 * M2 + R,
      y: 4 * M2 + 2 * R,
      rot: 1,
      shade: "dark"
    }]
  }
];
function TangramLetter(props) {
  let [dist, setDist] = createSignal(EXPLODE_DIST);
  let letterCx = props.letter.width / 2;
  let letterCy = props.letter.height / 2;
  let pieceVectors = props.letter.pieces.map((p3) => {
    let [scx, scy] = shapeCenter(p3.shape, p3.rot);
    return [p3.x + scx - letterCx, p3.y + scy - letterCy];
  });
  let pieceSpins = props.letter.pieces.map((_, i3) => ((i3 * 7 + 3) % 11 - 5) * 30);
  onFrame((tick, frame) => {
    let cycleLen = ANIM_DURATION + HOLD_ASSEMBLED + ANIM_DURATION + HOLD_EXPLODED;
    let t3 = (tick - props.delay) % cycleLen;
    if (t3 < 0) {
      setDist(EXPLODE_DIST);
    } else if (t3 < ANIM_DURATION) {
      let p3 = t3 / ANIM_DURATION;
      let ease = p3 * p3 * (3 - 2 * p3);
      setDist((1 - ease) * EXPLODE_DIST);
    } else if (t3 < ANIM_DURATION + HOLD_ASSEMBLED) {
      setDist(0);
    } else if (t3 < 2 * ANIM_DURATION + HOLD_ASSEMBLED) {
      let p3 = (t3 - ANIM_DURATION - HOLD_ASSEMBLED) / ANIM_DURATION;
      let ease = p3 * p3 * (3 - 2 * p3);
      setDist(ease * EXPLODE_DIST);
    } else {
      setDist(EXPLODE_DIST);
    }
  });
  var _el$ = createElement("view");
  insert(_el$, () => props.letter.pieces.map((p3, i3) => (() => {
    var _el$2 = createElement("view"), _el$3 = createElement("d-path");
    insertNode2(_el$2, _el$3);
    effect3(() => ({
      e: pieceVectors[i3][0] * dist(),
      t: pieceVectors[i3][1] * dist(),
      a: 1 + dist() * 0.5,
      o: pieceSpins[i3] * dist() / EXPLODE_DIST / 150,
      i: props.colors[p3.shade],
      n: p3.x,
      s: p3.y,
      h: path(p3.shape, p3.rot)
    }), ({
      e: e3,
      t: t3,
      a: a3,
      o: o3,
      i: i4,
      n: n3,
      s: s2,
      h: h3
    }, _p$) => {
      e3 !== _p$?.e && setProp(_el$2, "x", e3, _p$?.e);
      t3 !== _p$?.t && setProp(_el$2, "y", t3, _p$?.t);
      a3 !== _p$?.a && setProp(_el$2, "scale", a3, _p$?.a);
      o3 !== _p$?.o && setProp(_el$2, "rotate", o3, _p$?.o);
      i4 !== _p$?.i && setProp(_el$3, "color", i4, _p$?.i);
      n3 !== _p$?.n && setProp(_el$3, "x", n3, _p$?.n);
      s2 !== _p$?.s && setProp(_el$3, "y", s2, _p$?.s);
      h3 !== _p$?.h && setProp(_el$3, "d", h3, _p$?.h);
    });
    return _el$2;
  })()));
  effect3(() => ({
    e: props.letter.width,
    t: props.letter.height,
    a: props.letter.scale
  }), ({
    e: e3,
    t: t3,
    a: a3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$, "width", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$, "height", t3, _p$?.t);
    a3 !== _p$?.a && setProp(_el$, "scale", a3, _p$?.a);
  });
  return _el$;
}
var LOGO_HEIGHT = Math.max(...letters.map((l2) => l2.height));
function Logo() {
  let scale = () => windowSize().width * 1.12 / 1500;
  var _el$4 = createElement("view", {
    justifyContent: "center",
    alignItems: "center",
    width: 1500
  }), _el$5 = createElement("view", {
    gap: 30,
    flexDirection: "row",
    alignItems: "flex-end"
  });
  insertNode2(_el$4, _el$5);
  insert(_el$5, () => letters.map((letter, i3) => createComponent2(TangramLetter, {
    letter,
    colors: i3 < 5 ? SOLID_COLORS : RT_COLORS,
    delay: i3 * STAGGER_DELAY
  })));
  effect3(() => ({
    e: LOGO_HEIGHT * scale(),
    t: scale()
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$4, "height", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$4, "scale", t3, _p$?.t);
  });
  return _el$4;
}

// lattice/default-app/app.tsx
var STATUS_TEXT = {
  idle: "not connected",
  searching: "searching...",
  connecting: "connecting...",
  connected: "connected"
};
function normalizeAddress(raw) {
  return raw.trim().replace(/^(ws|http):\/\//, "").replace(/\/+$/, "");
}
function Button(props) {
  var _el$ = createElement("view", {
    paddingLeft: 18,
    paddingRight: 18,
    paddingTop: 10,
    paddingBottom: 10,
    justifyContent: "center",
    alignItems: "center"
  }), _el$2 = createElement("d-rect", {
    radius: 8
  }), _el$3 = createElement("text", {
    color: "white"
  });
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  insert(_el$3, () => props.label);
  effect3(() => ({
    e: props.onTap,
    t: props.color
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$, "onPointerDown", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "color", t3, _p$?.t);
  });
  return _el$;
}
function CameraView(props) {
  let cam = createCamera(untrack(() => ({
    width: props.width,
    scan: props.scan
  })));
  createEffect(() => cam.barcode(), (b2) => {
    if (b2)
      props.onBarcode?.(b2);
  });
  createEffect(() => cam.error(), (e3) => {
    if (e3)
      props.onError?.(e3);
  });
  var _el$4 = createElement("texture");
  effect3(() => ({
    e: cam.texture(),
    t: props.width
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$4, "src", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$4, "width", t3, _p$?.t);
  });
  return _el$4;
}
function recentLabel(entry) {
  if (!entry.includes("|"))
    return entry;
  let id2 = entry.split("|")[0];
  return "ticket " + id2.slice(0, 8);
}
function App() {
  let dev = devAvailable;
  let hasCamera = () => cameraDevices().length > 0;
  let [state, setState] = createSignal("idle");
  let [address, setAddress] = createSignal(null);
  let [tunneled, setTunneled] = createSignal(false);
  let [recents, setRecents] = createSignal(initialRecents);
  let [scanning, setScanning] = createSignal(false);
  let [scanError, setScanError] = createSignal(null);
  if (dev) {
    on4("dev", (e3) => {
      setState(e3.state);
      setAddress(e3.address);
      setTunneled(e3.tunneled);
      if (e3.recents) {
        setRecents(e3.recents);
        console.log("got recents", e3.recents);
      }
    });
  }
  if (dev && launchAddress && state() === "idle") {
    connect(launchAddress);
  }
  let idle = () => state() === "idle";
  let busy = () => state() === "searching" || state() === "connecting";
  let connected = () => state() === "connected";
  let status = () => scanning() ? "scan the dev server QR code" : connected() ? `connected to ${address()}${tunneled() ? " (tunneled)" : ""}` : scanError() ?? STATUS_TEXT[state()];
  let startScan = () => {
    setScanError(null);
    setScanning(true);
  };
  let onScanned = (data) => {
    setScanning(false);
    connect(normalizeAddress(data));
  };
  var _el$5 = createElement("window", {
    title: "solidrt-go"
  }), _el$6 = createElement("d-rect", {
    color: "#111"
  }), _el$7 = createElement("view", {
    flexGrow: 1,
    justifyContent: "center",
    alignItems: "center",
    flexDirection: "column-reverse",
    gap: 40
  }), _el$8 = createElement("view", {
    flexDirection: "column",
    alignItems: "center",
    gap: 16
  }), _el$9 = createElement("text", {
    color: "lightgrey"
  }), _el$0 = createElement("view", {
    flexDirection: "row",
    gap: 12
  });
  insertNode2(_el$5, _el$6);
  insertNode2(_el$5, _el$7);
  insertNode2(_el$7, _el$8);
  insertNode2(_el$8, _el$9);
  insertNode2(_el$8, _el$0);
  insert(_el$8, createComponent2(Show, {
    get when() {
      return scanning();
    },
    get children() {
      return createComponent2(CameraView, {
        width: 280,
        scan: ["qr"],
        onBarcode: (r3) => onScanned(r3.data),
        onError: (e3) => {
          setScanError(`camera: ${e3.message}`);
          setScanning(false);
        }
      });
    }
  }), _el$9);
  insert(_el$9, status);
  insert(_el$0, (() => {
    var _c$ = memo2(() => !!(idle() && !scanning() && canDiscover));
    return () => _c$() ? createComponent2(Button, {
      label: "Discover",
      color: "#3366b3",
      onTap: () => discover()
    }) : idle() && !scanning() && canDiscover;
  })(), null);
  insert(_el$0, (() => {
    var _c$2 = memo2(() => !!(idle() && !scanning() && dev && hasCamera()));
    return () => _c$2() ? createComponent2(Button, {
      label: "Scan QR",
      color: "#3366b3",
      onTap: startScan
    }) : idle() && !scanning() && dev && hasCamera();
  })(), null);
  insert(_el$0, (() => {
    var _c$3 = memo2(() => !!(idle() && !scanning() && launchAddress));
    return () => _c$3() ? createComponent2(Button, {
      label: "Connect",
      color: "#3366b3",
      onTap: () => connect(launchAddress)
    }) : idle() && !scanning() && launchAddress;
  })(), null);
  insert(_el$0, (() => {
    var _c$4 = memo2(() => !!scanning());
    return () => _c$4() ? createComponent2(Button, {
      label: "Cancel",
      color: "#555",
      onTap: () => setScanning(false)
    }) : scanning();
  })(), null);
  insert(_el$0, (() => {
    var _c$5 = memo2(() => !!busy());
    return () => _c$5() ? createComponent2(Button, {
      label: "Cancel",
      color: "#555",
      onTap: () => stop()
    }) : busy();
  })(), null);
  insert(_el$0, (() => {
    var _c$6 = memo2(() => !!connected());
    return () => _c$6() ? createComponent2(Button, {
      label: "Disconnect",
      color: "#555",
      onTap: () => stop()
    }) : connected();
  })(), null);
  insert(_el$8, createComponent2(Show, {
    get when() {
      return memo2(() => !!(idle() && !scanning()))() ? recents().length > 0 : idle() && !scanning();
    },
    get children() {
      var _el$1 = createElement("view", {
        flexDirection: "column",
        alignItems: "center",
        gap: 8
      }), _el$10 = createElement("text", {
        color: "grey"
      });
      insertNode2(_el$1, _el$10);
      insertNode2(_el$10, createTextNode(`recent`));
      insert(_el$1, createComponent2(For, {
        get each() {
          return recents();
        },
        children: (addr) => createComponent2(Button, {
          get label() {
            return recentLabel(addr);
          },
          color: "#333",
          onTap: () => connect(addr)
        })
      }), null);
      return _el$1;
    }
  }), null);
  insert(_el$7, createComponent2(Logo, {}), null);
  return _el$5;
}
render(() => createComponent2(App, {}));
