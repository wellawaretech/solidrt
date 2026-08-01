// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/error.js
class NotReadyError extends Error {
  source;
  constructor(r) {
    const o = Error;
    const t = o.stackTraceLimit;
    if (t !== undefined)
      o.stackTraceLimit = 0;
    super();
    if (t !== undefined)
      o.stackTraceLimit = t;
    this.source = r;
  }
}

class StatusError extends Error {
  source;
  constructor(r, o) {
    super(o instanceof Error ? o.message : String(o), {
      cause: o
    });
    this.source = r;
  }
}
function unwrapStatusError(r) {
  return r instanceof StatusError ? r.cause : r;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/constants.js
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
var REACTIVE_REASK = 1 << 11;
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
var OVERRIDE_UNDEFINED = {};
function unwrapOverride(E) {
  return E === OVERRIDE_UNDEFINED ? undefined : E;
}
var SUPPORTS_PROXY = typeof Proxy === "function";
var defaultContext = {};
var $REFRESH = Symbol("refresh");

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/lanes.js
var signalLanes = new WeakMap;
var activeLanes = new Set;
function findLane(n) {
  while (n.tn)
    n = n.tn;
  return n;
}
function mergeLanes(n, e) {
  n = findLane(n);
  e = findLane(e);
  if (n === e)
    return n;
  e.tn = n;
  for (const i of e.Ne)
    n.Ne.add(i);
  e.Ne.clear();
  n.rn[0].push(...e.rn[0]);
  n.rn[1].push(...e.rn[1]);
  e.rn[0].length = 0;
  e.rn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.Me;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  n.Me = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.sn) {
    const e = n.sn = currentTransition(n.sn);
    if (e.fn !== true)
      return e;
    n.sn = null;
  }
  return resolveLane(n)?.Ie ?? n.Ie;
}
function hasActiveOverride(n) {
  return !!(n.Ae !== undefined && n.Ae !== NOT_PENDING);
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const r = n.Me;
  if (r) {
    if (r.tn) {
      n.Me = e;
      return;
    }
    const t = findLane(r);
    if (activeLanes.has(t)) {
      if (t !== i && !hasActiveOverride(n)) {
        if (i.an && findLane(i.an) === t) {
          n.Me = e;
        } else if (t.an && findLane(t.an) === i)
          ;
        else
          mergeLanes(i, t);
      }
      return;
    }
  }
  n.Me = e;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  He: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  He: 0,
  EE: 0
};
var clock = 0;
var activeTransition = null;
var scheduled = false;
var halted = false;
var haltNotified = false;
var syncDepth = 0;
var projectionWriteActive = false;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(e) {
  const t = e.m;
  return transitions.size === 0 && activeLanes.size === 0 && e.vt.length === 0 && t.je.length === 0 && t.A.length === 0 && t.cn.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.o !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e._e !== NOT_PENDING)
      continue;
    if (e.Ae !== undefined && e.Ae !== NOT_PENDING)
      continue;
    if (e.t)
      continue;
    transientStoreNodes.delete(e);
    e.ut?.();
  }
}
function createBatch() {
  return {
    Se: clock,
    Qt: [],
    Ee: new Map,
    je: [],
    A: [],
    cn: new Set,
    ie: [],
    yt: {
      gt: [[], []],
      vt: []
    },
    fn: false,
    ln: new Set
  };
}
function mergeTransitionState(e, t) {
  t.fn = e;
  e.ie.push(...t.ie);
  for (const i of activeLanes)
    if (i.Ie === t)
      i.Ie = e;
  if (t.je.length) {
    e.je.push(...t.je);
    t.je.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const i of t.cn)
    e.cn.add(i);
  for (const [i, n] of t.Ee) {
    let t2 = e.Ee.get(i);
    if (!t2)
      e.Ee.set(i, t2 = new Set);
    for (const e2 of n)
      t2.add(e2);
  }
  for (const i of t.ln)
    e.ln.add(i);
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.bt && !projectionWriteActive)
    queueMicrotask(flush);
}
function haltReactivity(e) {
  if (halted)
    return;
  halted = true;
  let t = "[REACTIVITY_HALTED]";
  e === undefined ? console.error(t) : console.error(t, e);
}
function notifyHalted() {
  if (haltNotified)
    return;
  haltNotified = true;
  console.error("[REACTIVITY_HALTED]");
}
class Queue {
  ve = null;
  gt = [[], []];
  vt = [];
  created = clock;
  addChild(e) {
    this.vt.push(e);
    e.ve = this;
  }
  removeChild(e) {
    const t = this.vt.indexOf(e);
    if (t >= 0) {
      this.vt.splice(t, 1);
      e.ve = null;
    }
  }
  notify(e, t, i, n) {
    if (this.ve)
      return this.ve.notify(e, t, i, n);
    return false;
  }
  run(e) {
    if (this.gt[e - 1].length) {
      const t = this.gt[e - 1];
      this.gt[e - 1] = [];
      runQueue(t, e);
    }
    for (let t = 0;t < this.vt.length; t++)
      this.vt[t].run?.(e);
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const i = findLane(currentOptimisticLane);
        i.rn[e - 1].push(t);
      } else {
        this.gt[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.gt[0].push(...this.gt[0]);
    e.gt[1].push(...this.gt[1]);
    this.gt = [[], []];
    for (let t = 0;t < this.vt.length; t++) {
      let i = this.vt[t];
      let n = e.vt[t];
      if (!n) {
        n = {
          gt: [[], []],
          vt: []
        };
        e.vt[t] = n;
      }
      i.stashQueues(n);
    }
  }
  restoreQueues(e) {
    this.gt[0].push(...e.gt[0]);
    this.gt[1].push(...e.gt[1]);
    for (let t = 0;t < e.vt.length; t++) {
      const i = e.vt[t];
      let n = this.vt[t];
      if (n)
        n.restoreQueues(i);
    }
  }
}

class GlobalQueue extends Queue {
  bt = false;
  m = createBatch();
  static pe;
  static Ce;
  static Xe;
  static kt = null;
  static p = null;
  static G = null;
  static M = null;
  static h = null;
  static It = null;
  static dt = null;
  static De = null;
  static ce = null;
  static Oe = null;
  static un = null;
  static St = null;
  static At = null;
  static Pt = null;
  static $e = null;
  static k = null;
  static Dt = null;
  static ht = null;
  static En = null;
  static dn = null;
  static Tn = null;
  static In = null;
  static Ot = null;
  static Ct = null;
  static Rt = null;
  static Ze = null;
  static ze = null;
  static Ke = null;
  static Nn = null;
  flush() {
    if (this.bt)
      return;
    this.bt = true;
    try {
      if (false)
        ;
      runHeap(dirtyQueue, GlobalQueue.pe);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, GlobalQueue.pe);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue.In(EFFECT_RENDER);
            GlobalQueue.In(EFFECT_USER);
          }
          this.stashQueues(e2.yt);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.He || this.m.Qt.length > 0;
          reassignPendingTransition(e2.Qt);
          activeTransition = null;
          finalizePureQueue(null, true);
          return;
        }
        const t = activeTransition;
        const i = this.m;
        i !== t && i.Qt.push(...t.Qt);
        this.restoreQueues(t.yt);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(i.Qt);
        finalizePureQueue(t);
        if (i === t) {
          const e2 = createBatch();
          e2.Qt = i.Qt;
          e2.je = i.je;
          e2.A = i.A;
          e2.cn = i.cn;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.He) {
            runHeap(dirtyQueue, GlobalQueue.pe);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.pe);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.He;
      activeLanes.size && GlobalQueue.In(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && GlobalQueue.In(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
      if (false)
        ;
      if (false)
        ;
    } finally {
      this.bt = false;
    }
  }
  notify(e, t, i, n) {
    if (t & STATUS_PENDING) {
      if (i & STATUS_PENDING) {
        const t2 = n !== undefined ? n : e._;
        if (t2?.l)
          return true;
        if (activeTransition && t2) {
          const i2 = t2.source;
          let n2 = activeTransition.Ee.get(i2);
          if (!n2)
            activeTransition.Ee.set(i2, n2 = new Set);
          const s = n2.size;
          n2.add(e);
          if (n2.size !== s)
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
    if (!e && activeTransition && activeTransition.Se === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? createBatch();
    } else if (e) {
      const t2 = activeTransition;
      mergeTransitionState(e, t2);
      transitions.delete(t2);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.Se = clock;
    const t = this.m;
    if (t !== activeTransition) {
      for (let e2 = 0;e2 < t.Qt.length; e2++) {
        const i = t.Qt[e2];
        i.Ie = activeTransition;
        activeTransition.Qt.push(i);
      }
      for (let e2 = 0;e2 < t.je.length; e2++) {
        const i = t.je[e2];
        i.Ie = activeTransition;
        activeTransition.je.push(i);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e2 of t.cn)
        activeTransition.cn.add(e2);
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.Ie)
        e2.Ie = activeTransition;
    }
  }
}
function queuePendingNode(e) {
  currentBatch.Qt.push(e);
}
var reaskArmed = false;
function insertSubs(e, t = false) {
  const i = e.Me || currentOptimisticLane;
  const n = e.xe !== undefined;
  const s = reaskArmed;
  for (let r = e.o;r !== null; r = r.ue) {
    if (s)
      r.le.se &= ~REACTIVE_REASK;
    if (n && r.le.T & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.le.se |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && i) {
      r.le.se |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.le, i);
    } else if (t) {
      r.le.se |= REACTIVE_OPTIMISTIC_DIRTY;
      r.le.Me = undefined;
    }
    enqueueSub(r.le);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.ae) {
    if (e._e !== NOT_PENDING) {
      e.Ue = e._e;
      e._e = NOT_PENDING;
    }
    if (e.he || e.Ge)
      GlobalQueue.un(e);
    return;
  }
  if (e._e !== NOT_PENDING) {
    e.Ue = e._e;
    e._e = NOT_PENDING;
    if (e.Pe && e.Pe !== EFFECT_TRACKED)
      e.Be = true;
  }
  t.se &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  if (t.Qe !== null || t.ye !== null)
    GlobalQueue.Ce(t, false, true);
  if (e.he || e.Ge)
    GlobalQueue.un(e);
}
function commitPendingNodes() {
  const e = currentBatch.Qt;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
}
function finalizePureQueue(e = null, t = false) {
  const i = !t;
  if (i)
    commitPendingNodes();
  if (!t && globalQueue.vt.length)
    checkBoundaryChildren(globalQueue);
  const n = dirtyQueue.EE >= dirtyQueue.He;
  if (n)
    runHeap(dirtyQueue, GlobalQueue.pe);
  if (i) {
    if (n)
      commitPendingNodes();
    const t2 = e ?? globalQueue.m;
    if (t2.je.length)
      GlobalQueue.En(t2.je);
    if (e && e.ln.size) {
      for (const t3 of e.ln) {
        if (t3.se & REACTIVE_DISPOSED)
          continue;
        enqueueSub(t3);
      }
      e.ln.clear();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.vt.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.cn.size)
      GlobalQueue.kt(t2.cn, e);
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.Tn(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.vt) {
    t.ne?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].Ie = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
var currentBatch = globalQueue.m;
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
  if (globalQueue.bt) {
    return;
  }
  if (halted)
    return;
  while (scheduled || activeTransition) {
    globalQueue.flush();
  }
}
function runQueue(e, t) {
  for (let i = 0;i < e.length; i++)
    e[i](t);
}
function reporterBlocksSource(e, t) {
  if (e.se & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.oe?.has(t))
    return true;
  for (let i = e.et;i; i = i.tt) {
    let e2 = i.nt;
    while (e2) {
      if (e2 === t || e2.it === t)
        return true;
      e2 = e2.nn;
    }
  }
  return !!(e.S & STATUS_PENDING && e._ instanceof NotReadyError && e._.source === t);
}
function transitionComplete(e) {
  if (e.fn)
    return true;
  if (e.ie.length)
    return false;
  let t = true;
  for (const [i, n] of e.Ee) {
    let s = false;
    for (const e2 of n) {
      if (reporterBlocksSource(e2, i)) {
        s = true;
        break;
      }
      n.delete(e2);
    }
    if (!s)
      e.Ee.delete(i);
    else if (i.S & STATUS_PENDING && i._?.source === i) {
      t = false;
      break;
    }
  }
  if (t && e.je.length && GlobalQueue.dn(e))
    t = false;
  t && (e.fn = true);
  return t;
}
function currentTransition(e) {
  while (e.fn && typeof e.fn === "object")
    e = e.fn;
  return e;
}
function runInTransition(e, t) {
  const i = activeTransition;
  try {
    activeTransition = currentTransition(e);
    return t();
  } finally {
    activeTransition = i;
  }
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.se & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  if (e.Pe === EFFECT_TRACKED) {
    const E2 = e;
    if (!E2.Be) {
      E2.Be = true;
      E2.C.enqueue(EFFECT_USER, E2.Ft);
    }
    return;
  }
  const E = queueFor(e);
  if (E.He > e.Ve)
    E.He = e.Ve;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.ve?.Nt ? e.ve.Tt?.Ve : e.ve?.Ve) ?? -1;
  if (t >= e.Ve)
    e.Ve = t + 1;
  const n = e.Ve;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.ot;
    E2.lt = e;
    e.ot = E2;
    I.ot = e;
  }
  if (n > E.EE)
    E.EE = n;
}
function insertIntoHeap(e, E) {
  let t = e.se;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (t & REACTIVE_CHECK) {
    e.se = t & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else {
    e.se = t | REACTIVE_IN_HEAP;
    if (E.tE && !(t & REACTIVE_DIRTY))
      E.tE = false;
  }
  if (!(t & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, E);
}
function insertIntoHeapHeight(e, E) {
  let t = e.se;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.se = t | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, E);
}
function deleteFromHeap(e, E) {
  const t = e.se;
  if (!(t & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.se = t & -25;
  const n = e.Ve;
  if (e.ot === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.lt;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.ot.lt = t2;
    o.ot = e.ot;
  }
  e.ot = e;
  e.lt = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.lt) {
      if (t.se & REACTIVE_IN_HEAP)
        markNode(t);
    }
  }
}
function markNode(e, E = REACTIVE_DIRTY) {
  const t = e.se;
  if ((t & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= E)
    return;
  e.se = t & -4 | E;
  for (let E2 = e.o;E2 !== null; E2 = E2.ue) {
    markNode(E2.le, REACTIVE_CHECK);
  }
  if (e.u !== null) {
    for (let E2 = e.u;E2 !== null; E2 = E2.fe) {
      for (let e2 = E2.o;e2 !== null; e2 = e2.ue) {
        markNode(e2.le, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.He = 0;e.He <= e.EE; e.He++) {
    let t = e.eE[e.He];
    while (t !== undefined) {
      if (t.se & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.He];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.Ve;
  for (let E2 = e.et;E2; E2 = E2.tt) {
    const e2 = E2.nt;
    const n = e2.it || e2;
    if (n.ae && n.Ve >= t)
      t = n.Ve + 1;
  }
  if (e.Ve !== t) {
    e.Ve = t;
    for (let E2 = e.o;E2 !== null; E2 = E2.ue) {
      insertIntoHeapHeight(E2.le, queueFor(E2.le));
    }
  }
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/owner.js
function markDisposal(e) {
  let n = e.ke;
  while (n) {
    const e2 = n.se;
    n.se = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(n, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(n, zombieQueue);
      else
        insertIntoHeapHeight(n, zombieQueue);
    }
    markDisposal(n);
    n = n.Fe;
  }
}
function disposeChildren(e, n = false, t) {
  const i = e.se;
  if (i & REACTIVE_DISPOSED)
    return;
  if (n) {
    e.se = i | REACTIVE_DISPOSED;
    const n2 = e;
    if (n2.he || n2.Ge)
      GlobalQueue.un(n2);
  }
  if (n && e.ae)
    e.Te = null;
  let l = t ? e.Qe : e.ke;
  while (l) {
    const e2 = l.Fe;
    if (l.et) {
      const e3 = l;
      deleteFromHeap(e3, queueFor(e3));
      let n2 = e3.et;
      do {
        n2 = unlinkSubs(n2);
      } while (n2 !== null);
      e3.et = null;
      e3.We = null;
    }
    disposeChildren(l, true);
    l = e2;
  }
  if (t) {
    e.Qe = null;
  } else {
    e.ke = null;
    e.qe = 0;
  }
  if (n && !t && !(i & REACTIVE_ZOMBIE) && e.ve !== null && !(e.ve.se & REACTIVE_DISPOSED)) {
    const n2 = e.rt;
    const t2 = e.Fe;
    if (n2 !== null)
      n2.Fe = t2;
    else
      e.ve.ke = t2;
    if (t2 !== null)
      t2.rt = n2;
    e.rt = null;
  }
  runDisposal(e, t);
  if (n && e.Et) {
    const n2 = e.Et;
    e.Et = undefined;
    n2();
  }
}
function runDisposal(e, n) {
  let t = n ? e.ye : e.Le;
  if (!t)
    return;
  if (Array.isArray(t)) {
    for (let e2 = 0;e2 < t.length; e2++) {
      const n2 = t[e2];
      n2.call(n2);
    }
  } else {
    t.call(t);
  }
  n ? e.ye = null : e.Le = null;
}
function childId(e, n) {
  let t = e;
  while (t.T & CONFIG_TRANSPARENT && t.ve)
    t = t.ve;
  if (t.id != null)
    return formatId(t.id, n ? t.qe++ : t.qe);
  throw new Error("");
}
function getNextChildId(e) {
  return childId(e, true);
}
function inheritId(e, n, t) {
  return e?.id ?? (n ? t?.id : t?.id != null ? getNextChildId(t) : undefined);
}
function formatId(e, n) {
  const t = n.toString(36), i = t.length - 1;
  return e + (i ? String.fromCharCode(64 + i) : "") + t;
}
function getOwner() {
  return context;
}
function cleanup(e) {
  if (!context)
    return e;
  if (!context.Le)
    context.Le = e;
  else if (Array.isArray(context.Le))
    context.Le.push(e);
  else
    context.Le = [context.Le, e];
  return e;
}
function disposeRootSelf(e = true) {
  disposeChildren(this, e);
}
function createOwner(e) {
  const n = context;
  const t = e?.transparent ?? false;
  const i = {
    id: inheritId(e, t, n),
    T: t ? CONFIG_TRANSPARENT : 0,
    Nt: true,
    Tt: n?.Nt ? n.Tt : n,
    ke: null,
    Fe: null,
    rt: null,
    Le: null,
    C: n?.C ?? globalQueue,
    we: n?.we || defaultContext,
    qe: 0,
    ye: null,
    Qe: null,
    ve: n,
    dispose: disposeRootSelf
  };
  if (n) {
    const e2 = n.ke;
    if (e2 === null) {
      n.ke = i;
    } else {
      i.Fe = e2;
      e2.rt = i;
      n.ke = i;
    }
  }
  return i;
}
function createRoot(e, n) {
  const t = createOwner(n);
  return runWithOwner(t, () => e(() => t.dispose()));
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(l) {
  const n = l.nt;
  const e = l.tt;
  const u = l.ue;
  const s = l.ll;
  if (u !== null)
    u.ll = s;
  else
    n.st = s;
  if (s !== null)
    s.ue = u;
  else {
    n.o = u;
    if (u === null) {
      n.ut?.();
      const l2 = n;
      l2.ae && l2.T & CONFIG_AUTO_DISPOSE && !(l2.se & REACTIVE_ZOMBIE) && !(l2.S & STATUS_PENDING) && unobserved(l2);
    }
  }
  return e;
}
function trimStaleDeps(l) {
  const n = l.We;
  let e = n !== null ? n.tt : l.et;
  if (e !== null) {
    do {
      e = unlinkSubs(e);
    } while (e !== null);
    if (n !== null)
      n.tt = null;
    else
      l.et = null;
  }
}
function unobserved(l) {
  deleteFromHeap(l, queueFor(l));
  let n = l.et;
  while (n !== null) {
    n = unlinkSubs(n);
  }
  l.et = null;
  l.We = null;
  disposeChildren(l, true);
}
function link(l, n, e = false) {
  const u = n.We;
  if (u !== null && u.nt === l) {
    u.ge &&= e;
    return;
  }
  let s = null;
  const t = n.se & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = u !== null ? u.tt : n.et;
    if (s !== null && s.nt === l) {
      s.nl = n.Ye;
      n.We = s;
      s.ge = e;
      return;
    }
  }
  const i = l.st;
  if (i !== null && i.le === n && (!t || i.nl === n.Ye)) {
    if (t)
      i.ge &&= e;
    else
      i.ge = e;
    return;
  }
  const o = n.We = l.st = {
    nt: l,
    le: n,
    tt: s,
    ll: i,
    ue: null,
    nl: n.Ye,
    ge: e
  };
  if (u !== null)
    u.tt = o;
  else
    n.et = o;
  if (i !== null)
    i.ue = o;
  else
    l.o = o;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.oe?.has(n))
    return false;
  (e.oe ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  if (!e.oe?.delete(n))
    return false;
  if (e.oe.size === 0)
    e.oe = undefined;
  return true;
}
function clearPendingSources(e) {
  e.oe?.clear();
  e.oe = undefined;
}
function setPendingError(e, n, t) {
  if (!n) {
    e._ = null;
    return;
  }
  if (t instanceof NotReadyError && t.source === n) {
    e._ = t;
    return;
  }
  const r = e._;
  if (!(r instanceof NotReadyError) || r.source !== n) {
    e._ = new NotReadyError(n);
  }
}
function forEachDependent(e, n) {
  for (let t = e.o;t !== null; t = t.ue)
    n(t.le, t);
  for (let t = e.u ?? null;t !== null; t = t.fe) {
    for (let e2 = t.o;e2 !== null; e2 = e2.ue)
      n(e2.le, e2);
  }
}
function releaseIfSettledUnobserved(e) {
  e.ae && e.T & CONFIG_AUTO_DISPOSE && !e.o && !(e.se & REACTIVE_ZOMBIE) && !(e.S & STATUS_PENDING) && unobserved(e);
}
function releaseSettledDependents(e) {
  let n;
  const t = new Set;
  const visit = (e2) => {
    if (t.has(e2))
      return;
    t.add(e2);
    if (!e2.o && e2.T & CONFIG_AUTO_DISPOSE)
      (n ??= []).push(e2);
    forEachDependent(e2, visit);
  };
  forEachDependent(e, visit);
  if (n)
    for (const e2 of n)
      releaseIfSettledUnobserved(e2);
}
function settlePendingSource(e) {
  let n = false;
  let t;
  const r = new Set;
  const o = GlobalQueue.ce;
  const settle = (u) => {
    if (r.has(u) || !removePendingSource(u, e))
      return;
    r.add(u);
    u.Se = clock;
    const l = u.oe?.values().next().value;
    if (l) {
      setPendingError(u, l);
      o !== null && o(u);
    } else {
      u.S &= ~STATUS_PENDING;
      setPendingError(u);
      o !== null && o(u);
      if (u.de) {
        enqueueSub(u);
        n = true;
      }
      u.de = false;
      if (!u.o && u.T & CONFIG_AUTO_DISPOSE)
        (t ??= []).push(u);
    }
    forEachDependent(u, settle);
  };
  forEachDependent(e, settle);
  if (t)
    for (const e2 of t)
      releaseIfSettledUnobserved(e2);
  if (n)
    schedule();
}
function isThenable(e) {
  return e != null && typeof e === "object" && typeof e.then === "function";
}
function handleAsync(e, n, t) {
  let r = false;
  let o = false;
  if (typeof n === "object" && n !== null) {
    untrack(() => {
      r = n[Symbol.asyncIterator];
      o = !r && isThenable(n);
    });
  }
  if (!o && !r) {
    e.Te = null;
    return n;
  }
  e.Te = n;
  let u;
  const settleTransition = () => {
    const n2 = resolveTransition(e);
    if (n2 && e.S & STATUS_UNINITIALIZED && !currentTransition(n2).Ee.has(e)) {
      e.Ie = null;
      return;
    }
    globalQueue.initTransition(n2);
  };
  const handleError = (t2) => {
    if (e.Te !== n)
      return;
    settleTransition();
    const r2 = t2 instanceof NotReadyError;
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    e.Se = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, o2) => {
    if (e.Te !== n)
      return;
    if (e.se & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    settleTransition();
    const u2 = !!(e.S & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const l = resolveLane(e);
    if (l)
      l.Ne.delete(e);
    if (t) {
      t(r2);
      if (u2)
        clearStatus(e, true);
    } else if (e.Ae !== undefined) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = r2;
      GlobalQueue.De !== null && GlobalQueue.De(e, r2);
      if (!hasActiveOverride(e))
        insertSubs(e);
      e.Se = clock;
    } else if (l) {
      const n2 = e.Pe;
      const t2 = e.Ue;
      const o3 = e.Re;
      try {
        if (!n2 && u2 || !o3 || !o3(r2, t2)) {
          e.Ue = r2;
          e.Se = clock;
          GlobalQueue.De !== null && GlobalQueue.De(e, r2);
          insertSubs(e, true);
        }
      } catch (n3) {
        notifyStatus(e, STATUS_ERROR, n3);
      }
    } else {
      try {
        setSignal(e, () => r2);
      } catch (n2) {
        notifyStatus(e, STATUS_ERROR, n2);
      }
    }
    settlePendingSource(e);
    schedule();
    flush();
    o2?.();
  };
  const settleAutodispose = () => {
    if (e.T & CONFIG_AUTO_DISPOSE && !e.o && !(e.S & STATUS_PENDING)) {
      unobserved(e);
      return true;
    }
    return false;
  };
  if (o) {
    let t2 = false, r2 = false, o2, l = true;
    n.then((e2) => {
      if (l) {
        u = e2;
        t2 = true;
      } else {
        asyncWrite(e2);
        settleAutodispose();
      }
    }, (e2) => {
      if (l) {
        o2 = e2;
        r2 = true;
      } else {
        handleError(e2);
        settleAutodispose();
      }
    });
    l = false;
    if (r2) {
      handleError(o2);
      throw o2;
    } else if (!t2) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  if (r) {
    const t2 = n[Symbol.asyncIterator]();
    let r2 = false;
    let o2 = false;
    let l = true;
    cleanup(() => {
      if (o2)
        return;
      o2 = true;
      try {
        const e2 = t2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    });
    const iterateOrRelease = () => {
      if (!settleAutodispose())
        iterate();
    };
    const iterate = () => {
      let s2, i, f = false, a = false, c = true;
      t2.next().then((t3) => {
        if (c) {
          s2 = t3;
          f = true;
          if (t3.done)
            o2 = true;
        } else if (e.Te !== n) {
          return;
        } else if (!t3.done) {
          r2 = true;
          asyncWrite(t3.value, iterateOrRelease);
        } else {
          o2 = true;
          if (r2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
          settleAutodispose();
        }
      }, (t3) => {
        if (c) {
          i = t3;
          a = true;
        } else if (e.Te === n) {
          o2 = true;
          handleError(t3);
          settleAutodispose();
        }
      });
      c = false;
      if (a) {
        o2 = true;
        handleError(i);
        if (l)
          throw i;
        return true;
      }
      if (f && !s2.done) {
        u = s2.value;
        r2 = true;
        return iterate();
      }
      return f && s2.done;
    };
    const s = iterate();
    l = false;
    if (!r2 && !s) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  return u;
}
function clearStatus(e, n = false) {
  if (e.oe)
    clearPendingSources(e);
  if (e.de)
    e.de = false;
  e.be = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e._)
    setPendingError(e);
  if (e.he || e.Ge)
    GlobalQueue.ce(e);
  if (e.u && GlobalQueue.Oe !== null)
    GlobalQueue.Oe(e);
  if (e.i)
    e.i();
}
function notifyStatus(e, n, t, r, o) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const u = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const l = u === e;
  const s = n === STATUS_PENDING && e.Ae !== undefined && !l;
  const i = s && hasActiveOverride(e);
  if (!r) {
    if (n === STATUS_PENDING && u) {
      addPendingSource(e, u);
      e.S = STATUS_PENDING | e.S & STATUS_UNINITIALIZED;
      setPendingError(e, u, t);
    } else {
      clearPendingSources(e);
      e.S = n | (n !== STATUS_ERROR ? e.S & STATUS_UNINITIALIZED : 0);
      e._ = t;
    }
    GlobalQueue.ce !== null && GlobalQueue.ce(e);
    if (e.u && GlobalQueue.Oe !== null)
      GlobalQueue.Oe(e);
  }
  if (o && !r) {
    assignOrMergeLane(e, o);
  }
  const f = r || i;
  const a = r || s ? undefined : o;
  if (e.i) {
    if (r && n === STATUS_PENDING) {
      return;
    }
    if (f) {
      e.i(n, t);
    } else {
      e.i();
    }
    return;
  }
  forEachDependent(e, (e2, r2) => {
    e2.Se = clock;
    if (n === STATUS_PENDING && u && !e2.oe?.has(u) || n !== STATUS_PENDING && (e2._ !== t || e2.oe)) {
      if (r2.ge && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f && !e2.Ie)
        queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.pe = recompute;
GlobalQueue.Ce = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.me)
      return true;
    e = e.ve;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.Pe;
  if (!t) {
    if (e.Ie && (!n || activeTransition) && activeTransition !== e.Ie)
      globalQueue.initTransition(e.Ie);
    deleteFromHeap(e, queueFor(e));
    e.Te = null;
    if (e.Ie || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ke !== null || e.Le !== null) {
      markDisposal(e);
      e.ye = e.Le;
      e.Qe = e.ke;
      e.Le = null;
      e.ke = null;
      e.qe = 0;
    }
  }
  let i = !!(e.se & REACTIVE_OPTIMISTIC_DIRTY);
  const u = e.Ae !== undefined && e.Ae !== NOT_PENDING;
  const l = !!(e.S & STATUS_UNINITIALIZED);
  const o = (e.se & REACTIVE_REASK) !== 0;
  const s = context;
  context = e;
  e.We = null;
  e.Ye++;
  e.se = REACTIVE_RECOMPUTING_DEPS;
  e.Se = clock;
  let a = e._e === NOT_PENDING ? e.Ue : e._e;
  let r = e.Ve;
  let c = tracking;
  let _ = currentOptimisticLane;
  tracking = true;
  const f = latestReadActive;
  latestReadActive = false;
  if (i) {
    const t2 = GlobalQueue.Ze(e, true);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.je.length) {
    const t2 = GlobalQueue.Ze(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const E = n && n !== EFFECT_USER;
  const N = stale;
  if (E)
    stale = true;
  try {
    if (e.T & CONFIG_SYNC) {
      a = e.ae(a);
      e.Te = null;
    } else {
      const t2 = e.Te;
      const n2 = e.ae(a);
      const i2 = typeof n2 === "object" && n2 !== null;
      const u2 = e.Te !== t2;
      a = u2 || !i2 ? n2 : handleAsync(e, n2);
      if (!u2 && !i2)
        e.Te = null;
    }
    if (e.S !== 0 || e.i !== undefined || e._ || e.be || e.de || e.oe !== undefined || e.he !== undefined || e.Ge !== undefined || e.u !== null)
      clearStatus(e, t);
    if (e.Me)
      GlobalQueue.Ke(e);
  } catch (t2) {
    if (t2 instanceof NotReadyError && currentOptimisticLane)
      GlobalQueue.ze(e);
    let n2 = false;
    if (t2 instanceof NotReadyError) {
      e.de = true;
      if (GlobalQueue.$e !== null)
        n2 = GlobalQueue.$e(e, o);
    }
    notifyStatus(e, t2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, t2, undefined, t2 instanceof NotReadyError ? e.Me : undefined);
    if (n2)
      GlobalQueue.k(e);
  } finally {
    tracking = c;
    latestReadActive = f;
    if (E)
      stale = N;
    e.se = REACTIVE_NONE | (t ? e.se & REACTIVE_SNAPSHOT_STALE : 0);
    context = s;
  }
  if (!e._) {
    trimStaleDeps(e);
    const o2 = u ? unwrapOverride(e.Ae) : e._e === NOT_PENDING ? e.Ue : e._e;
    let s2 = false;
    try {
      s2 = !n && l || !e.Re || !e.Re(o2, a);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && s2) {
      e.Be = !e._;
      if (!t)
        e.C.enqueue(n, e.Je ??= GlobalQueue.Xe.bind(null, e));
    }
    if (e._)
      ;
    else if (s2) {
      const l2 = u ? e.Ae : undefined;
      if (t || n && (activeTransition !== e.Ie || activeTransition === null) || i) {
        e.Ue = a;
        if (u && i) {
          e.Ae = a === undefined ? OVERRIDE_UNDEFINED : a;
          e._e = NOT_PENDING;
        }
      } else {
        e._e = a;
        if ((activeTransition || e.Ie) && GlobalQueue.De !== null)
          GlobalQueue.De(e, a);
      }
      if (e.o !== null && (!u || i || e.Ae !== l2))
        insertSubs(e, i || u);
    } else if (u) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = a;
    } else if (e.Ve != r) {
      for (let t2 = e.o;t2 !== null; t2 = t2.ue) {
        insertIntoHeapHeight(t2.le, queueFor(t2.le));
      }
    }
  }
  currentOptimisticLane = _;
  const T = e._e !== NOT_PENDING || e.Qe !== null || e.ye !== null || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  T && (!t || e.S & STATUS_PENDING) && (!e.Ie || u) && queuePendingNode(e);
  e.Ie && n && activeTransition !== e.Ie && runInTransition(e.Ie, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.se & REACTIVE_CHECK) {
    for (let t = e.et;t; t = t.tt) {
      const n = t.nt;
      const i = n.it || n;
      if (i.ae) {
        updateIfNecessary(i);
      }
      if (e.se & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.se & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e._ && e.Se < clock && !e.Te) {
    recompute(e);
  }
  e.se = e.se & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = {
    id: inheritId(t, n, context),
    T: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.V ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Re: t?.equals != null ? t.equals : isEqual,
    ut: t?.unobserved,
    Le: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ae: e,
    Ue: undefined,
    Ve: 0,
    u: null,
    lt: undefined,
    ot: null,
    et: null,
    We: null,
    Ye: 0,
    o: null,
    st: null,
    ve: context,
    Fe: null,
    rt: null,
    ke: null,
    se: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: STATUS_UNINITIALIZED,
    Se: clock,
    _e: NOT_PENDING,
    ye: null,
    Qe: null,
    Te: null,
    Ie: null,
    be: false
  };
  setupComputedNode(i, t);
  return i;
}
function createEffectNode(e, t, n, i, u, l) {
  const o = l?.transparent ?? false;
  const s = {
    id: inheritId(l, o, context),
    T: (o ? CONFIG_TRANSPARENT : 0) | (l?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (l?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Re: false,
    ut: l?.unobserved,
    Le: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ae: e,
    Ue: undefined,
    Ve: 0,
    u: null,
    lt: undefined,
    ot: null,
    et: null,
    We: null,
    Ye: 0,
    o: null,
    st: null,
    ve: context,
    Fe: null,
    rt: null,
    ke: null,
    se: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    Se: clock,
    _e: NOT_PENDING,
    ye: null,
    Qe: null,
    Te: null,
    Ie: null,
    be: false,
    Be: false,
    ct: undefined,
    _t: t,
    ft: n,
    Et: undefined,
    Pe: i,
    i: u
  };
  setupComputedNode(s, lazyOptions);
  return s;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.ot = e;
  const n = context?.Nt ? context.Tt : context;
  if (context) {
    const t2 = context.ke;
    if (t2 === null) {
      context.ke = e;
    } else {
      e.Fe = t2;
      t2.rt = e;
      context.ke = e;
    }
  }
  if (n)
    e.Ve = n.Ve + 1;
  if (GlobalQueue.It !== null)
    GlobalQueue.It(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      e.xe = e.Ue === undefined ? NO_SNAPSHOT : e.Ue;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    Re: t?.equals != null ? t.equals : isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.V ? CONFIG_NO_SNAPSHOT : 0),
    ut: t?.unobserved,
    Ue: e,
    o: null,
    st: null,
    Se: clock,
    it: n,
    fe: n?.u || null,
    _e: NOT_PENDING
  };
  n && (n.u = i);
  if (snapshotCaptureActive && !(i.T & CONFIG_NO_SNAPSHOT) && !((n?.S ?? 0) & STATUS_PENDING)) {
    i.xe = e === undefined ? NO_SNAPSHOT : e;
    snapshotSources.add(i);
  }
  return i;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (GlobalQueue.dt === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.dt !== null)
      return GlobalQueue.dt(e);
    return e();
  } finally {
    tracking = n;
  }
}
function prepareComputed(e, t) {
  if (e.se & REACTIVE_LAZY) {
    e.se &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.se & REACTIVE_DISPOSED) {
    recompute(e, true);
  } else if (t) {
    updateIfNecessary(e);
  }
}
var READ_SLOW = Symbol("read-slow");
function read(e) {
  if (latestReadActive)
    return GlobalQueue.St(e);
  let t = context;
  if (t?.Nt)
    t = t.Tt;
  const n = e;
  const i = e.it;
  const u = i || e;
  if (pendingCheckActive) {
    GlobalQueue.At(e, t, u, i);
  } else if (typeof n.ae === "function") {
    prepareComputed(e, false);
  }
  if (!n.ae && u === e && e.Ae === undefined && e.xe === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e._e === NOT_PENDING ? e.Ue : e._e;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (u.ae) {
      const n2 = queueFor(e);
      if (u.Ve >= n2.He) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(u);
      }
      const i2 = u.Ve;
      if (i2 >= t.Ve && e.ve !== t) {
        t.Ve = i2 + 1;
      }
    }
  }
  if (u.S & STATUS_PENDING) {
    if (t && !(stale && u.Ie && activeTransition !== u.Ie)) {
      if (currentOptimisticLane === null || GlobalQueue.Ct(u)) {
        if (!tracking && e !== t)
          link(e, t);
        throw u._;
      }
    } else if (t && u !== e && u.S & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw u._;
    } else if (!t && u.S & STATUS_UNINITIALIZED) {
      throw u._;
    }
  }
  if (e.ae && e.S & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && e.Se < clock) {
      recompute(e);
      return read(e);
    } else
      throw e._;
  }
  if (snapshotCaptureActive && t && t.T & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.xe;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const u2 = e._e !== NOT_PENDING ? e._e : e.Ue;
      if (u2 !== i2)
        t.se |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.Ae !== undefined && e.Ae !== NOT_PENDING) {
    return unwrapOverride(e.Ae);
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.Ot(e, u, t)) {
    return e.Ue;
  }
  const l = !t || currentOptimisticLane !== null && GlobalQueue.Rt(e, u, t) || e._e === NOT_PENDING || stale && e.Ie && activeTransition !== e.Ie ? e.Ue : e._e;
  if (pendingCheckActive)
    GlobalQueue.Pt(e, l);
  if (!t && u === e && typeof n.ae === "function" && e.T & CONFIG_AUTO_DISPOSE && !(u.S & STATUS_PENDING) && !e.o) {
    unobserved(e);
  }
  return l;
}
function setSignal(e, t) {
  if (e.Ie && activeTransition !== e.Ie)
    globalQueue.initTransition(e.Ie);
  if (e.Ae !== undefined && !projectionWriteActive)
    return GlobalQueue.ht(e, t);
  const n = e._e === NOT_PENDING ? e.Ue : e._e;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.Re || !e.Re(n, t);
  if (!i)
    return t;
  if (e._e === NOT_PENDING)
    queuePendingNode(e);
  e._e = t;
  (e.he !== undefined || e.Ge !== undefined) && GlobalQueue.De !== null && GlobalQueue.De(e, t);
  e.Se = clock;
  insertSubs(e);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.se & REACTIVE_MANUAL_WRITE) && e._e === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.se = e.se & -4 | REACTIVE_MANUAL_WRITE;
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
function staleValues(e, t = true) {
  const n = stale;
  stale = t;
  try {
    return e();
  } finally {
    stale = n;
  }
}
// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, E, e, R) {
  const r = !!R?.user;
  const f = createEffectNode(t, E, e, r ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, R);
  recompute(f, true);
  !R?.defer && (f.Pe === EFFECT_USER || R?.schedule ? f.C.enqueue(f.Pe, runEffect.bind(null, f)) : runEffect(f));
}
function notifyEffectStatus(t, E) {
  const e = t !== undefined ? t : this.S;
  const R = E !== undefined ? E : this._;
  if (e & STATUS_ERROR) {
    this.C.notify(this, STATUS_PENDING, 0);
    if (this.Pe === EFFECT_USER) {
      if (this.S & STATUS_ERROR) {
        this.Be = true;
        this.C.enqueue(this.Pe, this.Je ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.C.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(R));
      throw R;
    }
  } else if (this.Pe === EFFECT_RENDER) {
    this.C.notify(this, STATUS_PENDING | STATUS_ERROR, e, R);
  }
}
function runEffect(t) {
  if (!t.Be || t.se & REACTIVE_DISPOSED)
    return;
  if (t.S & STATUS_ERROR && t.Pe === EFFECT_USER) {
    const E2 = unwrapStatusError(t._);
    t.ct = t.Ue;
    t.Be = false;
    try {
      t.ft ? t.ft(E2, () => {
        const E3 = t.Et;
        t.Et = undefined;
        E3?.();
      }) : console.error(E2);
    } catch (E3) {
      if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(E3);
        throw E3;
      }
    }
    return;
  }
  const E = t.Et;
  t.Et = undefined;
  try {
    E?.();
    const e = t._t(t.Ue, t.ct);
    if (false)
      ;
    t.Et = e;
  } catch (E2) {
    t._ = new StatusError(t, E2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(E2);
      throw E2;
    }
  } finally {
    t.ct = t.Ue;
    t.Be = false;
  }
}
GlobalQueue.Xe = runEffect;
function trackedEffect(t, E) {
  const run = () => {
    if (!e.Be || e.se & REACTIVE_DISPOSED)
      return;
    try {
      e.Be = false;
      recompute(e);
    } finally {}
  };
  const e = computed(() => {
    const E2 = e.Et;
    e.Et = undefined;
    E2?.();
    const R = staleValues(t);
    e.Et = R;
  }, {
    ...E,
    lazy: true
  });
  e.Et = undefined;
  e.T = e.T & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  e.Be = true;
  e.Pe = EFFECT_TRACKED;
  e.i = (t2, E2) => {
    const R = t2 !== undefined ? t2 : e.S;
    if (R & STATUS_ERROR) {
      e.C.notify(e, STATUS_PENDING, 0);
      const t3 = E2 !== undefined ? E2 : e._;
      if (!e.C.notify(e, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(unwrapStatusError(t3));
        throw t3;
      }
    }
  };
  e.Ft = run;
  e.C.enqueue(EFFECT_USER, run);
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/signals.js
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
    n2.T &= ~CONFIG_AUTO_DISPOSE;
    return [accessor(n2), setMemo.bind(null, n2)];
  }
  const n = signal(e, t);
  return [accessor(n), setSignal.bind(null, n)];
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
  t && !(t.T & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    e();
  });
}
// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var $AFFECTS = Symbol(0);
var STORE_SELF_PENDING = Symbol(0);
var storeLookup = new WeakMap;
var symbolKeyedRecords = new WeakSet;
var rawValues = new WeakSet;
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
var affectsScopes = new Map;

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/store/utils.js
function trueFn() {
  return true;
}
var propTraps = {
  get(e, t, r) {
    if (t === $PROXY)
      return r;
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
  const r = [];
  for (let n2 = 0;n2 < e.length; n2++) {
    const o2 = e[n2];
    t = t || !!o2 && $PROXY in o2;
    const s2 = !!o2 && o2[$SOURCES];
    if (s2) {
      for (let e2 = 0;e2 < s2.length; e2++)
        r.push(s2[e2]);
    } else
      r.push(typeof o2 === "function" ? (t = true, createMemo(o2)) : o2);
  }
  if (SUPPORTS_PROXY && t) {
    return new Proxy({
      get(e2) {
        if (e2 === $SOURCES)
          return r;
        for (let t2 = r.length - 1;t2 >= 0; t2--) {
          const n2 = resolveSource(r[t2]);
          if (e2 in n2)
            return n2[e2];
        }
      },
      has(e2) {
        for (let t2 = r.length - 1;t2 >= 0; t2--) {
          if (e2 in resolveSource(r[t2]))
            return true;
        }
        return false;
      },
      keys() {
        const e2 = new Set;
        for (let t2 = 0;t2 < r.length; t2++) {
          const n2 = ownEnumerableKeys(resolveSource(r[t2]));
          for (let t3 = 0;t3 < n2.length; t3++)
            e2.add(n2[t3]);
        }
        return [...e2];
      }
    }, propTraps);
  }
  const n = Object.create(null);
  let o = false;
  let s = r.length - 1;
  for (let e2 = s;e2 >= 0; e2--) {
    const t2 = r[e2];
    if (!t2) {
      e2 === s && s--;
      continue;
    }
    const i2 = Object.getOwnPropertyNames(t2);
    for (let r2 = i2.length - 1;r2 >= 0; r2--) {
      const c2 = i2[r2];
      if (c2 === "__proto__" || c2 === "constructor")
        continue;
      if (!n[c2]) {
        o = o || e2 !== s;
        const r3 = Object.getOwnPropertyDescriptor(t2, c2);
        n[c2] = r3.get ? {
          enumerable: true,
          configurable: true,
          get: r3.get.bind(t2)
        } : r3;
      }
    }
  }
  if (!o)
    return r[s];
  const i = {};
  const c = Object.keys(n);
  for (let e2 = c.length - 1;e2 >= 0; e2--) {
    const t2 = c[e2], r2 = n[t2];
    if (r2.get)
      Object.defineProperty(i, t2, r2);
    else
      i[t2] = r2.value;
  }
  i[$SOURCES] = r;
  return i;
}
// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/boundaries.js
var ON_INIT = Symbol();
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.O() : e.v.size === 0 && !e.N;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.U() : isSlotReady(e);
}
function setSlotState(e, t, r, n) {
  setSignal(e.I, r);
  setSignal(e.D, n);
  if (isRevealController(e)) {
    if (!r && e.P === t)
      e.P = undefined;
    return e.j(r, n);
  }
  if (!r && e.B === t && e.W)
    e.B = undefined;
}

class RevealController {
  L;
  F;
  q = [];
  P;
  I = signal(false, {
    ownedWrite: true,
    V: true
  });
  D = signal(false, {
    ownedWrite: true,
    V: true
  });
  H = true;
  J = true;
  K = false;
  constructor(e, t) {
    this.L = e;
    this.F = t;
  }
  X(e) {
    for (let t = 0;t < this.q.length; t++) {
      const r = this.q[t];
      if ((isRevealController(r) ? r.P : r.B) !== this)
        continue;
      if (e(r) === false)
        return false;
    }
    return true;
  }
  O() {
    return this.X(isSlotReady);
  }
  U() {
    const e = untrack(this.L);
    if (e === "together")
      return this.X(isSlotMinimallyReady);
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.X((r) => {
        e2 = true;
        if (isSlotMinimallyReady(r)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.X((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  Y(e) {
    if (this.q.includes(e))
      return;
    this.q.push(e);
    const t = untrack(this.L);
    setSignal(e.I, true), setSignal(e.D, t === "sequential" ? !!untrack(this.F) : false);
    untrack(() => this.j());
  }
  Z(e) {
    const t = this.q.indexOf(e);
    if (t >= 0)
      this.q.splice(t, 1);
    untrack(() => this.j());
  }
  j(e, t) {
    if (this.K)
      return;
    this.K = true;
    const r = this.H;
    const n = this.J;
    try {
      const r2 = e ?? read(this.I), n2 = untrack(this.L), s = n2 === "sequential" && !!untrack(this.F), i = t ?? s;
      if (r2) {
        this.X((e2) => setSlotState(e2, this, true, i));
      } else if (n2 === "natural") {
        this.X((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.D, false);
            setSignal(e2.I, false);
            e2.j(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (n2 === "together") {
        const e2 = this.X(isSlotMinimallyReady);
        this.X((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.X((t2) => {
          if (e2)
            return setSlotState(t2, this, true, s);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.D, false);
            setSignal(t2.I, false);
            t2.j(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.H = this.O();
      this.J = this.U();
      this.K = false;
    }
    if (this.P && (r !== this.H || n !== this.J))
      this.P.j();
  }
}

class CollectionQueue extends Queue {
  $;
  v = new Set;
  ee;
  N = true;
  I = signal(false, {
    ownedWrite: true,
    V: true
  });
  _;
  D = signal(false, {
    ownedWrite: true,
    V: true
  });
  B;
  W = false;
  te;
  re = ON_INIT;
  constructor(e) {
    super();
    this.$ = e;
  }
  run(e) {
    if (!e || read(this.I) && (!_revealUsed || read(this.D)))
      return;
    return super.run(e);
  }
  notify(e, t, r, n) {
    if (!(t & this.$))
      return super.notify(e, t, r, n);
    if (this.W && this.te) {
      const e2 = untrack(() => {
        try {
          return this.te();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.re) {
        this.re = e2;
        this.W = false;
        this.v.clear();
      }
    }
    if (this.$ & STATUS_PENDING && this.W)
      return super.notify(e, t, r, n);
    if (r & this.$) {
      this.N = true;
      const t2 = n?.source || e._?.source;
      if (t2) {
        const e2 = this.v.size === 0;
        this.v.add(t2);
        if (e2)
          setSignal(this.I, true);
        if (this.$ & STATUS_ERROR) {
          setSignal(this._, unwrapStatusError(t2._));
        }
      }
    }
    t &= ~this.$;
    return t ? super.notify(e, t, r, n) : true;
  }
  ne() {
    for (const e of this.v) {
      if (e.se & REACTIVE_DISPOSED || !e.t && !(e.S & this.$) && !(this.$ & STATUS_ERROR && e.S & STATUS_PENDING))
        this.v.delete(e);
    }
    if (!this.v.size) {
      if (this.$ & STATUS_PENDING && this.N && !this.W && this.ee) {
        this.N = !!(this.ee.S & this.$);
      } else {
        this.N = false;
      }
      if (!this.N) {
        setSignal(this.I, false);
        if (this.te) {
          try {
            this.re = untrack(() => this.te());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this.B?.j();
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
    let r = [];
    if (flattenArray(e, r, t)) {
      return () => {
        let e2 = [];
        flattenArray(r, e2, {
          ...t,
          doNotUnwrap: false
        });
        return e2;
      };
    }
    return r;
  }
  return e;
}
function flattenArray(e, t = [], r) {
  let n = null;
  let s = false;
  for (let i = 0;i < e.length; i++) {
    try {
      let n2 = e[i];
      if (typeof n2 === "function" && !n2.length) {
        if (r?.doNotUnwrap) {
          t.push(n2);
          s = true;
          continue;
        }
        do {
          n2 = n2();
        } while (typeof n2 === "function" && !n2.length);
      }
      if (Array.isArray(n2)) {
        s = flattenArray(n2, t, r);
      } else if (r?.skipNonRendered && (n2 == null || n2 === true || n2 === false || n2 === "")) {} else
        t.push(n2);
    } catch (e2) {
      if (!(e2 instanceof NotReadyError))
        throw e2;
      n = e2;
    }
  }
  if (n)
    throw n;
  return s;
}
// node_modules/.bun/solid-js@2.0.0-beta.26/node_modules/solid-js/dist/solid.js
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createMemo;
var _createRenderEffect;
var createMemo2 = (...args) => (_createMemo || createMemo)(...args);
var createRenderEffect2 = (...args) => (_createRenderEffect || createRenderEffect)(...args);
function createComponent(Comp, props) {
  return untrack(() => Comp(props || {}));
}

// node_modules/.bun/@solidjs+universal@2.0.0-beta.26+bdd014273d80398c/node_modules/@solidjs/universal/dist/universal.js
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
        createRoot((dispose2) => {
          disposer = dispose2;
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
      let dispose2;
      createRoot((d) => {
        dispose2 = d;
        const tree = code();
        baseInsert(element, () => tree, undefined, undefined, {
          schedule: true
        });
      });
      flush();
      return dispose2;
    }
  };
}

// packages/core/src/renderer.ts
import * as tree2 from "flux:rendertree";

// packages/core/src/window.ts
import { requestFrame } from "flux:rendertree";
import { renderFrame } from "srt:render";
import { on as on2, once } from "srt:events";
import { exit } from "srt:app";

// packages/core/src/core.ts
import * as tree from "flux:rendertree";
import { on } from "srt:events";
var handlers = new Map;
var MOVE_BIT = 1;
var POINTER_INTEREST = {
  onPointerMove: MOVE_BIT,
  onPointerDown: 2,
  onPointerUp: 4,
  onPointerEnter: 8,
  onPointerLeave: 16,
  onWheel: 32
};
var interests = new Map;
function syncInterest(nodeId) {
  let mask = 0;
  let nodeHandlers = handlers.get(nodeId);
  if (nodeHandlers)
    for (let name of nodeHandlers.keys())
      mask |= POINTER_INTEREST[name] ?? 0;
  if (nodeId === interestRoot && globalMoveSubs.size > 0)
    mask |= MOVE_BIT;
  if ((interests.get(nodeId) ?? 0) === mask)
    return;
  if (mask === 0)
    interests.delete(nodeId);
  else
    interests.set(nodeId, mask);
  tree.setEventInterest(nodeId, mask);
}
var globalMoveSubs = new Set;
var interestRoot = null;
function setInterestRoot(nodeId) {
  interestRoot = nodeId;
  if (nodeId != null)
    syncInterest(nodeId);
}
function setEventHandler(nodeId, name, fn) {
  if (fn == null) {
    handlers.get(nodeId)?.delete(name);
    if (name in POINTER_INTEREST)
      syncInterest(nodeId);
    return;
  }
  let nodeHandlers = handlers.get(nodeId);
  if (!nodeHandlers) {
    nodeHandlers = new Map;
    handlers.set(nodeId, nodeHandlers);
  }
  nodeHandlers.set(name, fn);
  if (name in POINTER_INTEREST)
    syncInterest(nodeId);
}
function getEventHandler(nodeId, name) {
  return handlers.get(nodeId)?.get(name);
}
function cleanupNode(nodeId) {
  handlers.delete(nodeId);
  interests.delete(nodeId);
  focusables.delete(nodeId);
  textHints.delete(nodeId);
}
var textHints = new Map;
function setTextInputHints(nodeId, hints) {
  if (hints == null)
    textHints.delete(nodeId);
  else
    textHints.set(nodeId, hints);
}
var focusedNodeId = null;
var [trackFocusedNode, setFocusedNodeSignal] = createSignal(null);
var textInputActiveNow = false;
var [trackTextInputActive, setTextInputActiveSignal] = createSignal(false);
function focusedNode() {
  trackFocusedNode();
  return focusedNodeId;
}
function textInputActive() {
  trackTextInputActive();
  return textInputActiveNow;
}
tree.setTextInputActive(false);
var screenKeyboard = true;
var physicalKeyboard = false;
on("inputDevices", (d) => {
  physicalKeyboard = !!d.keyboard;
  screenKeyboard = !!d.screenKeyboard;
  syncTextInput(textInputEligible() && (textInputActive() || textInputInvisible()));
});
var focusables = new Set;
function setFocusable(nodeId, focusable) {
  if (focusable)
    focusables.add(nodeId);
  else
    focusables.delete(nodeId);
}
function textInputEligible() {
  return focusedNodeId != null && getEventHandler(focusedNodeId, "onTextInput") != null;
}
function textInputInvisible() {
  return !screenKeyboard || physicalKeyboard;
}
var sessionNodeId = null;
function syncTextInput(active) {
  let target = active ? focusedNodeId : null;
  if (active === textInputActiveNow && target === sessionNodeId)
    return;
  textInputActiveNow = active;
  sessionNodeId = target;
  setTextInputActiveSignal(active);
  if (active)
    tree.setTextInputActive(true, textHints.get(target));
  else
    tree.setTextInputActive(false);
}
function setFocus(nodeId) {
  if (nodeId === focusedNodeId)
    return;
  let oldId = focusedNodeId;
  focusedNodeId = nodeId;
  setFocusedNodeSignal(nodeId);
  if (oldId != null) {
    getEventHandler(oldId, "onBlur")?.();
  }
  if (nodeId != null) {
    getEventHandler(nodeId, "onFocus")?.();
  }
  syncTextInput(textInputEligible() && (textInputActiveNow || textInputInvisible()));
}
function activateTextInput() {
  if (textInputEligible())
    syncTextInput(true);
}

// packages/core/src/window.ts
var animationFrames = new Map;
var refreshRate = 60;
var backHandlers = [];
function attachWindow(nodeId) {
  setInterestRoot(nodeId);
  let unsubscribe = null;
  let unsubDown = null;
  let unsubUp = null;
  let unsubMove = null;
  let unsubEnter = null;
  let unsubLeave = null;
  let unsubWheel = null;
  let unsubKeyDown = null;
  let unsubKeyUp = null;
  let unsubBack = null;
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
    scanForOrphans(t);
    renderFrame();
  }
  onSettled(() => {
    unsubRefreshRate = on2("displayRefreshRate", ({
      hz
    }) => {
      if (hz > 0)
        refreshRate = hz;
    });
    unsubscribe = on2("render", ({
      time,
      frame
    }) => {
      runFrame(time * 1000, frame);
    });
    let dispatchPath = (raw, handler, reverse) => {
      let {
        targets,
        localX,
        localY,
        parentX,
        parentY,
        ...e
      } = raw;
      let stopped = false;
      e.stopPropagation = () => {
        stopped = true;
      };
      let n = targets.length;
      for (let k = 0;k < n; k++) {
        let i = reverse ? n - 1 - k : k;
        e.currentTarget = targets[i];
        e.localX = localX[i];
        e.localY = localY[i];
        e.parentX = parentX[i];
        e.parentY = parentY[i];
        getEventHandler(targets[i], handler)?.(e);
        if (stopped)
          break;
      }
    };
    let bubble = (raw, handler) => dispatchPath(raw, handler, true);
    let dispatchOrdered = (raw, handler) => dispatchPath(raw, handler, false);
    unsubDown = on2("pointerDown", (raw) => {
      bubble(raw, "onPointerDown");
      let focused = focusedNode();
      if (focused != null && !raw.targets.includes(focused)) {
        setFocus(null);
      } else if (focused != null) {
        activateTextInput();
      }
    });
    unsubUp = on2("pointerUp", (raw) => {
      bubble(raw, "onPointerUp");
    });
    unsubMove = on2("pointerMove", (raw) => {
      bubble(raw, "onPointerMove");
    });
    unsubEnter = on2("pointerEnter", (raw) => {
      dispatchOrdered(raw, "onPointerEnter");
    });
    unsubLeave = on2("pointerLeave", (raw) => {
      dispatchOrdered(raw, "onPointerLeave");
    });
    unsubWheel = on2("wheel", (raw) => {
      bubble(raw, "onWheel");
    });
    let dispatchKey = (raw, handler) => {
      let target = focusedNode() ?? nodeId;
      let stopped = false;
      let e = {
        ...raw,
        target,
        stopPropagation: () => stopped = true
      };
      let path = getNodePath(target);
      if (path[path.length - 1] !== nodeId)
        path.push(nodeId);
      for (let id of path) {
        e.currentTarget = id;
        getEventHandler(id, handler)?.(e);
        if (stopped)
          break;
      }
    };
    unsubKeyDown = on2("keydown", (raw) => dispatchKey(raw, "onKeyDown"));
    unsubKeyUp = on2("keyup", (raw) => dispatchKey(raw, "onKeyUp"));
    unsubBack = on2("back", () => {
      let prevented = false;
      let e = {
        preventDefault: () => {
          prevented = true;
        }
      };
      let stack = [...backHandlers];
      for (let i = stack.length - 1;i >= 0 && !prevented; i--)
        stack[i](e);
      if (!prevented)
        exit();
    });
    unsubTextInput = on2("textInput", (e) => {
      let id = focusedNode();
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e);
      }
    });
    unsubKeyboardVisibility = on2("keyboardVisibility", ({
      shown
    }) => {
      if (!shown)
        setFocus(null);
    });
    unsubFirstResize = once("resize", () => {
      queueMicrotask(() => runFrame(0, 0));
    });
  });
  onCleanup(() => {
    setInterestRoot(null);
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
    if (unsubBack)
      unsubBack();
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
  let {
    r: r3,
    g: g2,
    b: b2,
    a: a3
  } = w(color).toRgb();
  return ((r3 & 255) << 24 | (g2 & 255) << 16 | (b2 & 255) << 8 | a3 * 255 & 255) >>> 0;
}
function isGradient(value) {
  return typeof value === "object" && value !== null && "__gradient" in value;
}

// packages/core/src/renderer.ts
var nodes = new Map;
var id = 1;
function createProxyNode(elementType) {
  let node = {
    id,
    elementType,
    children: []
  };
  nodes.set(id, node);
  id += 1;
  return node;
}
function getNodePath(id2) {
  let path = [];
  let node = nodes.get(id2);
  for (;node; node = node.parent)
    path.push(node.id);
  return path;
}
var pendingDestroy = new Map;
var destroyScheduled = false;
function destroyNode2(node) {
  tree2.destroyNode(node.id);
  let cleanup2 = (n3) => {
    for (let child of n3.children)
      if (child.parent === n3)
        cleanup2(child);
    if (n3.id === focusedNode())
      setFocus(null);
    nodes.delete(n3.id);
    cleanupNode(n3.id);
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
var SENTINEL_INTERVAL_MS = 5000;
var sentinelDue = 0;
var warnedLeakTypes = new Set;
function scanForOrphans(now) {
  if (true)
    return;
  if (now < sentinelDue)
    return;
  sentinelDue = now + SENTINEL_INTERVAL_MS;
  let counts = new Map;
  let total = 0;
  for (let node of nodes.values()) {
    if (node.parent !== undefined || node.elementType === "window" || pendingDestroy.has(node.id))
      continue;
    total += 1;
    counts.set(node.elementType, (counts.get(node.elementType) ?? 0) + 1);
  }
  if (total === 0)
    return;
  let fresh = [...counts].filter(([type]) => !warnedLeakTypes.has(type));
  if (fresh.length === 0)
    return;
  for (let [type] of fresh)
    warnedLeakTypes.add(type);
  let list = fresh.map(([type, n3]) => `<${type}> x${n3}`).join(", ");
  console.warn(`Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` + `The usual cause is reading an element-valued prop more than once (every read ` + `builds a new subtree); read it once where it mounts, or resolve it with ` + `children(). If these nodes are intentionally kept for later mounting, ignore ` + `this. Element types already reported are not reported again.`);
}
var warnedRejectedProps = new Set;
function setTreeProperty(node, name, value) {
  try {
    tree2.setProperty(node.id, name, value);
  } catch (e3) {
    let message = String(e3);
    if (!message.includes("unknown property") && !message.includes("detached-only"))
      throw e3;
    let key = node.elementType + "." + name;
    if (warnedRejectedProps.has(key))
      return;
    warnedRejectedProps.add(key);
    let stack = new Error().stack ?? "";
    console.warn(`Ignoring property '${name}' on <${node.elementType}>: ${message}
${stack}`);
  }
}
function applyProp(node, name, value) {
  if (!node)
    return;
  if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
    setEventHandler(node.id, name, value);
    return;
  }
  if (name === "focusable") {
    setFocusable(node.id, value === true);
    return;
  }
  if (name === "textInputHints") {
    setTextInputHints(node.id, value);
    return;
  }
  if (name === "color" && isGradient(value)) {
    setTreeProperty(node, name, value);
    return;
  }
  if (name === "color" && typeof value === "string") {
    setTreeProperty(node, name, parseColor(value));
    return;
  }
  setTreeProperty(node, name, value);
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
import { on as on3 } from "srt:events";
// packages/core/src/gamepad.ts
import { on as on4 } from "srt:events";
// packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { destroyTexture as destroyTexture2, resizeTexture, setShaderParams as setShaderParams2, setShaderSize as setShaderSize2, setShaderTextures, uploadTexture } from "flux:gpu";
import { copyTexture, destroyBuffer as destroyBuffer2, renderTarget, setDraw } from "flux:gpu";
import { limits } from "flux:gpu";
import { compileShader, createRenderPipeline, destroyProgram, destroyRenderPipeline, destroyShader, linkProgram } from "flux:gpu";
import { captureSnapshot, readTexture } from "flux:gpu";
var glsl = String.raw;
// packages/core/src/image.ts
var imageCache = new Map;
// packages/core/src/svg.ts
import { parseSvg as fluxParseSvg } from "flux:svg";
var svg = String.raw;
// lattice/launcher/bsod.tsx
function Bsod() {
  var _el$ = createElement("window", {
    title: "solidrt"
  }), _el$2 = createElement("d-rect", {
    color: "#1144bb"
  }), _el$3 = createElement("view", {
    flexGrow: 1,
    justifyContent: "center",
    alignItems: "center",
    flexDirection: "column",
    gap: 16
  }), _el$4 = createElement("text", {
    color: "white",
    fontSize: 64,
    fontWeight: 700
  }), _el$6 = createElement("text", {
    color: "white",
    fontSize: 22
  }), _el$8 = createElement("text", {
    color: "#aac2ff",
    fontSize: 15
  });
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  insertNode2(_el$3, _el$4);
  insertNode2(_el$3, _el$6);
  insertNode2(_el$3, _el$8);
  insertNode2(_el$4, createTextNode(`:(`));
  insertNode2(_el$6, createTextNode(`Something went wrong`));
  insertNode2(_el$8, createTextNode(`The application could not be started.`));
  return _el$;
}
render(() => createComponent2(Bsod, {}));
