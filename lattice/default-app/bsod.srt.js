// node_modules/.bun/@solidjs+signals@2.0.0-beta.15/node_modules/@solidjs/signals/dist/prod.js
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
  e.O = false;
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
  e.l[0].push(...t.l[0]);
  e.l[1].push(...t.l[1]);
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
  return !!(e.R !== undefined && e.R !== NOT_PENDING);
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
var dirtyQueue = { I: new Array(2000).fill(undefined), p: false, h: 0, N: 0 };
var zombieQueue = { I: new Array(2000).fill(undefined), p: false, h: 0, N: 0 };
var clock = 0;
var activeTransition = null;
var scheduled = false;
var syncDepth = 0;
var projectionWriteActive = false;
var stashedOptimisticReads = null;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(e) {
  return transitions.size === 0 && activeLanes.size === 0 && e.A.length === 0 && e.P.length === 0 && e.C.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.D !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.m !== NOT_PENDING)
      continue;
    if (e.R !== undefined && e.R !== NOT_PENDING)
      continue;
    transientStoreNodes.delete(e);
    e.V?.();
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
  for (let t = e.D;t !== null; t = t.L) {
    const e2 = t.U;
    if (!e2.G)
      continue;
    if (e2.G === EFFECT_TRACKED) {
      if (!e2.k) {
        e2.k = true;
        e2.F.enqueue(EFFECT_USER, e2.W);
      }
      continue;
    }
    const n = e2.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (n.h > e2.H)
      n.h = e2.H;
    insertIntoHeap(e2, n);
  }
}
function mergeTransitionState(e, t) {
  t.j = e;
  e.$.push(...t.$);
  for (const n of activeLanes)
    if (n.S === t)
      n.S = e;
  e.P.push(...t.P);
  for (const n of t.C)
    e.C.add(n);
  for (const [n, i] of t.K) {
    let t2 = e.K.get(n);
    if (!t2)
      e.K.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
  for (const n of t.Y)
    e.Y.add(n);
}
function resolveOptimisticNodes(e) {
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    n.i = undefined;
    if (n.m !== NOT_PENDING) {
      n.Z = n.m;
      n.m = NOT_PENDING;
    }
    const i = n.R;
    n.R = NOT_PENDING;
    if (i !== NOT_PENDING && n.Z !== i)
      insertSubs(n, true);
    n.S = null;
  }
  e.length = 0;
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
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.B && !projectionWriteActive)
    queueMicrotask(flush);
}

class Queue {
  q = null;
  X = [[], []];
  A = [];
  created = clock;
  addChild(e) {
    this.A.push(e);
    e.q = this;
  }
  removeChild(e) {
    const t = this.A.indexOf(e);
    if (t >= 0) {
      this.A.splice(t, 1);
      e.q = null;
    }
  }
  notify(e, t, n, i) {
    if (this.q)
      return this.q.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.X[e - 1].length) {
      const t = this.X[e - 1];
      this.X[e - 1] = [];
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
        this.X[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.X[0].push(...this.X[0]);
    e.X[1].push(...this.X[1]);
    this.X = [[], []];
    for (let t = 0;t < this.A.length; t++) {
      let n = this.A[t];
      let i = e.A[t];
      if (!i) {
        i = { X: [[], []], A: [] };
        e.A[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.X[0].push(...e.X[0]);
    this.X[1].push(...e.X[1]);
    for (let t = 0;t < e.A.length; t++) {
      const n = e.A[t];
      let i = this.A[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue extends Queue {
  B = false;
  J = null;
  ee = [];
  P = [];
  C = new Set;
  static te;
  static ne;
  static ie;
  static re = null;
  flush() {
    if (this.B)
      return;
    this.B = true;
    try {
      runHeap(dirtyQueue, GlobalQueue.te);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, GlobalQueue.te);
          this.J = null;
          this.ee = [];
          this.P = [];
          this.C = new Set;
          runLaneEffects(EFFECT_RENDER);
          runLaneEffects(EFFECT_USER);
          this.stashQueues(e2.oe);
          clock++;
          scheduled = dirtyQueue.N >= dirtyQueue.h;
          reassignPendingTransition(e2.ee);
          activeTransition = null;
          if (!e2.$.length && !e2.K.size && e2.P.length) {
            stashedOptimisticReads = new Set;
            for (let t2 = 0;t2 < e2.P.length; t2++) {
              const n = e2.P[t2];
              if (n.se || n.ue & CONFIG_OWNED_WRITE)
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
        this.ee !== activeTransition.ee && this.ee.push(...activeTransition.ee);
        this.restoreQueues(activeTransition.oe);
        transitions.delete(activeTransition);
        const t = activeTransition;
        activeTransition = null;
        reassignPendingTransition(this.ee);
        finalizePureQueue(t);
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.N >= dirtyQueue.h) {
            runHeap(dirtyQueue, GlobalQueue.te);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.te);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.N >= dirtyQueue.h;
      activeLanes.size && runLaneEffects(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && runLaneEffects(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
    } finally {
      this.B = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING) {
      if (n & STATUS_PENDING) {
        const t2 = i !== undefined ? i : e.ce;
        if (activeTransition && t2) {
          const n2 = t2.source;
          let i2 = activeTransition.K.get(n2);
          if (!i2)
            activeTransition.K.set(n2, i2 = new Set);
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
    if (!e && activeTransition && activeTransition.le === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? {
        le: clock,
        ee: [],
        K: new Map,
        P: [],
        C: new Set,
        $: [],
        oe: { X: [[], []], A: [] },
        j: false,
        Y: new Set
      };
    } else if (e) {
      const t = activeTransition;
      mergeTransitionState(e, t);
      transitions.delete(t);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.le = clock;
    if (this.J !== null) {
      this.J.S = activeTransition;
      activeTransition.ee.push(this.J);
      this.J = null;
    }
    if (this.ee !== activeTransition.ee) {
      for (let e2 = 0;e2 < this.ee.length; e2++) {
        const t = this.ee[e2];
        t.S = activeTransition;
        activeTransition.ee.push(t);
      }
      this.ee = activeTransition.ee;
    }
    if (this.P !== activeTransition.P) {
      for (let e2 = 0;e2 < this.P.length; e2++) {
        const t = this.P[e2];
        t.S = activeTransition;
        activeTransition.P.push(t);
      }
      this.P = activeTransition.P;
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
    globalQueue.ee.push(e);
    return;
  }
  if (globalQueue.J === null && globalQueue.ee.length === 0) {
    globalQueue.J = e;
    return;
  }
  if (globalQueue.J !== null) {
    globalQueue.ee.push(globalQueue.J);
    globalQueue.J = null;
  }
  globalQueue.ee.push(e);
}
function insertSubs(e, t = false) {
  const n = e.i || currentOptimisticLane;
  const i = e.ae !== undefined;
  for (let r = e.D;r !== null; r = r.L) {
    if (i && r.U.ue & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.U.M |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && n) {
      r.U.M |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.U, n);
    } else if (t) {
      r.U.M |= REACTIVE_OPTIMISTIC_DIRTY;
      r.U.i = undefined;
    }
    const e2 = r.U;
    if (e2.G === EFFECT_TRACKED) {
      if (!e2.k) {
        e2.k = true;
        e2.F.enqueue(EFFECT_USER, e2.W);
      }
      continue;
    }
    const o = r.U.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (o.h > r.U.H)
      o.h = r.U.H;
    insertIntoHeap(r.U, o);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.se) {
    if (e.m !== NOT_PENDING) {
      e.Z = e.m;
      e.m = NOT_PENDING;
    }
    return;
  }
  if (e.m !== NOT_PENDING) {
    e.Z = e.m;
    e.m = NOT_PENDING;
    if (e.G && e.G !== EFFECT_TRACKED)
      e.k = true;
  }
  t.M &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.fe & STATUS_PENDING))
    t.fe &= ~STATUS_UNINITIALIZED;
  if (t.Ee !== null || t.Te !== null)
    GlobalQueue.ne(t, false, true);
}
function commitPendingNodes() {
  if (globalQueue.J !== null) {
    commitPendingNode(globalQueue.J);
    globalQueue.J = null;
  }
  const e = globalQueue.ee;
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
  const i = dirtyQueue.N >= dirtyQueue.h;
  if (i)
    runHeap(dirtyQueue, GlobalQueue.te);
  if (n) {
    if (i)
      commitPendingNodes();
    resolveOptimisticNodes(e ? e.P : globalQueue.P);
    if (e && e.Y.size) {
      for (const t3 of e.Y) {
        if (t3.M & REACTIVE_DISPOSED)
          continue;
        if (t3.G === EFFECT_TRACKED) {
          if (!t3.k) {
            t3.k = true;
            t3.F.enqueue(EFFECT_USER, t3.W);
          }
          continue;
        }
        const e2 = t3.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
        if (e2.h > t3.H)
          e2.h = t3.H;
        insertIntoHeap(t3, e2);
      }
      e.Y.clear();
    }
    const t2 = e ? e.C : globalQueue.C;
    if (GlobalQueue.re && t2.size) {
      for (const e2 of t2) {
        GlobalQueue.re(e2);
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
  if (globalQueue.B) {
    return;
  }
  while (scheduled || activeTransition) {
    globalQueue.flush();
  }
}
function runQueue(e, t) {
  for (let n = 0;n < e.length; n++)
    e[n](t);
}
function reporterBlocksSource(e, t) {
  if (e.M & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.Se === t || e.de?.has(t))
    return true;
  for (let n = e._e;n; n = n.Oe) {
    let e2 = n.Re;
    while (e2) {
      if (e2 === t || e2.Ie === t)
        return true;
      e2 = e2.t;
    }
  }
  return !!(e.fe & STATUS_PENDING && e.ce instanceof NotReadyError && e.ce.source === t);
}
function transitionComplete(e) {
  if (e.j)
    return true;
  if (e.$.length)
    return false;
  let t = true;
  for (const [n, i] of e.K) {
    let r = false;
    for (const e2 of i) {
      if (reporterBlocksSource(e2, n)) {
        r = true;
        break;
      }
      i.delete(e2);
    }
    if (!r)
      e.K.delete(n);
    else if (n.fe & STATUS_PENDING && n.ce?.source === n) {
      t = false;
      break;
    }
  }
  if (t) {
    for (let n = 0;n < e.P.length; n++) {
      const i = e.P[n];
      if (hasActiveOverride(i) && "fe" in i && i.fe & STATUS_PENDING && i.ce instanceof NotReadyError && i.ce.source !== i) {
        t = false;
        break;
      }
    }
  }
  t && (e.j = true);
  return t;
}
function currentTransition(e) {
  while (e.j && typeof e.j === "object")
    e = e.j;
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
  const n = (e.q?.pe ? e.q.he?.H : e.q?.H) ?? -1;
  if (n >= e.H)
    e.H = n + 1;
  const i = e.H;
  const r = t.I[i];
  if (r === undefined)
    t.I[i] = e;
  else {
    const t2 = r.Ne;
    t2.Ae = e;
    e.Ne = t2;
    r.Ne = e;
  }
  if (i > t.N)
    t.N = i;
}
function insertIntoHeap(e, t) {
  let n = e.M;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (n & REACTIVE_CHECK) {
    e.M = n & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else
    e.M = n | REACTIVE_IN_HEAP;
  if (!(n & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, t);
}
function insertIntoHeapHeight(e, t) {
  let n = e.M;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.M = n | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, t);
}
function deleteFromHeap(e, t) {
  const n = e.M;
  if (!(n & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.M = n & -25;
  const i = e.H;
  if (e.Ne === e)
    t.I[i] = undefined;
  else {
    const n2 = e.Ae;
    const r = t.I[i];
    const o = n2 ?? r;
    if (e === r)
      t.I[i] = n2;
    else
      e.Ne.Ae = n2;
    o.Ne = e.Ne;
  }
  e.Ne = e;
  e.Ae = undefined;
}
function markHeap(e) {
  if (e.p)
    return;
  e.p = true;
  for (let t = 0;t <= e.N; t++) {
    for (let n = e.I[t];n !== undefined; n = n.Ae) {
      if (n.M & REACTIVE_IN_HEAP)
        markNode(n);
    }
  }
}
function markNode(e, t = REACTIVE_DIRTY) {
  const n = e.M;
  if ((n & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= t)
    return;
  e.M = n & -4 | t;
  for (let t2 = e.D;t2 !== null; t2 = t2.L) {
    markNode(t2.U, REACTIVE_CHECK);
  }
  if (e.Pe !== null) {
    for (let t2 = e.Pe;t2 !== null; t2 = t2.Ce) {
      for (let e2 = t2.D;e2 !== null; e2 = e2.L) {
        markNode(e2.U, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, t) {
  e.p = false;
  for (e.h = 0;e.h <= e.N; e.h++) {
    let n = e.I[e.h];
    while (n !== undefined) {
      if (n.M & REACTIVE_IN_HEAP)
        t(n);
      else
        adjustHeight(n, e);
      n = e.I[e.h];
    }
  }
  e.N = 0;
}
function adjustHeight(e, t) {
  deleteFromHeap(e, t);
  let n = e.H;
  for (let t2 = e._e;t2; t2 = t2.Oe) {
    const e2 = t2.Re;
    const i = e2.Ie || e2;
    if (i.se && i.H >= n)
      n = i.H + 1;
  }
  if (e.H !== n) {
    e.H = n;
    for (let t2 = e.D;t2 !== null; t2 = t2.L) {
      insertIntoHeapHeight(t2.U, t2.U.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    }
  }
}
function markDisposal(e) {
  let t = e.ge;
  while (t) {
    const e2 = t.M;
    t.M = e2 | REACTIVE_ZOMBIE;
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
  const i = e.M;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t)
    e.M = i | REACTIVE_DISPOSED;
  if (t && e.se)
    e.ve = null;
  let r = n ? e.Ee : e.ge;
  while (r) {
    const e2 = r.De;
    if (r._e) {
      const e3 = r;
      deleteFromHeap(e3, e3.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      let t2 = e3._e;
      do {
        t2 = unlinkSubs(t2);
      } while (t2 !== null);
      e3._e = null;
      e3.ye = null;
    }
    disposeChildren(r, true);
    r = e2;
  }
  if (n) {
    e.Ee = null;
  } else {
    e.ge = null;
    e.me = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e.q !== null && !(e.q.M & REACTIVE_DISPOSED)) {
    const t2 = e.be;
    const n2 = e.De;
    if (t2 !== null)
      t2.De = n2;
    else
      e.q.ge = n2;
    if (n2 !== null)
      n2.be = t2;
    e.be = null;
  }
  runDisposal(e, n);
}
function runDisposal(e, t) {
  let n = t ? e.Te : e.Ve;
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
  t ? e.Te = null : e.Ve = null;
}
function childId(e, t) {
  let n = e;
  while (n.ue & CONFIG_TRANSPARENT && n.q)
    n = n.q;
  if (n.id != null)
    return formatId(n.id, t ? n.me++ : n.me);
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
  if (!context.Ve)
    context.Ve = e;
  else if (Array.isArray(context.Ve))
    context.Ve.push(e);
  else
    context.Ve = [context.Ve, e];
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
    ue: n ? CONFIG_TRANSPARENT : 0,
    pe: true,
    he: t?.pe ? t.he : t,
    ge: null,
    De: null,
    be: null,
    Ve: null,
    F: t?.F ?? globalQueue,
    we: t?.we || defaultContext,
    me: 0,
    Te: null,
    Ee: null,
    q: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.ge;
    if (e2 === null) {
      t.ge = i;
    } else {
      i.De = e2;
      e2.be = i;
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
  const t = e.Re;
  const n = e.Oe;
  const i = e.L;
  const r = e.Le;
  if (i !== null)
    i.Le = r;
  else
    t.Ue = r;
  if (r !== null)
    r.L = i;
  else {
    t.D = i;
    if (i === null) {
      t.V?.();
      const e2 = t;
      e2.se && e2.ue & CONFIG_AUTO_DISPOSE && !(e2.M & REACTIVE_ZOMBIE) && unobserved(e2);
    }
  }
  return n;
}
function trimStaleDeps(e) {
  const t = e.ye;
  let n = t !== null ? t.Oe : e._e;
  if (n !== null) {
    do {
      n = unlinkSubs(n);
    } while (n !== null);
    if (t !== null)
      t.Oe = null;
    else
      e._e = null;
  }
}
function unobserved(e) {
  deleteFromHeap(e, e.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  let t = e._e;
  while (t !== null) {
    t = unlinkSubs(t);
  }
  e._e = null;
  e.ye = null;
  disposeChildren(e, true);
}
function link(e, t) {
  const n = t.ye;
  if (n !== null && n.Re === e)
    return;
  let i = null;
  const r = t.M & REACTIVE_RECOMPUTING_DEPS;
  if (r) {
    i = n !== null ? n.Oe : t._e;
    if (i !== null && i.Re === e) {
      t.ye = i;
      return;
    }
  }
  const o = e.Ue;
  if (o !== null && o.U === t && (!r || isValidLink(o, t)))
    return;
  const s = t.ye = e.Ue = { Re: e, U: t, Oe: i, Le: o, L: null };
  if (n !== null)
    n.Oe = s;
  else
    t._e = s;
  if (o !== null)
    o.L = s;
  else
    e.D = s;
}
function isValidLink(e, t) {
  const n = t.ye;
  if (n !== null) {
    let i = t._e;
    do {
      if (i === e)
        return true;
      if (i === n)
        break;
      i = i.Oe;
    } while (i !== null);
  }
  return false;
}
function addPendingSource(e, t) {
  if (e.Se === t || e.de?.has(t))
    return false;
  if (!e.Se) {
    e.Se = t;
    return true;
  }
  if (!e.de) {
    e.de = new Set([e.Se, t]);
  } else {
    e.de.add(t);
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
  if (!e.de?.delete(t))
    return false;
  if (e.de.size === 1) {
    e.Se = e.de.values().next().value;
    e.de = undefined;
  } else if (e.de.size === 0) {
    e.de = undefined;
  }
  return true;
}
function clearPendingSources(e) {
  e.Se = undefined;
  e.de?.clear();
  e.de = undefined;
}
function setPendingError(e, t, n) {
  if (!t) {
    e.ce = null;
    return;
  }
  if (n instanceof NotReadyError && n.source === t) {
    e.ce = n;
    return;
  }
  const i = e.ce;
  if (!(i instanceof NotReadyError) || i.source !== t) {
    e.ce = new NotReadyError(t);
  }
}
function forEachDependent(e, t) {
  for (let n = e.D;n !== null; n = n.L)
    t(n.U);
  for (let n = e.Pe;n !== null; n = n.Ce) {
    for (let e2 = n.D;e2 !== null; e2 = e2.L)
      t(e2.U);
  }
}
function settlePendingSource(e) {
  let t = false;
  const n = new Set;
  const settle = (i) => {
    if (n.has(i) || !removePendingSource(i, e))
      return;
    n.add(i);
    i.le = clock;
    const r = i.Se ?? i.de?.values().next().value;
    if (r) {
      setPendingError(i, r);
      updatePendingSignal(i);
    } else {
      i.fe &= ~STATUS_PENDING;
      setPendingError(i);
      updatePendingSignal(i);
      if (i.Ge) {
        if (i.G === EFFECT_TRACKED) {
          const e2 = i;
          if (!e2.k) {
            e2.k = true;
            e2.F.enqueue(EFFECT_USER, e2.W);
          }
        } else {
          const e2 = i.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
          if (e2.h > i.H)
            e2.h = i.H;
          insertIntoHeap(i, e2);
        }
        t = true;
      }
      i.Ge = false;
    }
    forEachDependent(i, settle);
  };
  forEachDependent(e, settle);
  if (t)
    schedule();
}
function handleAsync(e, t, n) {
  let i = false;
  let r = false;
  if (typeof t === "object" && t !== null) {
    untrack(() => {
      i = t[Symbol.asyncIterator];
      r = !i && typeof t.then === "function";
    });
  }
  if (!r && !i) {
    e.ve = null;
    return t;
  }
  e.ve = t;
  let o;
  const handleError = (n2) => {
    if (e.ve !== t)
      return;
    globalQueue.initTransition(resolveTransition(e));
    notifyStatus(e, n2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, n2);
    e.le = clock;
  };
  const asyncWrite = (i2, r2) => {
    if (e.ve !== t)
      return;
    if (e.M & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    globalQueue.initTransition(resolveTransition(e));
    const o2 = !!(e.fe & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const s = resolveLane(e);
    if (s)
      s.u.delete(e);
    if (n) {
      n(i2);
      if (o2)
        clearStatus(e, true);
    } else if (e.R !== undefined) {
      if (e.R !== undefined && e.R !== NOT_PENDING)
        e.m = i2;
      else {
        e.Z = i2;
        insertSubs(e);
      }
      e.le = clock;
    } else if (s) {
      const t2 = e.G;
      const n2 = e.Z;
      const r3 = e.ke;
      if (!t2 && o2 || !r3 || !r3(i2, n2)) {
        e.Z = i2;
        e.le = clock;
        if (e.Fe) {
          setSignal(e.Fe, i2);
        }
        insertSubs(e, true);
      }
    } else {
      setSignal(e, () => i2);
    }
    settlePendingSource(e);
    schedule();
    flush();
    r2?.();
  };
  if (r) {
    let n2 = false, i2 = true;
    t.then((e2) => {
      if (i2) {
        o = e2;
        n2 = true;
      } else
        asyncWrite(e2);
    }, (e2) => {
      if (!i2)
        handleError(e2);
    });
    i2 = false;
    if (!n2) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  if (i) {
    const n2 = t[Symbol.asyncIterator]();
    let i2 = false;
    let r2 = false;
    cleanup(() => {
      if (r2)
        return;
      r2 = true;
      try {
        const e2 = n2.return?.();
        if (e2 && typeof e2.then === "function") {
          e2.then(undefined, () => {});
        }
      } catch {}
    });
    const iterate = () => {
      let s2, u = false, c = true;
      n2.next().then((n3) => {
        if (c) {
          s2 = n3;
          u = true;
          if (n3.done)
            r2 = true;
        } else if (e.ve !== t) {
          return;
        } else if (!n3.done)
          asyncWrite(n3.value, iterate);
        else {
          r2 = true;
          schedule();
          flush();
        }
      }, (n3) => {
        if (!c && e.ve === t) {
          r2 = true;
          handleError(n3);
        }
      });
      c = false;
      if (u && !s2.done) {
        o = s2.value;
        i2 = true;
        return iterate();
      }
      return u && s2.done;
    };
    const s = iterate();
    if (!i2 && !s) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  return o;
}
function clearStatus(e, t = false) {
  if (e.Se || e.de)
    clearPendingSources(e);
  if (e.Ge)
    e.Ge = false;
  e.fe = t ? 0 : e.fe & STATUS_UNINITIALIZED;
  if (e.ce)
    setPendingError(e);
  if (e.We)
    updatePendingSignal(e);
  if (e.Me)
    e.Me();
}
function notifyStatus(e, t, n, i, r) {
  if (t === STATUS_ERROR && !(n instanceof StatusError) && !(n instanceof NotReadyError))
    n = new StatusError(e, n);
  const o = t === STATUS_PENDING && n instanceof NotReadyError ? n.source : undefined;
  const s = o === e;
  const u = t === STATUS_PENDING && e.R !== undefined && !s;
  const c = u && hasActiveOverride(e);
  if (!i) {
    if (t === STATUS_PENDING && o) {
      addPendingSource(e, o);
      e.fe = STATUS_PENDING | e.fe & STATUS_UNINITIALIZED;
      setPendingError(e, o, n);
    } else {
      clearPendingSources(e);
      e.fe = t | (t !== STATUS_ERROR ? e.fe & STATUS_UNINITIALIZED : 0);
      e.ce = n;
    }
    updatePendingSignal(e);
  }
  if (r && !i) {
    assignOrMergeLane(e, r);
  }
  const l = i || c;
  const a = i || u ? undefined : r;
  if (e.Me) {
    if (i && t === STATUS_PENDING) {
      return;
    }
    if (l) {
      e.Me(t, n);
    } else {
      e.Me();
    }
    return;
  }
  forEachDependent(e, (e2) => {
    e2.le = clock;
    if (t === STATUS_PENDING && o && e2.Se !== o && !e2.de?.has(o) || t !== STATUS_PENDING && (e2.ce !== n || e2.Se || e2.de)) {
      if (!l && !e2.S)
        queuePendingNode(e2);
      notifyStatus(e2, t, n, l, a);
    }
  });
}
var externalSourceConfig = null;
GlobalQueue.te = recompute;
GlobalQueue.ne = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var foundPending = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var pendingCheckSources = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.He)
      return true;
    e = e.q;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.G;
  if (!t) {
    if (e.S && (!n || activeTransition) && activeTransition !== e.S)
      globalQueue.initTransition(e.S);
    deleteFromHeap(e, e.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    e.ve = null;
    if (e.S || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ge !== null || e.Ve !== null) {
      markDisposal(e);
      e.Te = e.Ve;
      e.Ee = e.ge;
      e.Ve = null;
      e.ge = null;
      e.me = 0;
    }
  }
  let i = !!(e.M & REACTIVE_OPTIMISTIC_DIRTY);
  const r = e.R !== undefined && e.R !== NOT_PENDING;
  const o = !!(e.fe & STATUS_PENDING);
  const s = !!(e.fe & STATUS_UNINITIALIZED);
  const u = context;
  context = e;
  e.ye = null;
  e.M = REACTIVE_RECOMPUTING_DEPS;
  e.le = clock;
  let c = e.m === NOT_PENDING ? e.Z : e.m;
  let l = e.H;
  let a = tracking;
  let f = currentOptimisticLane;
  tracking = true;
  if (i) {
    const t2 = resolveLane(e);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.P.length) {
    for (let t2 = e._e;t2; t2 = t2.Oe) {
      const n2 = t2.Re;
      if (n2.M & REACTIVE_OPTIMISTIC_DIRTY) {
        const t3 = resolveLane(n2);
        if (t3) {
          i = true;
          currentOptimisticLane = t3;
          e.M |= REACTIVE_OPTIMISTIC_DIRTY;
          assignOrMergeLane(e, t3);
          break;
        }
      }
    }
  }
  const E = n && n !== EFFECT_USER;
  const T = stale;
  if (E)
    stale = true;
  try {
    if (e.ue & CONFIG_SYNC) {
      c = e.se(c);
      e.ve = null;
    } else {
      const t2 = e.ve;
      const n2 = e.se(c);
      const i2 = typeof n2 === "object" && n2 !== null;
      const r2 = e.ve !== t2;
      c = r2 || !i2 ? n2 : handleAsync(e, n2);
      if (!r2 && !i2)
        e.ve = null;
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
      e.Ge = true;
    notifyStatus(e, t2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, t2, undefined, t2 instanceof NotReadyError ? e.i : undefined);
  } finally {
    tracking = a;
    if (E)
      stale = T;
    e.M = REACTIVE_NONE | (t ? e.M & REACTIVE_SNAPSHOT_STALE : 0);
    context = u;
  }
  if (!e.ce) {
    trimStaleDeps(e);
    const u2 = r ? e.R : e.m === NOT_PENDING ? e.Z : e.m;
    const a2 = !n && s || !e.ke || !e.ke(u2, c);
    if (n && a2) {
      e.k = !e.ce;
      if (!t)
        e.F.enqueue(n, GlobalQueue.ie.bind(null, e));
    }
    if (a2) {
      const s2 = r ? e.R : undefined;
      if (t || n && activeTransition !== e.S || i) {
        e.Z = c;
        if (r && i) {
          e.R = c;
          e.m = c;
        }
      } else
        e.m = c;
      if (r && !i && o && !e.O)
        e.R = c;
      if (!r || i || e.R !== s2)
        insertSubs(e, i || r);
    } else if (r) {
      e.m = c;
    } else if (e.H != l) {
      for (let t2 = e.D;t2 !== null; t2 = t2.L) {
        insertIntoHeapHeight(t2.U, t2.U.M & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      }
    }
  }
  currentOptimisticLane = f;
  const S = e.m !== NOT_PENDING || e.Ee !== null || e.Te !== null || !!(e.fe & (STATUS_PENDING | STATUS_UNINITIALIZED));
  S && (!t || e.fe & STATUS_PENDING) && !e.S && !(activeTransition && r) && queuePendingNode(e);
  e.S && n && activeTransition !== e.S && runInTransition(e.S, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.M & REACTIVE_CHECK) {
    for (let t = e._e;t; t = t.Oe) {
      const n = t.Re;
      const i = n.Ie || n;
      if (i.se) {
        updateIfNecessary(i);
      }
      if (e.M & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.M & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.ce && e.le < clock && !e.ve) {
    recompute(e);
  }
  e.M = e.M & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = {
    id: t?.id ?? (n ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    ue: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ke: t?.equals != null ? t.equals : isEqual,
    V: t?.unobserved,
    Ve: null,
    F: context?.F ?? globalQueue,
    we: context?.we ?? defaultContext,
    me: 0,
    se: e,
    Z: undefined,
    H: 0,
    Pe: null,
    Ae: undefined,
    Ne: null,
    _e: null,
    ye: null,
    D: null,
    Ue: null,
    q: context,
    De: null,
    be: null,
    ge: null,
    M: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    fe: STATUS_UNINITIALIZED,
    le: clock,
    m: NOT_PENDING,
    Te: null,
    Ee: null,
    ve: null,
    S: null
  };
  setupComputedNode(i, t);
  return i;
}
function createEffectNode(e, t, n, i, r, o) {
  const s = o?.transparent ?? false;
  const u = {
    id: o?.id ?? (s ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    ue: (s ? CONFIG_TRANSPARENT : 0) | (o?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (o?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ke: false,
    V: o?.unobserved,
    Ve: null,
    F: context?.F ?? globalQueue,
    we: context?.we ?? defaultContext,
    me: 0,
    se: e,
    Z: undefined,
    H: 0,
    Pe: null,
    Ae: undefined,
    Ne: null,
    _e: null,
    ye: null,
    D: null,
    Ue: null,
    q: context,
    De: null,
    be: null,
    ge: null,
    M: REACTIVE_LAZY,
    fe: STATUS_UNINITIALIZED,
    le: clock,
    m: NOT_PENDING,
    Te: null,
    Ee: null,
    ve: null,
    S: null,
    k: false,
    xe: undefined,
    Qe: t,
    je: n,
    $e: undefined,
    Ke: false,
    G: i,
    Me: r
  };
  setupComputedNode(u, lazyOptions);
  return u;
}
var lazyOptions = { lazy: true };
function setupComputedNode(e, t) {
  e.Ne = e;
  const n = context?.pe ? context.he : context;
  if (context) {
    const t2 = context.ge;
    if (t2 === null) {
      context.ge = e;
    } else {
      e.De = t2;
      t2.be = e;
      context.ge = e;
    }
  }
  if (n)
    e.H = n.H + 1;
  if (externalSourceConfig) {
    const t2 = signal(undefined, { equals: false, ownedWrite: true });
    const n2 = externalSourceConfig.factory(e.se, () => {
      setSignal(t2, undefined);
    });
    cleanup(() => n2.dispose());
    e.se = (e2) => {
      read(t2);
      return n2.track(e2);
    };
  }
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.fe & STATUS_PENDING)) {
      e.ae = e.Z === undefined ? NO_SNAPSHOT : e.Z;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    ke: t?.equals != null ? t.equals : isEqual,
    ue: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.Ye ? CONFIG_NO_SNAPSHOT : 0),
    V: t?.unobserved,
    Z: e,
    D: null,
    Ue: null,
    le: clock,
    Ie: n,
    Ce: n?.Pe || null,
    m: NOT_PENDING
  };
  n && (n.Pe = i);
  if (snapshotCaptureActive && !(i.ue & CONFIG_NO_SNAPSHOT) && !((n?.fe ?? 0) & STATUS_PENDING)) {
    i.ae = e === undefined ? NO_SNAPSHOT : e;
    snapshotSources.add(i);
  }
  return i;
}
function optimisticComputed(e, t) {
  const n = computed(e, t);
  n.R = NOT_PENDING;
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
function read(e) {
  if (latestReadActive) {
    const t2 = getLatestValueComputed(e);
    const n2 = latestReadActive;
    latestReadActive = false;
    const i2 = e.R !== undefined && e.R !== NOT_PENDING ? e.R : e.Z;
    let r2;
    try {
      r2 = read(t2);
    } catch (e2) {
      if (!context && e2 instanceof NotReadyError)
        return i2;
      throw e2;
    } finally {
      latestReadActive = n2;
    }
    if (t2.fe & STATUS_PENDING)
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
  if (pendingCheckActive) {
    const t2 = e.Ie;
    const n2 = pendingCheckActive;
    pendingCheckActive = false;
    let i2 = context;
    if (i2?.pe)
      i2 = i2.he;
    const r2 = t2 || e;
    const o = e;
    if (typeof o.se === "function") {
      const t3 = e;
      if (t3.M & REACTIVE_LAZY) {
        t3.M &= ~REACTIVE_LAZY;
        recompute(t3, true);
      } else if (t3.M & REACTIVE_DISPOSED) {
        recompute(t3, true);
      } else {
        updateIfNecessary(t3);
      }
    }
    if (i2 && r2.fe & STATUS_PENDING && r2.fe & STATUS_UNINITIALIZED) {
      if (tracking && e !== i2)
        link(e, i2);
      pendingCheckActive = n2;
      throw r2.ce;
    }
    if (t2 && e.R !== undefined) {
      if (e.R !== NOT_PENDING && (t2.ve || !!(t2.fe & STATUS_PENDING))) {
        foundPending = true;
      }
      collectPendingSources(e);
      collectPendingSources(t2);
      if (i2 && tracking)
        link(e, i2);
    } else {
      collectPendingSources(e);
      if (t2)
        collectPendingSources(t2);
    }
    pendingCheckActive = n2;
  }
  let t = context;
  if (t?.pe)
    t = t.he;
  const n = e;
  if (typeof n.se === "function") {
    const t2 = e;
    if (t2.M & REACTIVE_LAZY) {
      t2.M &= ~REACTIVE_LAZY;
      recompute(t2, true);
    } else if (t2.M & REACTIVE_DISPOSED) {
      recompute(t2, true);
    }
  }
  const i = e.Ie || e;
  if (!n.se && i === e && e.R === undefined && e.ae === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.m === NOT_PENDING ? e.Z : e.m;
  }
  if (t && tracking) {
    link(e, t);
    if (i.se) {
      const n2 = e.M & REACTIVE_ZOMBIE;
      if (i.H >= (n2 ? zombieQueue.h : dirtyQueue.h)) {
        markNode(t);
        markHeap(n2 ? zombieQueue : dirtyQueue);
        updateIfNecessary(i);
      }
      const r2 = i.H;
      if (r2 >= t.H && e.q !== t) {
        t.H = r2 + 1;
      }
    }
  }
  if (i.fe & STATUS_PENDING) {
    if (t && !(stale && i.S && activeTransition !== i.S)) {
      if (currentOptimisticLane) {
        const n2 = i.i;
        const r2 = findLane(currentOptimisticLane);
        if (n2 && findLane(n2) === r2 && !hasActiveOverride(i)) {
          if (!tracking && e !== t)
            link(e, t);
          throw i.ce;
        }
      } else {
        if (!tracking && e !== t)
          link(e, t);
        throw i.ce;
      }
    } else if (t && i !== e && i.fe & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw i.ce;
    } else if (!t && i.fe & STATUS_UNINITIALIZED) {
      throw i.ce;
    }
  }
  if (e.se && e.fe & STATUS_ERROR) {
    if (e.le < clock) {
      recompute(e);
      return read(e);
    } else
      throw e.ce;
  }
  if (snapshotCaptureActive && t && t.ue & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.ae;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const r2 = e.m !== NOT_PENDING ? e.m : e.Z;
      if (r2 !== i2)
        t.M |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.R !== undefined && e.R !== NOT_PENDING) {
    if (t && stale && shouldReadStashedOptimisticValue(e))
      return e.Z;
    return e.R;
  }
  if (activeTransition !== null && currentOptimisticLane !== null && !latestReadActive && e.m !== NOT_PENDING && (i === e || !!(i.M & REACTIVE_MANUAL_WRITE)) && !e.se && t) {
    activeTransition.Y.add(t);
    return e.Z;
  }
  const r = !t || currentOptimisticLane !== null && (e.R !== undefined || e.i || i === e && stale || !!(i.fe & STATUS_PENDING)) || e.m === NOT_PENDING || stale && e.S && activeTransition !== e.S ? e.Z : e.m;
  if (!t && i === e && typeof n.se === "function" && e.ue & CONFIG_AUTO_DISPOSE && !(i.fe & STATUS_PENDING) && !e.D) {
    unobserved(e);
  }
  return r;
}
function setSignal(e, t) {
  if (e.S && activeTransition !== e.S)
    globalQueue.initTransition(e.S);
  const n = e.R !== undefined && !projectionWriteActive;
  const i = e.R !== undefined && e.R !== NOT_PENDING;
  const r = n ? i ? e.R : e.Z : e.m === NOT_PENDING ? e.Z : e.m;
  if (typeof t === "function")
    t = t(r);
  const o = !e.ke || !e.ke(r, t) || !!(e.fe & STATUS_UNINITIALIZED);
  if (!o) {
    if (n && i) {
      const t2 = resolveTransition(e);
      if (t2 && activeTransition !== t2)
        globalQueue.initTransition(t2);
      if (e.se) {
        insertSubs(e, true);
        schedule();
      }
    }
    return t;
  }
  if (n) {
    const n2 = e.R === NOT_PENDING;
    if (!n2)
      globalQueue.initTransition(resolveTransition(e));
    if (n2) {
      e.m = e.Z;
      globalQueue.P.push(e);
    }
    e.O = true;
    const i2 = getOrCreateLane(e);
    e.i = i2;
    e.R = t;
  } else {
    if (e.m === NOT_PENDING)
      queuePendingNode(e);
    e.m = t;
  }
  if (e.We)
    updatePendingSignal(e);
  if (e.Fe) {
    setSignal(e.Fe, t);
  }
  e.le = clock;
  insertSubs(e, n);
  schedule();
  return t;
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
  pendingCheckSources?.add(e);
  const t = e.Ie || e;
  if (t !== e)
    pendingCheckSources?.add(t);
}
function computePendingState(e) {
  const t = e;
  const n = e.Ie;
  if (e.t) {
    const n2 = e.t;
    if (n2.fe & STATUS_PENDING && !(n2.fe & STATUS_UNINITIALIZED))
      return true;
    return e.m !== NOT_PENDING && !(t.fe & STATUS_UNINITIALIZED);
  }
  if (n && e.m !== NOT_PENDING) {
    return !!(n.M & REACTIVE_MANUAL_WRITE) || !n.ve && !(n.fe & STATUS_PENDING);
  }
  if (e.R !== undefined && e.R !== NOT_PENDING) {
    if (t.fe & STATUS_PENDING && !(t.fe & STATUS_UNINITIALIZED))
      return true;
    if (e.t) {
      const t2 = e.i ? findLane(e.i) : null;
      return !!(t2 && t2.u.size > 0);
    }
    return true;
  }
  if (e.R !== undefined && e.R === NOT_PENDING && !e.t) {
    return false;
  }
  if (e.m !== NOT_PENDING && !(t.fe & STATUS_UNINITIALIZED))
    return true;
  return !!(t.fe & STATUS_PENDING && !(t.fe & STATUS_UNINITIALIZED));
}
function updatePendingSignal(e) {
  if (e.We) {
    const t = computePendingState(e);
    const n = e.We;
    setSignal(n, t);
    if (!t && n.i) {
      const t2 = resolveLane(e);
      if (t2 && t2.u.size > 0) {
        const e2 = findLane(n.i);
        if (e2 !== t2) {
          mergeLanes(t2, e2);
        }
      }
      signalLanes.delete(n);
      n.i = undefined;
    }
  }
}
function getLatestValueComputed(e) {
  if (!e.Fe) {
    const t = latestReadActive;
    latestReadActive = false;
    const n = pendingCheckActive;
    pendingCheckActive = false;
    const i = context;
    context = null;
    e.Fe = optimisticComputed(() => read(e));
    e.Fe.t = e;
    context = i;
    pendingCheckActive = n;
    latestReadActive = t;
  }
  return e.Fe;
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
  !i?.defer && (o.G === EFFECT_USER || i?.schedule ? o.F.enqueue(o.G, runEffect.bind(null, o)) : runEffect(o));
}
function notifyEffectStatus(e, t) {
  const n = e !== undefined ? e : this.fe;
  const i = t !== undefined ? t : this.ce;
  if (n & STATUS_ERROR) {
    let e2 = i;
    this.F.notify(this, STATUS_PENDING, 0);
    if (this.G === EFFECT_USER) {
      try {
        return this.je ? this.je(e2, () => {
          this.$e?.();
          this.$e = undefined;
        }) : console.error(e2);
      } catch (t2) {
        e2 = t2;
      }
    }
    if (!this.F.notify(this, STATUS_ERROR, STATUS_ERROR))
      throw e2;
  } else if (this.G === EFFECT_RENDER) {
    this.F.notify(this, STATUS_PENDING | STATUS_ERROR, n, i);
  }
}
function runEffect(e) {
  if (!e.k || e.M & REACTIVE_DISPOSED)
    return;
  e.$e?.();
  e.$e = undefined;
  try {
    const t = e.Qe(e.Z, e.xe);
    if (false)
      ;
    e.$e = t;
    if (e.$e && !e.Ke) {
      e.Ke = true;
      runWithOwner(e.q, () => cleanup(() => e.$e?.()));
    }
  } catch (t) {
    e.ce = new StatusError(e, t);
    e.fe |= STATUS_ERROR;
    if (!e.F.notify(e, STATUS_ERROR, STATUS_ERROR))
      throw t;
  } finally {
    e.xe = e.Z;
    e.k = false;
  }
}
GlobalQueue.ie = runEffect;
function trackedEffect(e, t) {
  const run = () => {
    if (!n.k || n.M & REACTIVE_DISPOSED)
      return;
    try {
      n.k = false;
      recompute(n);
    } finally {}
  };
  const n = computed(() => {
    n.$e?.();
    n.$e = undefined;
    const t2 = staleValues(e);
    n.$e = t2;
  }, { ...t, lazy: true });
  n.$e = undefined;
  n.ue = n.ue & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  n.k = true;
  n.G = EFFECT_TRACKED;
  n.Me = (e2, t2) => {
    const i = e2 !== undefined ? e2 : n.fe;
    if (i & STATUS_ERROR) {
      n.F.notify(n, STATUS_PENDING, 0);
      const e3 = t2 !== undefined ? t2 : n.ce;
      if (!n.F.notify(n, STATUS_ERROR, STATUS_ERROR))
        throw e3;
    }
  };
  n.W = run;
  n.F.enqueue(EFFECT_USER, run);
  cleanup(() => n.$e?.());
}
function onCleanup(e) {
  return cleanup(e);
}
function accessor(e) {
  const t = read.bind(null, e);
  t[$REFRESH] = e;
  return t;
}
function createMemo(e, t) {
  return accessor(computed(e, t));
}
function createRenderEffect(e, t, n) {
  effect(e, t, undefined, n);
}
function createTrackedEffect(e, t) {
  trackedEffect(e, t);
}
function onSettled(e) {
  const t = getOwner();
  t && !(t.ue & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    const t2 = e();
    t2?.();
  });
}
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var STORE_SELF_PENDING = Symbol(0);
var storeLookup = new WeakMap;
function isWrappable(e) {
  if (e == null || typeof e !== "object" || Object.isFrozen(e))
    return false;
  return typeof Node === "undefined" || !(e instanceof Node);
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
    const n2 = Object.keys(o);
    for (let e2 = 0;e2 < n2.length; e2++) {
      const i2 = n2[e2];
      if (isPrototypePollutionKey(i2))
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
          const i2 = Object.keys(resolveSource(n[t2]));
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
var ON_INIT = Symbol();
var RevealControllerContext = createContext(null);
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.isReady() : e.ft.size === 0 && !e.Et;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.isMinimallyReady() : isSlotReady(e);
}
function setSlotState(e, t, n, i) {
  setSignal(e.Tt, n);
  setSignal(e.St, i);
  if (isRevealController(e)) {
    if (!n && e.dt === t)
      e.dt = undefined;
    return e.evaluate(n, i);
  }
  if (!n && e._t === t && e.Ot)
    e._t = undefined;
}

class RevealController {
  Rt;
  It;
  ht = [];
  dt;
  Tt = signal(false, { ownedWrite: true, Ye: true });
  St = signal(false, { ownedWrite: true, Ye: true });
  Nt = true;
  At = true;
  Pt = false;
  constructor(e, t) {
    this.Rt = e;
    this.It = t;
  }
  Ct(e) {
    for (let t = 0;t < this.ht.length; t++) {
      const n = this.ht[t];
      if ((isRevealController(n) ? n.dt : n._t) !== this)
        continue;
      if (e(n) === false)
        return false;
    }
    return true;
  }
  isReady() {
    return this.Ct(isSlotReady);
  }
  isMinimallyReady() {
    const e = untrack(this.Rt);
    if (e === "together")
      return this.isReady();
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.Ct((n) => {
        e2 = true;
        if (isSlotMinimallyReady(n)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.Ct((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  register(e) {
    if (this.ht.includes(e))
      return;
    this.ht.push(e);
    const t = untrack(this.Rt);
    setSignal(e.Tt, true), setSignal(e.St, t === "sequential" ? !!untrack(this.It) : false);
    untrack(() => this.evaluate());
  }
  unregister(e) {
    const t = this.ht.indexOf(e);
    if (t >= 0)
      this.ht.splice(t, 1);
    untrack(() => this.evaluate());
  }
  evaluate(e, t) {
    if (this.Pt)
      return;
    this.Pt = true;
    const n = this.Nt;
    const i = this.At;
    try {
      const n2 = e ?? read(this.Tt), i2 = untrack(this.Rt), r = i2 === "sequential" && !!untrack(this.It), o = t ?? r;
      if (n2) {
        this.Ct((e2) => setSlotState(e2, this, true, o));
      } else if (i2 === "natural") {
        this.Ct((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.St, false);
            setSignal(e2.Tt, false);
            e2.evaluate(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (i2 === "together") {
        const e2 = this.Ct(isSlotMinimallyReady);
        this.Ct((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.Ct((t2) => {
          if (e2)
            return setSlotState(t2, this, true, r);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.St, false);
            setSignal(t2.Tt, false);
            t2.evaluate(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.Nt = this.isReady();
      this.At = this.isMinimallyReady();
      this.Pt = false;
    }
    if (this.dt && (n !== this.Nt || i !== this.At))
      this.dt.evaluate();
  }
}

class CollectionQueue extends Queue {
  gt;
  ft = new Set;
  Dt;
  Et = true;
  Tt = signal(false, { ownedWrite: true, Ye: true });
  ce;
  St = signal(false, { ownedWrite: true, Ye: true });
  _t;
  Ot = false;
  yt;
  vt = ON_INIT;
  constructor(e) {
    super();
    this.gt = e;
  }
  run(e) {
    if (!e || read(this.Tt) && (!_revealUsed || read(this.St)))
      return;
    return super.run(e);
  }
  notify(e, t, n, i) {
    if (!(t & this.gt))
      return super.notify(e, t, n, i);
    if (this.Ot && this.yt) {
      const e2 = untrack(() => {
        try {
          return this.yt();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.vt) {
        this.vt = e2;
        this.Ot = false;
        this.ft.clear();
      }
    }
    if (this.gt & STATUS_PENDING && this.Ot)
      return super.notify(e, t, n, i);
    if (this.gt & STATUS_PENDING && n & STATUS_ERROR) {
      return super.notify(e, STATUS_ERROR, n, i);
    }
    if (n & this.gt) {
      this.Et = true;
      const t2 = i?.source || e.ce?.source;
      if (t2) {
        const e2 = this.ft.size === 0;
        this.ft.add(t2);
        if (e2)
          setSignal(this.Tt, true);
        if (this.gt & STATUS_ERROR) {
          setSignal(this.ce, t2.ce?.cause ?? t2.ce);
        }
      }
    }
    t &= ~this.gt;
    return t ? super.notify(e, t, n, i) : true;
  }
  checkSources() {
    for (const e of this.ft) {
      if (e.M & REACTIVE_DISPOSED || !(e.fe & this.gt) && !(this.gt & STATUS_ERROR && e.fe & STATUS_PENDING))
        this.ft.delete(e);
    }
    if (!this.ft.size) {
      if (this.gt & STATUS_PENDING && this.Et && !this.Ot && this.Dt) {
        this.Et = !!(this.Dt.fe & this.gt);
      } else {
        this.Et = false;
      }
      if (!this.Et) {
        setSignal(this.Tt, false);
        if (this.yt) {
          try {
            this.vt = untrack(() => this.yt());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this._t?.evaluate();
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
// node_modules/.bun/solid-js@2.0.0-beta.15/node_modules/solid-js/dist/solid.js
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

// node_modules/.bun/@solidjs+universal@2.0.0-beta.15+7fdbb79bf83cbf26/node_modules/@solidjs/universal/dist/universal.js
var transparentOptions = {
  transparent: true,
  sync: true
};
var syncOptions = {
  sync: true
};
var effect2 = (fn, effectFn, options) => createRenderEffect2(fn, effectFn, options ? {
  transparent: true,
  sync: true,
  ...options
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
var animationFrames = new Map;
var refreshRate = 60;
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
    unsubDown = on("pointerDown", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerDown")?.(e);
      }
      let focused = getFocusedNodeId();
      if (focused != null && !targets.includes(focused)) {
        setFocus(null);
      }
    });
    unsubUp = on("pointerUp", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerUp")?.(e);
      }
    });
    unsubMove = on("pointerMove", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerMove")?.(e);
      }
    });
    unsubEnter = on("pointerEnter", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerEnter")?.(e);
      }
    });
    unsubLeave = on("pointerLeave", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerLeave")?.(e);
      }
    });
    unsubWheel = on("wheel", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onWheel")?.(e);
      }
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

// packages/core/src/color.ts
k([names_default]);
function parseColor(color) {
  let { r: r2, g: g2, b: b2, a: a2 } = w(color).toRgb();
  return ((r2 & 255) << 24 | (g2 & 255) << 16 | (b2 & 255) << 8 | a2 * 255 & 255) >>> 0;
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
function removeNode(parent, node) {
  if (!node || !parent)
    return;
  let index = parent.children.indexOf(node);
  if (index !== -1) {
    parent.children.splice(index, 1);
  }
  node.parent = undefined;
  tree2.deleteNode(parent.id, node.id);
  let cleanup2 = (n2) => {
    for (let child of n2.children)
      cleanup2(child);
    if (n2.id === getFocusedNodeId())
      setFocus(null);
    nodes.delete(n2.id);
    cleanupNodeHandlers(n2.id);
  };
  cleanup2(node);
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
  createElement: (elementType) => {
    let proxy = createProxyNode(elementType);
    if (elementType === "window")
      tree2.createRoot(proxy.id);
    else
      tree2.createNode(proxy.id, elementType);
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
  },
  insertNode: (parent, node, anchor) => {
    if (!node)
      return;
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
// packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { destroyTexture as destroyTexture2, setShaderParams, uploadTexture } from "flux:gpu";
// lattice/default-app/bsod.tsx
function Bsod() {
  var _el$ = createElement("window"), _el$2 = createElement("d-rect"), _el$3 = createElement("view"), _el$4 = createElement("text"), _el$6 = createElement("text"), _el$8 = createElement("text");
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  setProp(_el$, "title", "solidrt");
  setProp(_el$2, "color", "#1144bb");
  insertNode2(_el$3, _el$4);
  insertNode2(_el$3, _el$6);
  insertNode2(_el$3, _el$8);
  setProp(_el$3, "flexGrow", 1);
  setProp(_el$3, "justifyContent", "center");
  setProp(_el$3, "alignItems", "center");
  setProp(_el$3, "flexDirection", "column");
  setProp(_el$3, "gap", 16);
  insertNode2(_el$4, createTextNode(`:(`));
  setProp(_el$4, "color", "white");
  setProp(_el$4, "fontSize", 64);
  setProp(_el$4, "fontWeight", 700);
  insertNode2(_el$6, createTextNode(`Something went wrong`));
  setProp(_el$6, "color", "white");
  setProp(_el$6, "fontSize", 22);
  insertNode2(_el$8, createTextNode(`The application could not be started.`));
  setProp(_el$8, "color", "#aac2ff");
  setProp(_el$8, "fontSize", 15);
  return _el$;
}
render(() => createComponent2(Bsod, {}));
