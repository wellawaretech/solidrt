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

class NoOwnerError extends Error {
  constructor() {
    super("");
  }
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
var STORE_SNAPSHOT_PROPS = "sp";
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
function registerTransientStoreNode(e) {
  transientStoreNodes.add(e);
}
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
function setProjectionWriteActive(e) {
  projectionWriteActive = e;
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
var PENDING_OWNER = {};
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
function getObserver() {
  if (pendingCheckActive || latestReadActive)
    return PENDING_OWNER;
  return tracking ? context : null;
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
function readNodeFast(e) {
  if (latestReadActive || pendingCheckActive || e.ae || e.it || e.Ae !== undefined || e.xe !== undefined || activeTransition !== null || currentOptimisticLane !== null || snapshotCaptureActive || false)
    return READ_SLOW;
  let t = context;
  if (t?.Nt)
    t = t.Tt;
  if (t && tracking)
    link(e, t);
  return !t || e._e === NOT_PENDING ? e.Ue : e._e;
}
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
// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/core/context.js
function setContext(e, t, n = getOwner()) {
  if (!n) {
    throw new NoOwnerError;
  }
  n.we = {
    ...n.we,
    [e.id]: isUndefined(t) ? e.defaultValue : t
  };
}
function isUndefined(e) {
  return typeof e === "undefined";
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
function createEffect(e, t, n) {
  effect(e, t.effect || t, t.error, {
    user: true,
    ...n
  });
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
// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/store/reconcile.js
function nodeKeys(e) {
  const t = Object.keys(e);
  if (symbolKeyedRecords.has(e)) {
    const n = Object.getOwnPropertySymbols(e);
    for (let e2 = 0, a = n.length;e2 < a; e2++) {
      if (n[e2] !== $TRACK)
        t.push(n[e2]);
    }
  }
  return t;
}
function unwrap(e) {
  if (e === null || typeof e !== "object")
    return e;
  return e[$TARGET]?.[STORE_VALUE] ?? e;
}
function getOverrideValue(e, t, n, a) {
  if (a && n in a)
    return a[n];
  return t && n in t ? t[n] : e[n];
}
function addEnumSymbols(e, t, n) {
  for (let a = 0, r = t.length;a < r; a++) {
    if (Object.prototype.propertyIsEnumerable.call(e, t[a]))
      n.add(t[a]);
  }
}
function getAllKeys(e, t, n) {
  const a = getKeys(e, t);
  const r = Object.keys(n);
  const i = e[$TARGET] ? untrack(() => Object.getOwnPropertySymbols(e)) : Object.getOwnPropertySymbols(e);
  const s = Object.getOwnPropertySymbols(n);
  if (i.length === 0 && s.length === 0) {
    if (a.length === r.length) {
      let e3 = true;
      for (let t2 = 0;t2 < a.length; t2++) {
        if (a[t2] !== r[t2]) {
          e3 = false;
          break;
        }
      }
      if (e3)
        return a;
    }
    const e2 = new Set(a);
    for (let t2 = 0;t2 < r.length; t2++)
      e2.add(r[t2]);
    return Array.from(e2);
  }
  const l = new Set(a);
  addEnumSymbols(e, i, l);
  if (t) {
    for (const e2 of Reflect.ownKeys(t)) {
      t[e2] === $DELETED ? l.delete(e2) : l.add(e2);
    }
  }
  for (let e2 = 0;e2 < r.length; e2++)
    l.add(r[e2]);
  addEnumSymbols(n, s, l);
  return Array.from(l);
}
function wrapValue(e, t) {
  return isWrappable(e) ? wrap(e, t) : e;
}
function itemKey(e, t) {
  return isWrappable(e) ? t(e) : e;
}
function keyedMatch(e, t, n) {
  return e === t || isWrappable(e) && isWrappable(t) && n(e) === n(t);
}
function syncArrayNodeMembership(e, t) {
  let n = e[STORE_NODE];
  if (n) {
    if (symbolKeyedRecords.has(n)) {
      const e2 = nodeKeys(n);
      for (let a = 0, r = e2.length;a < r; a++) {
        e2[a] in t || setSignal(n[e2[a]], undefined);
      }
    } else {
      for (const e2 in n) {
        e2 in t || setSignal(n[e2], undefined);
      }
    }
  }
  if (n = e[STORE_HAS]) {
    if (symbolKeyedRecords.has(n)) {
      const e2 = nodeKeys(n);
      for (let a = 0, r = e2.length;a < r; a++) {
        setSignal(n[e2[a]], e2[a] in t);
      }
    } else {
      for (const e2 in n) {
        setSignal(n[e2], e2 in t);
      }
    }
  }
}
function applyStateChild(e, t, n, a) {
  if (n[STORE_WRAP] !== undefined) {
    applyState(e, wrap(t, n), a);
    return;
  }
  const r = t[$TARGET] ?? storeLookup.get(t);
  if (r === undefined)
    return;
  e = unwrap(e);
  if (r[STORE_SHALLOW]) {
    applyStateShallow(e, r);
  } else if (r[STORE_OVERRIDE] || r[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(e, r, a);
  } else {
    applyStateFast(e, r, a);
  }
}
function applyArrayItem(e, t, n, a, r) {
  if (isWrappable(e) && isWrappable(t) && !(rawValuesUsed && (isRawValue(t) || isRawValue(e)))) {
    const i = wrap(t, n);
    a && setSignal(a, i);
    applyState(e, i, r);
  } else
    a && setSignal(a, wrapValue(e, n));
}
function applyDescendants(e, t, n, a, r, i, s) {
  const l = n[STORE_LOOKUP] || storeLookup;
  if (i) {
    const n2 = getKeys(e, i).concat(getStoreSymbols(e, i));
    for (let o2 = 0, f = n2.length;o2 < f; o2++) {
      const f2 = n2[o2];
      if (a?.[f2])
        continue;
      const p = unwrap(getOverrideValue(e, i, f2, s));
      if (!isWrappable(p))
        continue;
      descendInto(p, t[f2], l, r);
    }
    return;
  }
  for (const n2 in e) {
    if (a?.[n2])
      continue;
    const i2 = unwrap(e[n2]);
    if (!isWrappable(i2))
      continue;
    descendInto(i2, t[n2], l, r);
  }
  const o = Object.getOwnPropertySymbols(e);
  for (let n2 = 0, i2 = o.length;n2 < i2; n2++) {
    if (Object.prototype.propertyIsEnumerable.call(e, o[n2])) {
      if (a?.[o[n2]])
        continue;
      const i3 = unwrap(e[o[n2]]);
      if (!isWrappable(i3))
        continue;
      descendInto(i3, t[o[n2]], l, r);
    }
  }
}
function descendInto(e, t, n, a) {
  const r = lookupTarget(e, n);
  if (!r?.[STORE_DESC])
    return;
  const i = unwrap(t);
  if (e === i || !isWrappable(i) || Array.isArray(e) !== Array.isArray(i) || a(e) != null && a(e) !== a(i))
    return;
  if (r[STORE_SHALLOW]) {
    applyStateShallow(i, r);
  } else if (r[STORE_OVERRIDE] || r[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(i, r, a);
  } else {
    applyStateFast(i, r, a);
  }
}
function applyState(e, t, n) {
  e = unwrap(e);
  const a = t?.[$TARGET];
  if (!a)
    return;
  if (a[STORE_SHALLOW]) {
    applyStateShallow(e, a);
  } else if (a[STORE_OVERRIDE] || a[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(e, a, n);
  } else {
    applyStateFast(e, a, n);
  }
}
function shallowDiffNodes(e, t, n, a) {
  let r = false;
  for (const i in e) {
    if (a && i === "length")
      continue;
    if (i in t) {
      const a2 = t[i];
      if (a2 !== n(i)) {
        r = true;
        setSignal(e[i], a2);
      }
    } else {
      r = true;
      setSignal(e[i], undefined);
    }
  }
  return r;
}
function applyStateShallow(e, t, n) {
  const a = t[STORE_VALUE];
  const r = t[STORE_OVERRIDE];
  const i = t[STORE_OPTIMISTIC_OVERRIDE];
  if (e === a && !r && !i)
    return;
  const prevAt = (e2) => {
    const t2 = getOverrideValue(a, r, e2, i);
    return t2 === $DELETED ? undefined : t2;
  };
  t[STORE_OVERRIDE] = undefined;
  const s = t[STORE_LOOKUP];
  s !== undefined ? s.set(e, t[$PROXY]) : storeLookup.set(e, t);
  t[STORE_VALUE] = e;
  markRawIngest(e);
  const l = t[STORE_NODE];
  const o = l && l[$TRACK];
  let f = false;
  if (Array.isArray(a)) {
    const n2 = r?.length ?? i?.length ?? a.length;
    if (l) {
      f = shallowDiffNodes(l, e, prevAt, true);
      if (l.length && n2 !== e.length)
        setSignal(l.length, e.length);
    }
    if (!f && (o || t[STORE_HAS])) {
      if (n2 !== e.length)
        f = true;
      else {
        for (let t2 = 0, n3 = e.length;t2 < n3; t2++) {
          if (prevAt(t2) !== e[t2]) {
            f = true;
            break;
          }
        }
      }
    }
  } else {
    if (l) {
      f = shallowDiffNodes(l, e, prevAt, false);
    }
    if (!f && (o || t[STORE_HAS]))
      f = true;
  }
  let p = t[STORE_HAS];
  if (p) {
    for (const t2 in p) {
      setSignal(p[t2], t2 in e);
    }
  }
  f && notifySelf(t);
}
function applyStateFast(e, t, n) {
  const a = t[STORE_VALUE];
  if (e === a)
    return;
  const r = t[STORE_NODE];
  {
    const n2 = t[STORE_LOOKUP];
    n2 !== undefined ? n2.set(e, t[$PROXY]) : storeLookup.set(e, t);
  }
  t[STORE_VALUE] = e;
  if (Array.isArray(a)) {
    let i2 = false;
    const s2 = a.length;
    if (e.length && s2 && isWrappable(e[0]) && n(e[0]) != null) {
      let l, o, f, p, u, c, S, R;
      for (f = 0, p = Math.min(s2, e.length);f < p && keyedMatch(c = a[f], e[f], n); f++) {
        if (c !== e[f]) {
          if (rawValuesUsed && (isRawValue(c) || isRawValue(e[f]))) {
            r?.[f] && setSignal(r[f], wrapValue(e[f], t));
          } else
            applyStateChild(e[f], c, t, n);
        }
      }
      if (f === e.length && f === s2)
        return;
      const E = new Array(e.length), O = new Map;
      for (p = s2 - 1, u = e.length - 1;p >= f && u >= f && keyedMatch(c = a[p], e[u], n); p--, u--) {
        E[u] = c;
      }
      if (f > u || f > p) {
        for (o = f;o <= u; o++) {
          i2 = true;
          r?.[o] && setSignal(r[o], wrapValue(e[o], t));
        }
        for (;o < e.length; o++) {
          i2 = true;
          applyArrayItem(e[o], E[o], t, r?.[o], n);
        }
        syncArrayNodeMembership(t, e);
        (i2 || s2 !== e.length) && notifySelf(t);
        s2 !== e.length && r?.length && setSignal(r.length, e.length);
        return;
      }
      S = new Array(u + 1);
      for (o = u;o >= f; o--) {
        c = e[o];
        R = itemKey(c, n);
        l = O.get(R);
        S[o] = l === undefined ? -1 : l;
        O.set(R, o);
      }
      for (l = f;l <= p; l++) {
        c = a[l];
        R = itemKey(c, n);
        o = O.get(R);
        if (o !== undefined && o !== -1) {
          E[o] = c;
          o = S[o];
          O.set(R, o);
        }
      }
      for (o = f;o < e.length; o++) {
        if (o in E) {
          applyArrayItem(e[o], E[o], t, r?.[o], n);
        } else
          r?.[o] && setSignal(r[o], wrapValue(e[o], t));
      }
      if (f < e.length)
        i2 = true;
    } else if (e.length) {
      for (let s3 = 0, l = e.length;s3 < l; s3++) {
        const l2 = a[s3];
        if (isWrappable(l2) && isWrappable(e[s3]) && !(rawValuesUsed && (isRawValue(l2) || isRawValue(e[s3])))) {
          if (l2 !== e[s3])
            applyStateChild(e[s3], l2, t, n);
        } else {
          if (l2 !== e[s3])
            i2 = true;
          r?.[s3] && setSignal(r[s3], wrapValue(e[s3], t));
        }
      }
    }
    syncArrayNodeMembership(t, e);
    if (s2 !== e.length) {
      i2 = true;
      r?.length && setSignal(r.length, e.length);
    }
    i2 && notifySelf(t);
    return;
  }
  let i = t[STORE_NODE];
  let s;
  if (i) {
    s = i[$TRACK];
    if (s || symbolKeyedRecords.has(i)) {
      const r2 = s ? getAllKeys(a, undefined, e) : nodeKeys(i);
      for (let l = 0, o = r2.length;l < o; l++) {
        const o2 = r2[l];
        const f = i[o2];
        const p = unwrap(a[o2]);
        const u = unwrap(e[o2]);
        if (p === u)
          continue;
        if (!p || !isWrappable(p) || !isWrappable(u) || rawValuesUsed && (isRawValue(p) || isRawValue(u)) || Array.isArray(p) !== Array.isArray(u) || n(p) != null && n(p) !== n(u)) {
          s && setSignal(s, undefined);
          f && setSignal(f, isWrappable(u) ? wrap(u, t) : u);
        } else
          applyStateChild(u, p, t, n);
      }
    } else {
      for (const r2 in i) {
        const l = i[r2];
        const o = unwrap(a[r2]);
        const f = unwrap(e[r2]);
        if (o === f)
          continue;
        if (!o || !isWrappable(o) || !isWrappable(f) || rawValuesUsed && (isRawValue(o) || isRawValue(f)) || Array.isArray(o) !== Array.isArray(f) || n(o) != null && n(o) !== n(f)) {
          s && setSignal(s, undefined);
          l && setSignal(l, isWrappable(f) ? wrap(f, t) : f);
        } else
          applyStateChild(f, o, t, n);
      }
    }
  }
  if (!s && t[STORE_DESC])
    applyDescendants(a, e, t, i, n);
  if (i = t[STORE_HAS]) {
    const t2 = nodeKeys(i);
    for (let n2 = 0, a2 = t2.length;n2 < a2; n2++) {
      const a3 = t2[n2];
      setSignal(i[a3], a3 in e);
    }
  }
}
function applyStateSlow(e, t, n) {
  const a = t[STORE_VALUE];
  const r = t[STORE_OVERRIDE];
  const i = t[STORE_OPTIMISTIC_OVERRIDE];
  let s = t[STORE_NODE];
  {
    const n2 = t[STORE_LOOKUP];
    n2 !== undefined ? n2.set(e, t[$PROXY]) : storeLookup.set(e, t);
  }
  t[STORE_VALUE] = e;
  t[STORE_OVERRIDE] = undefined;
  if (Array.isArray(a)) {
    let l2 = false;
    const o = getOverrideValue(a, r, "length", i);
    if (e.length && o && isWrappable(e[0]) && n(e[0]) != null) {
      let f2, p, u, c, S, R, E, O;
      for (u = 0, c = Math.min(o, e.length);u < c && keyedMatch(R = getOverrideValue(a, r, u, i), e[u], n); u++) {
        if (isWrappable(R) && isWrappable(e[u]) && R !== e[u]) {
          if (rawValuesUsed && (isRawValue(R) || isRawValue(e[u]))) {
            s?.[u] && setSignal(s[u], wrapValue(e[u], t));
          } else
            applyState(e[u], wrap(R, t), n);
        }
      }
      const y = new Array(e.length), d = new Map;
      for (c = o - 1, S = e.length - 1;c >= u && S >= u && keyedMatch(R = getOverrideValue(a, r, c, i), e[S], n); c--, S--) {
        y[S] = R;
      }
      if (u > S || u > c) {
        for (p = u;p <= S; p++) {
          l2 = true;
          s?.[p] && setSignal(s[p], wrapValue(e[p], t));
        }
        for (;p < e.length; p++) {
          l2 = true;
          applyArrayItem(e[p], y[p], t, s?.[p], n);
        }
        const a2 = e.length;
        syncArrayNodeMembership(t, e);
        (l2 || o !== a2) && notifySelf(t);
        o !== a2 && s?.length && setSignal(s.length, a2);
        return;
      }
      E = new Array(S + 1);
      for (p = S;p >= u; p--) {
        R = e[p];
        O = itemKey(R, n);
        f2 = d.get(O);
        E[p] = f2 === undefined ? -1 : f2;
        d.set(O, p);
      }
      for (f2 = u;f2 <= c; f2++) {
        R = getOverrideValue(a, r, f2, i);
        O = itemKey(R, n);
        p = d.get(O);
        if (p !== undefined && p !== -1) {
          y[p] = R;
          p = E[p];
          d.set(O, p);
        }
      }
      for (p = u;p < e.length; p++) {
        if (p in y) {
          applyArrayItem(e[p], y[p], t, s?.[p], n);
        } else
          s?.[p] && setSignal(s[p], wrapValue(e[p], t));
      }
      if (u < e.length)
        l2 = true;
    } else if (e.length) {
      for (let o2 = 0, f2 = e.length;o2 < f2; o2++) {
        const f3 = getOverrideValue(a, r, o2, i);
        if (isWrappable(f3) && isWrappable(e[o2]) && !(rawValuesUsed && (isRawValue(f3) || isRawValue(e[o2])))) {
          if (f3 !== e[o2])
            applyState(e[o2], wrap(f3, t), n);
        } else {
          if (f3 !== e[o2])
            l2 = true;
          s?.[o2] && setSignal(s[o2], wrapValue(e[o2], t));
        }
      }
    }
    const f = e.length;
    syncArrayNodeMembership(t, e);
    if (o !== f) {
      l2 = true;
      s?.length && setSignal(s.length, f);
    }
    l2 && notifySelf(t);
    return;
  }
  let l;
  if (s) {
    l = s[$TRACK];
    const o = l ? getAllKeys(a, r, e) : nodeKeys(s);
    for (let f = 0, p = o.length;f < p; f++) {
      const p2 = o[f];
      const u = s[p2];
      const c = unwrap(getOverrideValue(a, r, p2, i));
      let S = unwrap(e[p2]);
      if (c === S)
        continue;
      if (!c || !isWrappable(c) || !isWrappable(S) || rawValuesUsed && (isRawValue(c) || isRawValue(S)) || Array.isArray(c) !== Array.isArray(S) || n(c) != null && n(c) !== n(S)) {
        l && setSignal(l, undefined);
        u && setSignal(u, isWrappable(S) ? wrap(S, t) : S);
      } else
        applyState(S, wrap(c, t), n);
    }
  }
  if (!l && t[STORE_DESC])
    applyDescendants(a, e, t, s, n, r, i);
  if (s = t[STORE_HAS]) {
    const t2 = nodeKeys(s);
    for (let n2 = 0, a2 = t2.length;n2 < a2; n2++) {
      const a3 = t2[n2];
      setSignal(s[a3], a3 in e);
    }
  }
}
var NOKEY = () => null;
function reconcile(e, t = "id") {
  return (n) => {
    if (n == null)
      throw new Error("");
    if (t === null) {
      applyState(e, n, NOKEY);
      return;
    }
    const a = typeof t === "string" ? (e2) => e2[t] : t;
    const r = a(n);
    if (r !== undefined && a(e) !== r)
      throw new Error("");
    applyState(e, n, a);
  };
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/store/projection.js
function createProjectionInternal(e, r, t) {
  let o;
  const n = new WeakMap;
  const i = !!t?.shallow;
  const wrapper = (e2) => {
    e2[STORE_WRAP] = wrapProjection;
    e2[STORE_LOOKUP] = n;
    if (i) {
      e2[STORE_SHALLOW] = true;
      markRawIngest(e2[STORE_VALUE]);
    }
    Object.defineProperty(e2, STORE_FIREWALL, {
      get() {
        return o;
      },
      configurable: true
    });
  };
  const wrapProjection = (e2) => {
    if (n.has(e2))
      return n.get(e2);
    if (e2[$TARGET]?.[STORE_WRAP] === wrapProjection)
      return e2;
    const r2 = createStoreProxy(e2, storeTraps, wrapper);
    n.set(e2, r2);
    return r2;
  };
  const c = wrapProjection(r);
  o = computed(() => {
    if (!o)
      o = getOwner();
    runProjectionComputed(c, e, t?.key || "id");
  }, undefined);
  o.T &= ~CONFIG_AUTO_DISPOSE;
  return {
    store: c,
    node: o
  };
}
function runProjectionComputed(e, r, t, o, n) {
  const i = getOwner();
  let c = false;
  let s;
  const u = new Proxy(e, createWriteTraps(() => !c || i.Te === s, n));
  storeSetter(u, (n2) => {
    s = r(n2);
    c = true;
    const commit = (r2) => {
      if (r2 === n2 || r2 === undefined)
        return;
      const write = () => storeSetter(e, reconcile(r2, t));
      o ? o(write) : write();
    };
    commit(handleAsync(i, s, commit));
  });
  return i;
}
function createWriteTraps(e, r) {
  const t = {
    get(e2, r2) {
      let o;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        o = e2[r2];
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      if (r2 === $TARGET)
        return o;
      return typeof o === "object" && o !== null ? new Proxy(o, t) : o;
    },
    has(e2, r2) {
      let t2;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        t2 = r2 in e2;
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return t2;
    },
    set(t2, o, n) {
      if (e && !e())
        return true;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        t2[o] = n;
        r?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return true;
    },
    deleteProperty(t2, o) {
      if (e && !e())
        return true;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        delete t2[o];
        r?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return true;
    }
  };
  return t;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var $AFFECTS = Symbol(0);
var STORE_VALUE = "v";
var STORE_OVERRIDE = "o";
var STORE_OPTIMISTIC_OVERRIDE = "x";
var STORE_NODE = "n";
var STORE_HAS = "h";
var STORE_CUSTOM_PROTO = "c";
var STORE_WRAP = "w";
var STORE_LOOKUP = "l";
var STORE_FIREWALL = "f";
var STORE_OPTIMISTIC = "p";
var STORE_OPTIMISTIC_OWNERS = "t";
var STORE_PARENT = "u";
var STORE_DESC = "d";
var STORE_SHALLOW = "s";
var STORE_SELF_PENDING = Symbol(0);
function initStoreFields(e) {
  e[STORE_OVERRIDE] = undefined;
  e[STORE_OPTIMISTIC_OVERRIDE] = undefined;
  e[STORE_OPTIMISTIC_OWNERS] = undefined;
  e[STORE_NODE] = undefined;
  e[STORE_HAS] = undefined;
  e[STORE_CUSTOM_PROTO] = undefined;
  e[STORE_WRAP] = undefined;
  e[STORE_LOOKUP] = undefined;
  e[STORE_FIREWALL] = undefined;
  e[STORE_OPTIMISTIC] = undefined;
  e[STORE_SNAPSHOT_PROPS] = undefined;
  e[STORE_PARENT] = undefined;
  e[STORE_DESC] = undefined;
  e[STORE_SHALLOW] = undefined;
  e[$PROXY] = null;
}
function createStoreProxy(e, t = storeTraps, r) {
  let n;
  if (Array.isArray(e)) {
    n = [];
    n[STORE_VALUE] = e;
    initStoreFields(n);
  } else {
    n = {
      [STORE_VALUE]: e
    };
    initStoreFields(n);
    const t2 = e?.[$TARGET]?.[STORE_VALUE] ?? e;
    const r2 = Object.getPrototypeOf(t2);
    if (r2 !== null && r2 !== Object.prototype) {
      n[STORE_CUSTOM_PROTO] = true;
    }
  }
  r && r(n);
  return n[$PROXY] = new Proxy(n, t);
}
var storeLookup = new WeakMap;
var symbolKeyedRecords = new WeakSet;
function lookupTarget(e, t) {
  if (t !== undefined && t !== storeLookup) {
    const r = t.get(e);
    if (r !== undefined)
      return r[$TARGET];
  }
  return storeLookup.get(e);
}
var rawValues = new WeakSet;
var rawValuesUsed = false;
function isRawValue(e) {
  return rawValuesUsed && rawValues.has(e);
}
function markRawOne(e) {
  if (isWrappable(e)) {
    if (e[$TARGET] !== undefined)
      return;
    rawValuesUsed = true;
    rawValues.add(e);
  }
}
function markRawIngest(e) {
  if (Array.isArray(e)) {
    for (let t = 0, r = e.length;t < r; t++)
      markRawOne(e[t]);
  } else {
    for (const t in e)
      markRawOne(e[t]);
  }
}
function wrap(e, t) {
  if (rawValuesUsed && rawValues.has(e))
    return e;
  if (t?.[STORE_WRAP]) {
    const r2 = t[STORE_WRAP](e, t);
    const n2 = r2[$TARGET];
    if (n2 && !n2[STORE_PARENT] && n2 !== t)
      n2[STORE_PARENT] = t;
    return r2;
  }
  const r = storeLookup.get(e);
  if (r !== undefined)
    return r[$PROXY];
  let n = e[$PROXY];
  if (!n) {
    n = createStoreProxy(e);
    const r2 = n[$TARGET];
    storeLookup.set(e, r2);
    if (t)
      r2[STORE_PARENT] = t;
  }
  return n;
}
function wrapShallow(e) {
  const t = storeLookup.get(e);
  if (t !== undefined) {
    if (t[STORE_SHALLOW])
      return t[$PROXY];
  }
  const r = createStoreProxy(e);
  const n = r[$TARGET];
  n[STORE_SHALLOW] = true;
  storeLookup.set(e, n);
  markRawIngest(e);
  return r;
}
function isWrappable(e) {
  if (e == null || typeof e !== "object" || Object.isFrozen(e))
    return false;
  return typeof Node === "undefined" || !(e instanceof Node);
}
var writeOverride = false;
function setWriteOverride(e) {
  writeOverride = e;
}
function writeOnly(e) {
  return writeOverride || !!Writing?.has(e);
}
function unwrapStoreValue(e, t, r) {
  const n = e?.[$TARGET] || lookupTarget(e, r);
  if (!n)
    return e;
  const o = n[STORE_OVERRIDE];
  if (!o)
    return n[STORE_VALUE];
  if (!t)
    t = new Map;
  if (t.has(e))
    return t.get(e);
  const i = n[STORE_VALUE];
  const O = Array.isArray(i);
  const s = O ? [] : Object.create(Object.getPrototypeOf(i));
  t.set(e, s);
  r = n[STORE_LOOKUP] ?? storeLookup;
  for (const e2 of getStoreKeys(i, o)) {
    if (O && e2 === "length")
      continue;
    const n2 = e2 in o ? o[e2] : i[e2];
    if (n2 !== $DELETED)
      s[e2] = unwrapStoreValue(n2, t, r);
  }
  if (O)
    s.length = o.length ?? i.length;
  return s;
}
function isPrototypePollutionKey(e) {
  return e === "__proto__" || e === "constructor" || e === "prototype";
}
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
function ownEnumerableSymbols(e) {
  const t = Object.getOwnPropertySymbols(e);
  const r = [];
  for (let n = 0, o = t.length;n < o; n++) {
    const o2 = t[n];
    if (Object.prototype.propertyIsEnumerable.call(e, o2))
      r.push(o2);
  }
  return r;
}
function ownEnumerableKeysPlain(e) {
  return Object.keys(e).concat(ownEnumerableSymbols(e));
}
function getOverlayLayer(e, t) {
  const r = e[STORE_OPTIMISTIC_OVERRIDE];
  if (r && t in r)
    return r;
  const n = e[STORE_OVERRIDE];
  if (n && t in n)
    return n;
  return;
}
function visibleNodeValue(e) {
  return e.Ae !== undefined && e.Ae !== NOT_PENDING ? unwrapOverride(e.Ae) : e._e !== NOT_PENDING ? e._e : e.Ue;
}
function hasOwnStoreProperty(e, t) {
  const r = getOverlayLayer(e, t);
  if (r)
    return r[t] !== $DELETED;
  return Object.prototype.hasOwnProperty.call(unwrapStoreValue(e[STORE_VALUE]), t);
}
function hasInheritedAccessor(e, t) {
  let r = Object.getPrototypeOf(e);
  while (r && r !== Object.prototype) {
    const e2 = Reflect.getOwnPropertyDescriptor(r, t);
    if (e2)
      return !!e2.get;
    r = Object.getPrototypeOf(r);
  }
  return false;
}
function getNodes(e, t) {
  let r = e[t];
  if (!r)
    e[t] = r = Object.create(null);
  return r;
}
function getNode(e, t, r, n, o = isEqual, i) {
  if (t[r])
    return t[r];
  const O = signal(n, {
    equals: o,
    unobserved() {
      if (t[r] === O) {
        delete t[r];
        if (typeof r === "symbol" && r !== $TRACK && r !== $AFFECTS && symbolKeyedRecords.has(t)) {
          const e2 = Object.getOwnPropertySymbols(t);
          let r2 = false;
          for (let t2 = 0, n2 = e2.length;t2 < n2; t2++) {
            if (e2[t2] !== $TRACK && e2[t2] !== $AFFECTS) {
              r2 = true;
              break;
            }
          }
          if (!r2)
            symbolKeyedRecords.delete(t);
        }
      }
    }
  }, e[STORE_FIREWALL]);
  if (e[STORE_OPTIMISTIC]) {
    O.Ae = NOT_PENDING;
  }
  if (i && r in i) {
    const e2 = i[r];
    O.xe = e2 === undefined ? NO_SNAPSHOT : e2;
    snapshotSources?.add(O);
  }
  if (typeof r === "symbol" && r !== $TRACK && r !== $AFFECTS)
    symbolKeyedRecords.add(t);
  if (r !== $AFFECTS && affectsScopes.size)
    inheritAffectsMarks(O, e[STORE_VALUE], r);
  let s = e;
  while (s && !s[STORE_DESC]) {
    s[STORE_DESC] = true;
    s = s[STORE_PARENT];
  }
  return t[r] = O;
}
function inheritAffectsMarks(e, t, r) {
  for (const [n, o] of affectsScopes) {
    if (n.t && o.scope.has(t) && (o.key === undefined || o.key === r)) {
      GlobalQueue.M(e);
      o.inherited.push(e);
    }
  }
}
var affectsScopes = new Map;
function witnessAffectsMark(e, t) {
  const r = e[STORE_NODE]?.[$AFFECTS];
  if (r?.t)
    GlobalQueue.Dt(r);
  if (affectsScopes.size) {
    const n = e[STORE_VALUE];
    for (const [e2, o] of affectsScopes) {
      if (e2 !== r && e2.t && o.scope.has(n) && (o.key === undefined || o.key === t))
        GlobalQueue.Dt(e2);
    }
  }
}
function trackSelf(e, t = $TRACK) {
  if (!getObserver())
    return;
  read(getNode(e, getNodes(e, STORE_NODE), t, undefined, false));
  if (t === $TRACK && !e[STORE_OVERRIDE] && !e[STORE_OPTIMISTIC_OVERRIDE] && e[STORE_VALUE][$TARGET])
    e[STORE_VALUE][$TRACK];
}
function notifySelf(e) {
  const t = e[STORE_NODE]?.[$TRACK];
  t && setSignal(t, e[STORE_OPTIMISTIC] && !projectionWriteActive ? STORE_SELF_PENDING : undefined);
}
function getKeysImpl(e, t, r, n) {
  const o = e[$TARGET] ? untrack(() => r ? n ? ownEnumerableKeys(e) : Object.keys(e) : Reflect.ownKeys(e)) : r ? n ? ownEnumerableKeysPlain(e) : Object.keys(e) : Reflect.ownKeys(e);
  return t ? mergeOverrideKeys(o, t) : o;
}
function getKeys(e, t, r = true) {
  return getKeysImpl(e, t, r, false);
}
function getStoreKeys(e, t) {
  return getKeysImpl(e, t, true, true);
}
function getStoreSymbols(e, t) {
  const r = e[$TARGET] ? untrack(() => ownEnumerableSymbols(e)) : ownEnumerableSymbols(e);
  return t ? mergeOverrideKeys(r, t, true) : r;
}
function mergeOverrideKeys(e, t, r) {
  const n = new Set(e);
  const o = r ? Object.getOwnPropertySymbols(t) : Reflect.ownKeys(t);
  for (const e2 of o) {
    if (t[e2] !== $DELETED)
      n.add(e2);
    else
      n.delete(e2);
  }
  return Array.from(n);
}
function getPropertyDescriptor(e, t, r) {
  if (t && r in t) {
    if (t[r] === $DELETED)
      return;
    const n = Reflect.getOwnPropertyDescriptor(t, r);
    if (n?.get || n?.set)
      return n;
    const o = Reflect.getOwnPropertyDescriptor(e, r);
    if (!o)
      return n;
    if (o.get || o.set)
      return o;
    o.value = t[r];
    return o;
  }
  return Reflect.getOwnPropertyDescriptor(e, r);
}
function prepareStoreWrite(e, t, r) {
  if (e[STORE_OPTIMISTIC]) {
    const t2 = e[STORE_FIREWALL];
    if (t2?.Ie) {
      globalQueue.initTransition(t2.Ie);
    }
  }
  const n = e[STORE_VALUE];
  const o = n[r];
  if (snapshotCaptureActive && typeof r !== "symbol" && !((e[STORE_FIREWALL]?.S ?? 0) & STATUS_PENDING)) {
    if (!e[STORE_SNAPSHOT_PROPS]) {
      e[STORE_SNAPSHOT_PROPS] = Object.create(null);
      snapshotSources?.add(e);
    }
    if (!(r in e[STORE_SNAPSHOT_PROPS])) {
      e[STORE_SNAPSHOT_PROPS][r] = o;
    }
  }
  const i = e[STORE_OPTIMISTIC] && !projectionWriteActive;
  const O = i ? STORE_OPTIMISTIC_OVERRIDE : STORE_OVERRIDE;
  return {
    base: o,
    overrideKey: O,
    state: n
  };
}
function armOptimisticStoreWrite(e, t) {
  if (e[STORE_OPTIMISTIC] && !projectionWriteActive) {
    GlobalQueue.Nn(t);
  }
}
function stampOptimisticOwner(e, t, r) {
  if (t === STORE_OPTIMISTIC_OVERRIDE)
    (e[STORE_OPTIMISTIC_OWNERS] ??= Object.create(null))[r] = activeTransition;
}
function upsertStoreNode(e, t, r, n, o) {
  if (t[r])
    return t[r];
  const i = isWrappable(n) ? wrap(n, e) : n;
  const O = getNode(e, t, r, i, isEqual, o);
  registerTransientStoreNode(O);
  return O;
}
function notifyStoreProperty(e, t, r, n, o, i) {
  const O = projectionWriteActive || e[STORE_OPTIMISTIC];
  const s = r !== "delete";
  const E = e[STORE_HAS]?.[t];
  if (E) {
    setSignal(E, s);
  } else if (!O && r !== "invalidate" && i !== s) {
    const r2 = upsertStoreNode(e, getNodes(e, STORE_HAS), t, i);
    setSignal(r2, s);
  }
  const S = getNodes(e, STORE_NODE);
  if (r === "set") {
    if (S[t]) {
      setSignal(S[t], () => isWrappable(n) ? wrap(n, e) : n);
    } else if (!O) {
      const r2 = upsertStoreNode(e, S, t, o, e[STORE_SNAPSHOT_PROPS]);
      setSignal(r2, () => isWrappable(n) ? wrap(n, e) : n);
    }
  } else if (r === "invalidate") {
    if (S[t]) {
      setSignal(S[t], {});
      delete S[t];
    }
  } else {
    if (S[t]) {
      setSignal(S[t], undefined);
    } else if (!O) {
      const r2 = upsertStoreNode(e, S, t, o, e[STORE_SNAPSHOT_PROPS]);
      setSignal(r2, undefined);
    }
  }
  notifySelf(e);
}
var Writing = null;
function throwIfUninitialized(e) {
  const t = e[STORE_FIREWALL];
  if (t && t.S & STATUS_UNINITIALIZED)
    throw t._ ?? new NotReadyError(t);
}
var storeTraps = {
  get(e, t, r) {
    if (t === $TARGET)
      return e;
    if (t === $PROXY)
      return r;
    if (t === $REFRESH)
      return e[STORE_FIREWALL];
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    if (t === $TRACK) {
      trackSelf(e);
      return r;
    }
    if (e[STORE_FIREWALL] === undefined && e[STORE_OVERRIDE] === undefined && e[STORE_OPTIMISTIC_OVERRIDE] === undefined && !writeOverride && (Writing === null || !Writing.has(r))) {
      const r2 = e[STORE_NODE];
      const n2 = r2 && r2[t];
      if (n2 !== undefined && e[STORE_VALUE][$TARGET] === undefined) {
        let t2 = readNodeFast(n2);
        if (t2 === READ_SLOW)
          t2 = read(n2);
        if (t2 === $DELETED)
          t2 = undefined;
        if (!snapshotCaptureActive) {
          return t2;
        }
        return isWrappable(t2) ? wrap(t2, e) : t2;
      }
    }
    const n = getObserver() === e[STORE_FIREWALL];
    const o = getNodes(e, STORE_NODE);
    const i = n ? undefined : o[t];
    const O = e[STORE_VALUE];
    if (!i && !e[STORE_OVERRIDE] && !e[STORE_OPTIMISTIC_OVERRIDE] && !e[STORE_CUSTOM_PROTO] && !e[STORE_OPTIMISTIC] && !e[STORE_SNAPSHOT_PROPS] && !O[$TARGET] && !(t in O) && getObserver() && !n && !writeOnly(r)) {
      return read(getNode(e, o, t, undefined));
    }
    const s = getOverlayLayer(e, t);
    const E = !!s;
    const S = !!e[STORE_VALUE][$TARGET];
    const f = s ?? e[STORE_VALUE];
    if (!i) {
      const n2 = Object.getOwnPropertyDescriptor(f, t);
      if (n2 && n2.get)
        return n2.get.call(r);
      if (!n2 && !E && e[STORE_CUSTOM_PROTO]) {
        const e2 = unwrapStoreValue(f);
        if (hasInheritedAccessor(e2, t)) {
          return Reflect.get(f, t, r);
        }
      }
    }
    if (writeOnly(r)) {
      if (isPrototypePollutionKey(t) && !hasOwnStoreProperty(e, t))
        return;
      let r2 = i && (E || !S) ? visibleNodeValue(i) : f[t];
      r2 === $DELETED && (r2 = undefined);
      if (!isWrappable(r2))
        return r2;
      if (e[STORE_SHALLOW])
        return r2;
      const n2 = wrap(r2, e);
      Writing?.add(n2);
      return n2;
    }
    let R = i ? E || !S ? read(o[t]) : (read(o[t]), f[t]) : f[t];
    R === $DELETED && (R = undefined);
    if (!i) {
      if (!E && typeof R === "function" && !Object.prototype.hasOwnProperty.call(f, t)) {
        let t2;
        return !Array.isArray(e[STORE_VALUE]) && (t2 = Object.getPrototypeOf(e[STORE_VALUE])) && t2 !== Object.prototype ? R.bind(f) : R;
      } else if (getObserver() && !n) {
        return read(getNode(e, o, t, isWrappable(R) ? wrap(R, e) : R, isEqual, e[STORE_SNAPSHOT_PROPS]));
      }
    }
    if (!n && !getObserver())
      throwIfUninitialized(e);
    return isWrappable(R) ? wrap(R, e) : R;
  },
  has(e, t) {
    if (t === $PROXY || t === $TRACK || t === "__proto__")
      return true;
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    const r = getOverlayLayer(e, t);
    const n = r ? r[t] !== $DELETED : (t in e[STORE_VALUE]);
    if (writeOnly(e[$PROXY]) || getObserver() === e[STORE_FIREWALL])
      return n;
    const o = getNodes(e, STORE_HAS);
    if (o[t])
      return read(o[t]);
    if (getObserver()) {
      return read(getNode(e, o, t, n));
    }
    throwIfUninitialized(e);
    return n;
  },
  set(e, t, r) {
    if (t === "__proto__")
      return true;
    const n = e[$PROXY];
    if (writeOnly(n)) {
      untrack(() => {
        const { base: o, overrideKey: i, state: O } = prepareStoreWrite(e, n, t);
        const s = getOverlayLayer(e, t);
        const E = s ? s[t] : o;
        const S = s ? s[t] !== $DELETED : (t in e[STORE_VALUE]);
        const f = !!e[STORE_SHALLOW] && r?.[$TARGET] !== undefined;
        const R = f ? r : unwrapStoreValue(r);
        if (e[STORE_SHALLOW] && !f && isWrappable(R)) {
          rawValuesUsed = true;
          rawValues.add(R);
        }
        const c = typeof t === "string" ? Number(t) : -1;
        const T = Array.isArray(O) && Number.isInteger(c) && c >= 0 && c < 4294967295 && String(c) === t;
        const u = T ? c + 1 : 0;
        const a = T && (getOverlayLayer(e, "length") ?? O).length;
        const l = T && u > a ? u : undefined;
        if (E === R && l === undefined)
          return true;
        armOptimisticStoreWrite(e, n);
        if (R !== undefined && R === o && l === undefined) {
          delete e[i]?.[t];
          if (i === STORE_OPTIMISTIC_OVERRIDE)
            delete e[STORE_OPTIMISTIC_OWNERS]?.[t];
        } else {
          const r2 = e[i] || (e[i] = Object.create(null));
          r2[t] = R;
          stampOptimisticOwner(e, i, t);
          if (l !== undefined) {
            r2.length = l;
            stampOptimisticOwner(e, i, "length");
          }
        }
        notifyStoreProperty(e, t, "set", R, E, S);
        if (Array.isArray(O) && t === "length" && typeof R === "number" && typeof E === "number" && R < E) {
          const t2 = e[i] || (e[i] = Object.create(null));
          for (let r2 = R;r2 < E; r2++) {
            if (t2[r2] === $DELETED)
              continue;
            const n2 = r2 in t2 ? t2[r2] : O[r2];
            if (!(r2 in t2) && !(r2 in O))
              continue;
            t2[r2] = $DELETED;
            stampOptimisticOwner(e, i, r2);
            notifyStoreProperty(e, r2, "delete", undefined, n2, true);
          }
        }
        if (Array.isArray(O) && t !== "length" && l !== undefined) {
          const t2 = getNodes(e, STORE_NODE);
          if (t2.length) {
            setSignal(t2.length, l);
          } else if (!projectionWriteActive && !e[STORE_OPTIMISTIC]) {
            const r2 = upsertStoreNode(e, t2, "length", a, e[STORE_SNAPSHOT_PROPS]);
            setSignal(r2, l);
          }
        }
        if (false)
          ;
      });
    }
    return true;
  },
  defineProperty(e, t, r) {
    if (t === "__proto__")
      return true;
    const n = e[$PROXY];
    if (writeOnly(n)) {
      untrack(() => {
        const { base: o, overrideKey: i } = prepareStoreWrite(e, n, t);
        armOptimisticStoreWrite(e, n);
        const O = "value" in r ? {
          ...r,
          value: unwrapStoreValue(r.value)
        } : r;
        Object.defineProperty(e[i] || (e[i] = Object.create(null)), t, O);
        stampOptimisticOwner(e, i, t);
        notifyStoreProperty(e, t, "invalidate");
        if (false)
          ;
      });
    }
    return true;
  },
  deleteProperty(e, t) {
    if (t === "__proto__")
      return true;
    const r = e[STORE_OPTIMISTIC_OVERRIDE]?.[t] === $DELETED;
    const n = e[STORE_OVERRIDE]?.[t] === $DELETED;
    if (writeOnly(e[$PROXY]) && !r && !n) {
      untrack(() => {
        const r2 = e[STORE_OPTIMISTIC] && !projectionWriteActive;
        const n2 = r2 ? STORE_OPTIMISTIC_OVERRIDE : STORE_OVERRIDE;
        const o = getOverlayLayer(e, t);
        const i = o ? o[t] : e[STORE_VALUE][t];
        if (t in e[STORE_VALUE] || e[STORE_OVERRIDE] && t in e[STORE_OVERRIDE]) {
          armOptimisticStoreWrite(e, e[$PROXY]);
          (e[n2] || (e[n2] = Object.create(null)))[t] = $DELETED;
          stampOptimisticOwner(e, n2, t);
        } else if (e[n2] && t in e[n2]) {
          armOptimisticStoreWrite(e, e[$PROXY]);
          delete e[n2][t];
          if (n2 === STORE_OPTIMISTIC_OVERRIDE)
            delete e[STORE_OPTIMISTIC_OWNERS]?.[t];
        } else
          return true;
        notifyStoreProperty(e, t, "delete", undefined, i, true);
      });
    }
    return true;
  },
  ownKeys(e) {
    if (pendingCheckActive)
      witnessAffectsMark(e);
    if (getObserver() !== e[STORE_FIREWALL]) {
      trackSelf(e);
      if (!getObserver() && !writeOnly(e[$PROXY]))
        throwIfUninitialized(e);
    }
    let t = getKeys(e[STORE_VALUE], e[STORE_OVERRIDE], false);
    if (e[STORE_OPTIMISTIC_OVERRIDE]) {
      const r = new Set(t);
      for (const t2 of Reflect.ownKeys(e[STORE_OPTIMISTIC_OVERRIDE])) {
        if (e[STORE_OPTIMISTIC_OVERRIDE][t2] !== $DELETED)
          r.add(t2);
        else
          r.delete(t2);
      }
      t = Array.from(r);
    }
    return t;
  },
  getOwnPropertyDescriptor(e, t) {
    if (t === $PROXY)
      return {
        value: e[$PROXY],
        writable: true,
        configurable: true
      };
    if (e[STORE_OPTIMISTIC_OVERRIDE] && t in e[STORE_OPTIMISTIC_OVERRIDE]) {
      if (e[STORE_OPTIMISTIC_OVERRIDE][t] === $DELETED)
        return;
      const r2 = Reflect.getOwnPropertyDescriptor(e[STORE_OPTIMISTIC_OVERRIDE], t);
      if (r2?.get || r2?.set || !(t in e[STORE_VALUE]))
        return r2;
      const n = getPropertyDescriptor(e[STORE_VALUE], e[STORE_OVERRIDE], t);
      if (n) {
        const r3 = Reflect.getOwnPropertyDescriptor(e, t);
        const o = !r3 || r3.configurable ? true : n.configurable;
        return {
          ...n,
          configurable: o,
          value: e[STORE_OPTIMISTIC_OVERRIDE][t]
        };
      }
      return {
        value: e[STORE_OPTIMISTIC_OVERRIDE][t],
        writable: true,
        enumerable: true,
        configurable: true
      };
    }
    const r = getPropertyDescriptor(e[STORE_VALUE], e[STORE_OVERRIDE], t);
    if (r && !r.configurable) {
      const n = Reflect.getOwnPropertyDescriptor(e, t);
      if (!n || n.configurable)
        return {
          ...r,
          configurable: true
        };
    }
    return r;
  },
  getPrototypeOf(e) {
    return Object.getPrototypeOf(e[STORE_VALUE]);
  }
};
function storeSetter(e, t) {
  const r = Writing;
  Writing = new Set;
  Writing.add(e);
  try {
    const r2 = t(e);
    if (r2 !== e && r2 !== undefined) {
      if (Array.isArray(r2)) {
        for (let t2 = 0, n = r2.length;t2 < n; t2++)
          e[t2] = r2[t2];
        e.length = r2.length;
      } else {
        const t2 = new Set([...ownEnumerableKeys(e), ...ownEnumerableKeys(r2)]);
        t2.forEach((t3) => {
          if (t3 in r2)
            e[t3] = r2[t3];
          else
            delete e[t3];
        });
      }
    }
  } finally {
    Writing.clear();
    Writing = r;
  }
}
function createStore(e, t, r) {
  const n = typeof e === "function", o = n ? createProjectionInternal(e, t, r).store : t?.shallow ? wrapShallow(e) : wrap(e);
  return [o, n ? (e2) => {
    suppressComputedRecompute(o[$REFRESH]);
    storeSetter(o, e2);
  } : (e2) => storeSetter(o, e2)];
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.26/node_modules/@solidjs/signals/dist/prod/map.js
function mapArray(t, s, i) {
  const e = typeof i?.keyed === "function" ? i.keyed : undefined;
  const r = s.length > 1;
  const n = s;
  const h = {
    wt: createOwner(),
    jt: 0,
    Wt: t,
    Mt: [],
    Kt: n,
    xt: [],
    Gt: [],
    Ut: e,
    $t: e || i?.keyed === false ? [] : undefined,
    qt: r && i?.keyed !== false ? [] : undefined,
    zt: i?.keyed === false,
    Bt: i?.fallback
  };
  const o = computed(updateKeyedMap.bind(h));
  h.wt.Tt = o;
  o.T &= ~CONFIG_AUTO_DISPOSE;
  return accessor(o);
}
var pureOptions = {
  ownedWrite: true
};
function updateKeyedMap() {
  const t = this.Wt() || [], s = t.length;
  t[$TRACK];
  runWithOwner(this.wt, () => {
    let i, e, r, n, h = this.$t ? this.zt ? () => {
      r[e] = signal(t[e], pureOptions);
      return this.Kt(accessor(r[e]), e);
    } : () => {
      r[e] = signal(t[e], pureOptions);
      n && (n[e] = signal(e, pureOptions));
      return this.Kt(accessor(r[e]), n ? accessor(n[e]) : undefined);
    } : this.qt ? () => {
      const s2 = t[e];
      n[e] = signal(e, pureOptions);
      return this.Kt(s2, accessor(n[e]));
    } : () => {
      const s2 = t[e];
      return this.Kt(s2);
    };
    if (s === 0) {
      if (this.jt !== 0) {
        this.wt.dispose(false);
        this.Gt = [];
        this.Mt = [];
        this.xt = [];
        this.jt = 0;
        this.$t && (this.$t = []);
        this.qt && (this.qt = []);
      }
      if (this.Bt && !this.xt[0]) {
        this.Gt[0]?.dispose();
        this.xt[0] = runWithOwner(this.Gt[0] = createOwner(), this.Bt);
      }
    } else if (this.jt === 0) {
      const o = new Array(s);
      const c = new Array(s);
      r = this.$t && new Array(s);
      n = this.qt && new Array(s);
      try {
        for (e = 0;e < s; e++)
          o[e] = runWithOwner(c[e] = createOwner(), h);
      } catch (t2) {
        for (i = 0;i <= e; i++)
          c[i]?.dispose();
        throw t2;
      }
      if (this.Gt[0])
        this.Gt[0].dispose();
      this.xt = o;
      this.Gt = c;
      r && (this.$t = r);
      n && (this.qt = n);
      this.Mt = t.slice(0);
      this.jt = s;
    } else {
      let o, c, a, f, u, p, w, l, d;
      for (o = 0, c = Math.min(this.jt, s);o < c && (this.Mt[o] === t[o] || this.$t && compare(this.Ut, this.Mt[o], t[o])); o++) {
        if (this.$t)
          setSignal(this.$t[o], t[o]);
      }
      for (c = this.jt - 1, a = s - 1;c >= o && a >= o && (this.Mt[c] === t[a] || this.$t && compare(this.Ut, this.Mt[c], t[a])); c--, a--)
        ;
      if (o === s && this.jt === s) {
        this.Mt = t.slice(0);
        return;
      }
      const O = s - this.jt;
      const m = new Array(s);
      const _ = new Array(s);
      r = this.$t ? new Array(s) : undefined;
      n = this.qt ? new Array(s) : undefined;
      p = new Map;
      w = new Array(a + 1);
      for (e = a;e >= o; e--) {
        f = t[e];
        u = this.Ut ? this.Ut(f) : f;
        i = p.get(u);
        w[e] = i === undefined ? -1 : i;
        p.set(u, e);
      }
      for (i = o;i <= c; i++) {
        f = this.Mt[i];
        u = this.Ut ? this.Ut(f) : f;
        e = p.get(u);
        if (e !== undefined && e !== -1) {
          m[e] = this.xt[i];
          _[e] = this.Gt[i];
          r && (r[e] = this.$t[i]);
          n && (n[e] = this.qt[i]);
          e = w[e];
          p.set(u, e);
        } else
          (l ??= []).push(this.Gt[i]);
      }
      try {
        for (e = o;e <= a; e++) {
          if (_[e] !== undefined)
            continue;
          (d ??= []).push(_[e] = createOwner());
          m[e] = runWithOwner(_[e], h);
        }
      } catch (t2) {
        if (d)
          for (i = 0;i < d.length; i++)
            d[i].dispose();
        throw t2;
      }
      for (i = 0;i < o; i++) {
        m[i] = this.xt[i];
        _[i] = this.Gt[i];
        r && (r[i] = this.$t[i]);
        n && (n[i] = this.qt[i]);
      }
      for (e = o;e <= a; e++) {
        if (r)
          setSignal(r[e], t[e]);
        if (n)
          setSignal(n[e], e);
      }
      for (e = a + 1;e < s; e++) {
        m[e] = this.xt[e - O];
        _[e] = this.Gt[e - O];
        if (r) {
          r[e] = this.$t[e - O];
          setSignal(r[e], t[e]);
        }
        if (n) {
          n[e] = this.qt[e - O];
          if (O !== 0)
            setSignal(n[e], e);
        }
      }
      this.xt = m;
      this.Gt = _;
      r && (this.$t = r);
      n && (this.qt = n);
      this.jt = s;
      this.Mt = t.slice(0);
      if (l)
        for (i = 0;i < l.length; i++)
          l[i].dispose();
    }
  });
  return this.xt;
}
function compare(t, s, i) {
  return t ? t(s) === t(i) : true;
}
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
var IS_DEV = false;
var $DEVCOMP = Symbol(0);
function createContext2(defaultValue, options) {
  const id = Symbol(options && options.name || "");
  function provider(props) {
    return createRoot(() => {
      setContext(provider, props.value);
      return children(() => props.children);
    });
  }
  provider.id = id;
  provider.defaultValue = defaultValue;
  return provider;
}
function children(fn) {
  const c = createMemo(fn, {
    lazy: true
  });
  const memo = createMemo(() => flatten(c()), {
    lazy: true,
    sync: true
  });
  memo.toArray = () => {
    const v = memo();
    return Array.isArray(v) ? v : v != null ? [v] : [];
  };
  return memo;
}
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
function Switch(props) {
  const chs = children(() => props.children);
  const switchFunc = createMemo(() => {
    const mps = chs.toArray();
    let func = () => {
      return;
    };
    for (let i = 0;i < mps.length; i++) {
      const index = i;
      const mp = mps[i];
      if (mp == null)
        continue;
      const prevFunc = func;
      const conditionValue = createMemo(() => prevFunc() ? undefined : mp.when, undefined);
      const condition = mp.keyed ? conditionValue : createMemo(conditionValue, {
        equals: (a, b) => !a === !b,
        sync: true
      });
      func = () => {
        const prev = prevFunc();
        if (prev)
          return prev;
        const c = condition();
        return c ? [index, c, conditionValue, mp] : undefined;
      };
    }
    return func;
  }, {
    sync: true
  });
  return createMemo(() => {
    const sel = switchFunc()();
    if (!sel)
      return props.fallback;
    const [index, value, conditionValue, mp] = sel;
    const child = mp.children;
    const fn = typeof child === "function" && child.length > 0;
    return fn ? mp.keyed ? untrack(() => child(value), IS_DEV) : untrack(() => child(() => {
      if (untrack(switchFunc)()?.[0] !== index)
        throw narrowedError("Match");
      return conditionValue();
    }), IS_DEV) : child;
  }, {
    sync: true
  });
}
function Match(props) {
  return props;
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
import { on, once } from "srt:events";
import { exit } from "srt:app";

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
function getBoundingBox2(node) {
  return tree.getBoundingBox(node.id);
}
function getBoundingBoxViewport2(node) {
  return tree.getBoundingBoxViewport(node.id);
}
function measureText2(text, options) {
  return tree.measureText(text, options);
}

// packages/core/src/window.ts
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
  let [size, setSize] = createSignal({
    width: 0,
    height: 0
  }, {
    ownedWrite: true
  });
  let [safe, setSafe] = createSignal({
    top: 0,
    left: 0,
    right: 0,
    bottom: 0
  }, {
    ownedWrite: true
  });
  let [scale, setScale] = createSignal(1, {
    ownedWrite: true
  });
  on("resize", (e) => {
    setSize({
      width: e.width,
      height: e.height
    });
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
function safeArea() {
  ensureResizeState();
  return safeAreaAccessor();
}
function displayScale() {
  ensureResizeState();
  return displayScaleAccessor();
}
var focusedAccessor;
function windowFocused() {
  if (!focusedAccessor) {
    let [focused, setFocused] = createSignal(true);
    on("windowFocus", () => setFocused(true));
    on("windowBlur", () => setFocused(false));
    focusedAccessor = focused;
  }
  return focusedAccessor();
}
var keyboardHeightAccessor;
function keyboardHeight() {
  if (!keyboardHeightAccessor) {
    let [height, setHeight] = createSignal(0);
    on("keyboardVisibility", ({
      height: h
    }) => setHeight(h ?? 0));
    keyboardHeightAccessor = height;
  }
  return keyboardHeightAccessor();
}
function onLayout(fn) {
  let unsubscribe = on("postLayout", fn);
  onCleanup(unsubscribe);
  return unsubscribe;
}
var backHandlers = [];
function onBack(fn) {
  backHandlers.push(fn);
  let cleanup2 = () => {
    let i = backHandlers.lastIndexOf(fn);
    if (i >= 0)
      backHandlers.splice(i, 1);
  };
  onCleanup(cleanup2);
  return cleanup2;
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
    unsubRefreshRate = on("displayRefreshRate", ({
      hz
    }) => {
      if (hz > 0)
        refreshRate = hz;
    });
    unsubscribe = on("render", ({
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
    unsubDown = on("pointerDown", (raw) => {
      bubble(raw, "onPointerDown");
      let focused = getFocusedNodeId();
      if (focused != null && !raw.targets.includes(focused)) {
        setFocus(null);
      }
    });
    unsubUp = on("pointerUp", (raw) => {
      bubble(raw, "onPointerUp");
    });
    unsubMove = on("pointerMove", (raw) => {
      bubble(raw, "onPointerMove");
    });
    unsubEnter = on("pointerEnter", (raw) => {
      dispatchOrdered(raw, "onPointerEnter");
    });
    unsubLeave = on("pointerLeave", (raw) => {
      dispatchOrdered(raw, "onPointerLeave");
    });
    unsubWheel = on("wheel", (raw) => {
      bubble(raw, "onWheel");
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
    unsubBack = on("back", () => {
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
    unsubTextInput = on("textInput", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e);
      }
    });
    unsubKeyboardVisibility = on("keyboardVisibility", ({
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
function mixColors(a3, b2, t3) {
  return w(a3).mix(b2, t3).toHex();
}
function brightness(color) {
  return w(color).brightness();
}
function createLinearGradient(x0, y0, x1, y1, stops) {
  return {
    __gradient: "linear",
    x0,
    y0,
    x1,
    y1,
    stops: parseStops(stops)
  };
}
function isGradient(value) {
  return typeof value === "object" && value !== null && "__gradient" in value;
}
function parseStops(stops) {
  return stops.map((s2) => ({
    offset: s2.offset,
    color: parseColor(s2.color)
  }));
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
var warnedUnknownProps = new Set;
function setTreeProperty(node, name, value) {
  try {
    tree2.setProperty(node.id, name, value);
  } catch (e3) {
    if (!String(e3).includes("unknown property"))
      throw e3;
    let key = node.elementType + "." + name;
    if (warnedUnknownProps.has(key))
      return;
    warnedUnknownProps.add(key);
    let stack = new Error().stack ?? "";
    console.warn(`Ignoring unknown property '${name}' on <${node.elementType}>
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
function createPortal(node, mount) {
  let target = mount ?? windowRoot;
  if (!target) {
    throw new Error("createPortal: no mount target (portals cannot mount during the initial render; open them after mount)");
  }
  if (node === null || typeof node !== "object" || Array.isArray(node)) {
    throw new Error("createPortal: node must be a single built element");
  }
  insertNode2(target, node);
  onCleanup(() => removeNode(target, node));
  return null;
}
// packages/core/src/environment.ts
import { on as on2 } from "srt:events";
var devicesAccessor;
function ensureDevicesState() {
  if (devicesAccessor)
    return;
  let [devices, setDevices] = createSignal(undefined, {
    ownedWrite: true
  });
  on2("inputDevices", (d2) => {
    setDevices({
      keyboard: !!d2.keyboard,
      mouse: !!d2.mouse,
      touch: !!d2.touch
    });
  });
  devicesAccessor = devices;
}
var systemThemeAccessor;
function ensureSystemThemeState() {
  if (systemThemeAccessor)
    return;
  let [theme, setTheme] = createSignal("unknown", {
    ownedWrite: true
  });
  on2("systemTheme", (e3) => setTheme(e3.theme ?? "unknown"));
  systemThemeAccessor = theme;
}
var visibilityAccessor;
function ensureVisibilityState() {
  if (visibilityAccessor)
    return;
  let [visibility, setVisibility] = createSignal("visible", {
    ownedWrite: true
  });
  on2("visibility", (e3) => setVisibility(e3.state === "hidden" ? "hidden" : "visible"));
  visibilityAccessor = visibility;
}
var orientationAccessor;
function ensureOrientationState() {
  if (orientationAccessor)
    return;
  let [orientation, setOrientation] = createSignal("unknown", {
    ownedWrite: true
  });
  on2("displayOrientation", (e3) => {
    setOrientation(e3.orientation ?? "unknown");
  });
  orientationAccessor = orientation;
}
var textScaleAccessor;
function ensureTextScaleState() {
  if (textScaleAccessor)
    return;
  let [scale, setScale] = createSignal(1, {
    ownedWrite: true
  });
  on2("textScale", (e3) => {
    setScale(typeof e3.scale === "number" && e3.scale > 0 ? e3.scale : 1);
  });
  textScaleAccessor = scale;
}
var mouseSeenAccessor;
var touchSeenAccessor;
function ensurePointerState() {
  if (mouseSeenAccessor)
    return;
  let [mouse, setMouse] = createSignal(false);
  let [touch, setTouch] = createSignal(false);
  let sawMouse = false;
  let sawTouch = false;
  let unsubs = [];
  let note = (e3) => {
    if (e3.pointerType === "mouse" && !sawMouse) {
      sawMouse = true;
      setMouse(true);
    } else if (e3.pointerType === "touch" && !sawTouch) {
      sawTouch = true;
      setTouch(true);
    }
    if (sawMouse && sawTouch)
      for (let u3 of unsubs)
        u3();
  };
  unsubs.push(on2("pointerMove", note), on2("pointerDown", note));
  mouseSeenAccessor = mouse;
  touchSeenAccessor = touch;
}
var keyboardSeenAccessor;
function ensureKeyboardState() {
  if (keyboardSeenAccessor)
    return;
  let [keyboard, setKeyboard] = createSignal(false);
  let unsub = on2("keydown", () => {
    setKeyboard(true);
    unsub();
  });
  keyboardSeenAccessor = keyboard;
}
var env = {
  get windowSize() {
    return windowSize();
  },
  get safeArea() {
    return safeArea();
  },
  get displayScale() {
    return displayScale();
  },
  get windowFocused() {
    return windowFocused();
  },
  get keyboardHeight() {
    return keyboardHeight();
  },
  get inputDevices() {
    ensureDevicesState();
    return devicesAccessor();
  },
  get systemTheme() {
    ensureSystemThemeState();
    return systemThemeAccessor();
  },
  get textScale() {
    ensureTextScaleState();
    return textScaleAccessor();
  },
  get visibility() {
    ensureVisibilityState();
    return visibilityAccessor();
  },
  get orientation() {
    ensureOrientationState();
    return orientationAccessor();
  },
  get mouseSeen() {
    ensurePointerState();
    return mouseSeenAccessor();
  },
  get touchSeen() {
    ensurePointerState();
    return touchSeenAccessor();
  },
  get keyboardSeen() {
    ensureKeyboardState();
    return keyboardSeenAccessor();
  }
};
// packages/core/src/gamepad.ts
import { on as on3 } from "srt:events";
// packages/core/src/capabilities.ts
var MEDIUM_MIN_WIDTH = 600;
var EXPANDED_MIN_WIDTH = 840;
var capabilities = {
  get hover() {
    return env.inputDevices?.mouse ?? env.mouseSeen;
  },
  get precisePointer() {
    return env.inputDevices?.mouse ?? env.mouseSeen;
  },
  get touch() {
    return env.inputDevices?.touch ?? env.touchSeen;
  },
  get keyboardNav() {
    return env.inputDevices?.keyboard ?? env.keyboardSeen;
  },
  get windowSizeClass() {
    let w2 = env.windowSize.width;
    return w2 >= EXPANDED_MIN_WIDTH ? "expanded" : w2 >= MEDIUM_MIN_WIDTH ? "medium" : "compact";
  }
};
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
function parseSvg(src, opts) {
  if (opts?.color != null)
    return fluxParseSvg(src, {
      color: parseColor(opts.color)
    });
  return fluxParseSvg(src);
}
// packages/core/src/scroll.ts
function createScroll(viewport, content, options = {}) {
  let axis = options.axis ?? "vertical";
  let canX = axis === "horizontal" || axis === "both";
  let canY = axis === "vertical" || axis === "both";
  let [offset, setOffset] = createSignal({
    x: 0,
    y: 0
  });
  let origin = new Error().stack ?? "";
  let warnedCollapsed = false;
  let maxX = 0;
  let maxY = 0;
  let clamp = (x2, y2) => ({
    x: canX ? Math.max(0, Math.min(x2, maxX)) : 0,
    y: canY ? Math.max(0, Math.min(y2, maxY)) : 0
  });
  let set = (x2, y2) => {
    let cur = offset();
    let next = clamp(x2, y2);
    if (next.x !== cur.x || next.y !== cur.y)
      setOffset(next);
  };
  onLayout(() => {
    let vp = viewport();
    let ct = content();
    if (!vp || !ct)
      return;
    let vb = getBoundingBox2(vp);
    let cb = getBoundingBox2(ct);
    if (!vb || !cb)
      return;
    if (!warnedCollapsed) {
      let zeroY = canY && vb.height === 0 && cb.height > 0;
      let zeroX = canX && vb.width === 0 && cb.width > 0;
      if (zeroY || zeroX) {
        warnedCollapsed = true;
        let axisName = zeroY ? "height" : "width";
        console.warn(`Scroll container resolved to ${axisName} 0, so its content is invisible. ` + `Give it an explicit ${axisName} or flex; maxHeight/maxWidth alone does not size it.
${origin}`);
      }
    }
    maxX = Math.max(0, cb.width - vb.width);
    maxY = Math.max(0, cb.height - vb.height);
    let cur = offset();
    let next = clamp(cur.x, cur.y);
    if (next.x !== cur.x || next.y !== cur.y) {
      setOffset(next);
      flush();
    }
  });
  return {
    offset,
    scrollBy: (dx, dy) => {
      let cur = offset();
      set(cur.x + dx, cur.y + dy);
    },
    scrollTo: (x2, y2) => set(x2, y2)
  };
}
// packages/components/src/window.tsx
function Window(props) {
  var _el$ = createElement("window");
  spread(_el$, mergeProps(() => props.layout, {
    get title() {
      return props.title;
    },
    get fullscreen() {
      return props.fullscreen;
    },
    get onPointerEnter() {
      return props.onPointerEnter;
    },
    get onPointerLeave() {
      return props.onPointerLeave;
    },
    get onPointerDown() {
      return props.onPointerDown;
    },
    get onPointerUp() {
      return props.onPointerUp;
    },
    get onPointerMove() {
      return props.onPointerMove;
    },
    get onWheel() {
      return props.onWheel;
    },
    get onFocus() {
      return props.onFocus;
    },
    get onBlur() {
      return props.onBlur;
    },
    get onKeyDown() {
      return props.onKeyDown;
    },
    get onKeyUp() {
      return props.onKeyUp;
    },
    get onTextInput() {
      return props.onTextInput;
    },
    get pointerEvents() {
      return props.pointerEvents;
    }
  }), true);
  insert(_el$, (() => {
    var _c$ = memo2(() => props.style?.backgroundColor != null);
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => props.style.backgroundColor, (_v$, _$p) => {
        setProp(_el$2, "color", _v$, _$p);
      });
      return _el$2;
    })() : null;
  })(), null);
  insert(_el$, () => props.children, null);
  return _el$;
}
// packages/components/src/view.tsx
function View(props) {
  let hasBackground = () => props.style?.backgroundColor != null || props.style?.borderRadius != null;
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0;
  var _el$ = createElement("view");
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, mergeProps(() => props.layout, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get scaleX() {
      return props.style?.scaleX;
    },
    get scaleY() {
      return props.style?.scaleY;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get rotateX() {
      return props.style?.rotateX;
    },
    get rotateY() {
      return props.style?.rotateY;
    },
    get perspective() {
      return props.style?.perspective;
    },
    get originX() {
      return props.style?.originX;
    },
    get originY() {
      return props.style?.originY;
    },
    get clipRadius() {
      return props.style?.clipRadius;
    },
    get opacity() {
      return props.style?.opacity;
    },
    get onPointerEnter() {
      return props.onPointerEnter;
    },
    get onPointerLeave() {
      return props.onPointerLeave;
    },
    get onPointerDown() {
      return props.onPointerDown;
    },
    get onPointerUp() {
      return props.onPointerUp;
    },
    get onPointerMove() {
      return props.onPointerMove;
    },
    get onWheel() {
      return props.onWheel;
    },
    get onFocus() {
      return props.onFocus;
    },
    get onBlur() {
      return props.onBlur;
    },
    get onKeyDown() {
      return props.onKeyDown;
    },
    get onKeyUp() {
      return props.onKeyUp;
    },
    get onTextInput() {
      return props.onTextInput;
    },
    get pointerEvents() {
      return props.pointerEvents;
    }
  }), true);
  insert(_el$, (() => {
    var _c$ = memo2(() => !!hasBackground());
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: props.style?.backgroundColor ?? "transparent",
        t: props.style?.borderRadius
      }), ({
        e: e3,
        t: t3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
      });
      return _el$2;
    })() : null;
  })(), null);
  insert(_el$, () => props.children, null);
  insert(_el$, (() => {
    var _c$2 = memo2(() => !!hasBorder());
    return () => _c$2() ? (() => {
      var _el$3 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: props.style?.borderColor ?? "transparent",
        t: props.style?.borderWidth,
        a: props.style?.borderRadius
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$3, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$3, "strokeWidth", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$3, "radius", a3, _p$?.a);
      });
      return _el$3;
    })() : null;
  })(), null);
  return _el$;
}
// packages/components/src/theme.ts
var TEXT = {
  fontFamily: "sans",
  caption: {
    size: 11,
    lineHeight: 1.3,
    weight: 400
  },
  label: {
    size: 12,
    lineHeight: 1.3,
    weight: 600
  },
  body: {
    size: 14,
    lineHeight: 1.5,
    weight: 400
  },
  title: {
    size: 18,
    lineHeight: 1.4,
    weight: 700
  },
  heading: {
    size: 22,
    lineHeight: 1.3,
    weight: 700
  }
};
var SPACING = {
  sm: 4,
  md: 8,
  lg: 16,
  xl: 20
};
var RADIUS = {
  sm: 4,
  md: 8,
  lg: 12
};
var BORDER_WIDTH = {
  sm: 1
};
var darkTheme = {
  text: TEXT,
  color: {
    background: "#0b0f17",
    surface: "#161b22",
    surfaceAlt: "#21262d",
    surfaceHover: "#262c34",
    text: "#e6edf3",
    textMuted: mixColors("#e6edf3", "#0b0f17", 0.4),
    border: "rgba(255,255,255,0.14)",
    primary: "#547ebf",
    primaryHover: "#7ea9ea",
    onPrimary: "#ffffff",
    secondary: "#2b5696",
    secondaryHover: "#3a68ab",
    onSecondary: "#ffffff",
    danger: "#f85149",
    dangerHover: "#ff7b72",
    scrim: "rgba(0,0,0,0.6)"
  },
  spacing: SPACING,
  radius: RADIUS,
  borderWidth: BORDER_WIDTH
};
var lightTheme = {
  text: TEXT,
  color: {
    background: "#ffffff",
    surface: "#f6f8fa",
    surfaceAlt: "#eaeef2",
    surfaceHover: "#e0e5eb",
    text: "#1f2328",
    textMuted: mixColors("#1f2328", "#ffffff", 0.4),
    border: "rgba(0,0,0,0.15)",
    primary: "#547ebf",
    primaryHover: "#3f5494",
    onPrimary: "#ffffff",
    secondary: "#2b5696",
    secondaryHover: "#1f4176",
    onSecondary: "#ffffff",
    danger: "#cf222e",
    dangerHover: "#a40e26",
    scrim: "rgba(0,0,0,0.4)"
  },
  spacing: SPACING,
  radius: RADIUS,
  borderWidth: BORDER_WIDTH
};
var [theme, setThemeStore] = createStore({
  ...darkTheme
});
function setTheme(partial) {
  setThemeStore((s2) => {
    for (let key in partial) {
      let k2 = key;
      Object.assign(s2[k2], partial[k2]);
    }
  });
}

// packages/components/src/policy.ts
function defaultPolicyResolver(caps) {
  let interaction = caps.touch && caps.precisePointer ? "hybrid" : caps.touch ? "touch" : caps.precisePointer ? "desktop" : "hybrid";
  return {
    interaction,
    density: interaction === "desktop" ? "compact" : "comfortable",
    motion: "normal",
    focusRing: caps.keyboardNav,
    textScale: env.textScale,
    textWeightDelta: env.displayScale < 1.5 ? 100 : 0,
    navigation: caps.windowSizeClass === "expanded" ? "sidebar" : caps.windowSizeClass === "medium" ? "rail" : "bottomTabs",
    layout: caps.windowSizeClass === "expanded" ? "twoPane" : "singlePane"
  };
}
var [resolverBox, setResolverBox] = createSignal({
  resolve: defaultPolicyResolver
});
var [overrides, setOverrides] = createSignal({});
var resolved = () => resolverBox().resolve(capabilities);
var policy = {
  get interaction() {
    return overrides().interaction ?? resolved().interaction;
  },
  get density() {
    return overrides().density ?? resolved().density;
  },
  get motion() {
    return overrides().motion ?? resolved().motion;
  },
  get focusRing() {
    return overrides().focusRing ?? resolved().focusRing;
  },
  get textScale() {
    return overrides().textScale ?? resolved().textScale;
  },
  get textWeightDelta() {
    return overrides().textWeightDelta ?? resolved().textWeightDelta;
  },
  get navigation() {
    return overrides().navigation ?? resolved().navigation;
  },
  get layout() {
    return overrides().layout ?? resolved().layout;
  }
};
var DENSITY_SCALE = {
  comfortable: 1,
  compact: 0.85,
  dense: 0.7
};
function densityScale() {
  return DENSITY_SCALE[policy.density];
}

// packages/components/src/typography.ts
var SMALL_TEXT = 16;
function lightOnDark(text, fill) {
  if (typeof text !== "string" || typeof fill !== "string" || fill === "transparent")
    return;
  return brightness(text) > brightness(fill);
}
function themeOnDark() {
  return lightOnDark(theme.color.text, theme.color.background) ?? false;
}
function typeWeight(weight, size, onDark) {
  let delta = onDark ?? themeOnDark() ? policy.textWeightDelta : 0;
  if (delta > 0 && size < SMALL_TEXT)
    delta += 100;
  return Math.min(900, weight + delta);
}
function typeStyle(variant, onDark) {
  let role = theme.text[variant];
  let size = role.size * policy.textScale;
  return {
    fontFamily: theme.text.fontFamily,
    fontSize: size,
    lineHeight: role.lineHeight,
    fontWeight: typeWeight(role.weight, size, onDark)
  };
}

// packages/components/src/text.tsx
var FONT_KEYS = ["fontFamily", "fontSize", "lineHeight", "fontStyle", "fontWeight", "textAlign", "maxLines"];
function Text(props) {
  let role = () => theme.text[props.variant ?? "body"];
  let size = () => (props.layout?.fontSize ?? role().size) * policy.textScale;
  let color = () => props.style?.color ?? theme.color[props.color ?? (props.muted ? "textMuted" : "text")];
  let box = createMemo(() => {
    let l2 = props.layout;
    if (!l2)
      return {};
    let out = {};
    for (let key in l2) {
      if (!FONT_KEYS.includes(key))
        out[key] = l2[key];
    }
    return out;
  });
  var _el$ = createElement("view"), _el$2 = createElement("text");
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, mergeProps(box, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get opacity() {
      return props.style?.opacity;
    },
    get onPointerEnter() {
      return props.onPointerEnter;
    },
    get onPointerLeave() {
      return props.onPointerLeave;
    },
    get onPointerDown() {
      return props.onPointerDown;
    },
    get onPointerUp() {
      return props.onPointerUp;
    },
    get onPointerMove() {
      return props.onPointerMove;
    },
    get onWheel() {
      return props.onWheel;
    },
    get onFocus() {
      return props.onFocus;
    },
    get onBlur() {
      return props.onBlur;
    },
    get onKeyDown() {
      return props.onKeyDown;
    },
    get onKeyUp() {
      return props.onKeyUp;
    },
    get onTextInput() {
      return props.onTextInput;
    },
    get pointerEvents() {
      return props.pointerEvents;
    }
  }), true);
  insert(_el$2, () => props.children);
  effect3(() => ({
    e: color(),
    t: props.layout?.fontFamily ?? theme.text.fontFamily,
    a: size(),
    o: props.layout?.lineHeight ?? role().lineHeight,
    i: props.layout?.fontStyle,
    n: typeWeight(props.layout?.fontWeight ?? role().weight, size()),
    s: props.layout?.textAlign,
    h: props.layout?.maxLines
  }), ({
    e: e3,
    t: t3,
    a: a3,
    o: o3,
    i: i3,
    n: n3,
    s: s2,
    h: h3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "fontFamily", t3, _p$?.t);
    a3 !== _p$?.a && setProp(_el$2, "fontSize", a3, _p$?.a);
    o3 !== _p$?.o && setProp(_el$2, "lineHeight", o3, _p$?.o);
    i3 !== _p$?.i && setProp(_el$2, "fontStyle", i3, _p$?.i);
    n3 !== _p$?.n && setProp(_el$2, "fontWeight", n3, _p$?.n);
    s2 !== _p$?.s && setProp(_el$2, "textAlign", s2, _p$?.s);
    h3 !== _p$?.h && setProp(_el$2, "maxLines", h3, _p$?.h);
  });
  return _el$;
}
// packages/components/src/safe-area.tsx
function SafeArea(props) {
  let pad = (edge) => {
    let defaultOn = edge === "top" || edge === "bottom";
    let p3 = props[edge] ?? defaultOn;
    if (p3 === false)
      return 0;
    if (p3 === true)
      return safeArea()[edge];
    return Math.max(safeArea()[edge], p3);
  };
  var _el$ = createElement("view", {
    flex: 1,
    flexDirection: "column"
  });
  insert(_el$, () => props.children);
  effect3(() => ({
    e: props.relative !== false ? "relative" : undefined,
    t: pad("top"),
    a: pad("bottom"),
    o: pad("left"),
    i: pad("right")
  }), ({
    e: e3,
    t: t3,
    a: a3,
    o: o3,
    i: i3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$, "position", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$, "marginTop", t3, _p$?.t);
    a3 !== _p$?.a && setProp(_el$, "marginBottom", a3, _p$?.a);
    o3 !== _p$?.o && setProp(_el$, "marginLeft", o3, _p$?.o);
    i3 !== _p$?.i && setProp(_el$, "marginRight", i3, _p$?.i);
  });
  return _el$;
}
// packages/core/src/text-input.ts
function createTextBuffer(options = {}) {
  let initial = options.defaultValue ?? "";
  let [internalValue, setInternalValue] = createSignal(initial);
  let [selectionState, setSelectionState] = createSignal({
    anchor: initial.length,
    focus: initial.length
  });
  let value = () => options.value?.() ?? internalValue();
  let selection = () => {
    let len = value().length;
    let s2 = selectionState();
    return {
      anchor: Math.min(s2.anchor, len),
      focus: Math.min(s2.focus, len)
    };
  };
  let range = () => {
    let {
      anchor,
      focus
    } = selection();
    return anchor <= focus ? [anchor, focus] : [focus, anchor];
  };
  let setCaret = (offset) => setSelectionState({
    anchor: offset,
    focus: offset
  });
  let apply = (next, caret) => {
    let max = options.maxLength?.();
    if (max != null && next.length > max)
      next = next.slice(0, max);
    caret = Math.min(caret, next.length);
    if (options.value?.() == null)
      setInternalValue(next);
    setCaret(caret);
    options.onInput?.(next);
  };
  return {
    value,
    selection,
    caret: () => selection().focus,
    insertText: (text) => {
      let v2 = value();
      let [start, end] = range();
      apply(v2.slice(0, start) + text + v2.slice(end), start + text.length);
    },
    deleteBackward: () => {
      let v2 = value();
      let [start, end] = range();
      if (start !== end)
        apply(v2.slice(0, start) + v2.slice(end), start);
      else if (start > 0)
        apply(v2.slice(0, start - 1) + v2.slice(start), start - 1);
    },
    deleteForward: () => {
      let v2 = value();
      let [start, end] = range();
      if (start !== end)
        apply(v2.slice(0, start) + v2.slice(end), start);
      else if (end < v2.length)
        apply(v2.slice(0, end) + v2.slice(end + 1), end);
    },
    move: (direction, opts) => {
      let extend = opts?.extend ?? false;
      let {
        anchor,
        focus
      } = selection();
      let len = value().length;
      if (!extend && anchor !== focus && (direction === "left" || direction === "right")) {
        setCaret(direction === "left" ? Math.min(anchor, focus) : Math.max(anchor, focus));
        return;
      }
      let next = focus;
      if (direction === "left")
        next = Math.max(0, focus - 1);
      else if (direction === "right")
        next = Math.min(len, focus + 1);
      else if (direction === "start")
        next = 0;
      else if (direction === "end")
        next = len;
      setSelectionState({
        anchor: extend ? anchor : next,
        focus: next
      });
    },
    setSelection: (anchor, focus) => {
      let len = value().length;
      setSelectionState({
        anchor: Math.min(anchor, len),
        focus: Math.min(focus, len)
      });
    },
    setValue: (next) => apply(next, next.length),
    clear: () => apply("", 0)
  };
}
function createCaretScroll(viewport, input) {
  let [scrollX, setScrollX] = createSignal(0);
  onLayout(() => {
    let node = viewport();
    if (!node)
      return;
    let vw = getBoundingBox2(node)?.width ?? 0;
    let {
      text,
      fontSize,
      caret,
      caretWidth = 0
    } = input();
    let len = text.length;
    let c3 = caret == null ? len : Math.max(0, Math.min(caret, len));
    let totalWidth = measureText2(text, {
      fontSize
    }).width;
    let caretX = c3 >= len ? totalWidth : measureText2(text.slice(0, c3), {
      fontSize
    }).width;
    let maxScroll = Math.max(0, totalWidth + caretWidth - vw);
    let cur = scrollX();
    let next = cur;
    if (vw <= 0) {
      next = 0;
    } else if (caretX < cur) {
      next = caretX;
    } else if (caretX + caretWidth > cur + vw) {
      next = caretX + caretWidth - vw;
    }
    next = Math.max(0, Math.min(next, maxScroll));
    if (next !== cur)
      setScrollX(next);
    flush();
  });
  return scrollX;
}

// packages/components/src/spacing.ts
function space(token) {
  return Math.round(theme.spacing[token] * densityScale());
}

// packages/components/src/text-input.tsx
var CARET_WIDTH = 1;
var TEXT_SHAPE_WIDTH = 1e9;
function TextInput(props) {
  let [focused, setFocused] = createSignal(false);
  let [caretOn, setCaretOn] = createSignal(true);
  let node;
  let viewport;
  let blinkId = null;
  let buffer = createTextBuffer({
    value: () => props.value,
    defaultValue: props.defaultValue,
    onInput: (v2) => props.onInput?.(v2),
    maxLength: () => props.maxLength
  });
  let value = buffer.value;
  createEffect(() => props.autoFocus, (autoFocus) => {
    if (autoFocus && node)
      setFocus(node.id);
  });
  let handlePointerDown = () => {
    if (props.disabled)
      return;
    if (node)
      setFocus(node.id);
  };
  let handleFocus = () => {
    setFocused(true);
    setCaretOn(true);
    if (blinkId == null) {
      blinkId = setInterval(() => setCaretOn((v2) => !v2), 500);
    }
    props.onFocus?.();
  };
  let handleBlur = () => {
    setFocused(false);
    if (blinkId != null) {
      clearInterval(blinkId);
      blinkId = null;
    }
    props.onBlur?.();
  };
  let handleKeyDown = (e3) => {
    if (props.disabled)
      return;
    if (e3.key === "Backspace") {
      buffer.deleteBackward();
      setCaretOn(true);
    } else if (e3.key === "Delete") {
      buffer.deleteForward();
      setCaretOn(true);
    } else if (e3.key === "ArrowLeft") {
      buffer.move("left");
      setCaretOn(true);
    } else if (e3.key === "ArrowRight") {
      buffer.move("right");
      setCaretOn(true);
    } else if (e3.key === "Home") {
      buffer.move("start");
      setCaretOn(true);
    } else if (e3.key === "End") {
      buffer.move("end");
      setCaretOn(true);
    } else if (e3.key === "Enter") {
      props.onSubmit?.(value());
      setFocus(null);
    } else if (e3.key === "Escape") {
      if (node)
        setFocus(null);
    }
  };
  let handleTextInput = (e3) => {
    if (props.disabled)
      return;
    buffer.insertText(e3.text ?? "");
    setCaretOn(true);
  };
  onCleanup(() => {
    if (blinkId != null)
      clearInterval(blinkId);
  });
  let textColor = () => props.style?.color ?? theme.color.text;
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface;
  let borderColor = () => props.style?.borderColor ?? (focused() && policy.focusRing ? theme.color.primary : theme.color.border);
  let borderWidth = () => props.style?.borderWidth ?? theme.borderWidth.sm;
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.sm;
  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0;
  let showCaret = () => focused() && caretOn() && !showPlaceholder();
  let fontSize = () => theme.text.body.size * policy.textScale;
  let rowHeight = () => Math.round(fontSize() * theme.text.body.lineHeight);
  let caretX = () => measureText2(value().slice(0, buffer.caret()), {
    fontSize: fontSize()
  }).width;
  let scrollX = createCaretScroll(() => viewport, () => ({
    text: value(),
    fontSize: fontSize(),
    caret: buffer.caret(),
    caretWidth: CARET_WIDTH
  }));
  let textStyle = (color) => ({
    w: TEXT_SHAPE_WIDTH,
    fontSize: fontSize(),
    lineHeight: theme.text.body.lineHeight,
    color,
    maxLines: 1
  });
  var _el$ = createElement("view"), _el$2 = createElement("d-rect"), _el$3 = createElement("d-rect", {
    drawStyle: "stroke"
  }), _el$4 = createElement("view", {
    flex: 1,
    overflow: "hidden"
  });
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  insertNode2(_el$, _el$4);
  ref(() => (n3) => node = n3, _el$);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "alignItems", "center");
  spread(_el$, mergeProps({
    get paddingLeft() {
      return space("md");
    },
    get paddingRight() {
      return space("md");
    },
    get paddingTop() {
      return space("sm");
    },
    get paddingBottom() {
      return space("sm");
    }
  }, () => props.layout, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get opacity() {
      return props.style?.opacity;
    },
    onPointerDown: handlePointerDown,
    onFocus: handleFocus,
    onBlur: handleBlur,
    onKeyDown: handleKeyDown,
    onTextInput: handleTextInput
  }), true);
  ref(() => (n3) => viewport = n3, _el$4);
  insert(_el$4, (() => {
    var _c$ = memo2(() => !!showPlaceholder());
    return () => _c$() ? (() => {
      var _el$5 = createElement("d-text");
      spread(_el$5, mergeProps(() => textStyle(theme.color.textMuted)), true);
      insert(_el$5, () => props.placeholder ?? "");
      return _el$5;
    })() : [(() => {
      var _el$6 = createElement("d-text");
      spread(_el$6, mergeProps(() => textStyle(textColor())), true);
      insert(_el$6, value);
      return _el$6;
    })(), memo2(() => memo2(() => !!showCaret())() ? (() => {
      var _el$7 = createElement("d-rect", {
        w: 1
      });
      effect3(() => ({
        e: textColor(),
        t: caretX(),
        a: (rowHeight() - fontSize()) / 2,
        o: fontSize()
      }), ({
        e: e3,
        t: t3,
        a: a3,
        o: o3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$7, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$7, "x", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$7, "y", a3, _p$?.a);
        o3 !== _p$?.o && setProp(_el$7, "h", o3, _p$?.o);
      });
      return _el$7;
    })() : null)];
  })());
  effect3(() => ({
    e: surfaceColor(),
    t: borderRadius(),
    a: borderColor(),
    o: borderWidth(),
    i: borderRadius(),
    n: rowHeight(),
    s: scrollX()
  }), ({
    e: e3,
    t: t3,
    a: a3,
    o: o3,
    i: i3,
    n: n3,
    s: s2
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
    a3 !== _p$?.a && setProp(_el$3, "color", a3, _p$?.a);
    o3 !== _p$?.o && setProp(_el$3, "strokeWidth", o3, _p$?.o);
    i3 !== _p$?.i && setProp(_el$3, "radius", i3, _p$?.i);
    n3 !== _p$?.n && setProp(_el$4, "height", n3, _p$?.n);
    s2 !== _p$?.s && setProp(_el$4, "scrollX", s2, _p$?.s);
  });
  return _el$;
}
// packages/components/src/arena.ts
var claims = new Map;
function claim(pointerId, owner) {
  if (claims.has(pointerId))
    return false;
  claims.set(pointerId, {
    owner,
    resolved: false
  });
  return true;
}
function steal(pointerId, owner) {
  let current = claims.get(pointerId);
  if (current) {
    if (current.resolved)
      return false;
    current.owner.cancel();
  }
  claims.set(pointerId, {
    owner,
    resolved: true
  });
  return true;
}
function release(pointerId, owner) {
  if (claims.get(pointerId)?.owner === owner)
    claims.delete(pointerId);
}

// packages/components/src/pan.ts
var PAN_SLOP = 8;
function createPan(options) {
  let origin = null;
  let active = null;
  let armed = null;
  let past = (e3) => {
    if (!origin)
      return false;
    let dx = Math.abs(e3.clientX - origin.x);
    let dy = Math.abs(e3.clientY - origin.y);
    let axis = options.axis ?? "both";
    if (axis === "vertical")
      return dy >= PAN_SLOP;
    if (axis === "horizontal")
      return dx >= PAN_SLOP;
    return dx * dx + dy * dy >= PAN_SLOP * PAN_SLOP;
  };
  let reset = () => {
    if (active != null) {
      release(active, owner);
      active = null;
    }
    armed = null;
    origin = null;
  };
  let cancel = reset;
  let owner = {
    cancel
  };
  onSettled(() => reset);
  let handlers2 = {
    onPointerDown: (e3) => {
      if (e3.button != null && e3.button !== 0)
        return;
      if (armed == null && active == null) {
        armed = e3.pointerId;
        origin = {
          x: e3.clientX,
          y: e3.clientY
        };
      }
    },
    onPointerMove: (e3) => {
      if (armed === e3.pointerId && past(e3)) {
        if (steal(e3.pointerId, owner)) {
          active = e3.pointerId;
          armed = null;
          origin = {
            x: e3.clientX,
            y: e3.clientY
          };
          options.onPanStart?.();
        } else {
          reset();
        }
        return;
      }
      if (active === e3.pointerId && origin) {
        options.onPanMove?.(e3.clientX - origin.x, e3.clientY - origin.y);
        origin = {
          x: e3.clientX,
          y: e3.clientY
        };
      }
    },
    onPointerUp: (e3) => {
      if (active === e3.pointerId) {
        reset();
        options.onPanEnd?.();
      } else if (armed === e3.pointerId) {
        reset();
      }
    }
  };
  return {
    handlers: handlers2,
    cancel
  };
}

// packages/components/src/scroll-view.tsx
function ScrollView(props) {
  let viewport;
  let content;
  let scroll = createScroll(() => viewport, () => content, {
    axis: props.horizontal ? "horizontal" : "vertical"
  });
  let pan = createPan({
    axis: props.horizontal ? "horizontal" : "vertical",
    onPanMove: (dx, dy) => scroll.scrollBy(-dx, -dy)
  });
  let onWheel = (e3) => {
    if (props.horizontal)
      scroll.scrollBy(e3.deltaX || e3.deltaY, 0);
    else
      scroll.scrollBy(e3.deltaX, e3.deltaY);
  };
  let direction = () => props.horizontal ? "row" : "column";
  let hasBackground = () => props.style?.backgroundColor != null || props.style?.borderRadius != null;
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0;
  var _el$ = createElement("view"), _el$2 = createElement("view"), _el$3 = createElement("view", {
    flexShrink: 0
  });
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, mergeProps(() => props.layout, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get opacity() {
      return props.style?.opacity;
    },
    get onPointerEnter() {
      return props.onPointerEnter;
    },
    get onPointerLeave() {
      return props.onPointerLeave;
    },
    get onPointerDown() {
      return props.onPointerDown;
    },
    get onPointerUp() {
      return props.onPointerUp;
    },
    get onPointerMove() {
      return props.onPointerMove;
    },
    get onWheel() {
      return props.onWheel;
    },
    get pointerEvents() {
      return props.pointerEvents;
    }
  }), true);
  insert(_el$, (() => {
    var _c$ = memo2(() => !!hasBackground());
    return () => _c$() ? (() => {
      var _el$4 = createElement("d-rect");
      effect3(() => ({
        e: props.style?.backgroundColor ?? "transparent",
        t: props.style?.borderRadius
      }), ({
        e: e3,
        t: t3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$4, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$4, "radius", t3, _p$?.t);
      });
      return _el$4;
    })() : null;
  })(), _el$2);
  insertNode2(_el$2, _el$3);
  ref(() => (n3) => viewport = n3, _el$2);
  setProp(_el$2, "flex", 1);
  setProp(_el$2, "overflow", "hidden");
  spread(_el$2, mergeProps({
    get clipRadius() {
      return props.style?.borderRadius;
    },
    get flexDirection() {
      return direction();
    },
    get scrollX() {
      return scroll.offset().x;
    },
    get scrollY() {
      return scroll.offset().y;
    }
  }, () => pan.handlers, {
    onWheel
  }), true);
  ref(() => (n3) => content = n3, _el$3);
  insert(_el$3, () => props.children);
  insert(_el$, (() => {
    var _c$2 = memo2(() => !!hasBorder());
    return () => _c$2() ? (() => {
      var _el$5 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: props.style?.borderColor ?? "transparent",
        t: props.style?.borderWidth,
        a: props.style?.borderRadius
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$5, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$5, "strokeWidth", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$5, "radius", a3, _p$?.a);
      });
      return _el$5;
    })() : null;
  })(), null);
  effect3(() => direction(), (_v$, _$p) => {
    setProp(_el$3, "flexDirection", _v$, _$p);
  });
  return _el$;
}
// packages/components/src/press.ts
function createPress(options) {
  let [pressed, setPressed] = createSignal(false);
  let [hovered, setHovered] = createSignal(false);
  let node = null;
  let active = null;
  let inside = false;
  let live = {
    get pressed() {
      return pressed();
    },
    get hovered() {
      return hovered();
    }
  };
  let state = () => live;
  let ref2 = (n3) => {
    node = n3;
  };
  let within = (e3) => {
    let b2 = node && getBoundingBoxViewport2(node);
    if (!b2)
      return true;
    return e3.clientX >= b2.x && e3.clientX < b2.x + b2.width && e3.clientY >= b2.y && e3.clientY < b2.y + b2.height;
  };
  let disengage = () => {
    if (active != null) {
      release(active, owner);
      active = null;
    }
  };
  let cancel = () => {
    disengage();
    setPressed(false);
  };
  let owner = {
    cancel
  };
  onSettled(() => disengage);
  let handlers2 = {
    onPointerDown: (e3) => {
      if (e3.button != null && e3.button !== 0)
        return;
      if (active == null && claim(e3.pointerId, owner)) {
        active = e3.pointerId;
        inside = true;
        setPressed(true);
      }
      options.onPointerDown?.(e3);
    },
    onPointerMove: (e3) => {
      if (active === e3.pointerId) {
        inside = within(e3);
        setPressed(inside);
      }
      options.onPointerMove?.(e3);
    },
    onPointerUp: (e3) => {
      if (active === e3.pointerId) {
        let fire = inside;
        cancel();
        if (fire)
          options.onPress?.();
      }
      options.onPointerUp?.(e3);
    },
    onPointerEnter: (e3) => {
      setHovered(true);
      options.onPointerEnter?.(e3);
    },
    onPointerLeave: (e3) => {
      setHovered(false);
      options.onPointerLeave?.(e3);
    }
  };
  return {
    pressed,
    hovered,
    state,
    ref: ref2,
    handlers: handlers2,
    cancel
  };
}

// packages/components/src/pressable.tsx
function Pressable(props) {
  let press = createPress(props);
  let style = () => typeof props.style === "function" ? props.style(press.state()) : props.style;
  let resolved2 = children(() => props.children);
  let kids = () => {
    let c3 = resolved2();
    return typeof c3 === "function" ? c3(press.state()) : c3;
  };
  let hasBackground = () => style()?.backgroundColor != null || style()?.borderRadius != null;
  let hasBorder = () => (style()?.borderWidth ?? 0) > 0;
  var _el$ = createElement("view");
  ref(() => (n3) => {
    press.ref(n3);
    props.ref?.(n3);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  spread(_el$, mergeProps(() => props.layout, {
    get x() {
      return style()?.x;
    },
    get y() {
      return style()?.y;
    },
    get scale() {
      return style()?.scale;
    },
    get rotate() {
      return style()?.rotate;
    },
    get opacity() {
      return style()?.opacity;
    },
    get onPointerEnter() {
      return press.handlers.onPointerEnter;
    },
    get onPointerLeave() {
      return press.handlers.onPointerLeave;
    },
    get onPointerDown() {
      return press.handlers.onPointerDown;
    },
    get onPointerUp() {
      return press.handlers.onPointerUp;
    },
    get onPointerMove() {
      return press.handlers.onPointerMove;
    },
    get onWheel() {
      return props.onWheel;
    },
    get onFocus() {
      return props.onFocus;
    },
    get onBlur() {
      return props.onBlur;
    },
    get onKeyDown() {
      return props.onKeyDown;
    },
    get onKeyUp() {
      return props.onKeyUp;
    },
    get onTextInput() {
      return props.onTextInput;
    },
    get pointerEvents() {
      return memo2(() => !!props.disabled)() ? "none" : props.pointerEvents;
    }
  }), true);
  insert(_el$, (() => {
    var _c$ = memo2(() => !!hasBackground());
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: style()?.backgroundColor ?? "transparent",
        t: style()?.borderRadius
      }), ({
        e: e3,
        t: t3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
      });
      return _el$2;
    })() : null;
  })(), null);
  insert(_el$, kids, null);
  insert(_el$, (() => {
    var _c$2 = memo2(() => !!hasBorder());
    return () => _c$2() ? (() => {
      var _el$3 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: style()?.borderColor ?? "transparent",
        t: style()?.borderWidth,
        a: style()?.borderRadius
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$3, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$3, "strokeWidth", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$3, "radius", a3, _p$?.a);
      });
      return _el$3;
    })() : null;
  })(), null);
  return _el$;
}
// packages/components/src/button.tsx
var SIZE_WIDTH = {
  sm: 88,
  md: 120,
  lg: 160
};
function Button(props) {
  let colors = () => {
    let c3 = theme.color;
    switch (props.variant ?? "primary") {
      case "secondary":
        return {
          fill: c3.secondary,
          hover: c3.secondaryHover,
          label: c3.onSecondary
        };
      case "ghost":
        return {
          fill: "transparent",
          hover: c3.surfaceHover,
          label: c3.text
        };
      case "danger":
        return {
          fill: c3.danger,
          hover: c3.dangerHover,
          label: c3.onPrimary
        };
      default:
        return {
          fill: c3.primary,
          hover: c3.primaryHover,
          label: c3.onPrimary
        };
    }
  };
  let idleFill = () => props.disabled ? props.variant === "ghost" ? "transparent" : theme.color.surface : colors().fill;
  let bg = (s2) => props.style?.backgroundColor ?? (props.disabled ? idleFill() : s2.hovered && policy.interaction !== "touch" ? colors().hover : colors().fill);
  let radius = () => props.style?.borderRadius ?? theme.radius.md;
  let label = () => props.disabled ? theme.color.textMuted : colors().label;
  let resolved2 = children(() => props.children);
  let isText = () => typeof resolved2() === "string" || typeof resolved2() === "number";
  let labelOnDark = () => lightOnDark(label(), props.style?.backgroundColor ?? idleFill());
  let press = createPress(props);
  let style = () => ({
    ...props.style,
    backgroundColor: bg(press.state()),
    borderRadius: radius(),
    scale: (props.style?.scale ?? 1) * (press.pressed() && policy.motion !== "none" ? 0.97 : 1)
  });
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  ref(() => (n3) => {
    press.ref(n3);
    props.ref?.(n3);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "alignItems", "center");
  setProp(_el$, "justifyContent", "center");
  spread(_el$, mergeProps({
    get paddingTop() {
      return space("md");
    },
    get paddingBottom() {
      return space("md");
    },
    get paddingLeft() {
      return space("lg");
    },
    get paddingRight() {
      return space("lg");
    }
  }, () => props.size ? {
    minWidth: SIZE_WIDTH[props.size]
  } : {
    width: "100%"
  }, () => props.layout, {
    get x() {
      return style().x;
    },
    get y() {
      return style().y;
    },
    get scale() {
      return style().scale;
    },
    get rotate() {
      return style().rotate;
    },
    get opacity() {
      return style().opacity;
    }
  }, () => press.handlers, {
    get pointerEvents() {
      return props.disabled ? "none" : undefined;
    }
  }), true);
  insert(_el$, createComponent2(Show, {
    get when() {
      return isText();
    },
    get fallback() {
      return resolved2();
    },
    get children() {
      var _el$3 = createElement("text");
      spread(_el$3, mergeProps({
        get color() {
          return label();
        }
      }, () => typeStyle("body", labelOnDark())), true);
      insert(_el$3, resolved2);
      return _el$3;
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return (style().borderWidth ?? 0) > 0;
    },
    get children() {
      var _el$4 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: style().borderColor ?? "transparent",
        t: style().borderWidth,
        a: style().borderRadius
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$4, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$4, "strokeWidth", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$4, "radius", a3, _p$?.a);
      });
      return _el$4;
    }
  }), null);
  effect3(() => ({
    e: style().backgroundColor ?? "transparent",
    t: style().borderRadius
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
  });
  return _el$;
}
// packages/components/src/radio.tsx
var RadioContext = createContext2();
// packages/components/src/card.tsx
function Card(props) {
  let bg = () => props.style?.backgroundColor ?? theme.color.surface;
  let radius = () => props.style?.borderRadius ?? theme.radius.lg;
  let hasBorder = () => props.style?.borderWidth != null || props.style?.borderColor != null;
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "column");
  spread(_el$, mergeProps({
    get gap() {
      return space("lg");
    },
    get padding() {
      return space("xl");
    }
  }, () => props.layout, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get opacity() {
      return props.style?.opacity;
    }
  }), true);
  insert(_el$, createComponent2(Show, {
    get when() {
      return props.title != null;
    },
    get children() {
      var _el$3 = createElement("text");
      spread(_el$3, mergeProps({
        get color() {
          return theme.color.text;
        }
      }, () => typeStyle("title")), true);
      insert(_el$3, () => props.title);
      return _el$3;
    }
  }), null);
  insert(_el$, () => props.children, null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return hasBorder();
    },
    get children() {
      var _el$4 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: props.style?.borderColor ?? theme.color.border,
        t: props.style?.borderWidth ?? theme.borderWidth.sm,
        a: radius()
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$4, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$4, "strokeWidth", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$4, "radius", a3, _p$?.a);
      });
      return _el$4;
    }
  }), null);
  effect3(() => ({
    e: bg(),
    t: radius()
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
  });
  return _el$;
}
// packages/components/src/spinner.tsx
var SIZE = 24;
var THICKNESS = 3;
function Spinner(props) {
  let size = () => props.size ?? SIZE;
  let thickness = () => props.thickness ?? THICKNESS;
  let color = () => props.style?.color ?? theme.color.primary;
  let speed = () => props.speed ?? 1;
  let [angle, setAngle] = createSignal(0);
  let Animate = () => {
    onFrame((tick) => setAngle(tick / 1000 * speed() * (policy.motion === "reduced" ? 0.5 : 1) * Math.PI * 2));
    return null;
  };
  let path = () => {
    let s2 = size();
    let r3 = (s2 - thickness()) / 2;
    let c3 = s2 / 2;
    return `M ${c3} ${c3 - r3} A ${r3} ${r3} 0 1 1 ${c3 - r3} ${c3}`;
  };
  var _el$ = createElement("view"), _el$2 = createElement("d-path", {
    drawStyle: "stroke",
    strokeCap: "round"
  });
  insertNode2(_el$, _el$2);
  spread(_el$, mergeProps({
    get width() {
      return size();
    },
    get height() {
      return size();
    }
  }, () => props.layout, {
    get rotate() {
      return angle();
    },
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get opacity() {
      return props.style?.opacity;
    }
  }), true);
  insert(_el$, createComponent2(Show, {
    get when() {
      return policy.motion !== "none";
    },
    get children() {
      return createComponent2(Animate, {});
    }
  }), _el$2);
  effect3(() => ({
    e: path(),
    t: color(),
    a: thickness()
  }), ({
    e: e3,
    t: t3,
    a: a3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "d", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "color", t3, _p$?.t);
    a3 !== _p$?.a && setProp(_el$2, "strokeWidth", a3, _p$?.a);
  });
  return _el$;
}
// packages/components/src/modal.tsx
function Modal(props) {
  let dismiss = (_e) => {
    if (props.dismissable !== false)
      props.onClose?.();
  };
  return createPortal((() => {
    var _el$ = createElement("view", {
      position: "absolute",
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      alignItems: "center",
      justifyContent: "center"
    }), _el$2 = createElement("view", {
      position: "absolute",
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      onPointerDown: dismiss
    }), _el$3 = createElement("d-rect");
    insertNode2(_el$, _el$2);
    insertNode2(_el$2, _el$3);
    insert(_el$, () => props.children, null);
    effect3(() => props.backdropColor ?? theme.color.scrim, (_v$, _$p) => {
      setProp(_el$3, "color", _v$, _$p);
    });
    return _el$;
  })());
}
// packages/components/src/segmented-control.tsx
function SegmentedControl(props) {
  let [internal, setInternal] = createSignal(props.defaultValue);
  let value = () => props.value !== undefined ? props.value : internal();
  let select = (v2) => {
    if (props.value === undefined)
      setInternal(() => v2);
    props.onChange?.(v2);
  };
  let radius = () => typeof props.style?.borderRadius === "number" ? props.style.borderRadius : theme.radius.md;
  let corners = (i3) => {
    let r3 = radius();
    let last = props.options.length - 1;
    if (last === 0)
      return r3;
    if (i3 === 0)
      return [r3, 0, 0, r3];
    if (i3 === last)
      return [0, r3, r3, 0];
    return 0;
  };
  let idleFill = () => props.style?.backgroundColor ?? theme.color.surfaceAlt;
  let activeFill = () => props.disabled ? theme.color.surface : theme.color.primary;
  let label = (active) => props.disabled ? theme.color.textMuted : active ? theme.color.onPrimary : theme.color.text;
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "gap", 0);
  spread(_el$, mergeProps(() => props.layout, {
    get x() {
      return props.style?.x;
    },
    get y() {
      return props.style?.y;
    },
    get scale() {
      return props.style?.scale;
    },
    get rotate() {
      return props.style?.rotate;
    },
    get opacity() {
      return props.style?.opacity;
    }
  }), true);
  insert(_el$, createComponent2(For, {
    get each() {
      return props.options;
    },
    children: (opt, i3) => {
      let active = () => value() === opt.value;
      let press = createPress({
        onPress: () => select(opt.value)
      });
      let fill = () => active() ? activeFill() : press.hovered() && !props.disabled && policy.interaction !== "touch" ? theme.color.surfaceHover : idleFill();
      var _el$3 = createElement("view"), _el$4 = createElement("d-rect"), _el$5 = createElement("text");
      insertNode2(_el$3, _el$4);
      insertNode2(_el$3, _el$5);
      var _ref$ = press.ref;
      typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$3) : press.ref = _el$3;
      setProp(_el$3, "repaintBoundary", true);
      setProp(_el$3, "flexGrow", 1);
      setProp(_el$3, "flexBasis", 0);
      setProp(_el$3, "alignItems", "center");
      spread(_el$3, mergeProps({
        get paddingTop() {
          return space("md");
        },
        get paddingBottom() {
          return space("md");
        },
        get paddingLeft() {
          return space("md");
        },
        get paddingRight() {
          return space("md");
        }
      }, () => press.handlers, {
        get pointerEvents() {
          return props.disabled ? "none" : undefined;
        }
      }), true);
      spread(_el$5, mergeProps({
        get color() {
          return label(active());
        }
      }, () => typeStyle("body", active() ? lightOnDark(label(true), activeFill()) : undefined)), true);
      insert(_el$5, () => opt.label);
      effect3(() => ({
        e: fill(),
        t: corners(i3())
      }), ({
        e: e3,
        t: t3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$4, "color", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$4, "radius", t3, _p$?.t);
      });
      return _el$3;
    }
  }), null);
  effect3(() => ({
    e: theme.color.border,
    t: radius()
  }), ({
    e: e3,
    t: t3
  }, _p$) => {
    e3 !== _p$?.e && setProp(_el$2, "color", e3, _p$?.e);
    t3 !== _p$?.t && setProp(_el$2, "radius", t3, _p$?.t);
  });
  return _el$;
}
// packages/components/src/split-view.tsx
var LIST_WIDTH = 320;
function SplitView(props) {
  return createComponent2(Show, {
    get when() {
      return policy.layout === "twoPane";
    },
    get fallback() {
      var _el$4 = createElement("view");
      setProp(_el$4, "flexDirection", "column");
      spread(_el$4, mergeProps(() => props.layout), true);
      insert(_el$4, createComponent2(Show, {
        get when() {
          return props.showDetail;
        },
        get fallback() {
          return props.list;
        },
        get children() {
          return props.detail;
        }
      }));
      return _el$4;
    },
    get children() {
      var _el$ = createElement("view"), _el$2 = createElement("view", {
        flexDirection: "column"
      }), _el$3 = createElement("view", {
        flex: 1,
        flexDirection: "column"
      });
      insertNode2(_el$, _el$2);
      insertNode2(_el$, _el$3);
      setProp(_el$, "flexDirection", "row");
      spread(_el$, mergeProps(() => props.layout), true);
      insert(_el$2, () => props.list);
      insert(_el$3, () => props.detail);
      effect3(() => props.listWidth ?? LIST_WIDTH, (_v$, _$p) => {
        setProp(_el$2, "width", _v$, _$p);
      });
      return _el$;
    }
  });
}
// node_modules/.bun/qrcode-generator@2.0.4/node_modules/qrcode-generator/dist/qrcode.mjs
var qrcode = function(typeNumber, errorCorrectionLevel) {
  const PAD0 = 236;
  const PAD1 = 17;
  let _typeNumber = typeNumber;
  const _errorCorrectionLevel = QRErrorCorrectionLevel[errorCorrectionLevel];
  let _modules = null;
  let _moduleCount = 0;
  let _dataCache = null;
  const _dataList = [];
  const _this = {};
  const makeImpl = function(test, maskPattern) {
    _moduleCount = _typeNumber * 4 + 17;
    _modules = function(moduleCount) {
      const modules = new Array(moduleCount);
      for (let row = 0;row < moduleCount; row += 1) {
        modules[row] = new Array(moduleCount);
        for (let col = 0;col < moduleCount; col += 1) {
          modules[row][col] = null;
        }
      }
      return modules;
    }(_moduleCount);
    setupPositionProbePattern(0, 0);
    setupPositionProbePattern(_moduleCount - 7, 0);
    setupPositionProbePattern(0, _moduleCount - 7);
    setupPositionAdjustPattern();
    setupTimingPattern();
    setupTypeInfo(test, maskPattern);
    if (_typeNumber >= 7) {
      setupTypeNumber(test);
    }
    if (_dataCache == null) {
      _dataCache = createData(_typeNumber, _errorCorrectionLevel, _dataList);
    }
    mapData(_dataCache, maskPattern);
  };
  const setupPositionProbePattern = function(row, col) {
    for (let r3 = -1;r3 <= 7; r3 += 1) {
      if (row + r3 <= -1 || _moduleCount <= row + r3)
        continue;
      for (let c3 = -1;c3 <= 7; c3 += 1) {
        if (col + c3 <= -1 || _moduleCount <= col + c3)
          continue;
        if (0 <= r3 && r3 <= 6 && (c3 == 0 || c3 == 6) || 0 <= c3 && c3 <= 6 && (r3 == 0 || r3 == 6) || 2 <= r3 && r3 <= 4 && 2 <= c3 && c3 <= 4) {
          _modules[row + r3][col + c3] = true;
        } else {
          _modules[row + r3][col + c3] = false;
        }
      }
    }
  };
  const getBestMaskPattern = function() {
    let minLostPoint = 0;
    let pattern = 0;
    for (let i3 = 0;i3 < 8; i3 += 1) {
      makeImpl(true, i3);
      const lostPoint = QRUtil.getLostPoint(_this);
      if (i3 == 0 || minLostPoint > lostPoint) {
        minLostPoint = lostPoint;
        pattern = i3;
      }
    }
    return pattern;
  };
  const setupTimingPattern = function() {
    for (let r3 = 8;r3 < _moduleCount - 8; r3 += 1) {
      if (_modules[r3][6] != null) {
        continue;
      }
      _modules[r3][6] = r3 % 2 == 0;
    }
    for (let c3 = 8;c3 < _moduleCount - 8; c3 += 1) {
      if (_modules[6][c3] != null) {
        continue;
      }
      _modules[6][c3] = c3 % 2 == 0;
    }
  };
  const setupPositionAdjustPattern = function() {
    const pos = QRUtil.getPatternPosition(_typeNumber);
    for (let i3 = 0;i3 < pos.length; i3 += 1) {
      for (let j2 = 0;j2 < pos.length; j2 += 1) {
        const row = pos[i3];
        const col = pos[j2];
        if (_modules[row][col] != null) {
          continue;
        }
        for (let r3 = -2;r3 <= 2; r3 += 1) {
          for (let c3 = -2;c3 <= 2; c3 += 1) {
            if (r3 == -2 || r3 == 2 || c3 == -2 || c3 == 2 || r3 == 0 && c3 == 0) {
              _modules[row + r3][col + c3] = true;
            } else {
              _modules[row + r3][col + c3] = false;
            }
          }
        }
      }
    }
  };
  const setupTypeNumber = function(test) {
    const bits = QRUtil.getBCHTypeNumber(_typeNumber);
    for (let i3 = 0;i3 < 18; i3 += 1) {
      const mod = !test && (bits >> i3 & 1) == 1;
      _modules[Math.floor(i3 / 3)][i3 % 3 + _moduleCount - 8 - 3] = mod;
    }
    for (let i3 = 0;i3 < 18; i3 += 1) {
      const mod = !test && (bits >> i3 & 1) == 1;
      _modules[i3 % 3 + _moduleCount - 8 - 3][Math.floor(i3 / 3)] = mod;
    }
  };
  const setupTypeInfo = function(test, maskPattern) {
    const data = _errorCorrectionLevel << 3 | maskPattern;
    const bits = QRUtil.getBCHTypeInfo(data);
    for (let i3 = 0;i3 < 15; i3 += 1) {
      const mod = !test && (bits >> i3 & 1) == 1;
      if (i3 < 6) {
        _modules[i3][8] = mod;
      } else if (i3 < 8) {
        _modules[i3 + 1][8] = mod;
      } else {
        _modules[_moduleCount - 15 + i3][8] = mod;
      }
    }
    for (let i3 = 0;i3 < 15; i3 += 1) {
      const mod = !test && (bits >> i3 & 1) == 1;
      if (i3 < 8) {
        _modules[8][_moduleCount - i3 - 1] = mod;
      } else if (i3 < 9) {
        _modules[8][15 - i3 - 1 + 1] = mod;
      } else {
        _modules[8][15 - i3 - 1] = mod;
      }
    }
    _modules[_moduleCount - 8][8] = !test;
  };
  const mapData = function(data, maskPattern) {
    let inc = -1;
    let row = _moduleCount - 1;
    let bitIndex = 7;
    let byteIndex = 0;
    const maskFunc = QRUtil.getMaskFunction(maskPattern);
    for (let col = _moduleCount - 1;col > 0; col -= 2) {
      if (col == 6)
        col -= 1;
      while (true) {
        for (let c3 = 0;c3 < 2; c3 += 1) {
          if (_modules[row][col - c3] == null) {
            let dark = false;
            if (byteIndex < data.length) {
              dark = (data[byteIndex] >>> bitIndex & 1) == 1;
            }
            const mask = maskFunc(row, col - c3);
            if (mask) {
              dark = !dark;
            }
            _modules[row][col - c3] = dark;
            bitIndex -= 1;
            if (bitIndex == -1) {
              byteIndex += 1;
              bitIndex = 7;
            }
          }
        }
        row += inc;
        if (row < 0 || _moduleCount <= row) {
          row -= inc;
          inc = -inc;
          break;
        }
      }
    }
  };
  const createBytes = function(buffer, rsBlocks) {
    let offset = 0;
    let maxDcCount = 0;
    let maxEcCount = 0;
    const dcdata = new Array(rsBlocks.length);
    const ecdata = new Array(rsBlocks.length);
    for (let r3 = 0;r3 < rsBlocks.length; r3 += 1) {
      const dcCount = rsBlocks[r3].dataCount;
      const ecCount = rsBlocks[r3].totalCount - dcCount;
      maxDcCount = Math.max(maxDcCount, dcCount);
      maxEcCount = Math.max(maxEcCount, ecCount);
      dcdata[r3] = new Array(dcCount);
      for (let i3 = 0;i3 < dcdata[r3].length; i3 += 1) {
        dcdata[r3][i3] = 255 & buffer.getBuffer()[i3 + offset];
      }
      offset += dcCount;
      const rsPoly = QRUtil.getErrorCorrectPolynomial(ecCount);
      const rawPoly = qrPolynomial(dcdata[r3], rsPoly.getLength() - 1);
      const modPoly = rawPoly.mod(rsPoly);
      ecdata[r3] = new Array(rsPoly.getLength() - 1);
      for (let i3 = 0;i3 < ecdata[r3].length; i3 += 1) {
        const modIndex = i3 + modPoly.getLength() - ecdata[r3].length;
        ecdata[r3][i3] = modIndex >= 0 ? modPoly.getAt(modIndex) : 0;
      }
    }
    let totalCodeCount = 0;
    for (let i3 = 0;i3 < rsBlocks.length; i3 += 1) {
      totalCodeCount += rsBlocks[i3].totalCount;
    }
    const data = new Array(totalCodeCount);
    let index = 0;
    for (let i3 = 0;i3 < maxDcCount; i3 += 1) {
      for (let r3 = 0;r3 < rsBlocks.length; r3 += 1) {
        if (i3 < dcdata[r3].length) {
          data[index] = dcdata[r3][i3];
          index += 1;
        }
      }
    }
    for (let i3 = 0;i3 < maxEcCount; i3 += 1) {
      for (let r3 = 0;r3 < rsBlocks.length; r3 += 1) {
        if (i3 < ecdata[r3].length) {
          data[index] = ecdata[r3][i3];
          index += 1;
        }
      }
    }
    return data;
  };
  const createData = function(typeNumber2, errorCorrectionLevel2, dataList) {
    const rsBlocks = QRRSBlock.getRSBlocks(typeNumber2, errorCorrectionLevel2);
    const buffer = qrBitBuffer();
    for (let i3 = 0;i3 < dataList.length; i3 += 1) {
      const data = dataList[i3];
      buffer.put(data.getMode(), 4);
      buffer.put(data.getLength(), QRUtil.getLengthInBits(data.getMode(), typeNumber2));
      data.write(buffer);
    }
    let totalDataCount = 0;
    for (let i3 = 0;i3 < rsBlocks.length; i3 += 1) {
      totalDataCount += rsBlocks[i3].dataCount;
    }
    if (buffer.getLengthInBits() > totalDataCount * 8) {
      throw "code length overflow. (" + buffer.getLengthInBits() + ">" + totalDataCount * 8 + ")";
    }
    if (buffer.getLengthInBits() + 4 <= totalDataCount * 8) {
      buffer.put(0, 4);
    }
    while (buffer.getLengthInBits() % 8 != 0) {
      buffer.putBit(false);
    }
    while (true) {
      if (buffer.getLengthInBits() >= totalDataCount * 8) {
        break;
      }
      buffer.put(PAD0, 8);
      if (buffer.getLengthInBits() >= totalDataCount * 8) {
        break;
      }
      buffer.put(PAD1, 8);
    }
    return createBytes(buffer, rsBlocks);
  };
  _this.addData = function(data, mode) {
    mode = mode || "Byte";
    let newData = null;
    switch (mode) {
      case "Numeric":
        newData = qrNumber(data);
        break;
      case "Alphanumeric":
        newData = qrAlphaNum(data);
        break;
      case "Byte":
        newData = qr8BitByte(data);
        break;
      case "Kanji":
        newData = qrKanji(data);
        break;
      default:
        throw "mode:" + mode;
    }
    _dataList.push(newData);
    _dataCache = null;
  };
  _this.isDark = function(row, col) {
    if (row < 0 || _moduleCount <= row || col < 0 || _moduleCount <= col) {
      throw row + "," + col;
    }
    return _modules[row][col];
  };
  _this.getModuleCount = function() {
    return _moduleCount;
  };
  _this.make = function() {
    if (_typeNumber < 1) {
      let typeNumber2 = 1;
      for (;typeNumber2 < 40; typeNumber2++) {
        const rsBlocks = QRRSBlock.getRSBlocks(typeNumber2, _errorCorrectionLevel);
        const buffer = qrBitBuffer();
        for (let i3 = 0;i3 < _dataList.length; i3++) {
          const data = _dataList[i3];
          buffer.put(data.getMode(), 4);
          buffer.put(data.getLength(), QRUtil.getLengthInBits(data.getMode(), typeNumber2));
          data.write(buffer);
        }
        let totalDataCount = 0;
        for (let i3 = 0;i3 < rsBlocks.length; i3++) {
          totalDataCount += rsBlocks[i3].dataCount;
        }
        if (buffer.getLengthInBits() <= totalDataCount * 8) {
          break;
        }
      }
      _typeNumber = typeNumber2;
    }
    makeImpl(false, getBestMaskPattern());
  };
  _this.createTableTag = function(cellSize, margin) {
    cellSize = cellSize || 2;
    margin = typeof margin == "undefined" ? cellSize * 4 : margin;
    let qrHtml = "";
    qrHtml += '<table style="';
    qrHtml += " border-width: 0px; border-style: none;";
    qrHtml += " border-collapse: collapse;";
    qrHtml += " padding: 0px; margin: " + margin + "px;";
    qrHtml += '">';
    qrHtml += "<tbody>";
    for (let r3 = 0;r3 < _this.getModuleCount(); r3 += 1) {
      qrHtml += "<tr>";
      for (let c3 = 0;c3 < _this.getModuleCount(); c3 += 1) {
        qrHtml += '<td style="';
        qrHtml += " border-width: 0px; border-style: none;";
        qrHtml += " border-collapse: collapse;";
        qrHtml += " padding: 0px; margin: 0px;";
        qrHtml += " width: " + cellSize + "px;";
        qrHtml += " height: " + cellSize + "px;";
        qrHtml += " background-color: ";
        qrHtml += _this.isDark(r3, c3) ? "#000000" : "#ffffff";
        qrHtml += ";";
        qrHtml += '"/>';
      }
      qrHtml += "</tr>";
    }
    qrHtml += "</tbody>";
    qrHtml += "</table>";
    return qrHtml;
  };
  _this.createSvgTag = function(cellSize, margin, alt, title) {
    let opts = {};
    if (typeof arguments[0] == "object") {
      opts = arguments[0];
      cellSize = opts.cellSize;
      margin = opts.margin;
      alt = opts.alt;
      title = opts.title;
    }
    cellSize = cellSize || 2;
    margin = typeof margin == "undefined" ? cellSize * 4 : margin;
    alt = typeof alt === "string" ? { text: alt } : alt || {};
    alt.text = alt.text || null;
    alt.id = alt.text ? alt.id || "qrcode-description" : null;
    title = typeof title === "string" ? { text: title } : title || {};
    title.text = title.text || null;
    title.id = title.text ? title.id || "qrcode-title" : null;
    const size = _this.getModuleCount() * cellSize + margin * 2;
    let c3, mc, r3, mr, qrSvg = "", rect;
    rect = "l" + cellSize + ",0 0," + cellSize + " -" + cellSize + ",0 0,-" + cellSize + "z ";
    qrSvg += '<svg version="1.1" xmlns="http://www.w3.org/2000/svg"';
    qrSvg += !opts.scalable ? ' width="' + size + 'px" height="' + size + 'px"' : "";
    qrSvg += ' viewBox="0 0 ' + size + " " + size + '" ';
    qrSvg += ' preserveAspectRatio="xMinYMin meet"';
    qrSvg += title.text || alt.text ? ' role="img" aria-labelledby="' + escapeXml([title.id, alt.id].join(" ").trim()) + '"' : "";
    qrSvg += ">";
    qrSvg += title.text ? '<title id="' + escapeXml(title.id) + '">' + escapeXml(title.text) + "</title>" : "";
    qrSvg += alt.text ? '<description id="' + escapeXml(alt.id) + '">' + escapeXml(alt.text) + "</description>" : "";
    qrSvg += '<rect width="100%" height="100%" fill="white" cx="0" cy="0"/>';
    qrSvg += '<path d="';
    for (r3 = 0;r3 < _this.getModuleCount(); r3 += 1) {
      mr = r3 * cellSize + margin;
      for (c3 = 0;c3 < _this.getModuleCount(); c3 += 1) {
        if (_this.isDark(r3, c3)) {
          mc = c3 * cellSize + margin;
          qrSvg += "M" + mc + "," + mr + rect;
        }
      }
    }
    qrSvg += '" stroke="transparent" fill="black"/>';
    qrSvg += "</svg>";
    return qrSvg;
  };
  _this.createDataURL = function(cellSize, margin) {
    cellSize = cellSize || 2;
    margin = typeof margin == "undefined" ? cellSize * 4 : margin;
    const size = _this.getModuleCount() * cellSize + margin * 2;
    const min = margin;
    const max = size - margin;
    return createDataURL(size, size, function(x2, y2) {
      if (min <= x2 && x2 < max && min <= y2 && y2 < max) {
        const c3 = Math.floor((x2 - min) / cellSize);
        const r3 = Math.floor((y2 - min) / cellSize);
        return _this.isDark(r3, c3) ? 0 : 1;
      } else {
        return 1;
      }
    });
  };
  _this.createImgTag = function(cellSize, margin, alt) {
    cellSize = cellSize || 2;
    margin = typeof margin == "undefined" ? cellSize * 4 : margin;
    const size = _this.getModuleCount() * cellSize + margin * 2;
    let img = "";
    img += "<img";
    img += ' src="';
    img += _this.createDataURL(cellSize, margin);
    img += '"';
    img += ' width="';
    img += size;
    img += '"';
    img += ' height="';
    img += size;
    img += '"';
    if (alt) {
      img += ' alt="';
      img += escapeXml(alt);
      img += '"';
    }
    img += "/>";
    return img;
  };
  const escapeXml = function(s2) {
    let escaped = "";
    for (let i3 = 0;i3 < s2.length; i3 += 1) {
      const c3 = s2.charAt(i3);
      switch (c3) {
        case "<":
          escaped += "&lt;";
          break;
        case ">":
          escaped += "&gt;";
          break;
        case "&":
          escaped += "&amp;";
          break;
        case '"':
          escaped += "&quot;";
          break;
        default:
          escaped += c3;
          break;
      }
    }
    return escaped;
  };
  const _createHalfASCII = function(margin) {
    const cellSize = 1;
    margin = typeof margin == "undefined" ? cellSize * 2 : margin;
    const size = _this.getModuleCount() * cellSize + margin * 2;
    const min = margin;
    const max = size - margin;
    let y2, x2, r1, r22, p3;
    const blocks = {
      "██": "█",
      "█ ": "▀",
      " █": "▄",
      "  ": " "
    };
    const blocksLastLineNoMargin = {
      "██": "▀",
      "█ ": "▀",
      " █": " ",
      "  ": " "
    };
    let ascii = "";
    for (y2 = 0;y2 < size; y2 += 2) {
      r1 = Math.floor((y2 - min) / cellSize);
      r22 = Math.floor((y2 + 1 - min) / cellSize);
      for (x2 = 0;x2 < size; x2 += 1) {
        p3 = "█";
        if (min <= x2 && x2 < max && min <= y2 && y2 < max && _this.isDark(r1, Math.floor((x2 - min) / cellSize))) {
          p3 = " ";
        }
        if (min <= x2 && x2 < max && min <= y2 + 1 && y2 + 1 < max && _this.isDark(r22, Math.floor((x2 - min) / cellSize))) {
          p3 += " ";
        } else {
          p3 += "█";
        }
        ascii += margin < 1 && y2 + 1 >= max ? blocksLastLineNoMargin[p3] : blocks[p3];
      }
      ascii += `
`;
    }
    if (size % 2 && margin > 0) {
      return ascii.substring(0, ascii.length - size - 1) + Array(size + 1).join("▀");
    }
    return ascii.substring(0, ascii.length - 1);
  };
  _this.createASCII = function(cellSize, margin) {
    cellSize = cellSize || 1;
    if (cellSize < 2) {
      return _createHalfASCII(margin);
    }
    cellSize -= 1;
    margin = typeof margin == "undefined" ? cellSize * 2 : margin;
    const size = _this.getModuleCount() * cellSize + margin * 2;
    const min = margin;
    const max = size - margin;
    let y2, x2, r3, p3;
    const white = Array(cellSize + 1).join("██");
    const black = Array(cellSize + 1).join("  ");
    let ascii = "";
    let line = "";
    for (y2 = 0;y2 < size; y2 += 1) {
      r3 = Math.floor((y2 - min) / cellSize);
      line = "";
      for (x2 = 0;x2 < size; x2 += 1) {
        p3 = 1;
        if (min <= x2 && x2 < max && min <= y2 && y2 < max && _this.isDark(r3, Math.floor((x2 - min) / cellSize))) {
          p3 = 0;
        }
        line += p3 ? white : black;
      }
      for (r3 = 0;r3 < cellSize; r3 += 1) {
        ascii += line + `
`;
      }
    }
    return ascii.substring(0, ascii.length - 1);
  };
  _this.renderTo2dContext = function(context2, cellSize) {
    cellSize = cellSize || 2;
    const length = _this.getModuleCount();
    for (let row = 0;row < length; row++) {
      for (let col = 0;col < length; col++) {
        context2.fillStyle = _this.isDark(row, col) ? "black" : "white";
        context2.fillRect(col * cellSize, row * cellSize, cellSize, cellSize);
      }
    }
  };
  return _this;
};
qrcode.stringToBytes = function(s2) {
  const bytes = [];
  for (let i3 = 0;i3 < s2.length; i3 += 1) {
    const c3 = s2.charCodeAt(i3);
    bytes.push(c3 & 255);
  }
  return bytes;
};
qrcode.createStringToBytes = function(unicodeData, numChars) {
  const unicodeMap = function() {
    const bin = base64DecodeInputStream(unicodeData);
    const read2 = function() {
      const b2 = bin.read();
      if (b2 == -1)
        throw "eof";
      return b2;
    };
    let count = 0;
    const unicodeMap2 = {};
    while (true) {
      const b0 = bin.read();
      if (b0 == -1)
        break;
      const b1 = read2();
      const b2 = read2();
      const b3 = read2();
      const k2 = String.fromCharCode(b0 << 8 | b1);
      const v2 = b2 << 8 | b3;
      unicodeMap2[k2] = v2;
      count += 1;
    }
    if (count != numChars) {
      throw count + " != " + numChars;
    }
    return unicodeMap2;
  }();
  const unknownChar = 63;
  return function(s2) {
    const bytes = [];
    for (let i3 = 0;i3 < s2.length; i3 += 1) {
      const c3 = s2.charCodeAt(i3);
      if (c3 < 128) {
        bytes.push(c3);
      } else {
        const b2 = unicodeMap[s2.charAt(i3)];
        if (typeof b2 == "number") {
          if ((b2 & 255) == b2) {
            bytes.push(b2);
          } else {
            bytes.push(b2 >>> 8);
            bytes.push(b2 & 255);
          }
        } else {
          bytes.push(unknownChar);
        }
      }
    }
    return bytes;
  };
};
var QRMode = {
  MODE_NUMBER: 1 << 0,
  MODE_ALPHA_NUM: 1 << 1,
  MODE_8BIT_BYTE: 1 << 2,
  MODE_KANJI: 1 << 3
};
var QRErrorCorrectionLevel = {
  L: 1,
  M: 0,
  Q: 3,
  H: 2
};
var QRMaskPattern = {
  PATTERN000: 0,
  PATTERN001: 1,
  PATTERN010: 2,
  PATTERN011: 3,
  PATTERN100: 4,
  PATTERN101: 5,
  PATTERN110: 6,
  PATTERN111: 7
};
var QRUtil = function() {
  const PATTERN_POSITION_TABLE = [
    [],
    [6, 18],
    [6, 22],
    [6, 26],
    [6, 30],
    [6, 34],
    [6, 22, 38],
    [6, 24, 42],
    [6, 26, 46],
    [6, 28, 50],
    [6, 30, 54],
    [6, 32, 58],
    [6, 34, 62],
    [6, 26, 46, 66],
    [6, 26, 48, 70],
    [6, 26, 50, 74],
    [6, 30, 54, 78],
    [6, 30, 56, 82],
    [6, 30, 58, 86],
    [6, 34, 62, 90],
    [6, 28, 50, 72, 94],
    [6, 26, 50, 74, 98],
    [6, 30, 54, 78, 102],
    [6, 28, 54, 80, 106],
    [6, 32, 58, 84, 110],
    [6, 30, 58, 86, 114],
    [6, 34, 62, 90, 118],
    [6, 26, 50, 74, 98, 122],
    [6, 30, 54, 78, 102, 126],
    [6, 26, 52, 78, 104, 130],
    [6, 30, 56, 82, 108, 134],
    [6, 34, 60, 86, 112, 138],
    [6, 30, 58, 86, 114, 142],
    [6, 34, 62, 90, 118, 146],
    [6, 30, 54, 78, 102, 126, 150],
    [6, 24, 50, 76, 102, 128, 154],
    [6, 28, 54, 80, 106, 132, 158],
    [6, 32, 58, 84, 110, 136, 162],
    [6, 26, 54, 82, 110, 138, 166],
    [6, 30, 58, 86, 114, 142, 170]
  ];
  const G15 = 1 << 10 | 1 << 8 | 1 << 5 | 1 << 4 | 1 << 2 | 1 << 1 | 1 << 0;
  const G18 = 1 << 12 | 1 << 11 | 1 << 10 | 1 << 9 | 1 << 8 | 1 << 5 | 1 << 2 | 1 << 0;
  const G15_MASK = 1 << 14 | 1 << 12 | 1 << 10 | 1 << 4 | 1 << 1;
  const _this = {};
  const getBCHDigit = function(data) {
    let digit = 0;
    while (data != 0) {
      digit += 1;
      data >>>= 1;
    }
    return digit;
  };
  _this.getBCHTypeInfo = function(data) {
    let d2 = data << 10;
    while (getBCHDigit(d2) - getBCHDigit(G15) >= 0) {
      d2 ^= G15 << getBCHDigit(d2) - getBCHDigit(G15);
    }
    return (data << 10 | d2) ^ G15_MASK;
  };
  _this.getBCHTypeNumber = function(data) {
    let d2 = data << 12;
    while (getBCHDigit(d2) - getBCHDigit(G18) >= 0) {
      d2 ^= G18 << getBCHDigit(d2) - getBCHDigit(G18);
    }
    return data << 12 | d2;
  };
  _this.getPatternPosition = function(typeNumber) {
    return PATTERN_POSITION_TABLE[typeNumber - 1];
  };
  _this.getMaskFunction = function(maskPattern) {
    switch (maskPattern) {
      case QRMaskPattern.PATTERN000:
        return function(i3, j2) {
          return (i3 + j2) % 2 == 0;
        };
      case QRMaskPattern.PATTERN001:
        return function(i3, j2) {
          return i3 % 2 == 0;
        };
      case QRMaskPattern.PATTERN010:
        return function(i3, j2) {
          return j2 % 3 == 0;
        };
      case QRMaskPattern.PATTERN011:
        return function(i3, j2) {
          return (i3 + j2) % 3 == 0;
        };
      case QRMaskPattern.PATTERN100:
        return function(i3, j2) {
          return (Math.floor(i3 / 2) + Math.floor(j2 / 3)) % 2 == 0;
        };
      case QRMaskPattern.PATTERN101:
        return function(i3, j2) {
          return i3 * j2 % 2 + i3 * j2 % 3 == 0;
        };
      case QRMaskPattern.PATTERN110:
        return function(i3, j2) {
          return (i3 * j2 % 2 + i3 * j2 % 3) % 2 == 0;
        };
      case QRMaskPattern.PATTERN111:
        return function(i3, j2) {
          return (i3 * j2 % 3 + (i3 + j2) % 2) % 2 == 0;
        };
      default:
        throw "bad maskPattern:" + maskPattern;
    }
  };
  _this.getErrorCorrectPolynomial = function(errorCorrectLength) {
    let a3 = qrPolynomial([1], 0);
    for (let i3 = 0;i3 < errorCorrectLength; i3 += 1) {
      a3 = a3.multiply(qrPolynomial([1, QRMath.gexp(i3)], 0));
    }
    return a3;
  };
  _this.getLengthInBits = function(mode, type) {
    if (1 <= type && type < 10) {
      switch (mode) {
        case QRMode.MODE_NUMBER:
          return 10;
        case QRMode.MODE_ALPHA_NUM:
          return 9;
        case QRMode.MODE_8BIT_BYTE:
          return 8;
        case QRMode.MODE_KANJI:
          return 8;
        default:
          throw "mode:" + mode;
      }
    } else if (type < 27) {
      switch (mode) {
        case QRMode.MODE_NUMBER:
          return 12;
        case QRMode.MODE_ALPHA_NUM:
          return 11;
        case QRMode.MODE_8BIT_BYTE:
          return 16;
        case QRMode.MODE_KANJI:
          return 10;
        default:
          throw "mode:" + mode;
      }
    } else if (type < 41) {
      switch (mode) {
        case QRMode.MODE_NUMBER:
          return 14;
        case QRMode.MODE_ALPHA_NUM:
          return 13;
        case QRMode.MODE_8BIT_BYTE:
          return 16;
        case QRMode.MODE_KANJI:
          return 12;
        default:
          throw "mode:" + mode;
      }
    } else {
      throw "type:" + type;
    }
  };
  _this.getLostPoint = function(qrcode2) {
    const moduleCount = qrcode2.getModuleCount();
    let lostPoint = 0;
    for (let row = 0;row < moduleCount; row += 1) {
      for (let col = 0;col < moduleCount; col += 1) {
        let sameCount = 0;
        const dark = qrcode2.isDark(row, col);
        for (let r3 = -1;r3 <= 1; r3 += 1) {
          if (row + r3 < 0 || moduleCount <= row + r3) {
            continue;
          }
          for (let c3 = -1;c3 <= 1; c3 += 1) {
            if (col + c3 < 0 || moduleCount <= col + c3) {
              continue;
            }
            if (r3 == 0 && c3 == 0) {
              continue;
            }
            if (dark == qrcode2.isDark(row + r3, col + c3)) {
              sameCount += 1;
            }
          }
        }
        if (sameCount > 5) {
          lostPoint += 3 + sameCount - 5;
        }
      }
    }
    for (let row = 0;row < moduleCount - 1; row += 1) {
      for (let col = 0;col < moduleCount - 1; col += 1) {
        let count = 0;
        if (qrcode2.isDark(row, col))
          count += 1;
        if (qrcode2.isDark(row + 1, col))
          count += 1;
        if (qrcode2.isDark(row, col + 1))
          count += 1;
        if (qrcode2.isDark(row + 1, col + 1))
          count += 1;
        if (count == 0 || count == 4) {
          lostPoint += 3;
        }
      }
    }
    for (let row = 0;row < moduleCount; row += 1) {
      for (let col = 0;col < moduleCount - 6; col += 1) {
        if (qrcode2.isDark(row, col) && !qrcode2.isDark(row, col + 1) && qrcode2.isDark(row, col + 2) && qrcode2.isDark(row, col + 3) && qrcode2.isDark(row, col + 4) && !qrcode2.isDark(row, col + 5) && qrcode2.isDark(row, col + 6)) {
          lostPoint += 40;
        }
      }
    }
    for (let col = 0;col < moduleCount; col += 1) {
      for (let row = 0;row < moduleCount - 6; row += 1) {
        if (qrcode2.isDark(row, col) && !qrcode2.isDark(row + 1, col) && qrcode2.isDark(row + 2, col) && qrcode2.isDark(row + 3, col) && qrcode2.isDark(row + 4, col) && !qrcode2.isDark(row + 5, col) && qrcode2.isDark(row + 6, col)) {
          lostPoint += 40;
        }
      }
    }
    let darkCount = 0;
    for (let col = 0;col < moduleCount; col += 1) {
      for (let row = 0;row < moduleCount; row += 1) {
        if (qrcode2.isDark(row, col)) {
          darkCount += 1;
        }
      }
    }
    const ratio = Math.abs(100 * darkCount / moduleCount / moduleCount - 50) / 5;
    lostPoint += ratio * 10;
    return lostPoint;
  };
  return _this;
}();
var QRMath = function() {
  const EXP_TABLE = new Array(256);
  const LOG_TABLE = new Array(256);
  for (let i3 = 0;i3 < 8; i3 += 1) {
    EXP_TABLE[i3] = 1 << i3;
  }
  for (let i3 = 8;i3 < 256; i3 += 1) {
    EXP_TABLE[i3] = EXP_TABLE[i3 - 4] ^ EXP_TABLE[i3 - 5] ^ EXP_TABLE[i3 - 6] ^ EXP_TABLE[i3 - 8];
  }
  for (let i3 = 0;i3 < 255; i3 += 1) {
    LOG_TABLE[EXP_TABLE[i3]] = i3;
  }
  const _this = {};
  _this.glog = function(n3) {
    if (n3 < 1) {
      throw "glog(" + n3 + ")";
    }
    return LOG_TABLE[n3];
  };
  _this.gexp = function(n3) {
    while (n3 < 0) {
      n3 += 255;
    }
    while (n3 >= 256) {
      n3 -= 255;
    }
    return EXP_TABLE[n3];
  };
  return _this;
}();
var qrPolynomial = function(num, shift) {
  if (typeof num.length == "undefined") {
    throw num.length + "/" + shift;
  }
  const _num = function() {
    let offset = 0;
    while (offset < num.length && num[offset] == 0) {
      offset += 1;
    }
    const _num2 = new Array(num.length - offset + shift);
    for (let i3 = 0;i3 < num.length - offset; i3 += 1) {
      _num2[i3] = num[i3 + offset];
    }
    return _num2;
  }();
  const _this = {};
  _this.getAt = function(index) {
    return _num[index];
  };
  _this.getLength = function() {
    return _num.length;
  };
  _this.multiply = function(e3) {
    const num2 = new Array(_this.getLength() + e3.getLength() - 1);
    for (let i3 = 0;i3 < _this.getLength(); i3 += 1) {
      for (let j2 = 0;j2 < e3.getLength(); j2 += 1) {
        num2[i3 + j2] ^= QRMath.gexp(QRMath.glog(_this.getAt(i3)) + QRMath.glog(e3.getAt(j2)));
      }
    }
    return qrPolynomial(num2, 0);
  };
  _this.mod = function(e3) {
    if (_this.getLength() - e3.getLength() < 0) {
      return _this;
    }
    const ratio = QRMath.glog(_this.getAt(0)) - QRMath.glog(e3.getAt(0));
    const num2 = new Array(_this.getLength());
    for (let i3 = 0;i3 < _this.getLength(); i3 += 1) {
      num2[i3] = _this.getAt(i3);
    }
    for (let i3 = 0;i3 < e3.getLength(); i3 += 1) {
      num2[i3] ^= QRMath.gexp(QRMath.glog(e3.getAt(i3)) + ratio);
    }
    return qrPolynomial(num2, 0).mod(e3);
  };
  return _this;
};
var QRRSBlock = function() {
  const RS_BLOCK_TABLE = [
    [1, 26, 19],
    [1, 26, 16],
    [1, 26, 13],
    [1, 26, 9],
    [1, 44, 34],
    [1, 44, 28],
    [1, 44, 22],
    [1, 44, 16],
    [1, 70, 55],
    [1, 70, 44],
    [2, 35, 17],
    [2, 35, 13],
    [1, 100, 80],
    [2, 50, 32],
    [2, 50, 24],
    [4, 25, 9],
    [1, 134, 108],
    [2, 67, 43],
    [2, 33, 15, 2, 34, 16],
    [2, 33, 11, 2, 34, 12],
    [2, 86, 68],
    [4, 43, 27],
    [4, 43, 19],
    [4, 43, 15],
    [2, 98, 78],
    [4, 49, 31],
    [2, 32, 14, 4, 33, 15],
    [4, 39, 13, 1, 40, 14],
    [2, 121, 97],
    [2, 60, 38, 2, 61, 39],
    [4, 40, 18, 2, 41, 19],
    [4, 40, 14, 2, 41, 15],
    [2, 146, 116],
    [3, 58, 36, 2, 59, 37],
    [4, 36, 16, 4, 37, 17],
    [4, 36, 12, 4, 37, 13],
    [2, 86, 68, 2, 87, 69],
    [4, 69, 43, 1, 70, 44],
    [6, 43, 19, 2, 44, 20],
    [6, 43, 15, 2, 44, 16],
    [4, 101, 81],
    [1, 80, 50, 4, 81, 51],
    [4, 50, 22, 4, 51, 23],
    [3, 36, 12, 8, 37, 13],
    [2, 116, 92, 2, 117, 93],
    [6, 58, 36, 2, 59, 37],
    [4, 46, 20, 6, 47, 21],
    [7, 42, 14, 4, 43, 15],
    [4, 133, 107],
    [8, 59, 37, 1, 60, 38],
    [8, 44, 20, 4, 45, 21],
    [12, 33, 11, 4, 34, 12],
    [3, 145, 115, 1, 146, 116],
    [4, 64, 40, 5, 65, 41],
    [11, 36, 16, 5, 37, 17],
    [11, 36, 12, 5, 37, 13],
    [5, 109, 87, 1, 110, 88],
    [5, 65, 41, 5, 66, 42],
    [5, 54, 24, 7, 55, 25],
    [11, 36, 12, 7, 37, 13],
    [5, 122, 98, 1, 123, 99],
    [7, 73, 45, 3, 74, 46],
    [15, 43, 19, 2, 44, 20],
    [3, 45, 15, 13, 46, 16],
    [1, 135, 107, 5, 136, 108],
    [10, 74, 46, 1, 75, 47],
    [1, 50, 22, 15, 51, 23],
    [2, 42, 14, 17, 43, 15],
    [5, 150, 120, 1, 151, 121],
    [9, 69, 43, 4, 70, 44],
    [17, 50, 22, 1, 51, 23],
    [2, 42, 14, 19, 43, 15],
    [3, 141, 113, 4, 142, 114],
    [3, 70, 44, 11, 71, 45],
    [17, 47, 21, 4, 48, 22],
    [9, 39, 13, 16, 40, 14],
    [3, 135, 107, 5, 136, 108],
    [3, 67, 41, 13, 68, 42],
    [15, 54, 24, 5, 55, 25],
    [15, 43, 15, 10, 44, 16],
    [4, 144, 116, 4, 145, 117],
    [17, 68, 42],
    [17, 50, 22, 6, 51, 23],
    [19, 46, 16, 6, 47, 17],
    [2, 139, 111, 7, 140, 112],
    [17, 74, 46],
    [7, 54, 24, 16, 55, 25],
    [34, 37, 13],
    [4, 151, 121, 5, 152, 122],
    [4, 75, 47, 14, 76, 48],
    [11, 54, 24, 14, 55, 25],
    [16, 45, 15, 14, 46, 16],
    [6, 147, 117, 4, 148, 118],
    [6, 73, 45, 14, 74, 46],
    [11, 54, 24, 16, 55, 25],
    [30, 46, 16, 2, 47, 17],
    [8, 132, 106, 4, 133, 107],
    [8, 75, 47, 13, 76, 48],
    [7, 54, 24, 22, 55, 25],
    [22, 45, 15, 13, 46, 16],
    [10, 142, 114, 2, 143, 115],
    [19, 74, 46, 4, 75, 47],
    [28, 50, 22, 6, 51, 23],
    [33, 46, 16, 4, 47, 17],
    [8, 152, 122, 4, 153, 123],
    [22, 73, 45, 3, 74, 46],
    [8, 53, 23, 26, 54, 24],
    [12, 45, 15, 28, 46, 16],
    [3, 147, 117, 10, 148, 118],
    [3, 73, 45, 23, 74, 46],
    [4, 54, 24, 31, 55, 25],
    [11, 45, 15, 31, 46, 16],
    [7, 146, 116, 7, 147, 117],
    [21, 73, 45, 7, 74, 46],
    [1, 53, 23, 37, 54, 24],
    [19, 45, 15, 26, 46, 16],
    [5, 145, 115, 10, 146, 116],
    [19, 75, 47, 10, 76, 48],
    [15, 54, 24, 25, 55, 25],
    [23, 45, 15, 25, 46, 16],
    [13, 145, 115, 3, 146, 116],
    [2, 74, 46, 29, 75, 47],
    [42, 54, 24, 1, 55, 25],
    [23, 45, 15, 28, 46, 16],
    [17, 145, 115],
    [10, 74, 46, 23, 75, 47],
    [10, 54, 24, 35, 55, 25],
    [19, 45, 15, 35, 46, 16],
    [17, 145, 115, 1, 146, 116],
    [14, 74, 46, 21, 75, 47],
    [29, 54, 24, 19, 55, 25],
    [11, 45, 15, 46, 46, 16],
    [13, 145, 115, 6, 146, 116],
    [14, 74, 46, 23, 75, 47],
    [44, 54, 24, 7, 55, 25],
    [59, 46, 16, 1, 47, 17],
    [12, 151, 121, 7, 152, 122],
    [12, 75, 47, 26, 76, 48],
    [39, 54, 24, 14, 55, 25],
    [22, 45, 15, 41, 46, 16],
    [6, 151, 121, 14, 152, 122],
    [6, 75, 47, 34, 76, 48],
    [46, 54, 24, 10, 55, 25],
    [2, 45, 15, 64, 46, 16],
    [17, 152, 122, 4, 153, 123],
    [29, 74, 46, 14, 75, 47],
    [49, 54, 24, 10, 55, 25],
    [24, 45, 15, 46, 46, 16],
    [4, 152, 122, 18, 153, 123],
    [13, 74, 46, 32, 75, 47],
    [48, 54, 24, 14, 55, 25],
    [42, 45, 15, 32, 46, 16],
    [20, 147, 117, 4, 148, 118],
    [40, 75, 47, 7, 76, 48],
    [43, 54, 24, 22, 55, 25],
    [10, 45, 15, 67, 46, 16],
    [19, 148, 118, 6, 149, 119],
    [18, 75, 47, 31, 76, 48],
    [34, 54, 24, 34, 55, 25],
    [20, 45, 15, 61, 46, 16]
  ];
  const qrRSBlock = function(totalCount, dataCount) {
    const _this2 = {};
    _this2.totalCount = totalCount;
    _this2.dataCount = dataCount;
    return _this2;
  };
  const _this = {};
  const getRsBlockTable = function(typeNumber, errorCorrectionLevel) {
    switch (errorCorrectionLevel) {
      case QRErrorCorrectionLevel.L:
        return RS_BLOCK_TABLE[(typeNumber - 1) * 4 + 0];
      case QRErrorCorrectionLevel.M:
        return RS_BLOCK_TABLE[(typeNumber - 1) * 4 + 1];
      case QRErrorCorrectionLevel.Q:
        return RS_BLOCK_TABLE[(typeNumber - 1) * 4 + 2];
      case QRErrorCorrectionLevel.H:
        return RS_BLOCK_TABLE[(typeNumber - 1) * 4 + 3];
      default:
        return;
    }
  };
  _this.getRSBlocks = function(typeNumber, errorCorrectionLevel) {
    const rsBlock = getRsBlockTable(typeNumber, errorCorrectionLevel);
    if (typeof rsBlock == "undefined") {
      throw "bad rs block @ typeNumber:" + typeNumber + "/errorCorrectionLevel:" + errorCorrectionLevel;
    }
    const length = rsBlock.length / 3;
    const list = [];
    for (let i3 = 0;i3 < length; i3 += 1) {
      const count = rsBlock[i3 * 3 + 0];
      const totalCount = rsBlock[i3 * 3 + 1];
      const dataCount = rsBlock[i3 * 3 + 2];
      for (let j2 = 0;j2 < count; j2 += 1) {
        list.push(qrRSBlock(totalCount, dataCount));
      }
    }
    return list;
  };
  return _this;
}();
var qrBitBuffer = function() {
  const _buffer = [];
  let _length = 0;
  const _this = {};
  _this.getBuffer = function() {
    return _buffer;
  };
  _this.getAt = function(index) {
    const bufIndex = Math.floor(index / 8);
    return (_buffer[bufIndex] >>> 7 - index % 8 & 1) == 1;
  };
  _this.put = function(num, length) {
    for (let i3 = 0;i3 < length; i3 += 1) {
      _this.putBit((num >>> length - i3 - 1 & 1) == 1);
    }
  };
  _this.getLengthInBits = function() {
    return _length;
  };
  _this.putBit = function(bit) {
    const bufIndex = Math.floor(_length / 8);
    if (_buffer.length <= bufIndex) {
      _buffer.push(0);
    }
    if (bit) {
      _buffer[bufIndex] |= 128 >>> _length % 8;
    }
    _length += 1;
  };
  return _this;
};
var qrNumber = function(data) {
  const _mode = QRMode.MODE_NUMBER;
  const _data = data;
  const _this = {};
  _this.getMode = function() {
    return _mode;
  };
  _this.getLength = function(buffer) {
    return _data.length;
  };
  _this.write = function(buffer) {
    const data2 = _data;
    let i3 = 0;
    while (i3 + 2 < data2.length) {
      buffer.put(strToNum(data2.substring(i3, i3 + 3)), 10);
      i3 += 3;
    }
    if (i3 < data2.length) {
      if (data2.length - i3 == 1) {
        buffer.put(strToNum(data2.substring(i3, i3 + 1)), 4);
      } else if (data2.length - i3 == 2) {
        buffer.put(strToNum(data2.substring(i3, i3 + 2)), 7);
      }
    }
  };
  const strToNum = function(s2) {
    let num = 0;
    for (let i3 = 0;i3 < s2.length; i3 += 1) {
      num = num * 10 + chatToNum(s2.charAt(i3));
    }
    return num;
  };
  const chatToNum = function(c3) {
    if ("0" <= c3 && c3 <= "9") {
      return c3.charCodeAt(0) - 48;
    }
    throw "illegal char :" + c3;
  };
  return _this;
};
var qrAlphaNum = function(data) {
  const _mode = QRMode.MODE_ALPHA_NUM;
  const _data = data;
  const _this = {};
  _this.getMode = function() {
    return _mode;
  };
  _this.getLength = function(buffer) {
    return _data.length;
  };
  _this.write = function(buffer) {
    const s2 = _data;
    let i3 = 0;
    while (i3 + 1 < s2.length) {
      buffer.put(getCode(s2.charAt(i3)) * 45 + getCode(s2.charAt(i3 + 1)), 11);
      i3 += 2;
    }
    if (i3 < s2.length) {
      buffer.put(getCode(s2.charAt(i3)), 6);
    }
  };
  const getCode = function(c3) {
    if ("0" <= c3 && c3 <= "9") {
      return c3.charCodeAt(0) - 48;
    } else if ("A" <= c3 && c3 <= "Z") {
      return c3.charCodeAt(0) - 65 + 10;
    } else {
      switch (c3) {
        case " ":
          return 36;
        case "$":
          return 37;
        case "%":
          return 38;
        case "*":
          return 39;
        case "+":
          return 40;
        case "-":
          return 41;
        case ".":
          return 42;
        case "/":
          return 43;
        case ":":
          return 44;
        default:
          throw "illegal char :" + c3;
      }
    }
  };
  return _this;
};
var qr8BitByte = function(data) {
  const _mode = QRMode.MODE_8BIT_BYTE;
  const _data = data;
  const _bytes = qrcode.stringToBytes(data);
  const _this = {};
  _this.getMode = function() {
    return _mode;
  };
  _this.getLength = function(buffer) {
    return _bytes.length;
  };
  _this.write = function(buffer) {
    for (let i3 = 0;i3 < _bytes.length; i3 += 1) {
      buffer.put(_bytes[i3], 8);
    }
  };
  return _this;
};
var qrKanji = function(data) {
  const _mode = QRMode.MODE_KANJI;
  const _data = data;
  const stringToBytes = qrcode.stringToBytes;
  (function(c3, code) {
    const test = stringToBytes(c3);
    if (test.length != 2 || (test[0] << 8 | test[1]) != code) {
      throw "sjis not supported.";
    }
  })("友", 38726);
  const _bytes = stringToBytes(data);
  const _this = {};
  _this.getMode = function() {
    return _mode;
  };
  _this.getLength = function(buffer) {
    return ~~(_bytes.length / 2);
  };
  _this.write = function(buffer) {
    const data2 = _bytes;
    let i3 = 0;
    while (i3 + 1 < data2.length) {
      let c3 = (255 & data2[i3]) << 8 | 255 & data2[i3 + 1];
      if (33088 <= c3 && c3 <= 40956) {
        c3 -= 33088;
      } else if (57408 <= c3 && c3 <= 60351) {
        c3 -= 49472;
      } else {
        throw "illegal char at " + (i3 + 1) + "/" + c3;
      }
      c3 = (c3 >>> 8 & 255) * 192 + (c3 & 255);
      buffer.put(c3, 13);
      i3 += 2;
    }
    if (i3 < data2.length) {
      throw "illegal char at " + (i3 + 1);
    }
  };
  return _this;
};
var byteArrayOutputStream = function() {
  const _bytes = [];
  const _this = {};
  _this.writeByte = function(b2) {
    _bytes.push(b2 & 255);
  };
  _this.writeShort = function(i3) {
    _this.writeByte(i3);
    _this.writeByte(i3 >>> 8);
  };
  _this.writeBytes = function(b2, off, len) {
    off = off || 0;
    len = len || b2.length;
    for (let i3 = 0;i3 < len; i3 += 1) {
      _this.writeByte(b2[i3 + off]);
    }
  };
  _this.writeString = function(s2) {
    for (let i3 = 0;i3 < s2.length; i3 += 1) {
      _this.writeByte(s2.charCodeAt(i3));
    }
  };
  _this.toByteArray = function() {
    return _bytes;
  };
  _this.toString = function() {
    let s2 = "";
    s2 += "[";
    for (let i3 = 0;i3 < _bytes.length; i3 += 1) {
      if (i3 > 0) {
        s2 += ",";
      }
      s2 += _bytes[i3];
    }
    s2 += "]";
    return s2;
  };
  return _this;
};
var base64EncodeOutputStream = function() {
  let _buffer = 0;
  let _buflen = 0;
  let _length = 0;
  let _base64 = "";
  const _this = {};
  const writeEncoded = function(b2) {
    _base64 += String.fromCharCode(encode(b2 & 63));
  };
  const encode = function(n3) {
    if (n3 < 0) {
      throw "n:" + n3;
    } else if (n3 < 26) {
      return 65 + n3;
    } else if (n3 < 52) {
      return 97 + (n3 - 26);
    } else if (n3 < 62) {
      return 48 + (n3 - 52);
    } else if (n3 == 62) {
      return 43;
    } else if (n3 == 63) {
      return 47;
    } else {
      throw "n:" + n3;
    }
  };
  _this.writeByte = function(n3) {
    _buffer = _buffer << 8 | n3 & 255;
    _buflen += 8;
    _length += 1;
    while (_buflen >= 6) {
      writeEncoded(_buffer >>> _buflen - 6);
      _buflen -= 6;
    }
  };
  _this.flush = function() {
    if (_buflen > 0) {
      writeEncoded(_buffer << 6 - _buflen);
      _buffer = 0;
      _buflen = 0;
    }
    if (_length % 3 != 0) {
      const padlen = 3 - _length % 3;
      for (let i3 = 0;i3 < padlen; i3 += 1) {
        _base64 += "=";
      }
    }
  };
  _this.toString = function() {
    return _base64;
  };
  return _this;
};
var base64DecodeInputStream = function(str) {
  const _str = str;
  let _pos = 0;
  let _buffer = 0;
  let _buflen = 0;
  const _this = {};
  _this.read = function() {
    while (_buflen < 8) {
      if (_pos >= _str.length) {
        if (_buflen == 0) {
          return -1;
        }
        throw "unexpected end of file./" + _buflen;
      }
      const c3 = _str.charAt(_pos);
      _pos += 1;
      if (c3 == "=") {
        _buflen = 0;
        return -1;
      } else if (c3.match(/^\s$/)) {
        continue;
      }
      _buffer = _buffer << 6 | decode(c3.charCodeAt(0));
      _buflen += 6;
    }
    const n3 = _buffer >>> _buflen - 8 & 255;
    _buflen -= 8;
    return n3;
  };
  const decode = function(c3) {
    if (65 <= c3 && c3 <= 90) {
      return c3 - 65;
    } else if (97 <= c3 && c3 <= 122) {
      return c3 - 97 + 26;
    } else if (48 <= c3 && c3 <= 57) {
      return c3 - 48 + 52;
    } else if (c3 == 43) {
      return 62;
    } else if (c3 == 47) {
      return 63;
    } else {
      throw "c:" + c3;
    }
  };
  return _this;
};
var gifImage = function(width, height) {
  const _width = width;
  const _height = height;
  const _data = new Array(width * height);
  const _this = {};
  _this.setPixel = function(x2, y2, pixel) {
    _data[y2 * _width + x2] = pixel;
  };
  _this.write = function(out) {
    out.writeString("GIF87a");
    out.writeShort(_width);
    out.writeShort(_height);
    out.writeByte(128);
    out.writeByte(0);
    out.writeByte(0);
    out.writeByte(0);
    out.writeByte(0);
    out.writeByte(0);
    out.writeByte(255);
    out.writeByte(255);
    out.writeByte(255);
    out.writeString(",");
    out.writeShort(0);
    out.writeShort(0);
    out.writeShort(_width);
    out.writeShort(_height);
    out.writeByte(0);
    const lzwMinCodeSize = 2;
    const raster = getLZWRaster(lzwMinCodeSize);
    out.writeByte(lzwMinCodeSize);
    let offset = 0;
    while (raster.length - offset > 255) {
      out.writeByte(255);
      out.writeBytes(raster, offset, 255);
      offset += 255;
    }
    out.writeByte(raster.length - offset);
    out.writeBytes(raster, offset, raster.length - offset);
    out.writeByte(0);
    out.writeString(";");
  };
  const bitOutputStream = function(out) {
    const _out = out;
    let _bitLength = 0;
    let _bitBuffer = 0;
    const _this2 = {};
    _this2.write = function(data, length) {
      if (data >>> length != 0) {
        throw "length over";
      }
      while (_bitLength + length >= 8) {
        _out.writeByte(255 & (data << _bitLength | _bitBuffer));
        length -= 8 - _bitLength;
        data >>>= 8 - _bitLength;
        _bitBuffer = 0;
        _bitLength = 0;
      }
      _bitBuffer = data << _bitLength | _bitBuffer;
      _bitLength = _bitLength + length;
    };
    _this2.flush = function() {
      if (_bitLength > 0) {
        _out.writeByte(_bitBuffer);
      }
    };
    return _this2;
  };
  const getLZWRaster = function(lzwMinCodeSize) {
    const clearCode = 1 << lzwMinCodeSize;
    const endCode = (1 << lzwMinCodeSize) + 1;
    let bitLength = lzwMinCodeSize + 1;
    const table = lzwTable();
    for (let i3 = 0;i3 < clearCode; i3 += 1) {
      table.add(String.fromCharCode(i3));
    }
    table.add(String.fromCharCode(clearCode));
    table.add(String.fromCharCode(endCode));
    const byteOut = byteArrayOutputStream();
    const bitOut = bitOutputStream(byteOut);
    bitOut.write(clearCode, bitLength);
    let dataIndex = 0;
    let s2 = String.fromCharCode(_data[dataIndex]);
    dataIndex += 1;
    while (dataIndex < _data.length) {
      const c3 = String.fromCharCode(_data[dataIndex]);
      dataIndex += 1;
      if (table.contains(s2 + c3)) {
        s2 = s2 + c3;
      } else {
        bitOut.write(table.indexOf(s2), bitLength);
        if (table.size() < 4095) {
          if (table.size() == 1 << bitLength) {
            bitLength += 1;
          }
          table.add(s2 + c3);
        }
        s2 = c3;
      }
    }
    bitOut.write(table.indexOf(s2), bitLength);
    bitOut.write(endCode, bitLength);
    bitOut.flush();
    return byteOut.toByteArray();
  };
  const lzwTable = function() {
    const _map = {};
    let _size = 0;
    const _this2 = {};
    _this2.add = function(key) {
      if (_this2.contains(key)) {
        throw "dup key:" + key;
      }
      _map[key] = _size;
      _size += 1;
    };
    _this2.size = function() {
      return _size;
    };
    _this2.indexOf = function(key) {
      return _map[key];
    };
    _this2.contains = function(key) {
      return typeof _map[key] != "undefined";
    };
    return _this2;
  };
  return _this;
};
var createDataURL = function(width, height, getPixel) {
  const gif = gifImage(width, height);
  for (let y2 = 0;y2 < height; y2 += 1) {
    for (let x2 = 0;x2 < width; x2 += 1) {
      gif.setPixel(x2, y2, getPixel(x2, y2));
    }
  }
  const b2 = byteArrayOutputStream();
  gif.write(b2);
  const base64 = base64EncodeOutputStream();
  const bytes = b2.toByteArray();
  for (let i3 = 0;i3 < bytes.length; i3 += 1) {
    base64.writeByte(bytes[i3]);
  }
  base64.flush();
  return "data:image/gif;base64," + base64;
};
var stringToBytes = qrcode.stringToBytes;
// packages/components/src/icon.tsx
var SIZE2 = 24;
function Icon(props) {
  let size = () => props.size ?? SIZE2;
  let doc = createMemo(() => parseSvg(props.src, {
    color: props.color ?? theme.color.text
  }));
  var _el$ = createElement("view");
  setProp(_el$, "repaintBoundary", true);
  spread(_el$, mergeProps({
    get width() {
      return size();
    },
    get height() {
      return size();
    },
    get viewBox() {
      return [doc().width, doc().height];
    }
  }, () => props.layout), true);
  insert(_el$, createComponent2(For, {
    get each() {
      return doc().draws;
    },
    children: (draw) => (() => {
      var _el$2 = createElement("d-path");
      spread(_el$2, draw, false);
      return _el$2;
    })()
  }));
  return _el$;
}
// lattice/launcher/parts/nav.tsx
import { on as on4 } from "srt:events";
var targets = [];
var [focusedTarget, setFocusedTarget] = createSignal(null, {
  ownedWrite: true
});
function navTarget(action2, opts) {
  let target = {
    node: null,
    action: action2,
    modal: opts?.modal ?? (() => false),
    disabled: opts?.disabled ?? (() => false)
  };
  targets.push(target);
  onSettled(() => () => {
    targets.splice(targets.indexOf(target), 1);
    if (untrack(focusedTarget) === target)
      setFocusedTarget(null);
  });
  return {
    ref: (n3) => {
      target.node = n3;
    },
    focused: () => focusedTarget() === target
  };
}
function navRing(focused, radius) {
  if (!focused)
    return {};
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md
  };
}
function NavButton(props) {
  let nav = navTarget(() => props.onPress?.(), {
    modal: () => props.modal ?? false,
    disabled: () => props.disabled ?? false
  });
  return createComponent2(Button, {
    ref(r$) {
      var _ref$ = nav.ref;
      typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : nav.ref = r$;
    },
    get variant() {
      return props.variant;
    },
    get size() {
      return props.size;
    },
    get onPress() {
      return props.onPress;
    },
    get disabled() {
      return props.disabled;
    },
    get layout() {
      return props.layout;
    },
    get style() {
      return {
        ...props.style,
        ...navRing(nav.focused())
      };
    },
    get children() {
      return props.children;
    }
  });
}
function reachable() {
  let usable = targets.filter((t3) => !t3.disabled());
  let modal = usable.filter((t3) => t3.modal());
  let placed = [];
  for (let t3 of modal.length > 0 ? modal : usable) {
    let b2 = t3.node && getBoundingBoxViewport2(t3.node);
    if (b2)
      placed.push({
        target: t3,
        x: b2.x + b2.width / 2,
        y: b2.y + b2.height / 2
      });
  }
  return placed;
}
function focusFirst(placed) {
  let first = placed.reduce((a3, b2) => b2.y < a3.y - 1 || Math.abs(b2.y - a3.y) <= 1 && b2.x < a3.x ? b2 : a3);
  setFocusedTarget(first.target);
}
function move(dir) {
  let placed = reachable();
  if (placed.length === 0)
    return;
  let cur = untrack(focusedTarget);
  let from = cur && placed.find((p3) => p3.target === cur);
  if (!from)
    return focusFirst(placed);
  let best = null;
  let bestScore = Infinity;
  for (let p3 of placed) {
    if (p3 === from)
      continue;
    let dx = p3.x - from.x;
    let dy = p3.y - from.y;
    let ahead = dir === "up" ? -dy : dir === "down" ? dy : dir === "left" ? -dx : dx;
    if (ahead <= 1)
      continue;
    let across = Math.abs(dir === "up" || dir === "down" ? dx : dy);
    let score = ahead + 2 * across;
    if (score < bestScore) {
      bestScore = score;
      best = p3;
    }
  }
  if (best)
    setFocusedTarget(best.target);
}
function activate() {
  let placed = reachable();
  if (placed.length === 0)
    return;
  let cur = untrack(focusedTarget);
  let hit = cur && placed.find((p3) => p3.target === cur);
  if (!hit)
    return focusFirst(placed);
  hit.target.action();
}
on4("keydown", (e3) => {
  if (getFocusedNodeId() != null)
    return;
  if (e3.key === "ArrowUp")
    move("up");
  else if (e3.key === "ArrowDown")
    move("down");
  else if (e3.key === "ArrowLeft")
    move("left");
  else if (e3.key === "ArrowRight")
    move("right");
  else if ((e3.key === "Enter" || e3.code === "Select") && !e3.repeat)
    activate();
});
var prevButtons = new Set;
on4("gamepads", (e3) => {
  let now = new Set;
  for (let pad of e3.pads ?? [])
    for (let b2 of pad?.buttons ?? [])
      now.add(b2);
  for (let b2 of now) {
    if (prevButtons.has(b2))
      continue;
    if (b2 === "dpadUp")
      move("up");
    else if (b2 === "dpadDown")
      move("down");
    else if (b2 === "dpadLeft")
      move("left");
    else if (b2 === "dpadRight")
      move("right");
    else if (b2 === "south")
      activate();
  }
  prevButtons = now;
});

// lattice/launcher/parts/home-screen.tsx
import { canDiscover, discover, stop } from "srt:dev";
import { available as appsAvailable, list, launch, remove, info, clearCache } from "srt:apps";

// packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on5 } from "srt:events";
var devicesAccessor2;
function cameraDevices() {
  if (!devicesAccessor2) {
    let [devices, setDevices] = createSignal(listCameras());
    on5("cameraDeviceChange", () => setDevices(listCameras()));
    devicesAccessor2 = devices;
  }
  return devicesAccessor2();
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
  return {
    texture,
    width,
    height,
    barcode,
    error
  };
}

// lattice/launcher/parts/puzzle.tsx
var PUZZLE_SEGMENTS = [{
  light: "#3f5494",
  dark: "#162b6c",
  d: "M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z"
}, {
  light: "#547ebf",
  dark: "#2b5696",
  d: "M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z"
}, {
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z"
}, {
  light: "#547ebf",
  dark: "#2b5696",
  d: "M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z"
}, {
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z"
}, {
  light: "#3f5494",
  dark: "#162b6c",
  d: "M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z"
}, {
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z"
}];
function PuzzleMark(props) {
  return createComponent2(View, {
    get layout() {
      return {
        width: props.size,
        height: props.size
      };
    },
    get style() {
      return {
        scale: props.size / 100,
        originX: 0,
        originY: 0
      };
    },
    get children() {
      return createComponent2(For, {
        each: PUZZLE_SEGMENTS,
        children: (seg) => (() => {
          var _el$ = createElement("d-path");
          effect3(() => ({
            e: seg.d,
            t: createLinearGradient(0, 0, 1, 1, [{
              offset: 0,
              color: seg.light
            }, {
              offset: 1,
              color: seg.dark
            }])
          }), ({
            e: e3,
            t: t3
          }, _p$) => {
            e3 !== _p$?.e && setProp(_el$, "d", e3, _p$?.e);
            t3 !== _p$?.t && setProp(_el$, "color", t3, _p$?.t);
          });
          return _el$;
        })()
      });
    }
  });
}

// lattice/launcher/parts/app-icon.tsx
function AppIcon(props) {
  let doc = createMemo2(() => {
    let src = props.app.icon;
    if (!src)
      return;
    try {
      return parseSvg(src);
    } catch (err) {
      console.warn(`App icon for ${props.app.name} failed to parse: ${err}`);
      return;
    }
  });
  return createComponent2(Show, {
    get when() {
      return doc();
    },
    get fallback() {
      return createComponent2(View, {
        get layout() {
          return {
            width: props.size,
            height: props.size,
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0
          };
        },
        get style() {
          return {
            backgroundColor: theme.color.surfaceAlt,
            borderRadius: props.size / 4
          };
        },
        get children() {
          var _el$ = createElement("text", {
            fontWeight: 500
          });
          insert(_el$, () => props.app.name.slice(0, 1).toUpperCase());
          effect3(() => ({
            e: theme.color.textMuted,
            t: theme.text.fontFamily,
            a: props.size * 0.45
          }), ({
            e: e3,
            t: t3,
            a: a3
          }, _p$) => {
            e3 !== _p$?.e && setProp(_el$, "color", e3, _p$?.e);
            t3 !== _p$?.t && setProp(_el$, "fontFamily", t3, _p$?.t);
            a3 !== _p$?.a && setProp(_el$, "fontSize", a3, _p$?.a);
          });
          return _el$;
        }
      });
    },
    children: (d2) => (() => {
      var _el$2 = createElement("view", {
        repaintBoundary: true,
        flexShrink: 0
      });
      insert(_el$2, createComponent2(For, {
        get each() {
          return d2().draws;
        },
        children: (draw) => (() => {
          var _el$3 = createElement("d-path");
          spread(_el$3, draw, false);
          return _el$3;
        })()
      }));
      effect3(() => ({
        e: props.size,
        t: props.size,
        a: [d2().width, d2().height]
      }), ({
        e: e3,
        t: t3,
        a: a3
      }, _p$) => {
        e3 !== _p$?.e && setProp(_el$2, "width", e3, _p$?.e);
        t3 !== _p$?.t && setProp(_el$2, "height", t3, _p$?.t);
        a3 !== _p$?.a && setProp(_el$2, "viewBox", a3, _p$?.a);
      });
      return _el$2;
    })()
  });
}

// lattice/launcher/parts/detail-card.tsx
function DetailRow(props) {
  return createComponent2(View, {
    get layout() {
      return {
        flexDirection: "row",
        justifyContent: "space-between",
        gap: space("md")
      };
    },
    get children() {
      return [createComponent2(Text, {
        variant: "body",
        muted: true,
        get children() {
          return props.label;
        }
      }), createComponent2(Text, {
        variant: "body",
        get muted() {
          return props.mutedValue;
        },
        get children() {
          return props.value;
        }
      })];
    }
  });
}
function DetailCard(props) {
  return createComponent2(Card, {
    get layout() {
      return {
        gap: space("md"),
        padding: space("lg")
      };
    },
    get children() {
      return [createComponent2(Text, {
        variant: "title",
        muted: true,
        get children() {
          return props.title;
        }
      }), memo2(() => props.children)];
    }
  });
}

// lattice/launcher/parts/types.ts
var COLUMN_MAX_WIDTH = 440;
var DETAIL_MAX_WIDTH = 640;
var TAP_TARGET = 44;
var STATUS_TEXT = {
  idle: "Not connected",
  searching: "Searching...",
  connecting: "Connecting...",
  connected: "Connected"
};
function normalizeAddress(raw) {
  return raw.trim().replace(/^(ws|http):\/\//, "").replace(/\/+$/, "");
}

// lattice/launcher/parts/back-button.tsx
var ARROW_LEFT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12h-14"/></svg>`;
function BackButton(props) {
  let nav = navTarget(() => props.onPress());
  return createComponent2(Pressable, {
    ref(r$) {
      var _ref$ = nav.ref;
      typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : nav.ref = r$;
    },
    get onPress() {
      return props.onPress;
    },
    layout: {
      width: TAP_TARGET,
      height: TAP_TARGET,
      alignItems: "center",
      justifyContent: "center"
    },
    style: (s2) => ({
      backgroundColor: s2.hovered ? theme.color.surfaceHover : "transparent",
      borderRadius: theme.radius.md,
      ...navRing(nav.focused())
    }),
    get children() {
      return createComponent2(Icon, {
        src: ARROW_LEFT_SVG,
        size: 22
      });
    }
  });
}

// lattice/launcher/parts/dev-connection.ts
import { on as on6 } from "srt:events";
import { available as devAvailable, connect as devConnect, launchAddress } from "srt:dev";
var available = devAvailable;
var [state, setState] = createSignal("idle");
var [address, setAddress] = createSignal(null);
var [tunneled, setTunneled] = createSignal(false);
var [recents, setRecents] = createSignal([]);
if (available) {
  on6("dev", (e3) => {
    setState(e3.state);
    setAddress(e3.address);
    setTunneled(e3.tunneled);
    if (e3.recents)
      setRecents(e3.recents);
  });
  if (launchAddress)
    devConnect(launchAddress);
}
var connectionState = state;
var serverAddress = address;
var isTunneled = tunneled;
var recentAddresses = recents;
var isConnected = () => state() === "connected";
var isBusy = () => state() === "searching" || state() === "connecting";
var isIdle = () => state() === "idle";
function connect(addr) {
  devConnect(normalizeAddress(addr));
}

// lattice/launcher/parts/home-screen.tsx
var GEAR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>`;
var PLAY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 3 20 12 6 21 6 3"/></svg>`;
function formatStamp(ms) {
  if (!ms)
    return "";
  let then = new Date(ms);
  let pad = (n3) => String(n3).padStart(2, "0");
  let time = `${pad(then.getHours())}:${pad(then.getMinutes())}`;
  let midnight = (d2) => new Date(d2.getFullYear(), d2.getMonth(), d2.getDate()).getTime();
  let days = Math.round((midnight(new Date) - midnight(then)) / 86400000);
  if (days <= 0)
    return time;
  return days === 1 ? `${time}, yesterday` : `${time}, ${days} days ago`;
}
function formatSize(bytes) {
  if (bytes < 1024)
    return `${bytes} B`;
  let kb = bytes / 1024;
  if (kb < 1024)
    return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  let mb = kb / 1024;
  if (mb < 1024)
    return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}
function AppCard(props) {
  let subtitle = () => {
    let details = [formatSize(props.app.size), formatStamp(props.app.updated)].filter(Boolean).join(", ");
    return props.app.name === props.app.id ? details : `${props.app.id} - ${details}`;
  };
  let nav = navTarget(() => props.onPress());
  return createComponent2(Pressable, {
    ref(r$) {
      var _ref$ = nav.ref;
      typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : nav.ref = r$;
    },
    get onPress() {
      return props.onPress;
    },
    style: () => navRing(nav.focused(), theme.radius.lg),
    children: (s2) => createComponent2(Card, {
      get layout() {
        return {
          flexDirection: "row",
          alignItems: "center",
          gap: space("lg")
        };
      },
      get style() {
        return {
          backgroundColor: props.active ? theme.color.surfaceAlt : s2.hovered ? theme.color.surfaceHover : theme.color.surface
        };
      },
      get children() {
        return [createComponent2(AppIcon, {
          get app() {
            return props.app;
          },
          size: 40
        }), createComponent2(View, {
          layout: {
            flexDirection: "column",
            flexGrow: 1,
            gap: 2
          },
          get children() {
            return [createComponent2(Text, {
              variant: "title",
              get children() {
                return props.app.name;
              }
            }), createComponent2(Text, {
              variant: "body",
              muted: true,
              get children() {
                return subtitle();
              }
            })];
          }
        }), createComponent2(Pressable, {
          get onPress() {
            return props.onLaunch;
          },
          layout: {
            width: TAP_TARGET,
            height: TAP_TARGET,
            alignItems: "center",
            justifyContent: "center"
          },
          children: (ps) => createComponent2(Icon, {
            src: PLAY_SVG,
            size: 20,
            get color() {
              return memo2(() => !!(ps.pressed || ps.hovered))() ? theme.color.primaryHover : theme.color.primary;
            }
          })
        })];
      }
    })
  });
}
function groupCache(entries, key) {
  let groups = new Map;
  for (let e3 of entries) {
    let k2 = key(e3);
    let g2 = groups.get(k2);
    if (!g2)
      groups.set(k2, g2 = {
        key: k2,
        count: 0,
        size: 0
      });
    g2.count += 1;
    g2.size += e3.size;
  }
  return [...groups.values()].sort((a3, b2) => b2.size - a3.size);
}
function cacheDomain(url) {
  let m2 = /^[a-z][a-z0-9+.-]*:\/\/([^/]+)/i.exec(url);
  return m2?.[1] ?? "unknown";
}
function amount(count, size) {
  return `${count} file${count === 1 ? "" : "s"}, ${formatSize(size)}`;
}
function AppDetail(props) {
  let [confirming, setConfirming] = createSignal(false);
  createEffect(() => props.app.id, () => {
    setConfirming(false);
  });
  onBack((e3) => {
    if (confirming()) {
      e3.preventDefault();
      setConfirming(false);
    }
  });
  let [detailsGen, setDetailsGen] = createSignal(0);
  let details = createMemo2(() => {
    detailsGen();
    try {
      return info(props.app.id);
    } catch {
      return null;
    }
  });
  return createComponent2(ScrollView, {
    layout: {
      flexGrow: 1
    },
    get children() {
      return createComponent2(View, {
        get layout() {
          return {
            flexGrow: 1,
            alignItems: policy.layout === "twoPane" ? "flex-start" : "center"
          };
        },
        get children() {
          return createComponent2(View, {
            get layout() {
              return {
                flexDirection: "column",
                gap: space("lg"),
                padding: space("xl"),
                width: "100%",
                maxWidth: DETAIL_MAX_WIDTH
              };
            },
            get children() {
              return [createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "row",
                    alignItems: "center",
                    gap: space("lg")
                  };
                },
                get children() {
                  return [createComponent2(Show, {
                    get when() {
                      return props.onBack;
                    },
                    get children() {
                      return createComponent2(BackButton, {
                        onPress: () => props.onBack?.()
                      });
                    }
                  }), createComponent2(AppIcon, {
                    get app() {
                      return props.app;
                    },
                    size: 56
                  }), createComponent2(View, {
                    layout: {
                      flexDirection: "column",
                      flexGrow: 1,
                      gap: 2
                    },
                    get children() {
                      return [createComponent2(Text, {
                        variant: "heading",
                        get children() {
                          return props.app.name;
                        }
                      }), createComponent2(Text, {
                        variant: "body",
                        muted: true,
                        get children() {
                          return props.app.id;
                        }
                      })];
                    }
                  })];
                }
              }), createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "row",
                    gap: space("md")
                  };
                },
                get children() {
                  return [createComponent2(NavButton, {
                    onPress: () => props.onLaunch(),
                    children: "Launch"
                  }), createComponent2(NavButton, {
                    variant: "secondary",
                    onPress: () => setConfirming(true),
                    children: "Remove"
                  })];
                }
              }), createComponent2(Show, {
                get when() {
                  return confirming();
                },
                get children() {
                  return createComponent2(Modal, {
                    onClose: () => setConfirming(false),
                    get children() {
                      return createComponent2(View, {
                        get layout() {
                          return {
                            width: "100%",
                            maxWidth: 380,
                            padding: space("xl")
                          };
                        },
                        get children() {
                          return createComponent2(Card, {
                            get layout() {
                              return {
                                gap: space("lg")
                              };
                            },
                            get children() {
                              return [createComponent2(View, {
                                get layout() {
                                  return {
                                    flexDirection: "column",
                                    gap: space("sm")
                                  };
                                },
                                get children() {
                                  return [createComponent2(Text, {
                                    variant: "title",
                                    get children() {
                                      return ["Remove ", memo2(() => props.app.name), "?"];
                                    }
                                  }), createComponent2(Text, {
                                    variant: "body",
                                    muted: true,
                                    children: "This deletes the app and its stored data. This cannot be undone."
                                  })];
                                }
                              }), createComponent2(View, {
                                get layout() {
                                  return {
                                    flexDirection: "row",
                                    gap: space("md")
                                  };
                                },
                                get children() {
                                  return [createComponent2(NavButton, {
                                    modal: true,
                                    variant: "ghost",
                                    onPress: () => setConfirming(false),
                                    children: "Cancel"
                                  }), createComponent2(NavButton, {
                                    modal: true,
                                    variant: "danger",
                                    onPress: () => props.onRemove(),
                                    children: "Remove"
                                  })];
                                }
                              })];
                            }
                          });
                        }
                      });
                    }
                  });
                }
              }), createComponent2(Show, {
                get when() {
                  return details();
                },
                children: (d2) => [createComponent2(DetailCard, {
                  title: "Storage",
                  get children() {
                    return [createComponent2(DetailRow, {
                      label: "App",
                      get value() {
                        return formatSize(d2().installSize);
                      }
                    }), createComponent2(DetailRow, {
                      label: "Files",
                      get value() {
                        return amount(d2().files.length, d2().files.reduce((sum, f3) => sum + f3.size, 0));
                      }
                    }), createComponent2(DetailRow, {
                      label: "Data",
                      get value() {
                        return amount(d2().data.length, d2().dataSize);
                      }
                    }), createComponent2(DetailRow, {
                      label: "Cache",
                      get value() {
                        return amount(d2().cache.length, d2().cacheSize);
                      }
                    })];
                  }
                }), createComponent2(DetailCard, {
                  title: "Versions",
                  get children() {
                    return createComponent2(For, {
                      get each() {
                        return d2().versions;
                      },
                      children: (v2) => createComponent2(DetailRow, {
                        get label() {
                          return v2.id.slice(0, 12) + (v2.current ? " (current)" : "");
                        },
                        get value() {
                          return `${v2.solidrtVersion}, ${formatSize(v2.size)}`;
                        },
                        get mutedValue() {
                          return !v2.current;
                        }
                      })
                    });
                  }
                }), createComponent2(DetailCard, {
                  title: "Files",
                  get children() {
                    return createComponent2(For, {
                      get each() {
                        return d2().files;
                      },
                      children: (f3) => createComponent2(DetailRow, {
                        get label() {
                          return f3.path;
                        },
                        get value() {
                          return formatSize(f3.size);
                        }
                      })
                    });
                  }
                }), createComponent2(DetailCard, {
                  title: "Data",
                  get children() {
                    return createComponent2(Show, {
                      get when() {
                        return d2().data.length > 0;
                      },
                      get fallback() {
                        return createComponent2(Text, {
                          variant: "body",
                          muted: true,
                          children: "Empty"
                        });
                      },
                      get children() {
                        return createComponent2(For, {
                          get each() {
                            return d2().data;
                          },
                          children: (f3) => createComponent2(DetailRow, {
                            get label() {
                              return f3.path;
                            },
                            get value() {
                              return formatSize(f3.size);
                            }
                          })
                        });
                      }
                    });
                  }
                }), createComponent2(DetailCard, {
                  title: "Cache",
                  get children() {
                    return createComponent2(Show, {
                      get when() {
                        return d2().cache.length > 0;
                      },
                      get fallback() {
                        return createComponent2(Text, {
                          variant: "body",
                          muted: true,
                          children: "Empty"
                        });
                      },
                      get children() {
                        return [createComponent2(Text, {
                          variant: "body",
                          children: "By type"
                        }), createComponent2(For, {
                          get each() {
                            return groupCache(d2().cache, (e3) => e3.type ?? "unknown");
                          },
                          children: (g2) => createComponent2(DetailRow, {
                            get label() {
                              return g2.key;
                            },
                            get value() {
                              return amount(g2.count, g2.size);
                            }
                          })
                        }), createComponent2(Text, {
                          variant: "body",
                          children: "By domain"
                        }), createComponent2(For, {
                          get each() {
                            return groupCache(d2().cache, (e3) => cacheDomain(e3.url));
                          },
                          children: (g2) => createComponent2(DetailRow, {
                            get label() {
                              return g2.key;
                            },
                            get value() {
                              return amount(g2.count, g2.size);
                            }
                          })
                        })];
                      }
                    });
                  }
                }), createComponent2(Show, {
                  get when() {
                    return d2().cache.length > 0;
                  },
                  get children() {
                    return createComponent2(NavButton, {
                      variant: "danger",
                      onPress: () => {
                        clearCache(props.app.id);
                        setDetailsGen((n3) => n3 + 1);
                      },
                      children: "Clear cache"
                    });
                  }
                })]
              })];
            }
          });
        }
      });
    }
  });
}
function AppList(props) {
  return createComponent2(ScrollView, {
    layout: {
      flexGrow: 1
    },
    get children() {
      return createComponent2(View, {
        get layout() {
          return {
            flexDirection: "column",
            gap: space("md")
          };
        },
        get children() {
          return createComponent2(For, {
            get each() {
              return props.apps;
            },
            children: (app) => createComponent2(AppCard, {
              app,
              get active() {
                return memo2(() => !!props.twoPane)() ? props.selectedId === app.id : props.twoPane;
              },
              onPress: () => props.onSelect(app.id),
              onLaunch: () => props.onLaunch(app.id)
            })
          });
        }
      });
    }
  });
}
function NoApps() {
  return createComponent2(View, {
    get layout() {
      return {
        flexGrow: 1,
        flexDirection: "column",
        justifyContent: "center",
        alignItems: "center",
        gap: space("md")
      };
    },
    get children() {
      return [createComponent2(Text, {
        variant: "title",
        children: "No apps installed"
      }), createComponent2(Text, {
        muted: true,
        children: "Connect a dev server to install apps"
      })];
    }
  });
}
function DevCard(props) {
  let hasCamera = () => cameraDevices().length > 0;
  return createComponent2(Card, {
    get layout() {
      return {
        gap: space("md"),
        padding: space("lg")
      };
    },
    get children() {
      return [createComponent2(View, {
        get layout() {
          return {
            flexDirection: "row",
            alignItems: "center",
            gap: space("md")
          };
        },
        get children() {
          return [createComponent2(Show, {
            get when() {
              return props.busy;
            },
            get fallback() {
              return createComponent2(View, {
                layout: {
                  width: 8,
                  height: 8
                },
                get children() {
                  var _el$ = createElement("d-oval");
                  effect3(() => props.connected ? theme.color.primary : theme.color.textMuted, (_v$, _$p) => {
                    setProp(_el$, "color", _v$, _$p);
                  });
                  return _el$;
                }
              });
            },
            get children() {
              return createComponent2(Spinner, {
                size: 14,
                thickness: 2
              });
            }
          }), createComponent2(Text, {
            variant: "body",
            muted: true,
            layout: {
              flexGrow: 1
            },
            get children() {
              return props.status;
            }
          })];
        }
      }), createComponent2(View, {
        get layout() {
          return {
            flexDirection: "row",
            gap: space("sm")
          };
        },
        get children() {
          return [createComponent2(Show, {
            get when() {
              return props.idle;
            },
            get children() {
              return [createComponent2(Show, {
                when: canDiscover,
                get children() {
                  return createComponent2(NavButton, {
                    variant: "secondary",
                    onPress: () => discover(),
                    children: "Discover"
                  });
                }
              }), createComponent2(Show, {
                get when() {
                  return hasCamera();
                },
                get children() {
                  return createComponent2(NavButton, {
                    variant: "secondary",
                    get onPress() {
                      return props.onScan;
                    },
                    children: "Scan QR"
                  });
                }
              }), createComponent2(NavButton, {
                variant: "secondary",
                get onPress() {
                  return props.onManual;
                },
                children: "Address"
              })];
            }
          }), createComponent2(Show, {
            get when() {
              return props.busy;
            },
            get children() {
              return createComponent2(NavButton, {
                variant: "secondary",
                onPress: () => stop(),
                children: "Cancel"
              });
            }
          }), createComponent2(Show, {
            get when() {
              return props.connected;
            },
            get children() {
              return createComponent2(NavButton, {
                variant: "secondary",
                onPress: () => stop(),
                children: "Disconnect"
              });
            }
          })];
        }
      })];
    }
  });
}
function HomeScreen(props) {
  let [apps, setApps] = createSignal(appsAvailable ? list() : []);
  let gearNav = navTarget(() => props.onSettings());
  let twoPane = () => policy.layout === "twoPane";
  let selectedApp = () => apps().find((a3) => a3.id === props.selectedId) ?? null;
  let status = () => isConnected() ? `Connected to ${serverAddress()}${isTunneled() ? " (tunneled)" : ""}` : props.notice ?? STATUS_TEXT[connectionState()];
  let doLaunch = (id2) => {
    try {
      launch(id2);
    } catch (e3) {
      props.setNotice(e3 instanceof Error ? e3.message : String(e3));
    }
  };
  let doRemove = (id2) => {
    try {
      remove(id2);
    } catch (e3) {
      props.setNotice(e3 instanceof Error ? e3.message : String(e3));
    }
    props.setSelectedId(null);
    setApps(appsAvailable ? list() : []);
  };
  onBack((e3) => {
    if (!twoPane() && selectedApp() != null) {
      e3.preventDefault();
      props.setSelectedId(null);
    }
  });
  return createComponent2(SplitView, {
    layout: {
      flexGrow: 1
    },
    listWidth: 380,
    get showDetail() {
      return selectedApp() != null;
    },
    get list() {
      return createComponent2(View, {
        layout: {
          flexGrow: 1,
          flexDirection: "column",
          alignItems: "center"
        },
        get children() {
          return createComponent2(View, {
            get layout() {
              return {
                flexDirection: "column",
                flexGrow: 1,
                width: "100%",
                maxWidth: twoPane() ? undefined : COLUMN_MAX_WIDTH,
                padding: space("xl"),
                gap: space("xl")
              };
            },
            get children() {
              return [createComponent2(View, {
                layout: {
                  flexDirection: "row",
                  justifyContent: "space-between",
                  alignItems: "center"
                },
                get children() {
                  return [createComponent2(View, {
                    get layout() {
                      return {
                        flexDirection: "row",
                        alignItems: "center",
                        gap: space("md")
                      };
                    },
                    get children() {
                      return [createComponent2(PuzzleMark, {
                        size: 40
                      }), createComponent2(Text, {
                        variant: "heading",
                        children: "SolidRT"
                      })];
                    }
                  }), createComponent2(Pressable, {
                    ref(r$) {
                      var _ref$2 = gearNav.ref;
                      typeof _ref$2 === "function" || Array.isArray(_ref$2) ? applyRef(_ref$2, r$) : gearNav.ref = r$;
                    },
                    get onPress() {
                      return props.onSettings;
                    },
                    layout: {
                      width: TAP_TARGET,
                      height: TAP_TARGET,
                      alignItems: "center",
                      justifyContent: "center"
                    },
                    style: (s2) => ({
                      backgroundColor: s2.hovered ? theme.color.surfaceHover : "transparent",
                      borderRadius: theme.radius.md,
                      ...navRing(gearNav.focused())
                    }),
                    get children() {
                      return createComponent2(Icon, {
                        src: GEAR_SVG,
                        size: 22
                      });
                    }
                  })];
                }
              }), createComponent2(Show, {
                get when() {
                  return apps().length > 0;
                },
                get fallback() {
                  return createComponent2(NoApps, {});
                },
                get children() {
                  return createComponent2(AppList, {
                    get apps() {
                      return apps();
                    },
                    get selectedId() {
                      return props.selectedId;
                    },
                    get twoPane() {
                      return twoPane();
                    },
                    onSelect: (id2) => props.setSelectedId(id2),
                    onLaunch: (id2) => doLaunch(id2)
                  });
                }
              }), createComponent2(Show, {
                when: available,
                get children() {
                  return createComponent2(DevCard, {
                    get status() {
                      return status();
                    },
                    get idle() {
                      return isIdle();
                    },
                    get busy() {
                      return isBusy();
                    },
                    get connected() {
                      return isConnected();
                    },
                    get onScan() {
                      return props.onScan;
                    },
                    get onManual() {
                      return props.onManual;
                    }
                  });
                }
              })];
            }
          });
        }
      });
    },
    get detail() {
      return createComponent2(Show, {
        get when() {
          return selectedApp();
        },
        get fallback() {
          return createComponent2(View, {
            get layout() {
              return {
                flexGrow: 1,
                justifyContent: "center",
                alignItems: "center",
                gap: space("lg")
              };
            },
            get children() {
              return createComponent2(PuzzleMark, {
                size: 360
              });
            }
          });
        },
        children: (app) => createComponent2(AppDetail, {
          get app() {
            return app();
          },
          onLaunch: () => doLaunch(app().id),
          onRemove: () => doRemove(app().id),
          get onBack() {
            return twoPane() ? undefined : () => props.setSelectedId(null);
          }
        })
      });
    }
  });
}

// lattice/launcher/parts/settings-screen.tsx
import { version as buildVersion, profile as buildProfile, platform as buildPlatform } from "srt:apps";
function CapabilityChip(props) {
  return createComponent2(View, {
    get layout() {
      return {
        paddingLeft: space("md"),
        paddingRight: space("md"),
        paddingTop: space("sm"),
        paddingBottom: space("sm")
      };
    },
    get style() {
      return {
        backgroundColor: theme.color.surfaceAlt,
        borderRadius: theme.radius.sm
      };
    },
    get children() {
      return createComponent2(Text, {
        variant: "body",
        muted: true,
        get children() {
          return props.name;
        }
      });
    }
  });
}
var THEME_MODES = ["system", "light", "dark"];
function SettingsScreen(props) {
  let modeNav = navTarget(() => props.onMode(THEME_MODES[(THEME_MODES.indexOf(props.mode) + 1) % THEME_MODES.length]));
  return createComponent2(ScrollView, {
    layout: {
      flexGrow: 1
    },
    get children() {
      return createComponent2(View, {
        layout: {
          flexGrow: 1,
          alignItems: "center"
        },
        get children() {
          return createComponent2(View, {
            get layout() {
              return {
                flexDirection: "column",
                gap: space("lg"),
                width: "100%",
                maxWidth: COLUMN_MAX_WIDTH,
                padding: space("xl")
              };
            },
            get children() {
              return [createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "row",
                    alignItems: "center",
                    gap: space("md")
                  };
                },
                get children() {
                  return [createComponent2(BackButton, {
                    get onPress() {
                      return props.onBack;
                    }
                  }), createComponent2(Text, {
                    variant: "heading",
                    children: "Settings"
                  })];
                }
              }), createComponent2(DetailCard, {
                title: "Appearance",
                get children() {
                  return createComponent2(View, {
                    ref(r$) {
                      var _ref$ = modeNav.ref;
                      typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : modeNav.ref = r$;
                    },
                    get style() {
                      return navRing(modeNav.focused());
                    },
                    get children() {
                      return createComponent2(SegmentedControl, {
                        options: [{
                          value: "system",
                          label: "System"
                        }, {
                          value: "light",
                          label: "Light"
                        }, {
                          value: "dark",
                          label: "Dark"
                        }],
                        get value() {
                          return props.mode;
                        },
                        onChange: (v2) => props.onMode(v2)
                      });
                    }
                  });
                }
              }), createComponent2(DetailCard, {
                title: "About",
                get children() {
                  return [createComponent2(DetailRow, {
                    label: "Build version",
                    value: buildVersion
                  }), createComponent2(DetailRow, {
                    label: "Profile",
                    value: buildProfile
                  }), createComponent2(DetailRow, {
                    label: "Flux version",
                    get value() {
                      return Flux.version;
                    }
                  }), createComponent2(DetailRow, {
                    label: "Platform",
                    value: buildPlatform
                  })];
                }
              }), createComponent2(DetailCard, {
                title: "Capabilities",
                get children() {
                  return createComponent2(View, {
                    get layout() {
                      return {
                        flexDirection: "row",
                        flexWrap: "wrap",
                        gap: space("sm")
                      };
                    },
                    get children() {
                      return createComponent2(For, {
                        get each() {
                          return Flux.capabilities;
                        },
                        children: (name) => createComponent2(CapabilityChip, {
                          name
                        })
                      });
                    }
                  });
                }
              })];
            }
          });
        }
      });
    }
  });
}

// lattice/launcher/parts/scan-screen.tsx
var RETICLE_STROKE = 10;
var RETICLE_RADIUS = 20;
var CLOSE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`;
var SCRIM = "rgba(0, 0, 0, 0.45)";
var SCRIM_HOVER = "rgba(0, 0, 0, 0.65)";
function ScanScreen(props) {
  let cam = createCamera(untrack(() => ({
    scan: ["qr"]
  })));
  let closeNav = navTarget(() => props.onCancel());
  createEffect(() => cam.barcode(), (b2) => {
    if (b2)
      props.onScanned(b2.data);
  });
  createEffect(() => cam.error(), (e3) => {
    if (e3)
      props.onError(e3.message);
  });
  let crop = () => {
    let cw = cam.width();
    let ch = cam.height();
    let {
      width: w2,
      height: h3
    } = env.windowSize;
    if (!cw || !ch || !w2 || !h3)
      return null;
    let scale = Math.max(w2 / cw, h3 / ch);
    let srcW = w2 / scale;
    let srcH = h3 / scale;
    return {
      w: w2,
      h: h3,
      srcX: (cw - srcW) / 2,
      srcY: (ch - srcH) / 2,
      srcW,
      srcH
    };
  };
  let reticle = () => {
    let {
      width: w2,
      height: h3
    } = env.windowSize;
    let s2 = Math.round(Math.min(w2, h3) * 0.55);
    let l2 = Math.round(s2 * 0.18);
    let i3 = RETICLE_STROKE / 2;
    let r3 = RETICLE_RADIUS;
    return {
      size: s2,
      d: `M${i3} ${l2} L${i3} ${i3 + r3} A ${r3} ${r3} 0 0 1 ${i3 + r3} ${i3} L${l2} ${i3} ` + `M${s2 - l2} ${i3} L${s2 - i3 - r3} ${i3} A ${r3} ${r3} 0 0 1 ${s2 - i3} ${i3 + r3} L${s2 - i3} ${l2} ` + `M${s2 - i3} ${s2 - l2} L${s2 - i3} ${s2 - i3 - r3} A ${r3} ${r3} 0 0 1 ${s2 - i3 - r3} ${s2 - i3} L${s2 - l2} ${s2 - i3} ` + `M${l2} ${s2 - i3} L${i3 + r3} ${s2 - i3} A ${r3} ${r3} 0 0 1 ${i3} ${s2 - i3 - r3} L${i3} ${s2 - l2}`
    };
  };
  return createComponent2(View, {
    layout: {
      flexGrow: 1,
      position: "relative"
    },
    style: {
      backgroundColor: "black"
    },
    get children() {
      return [createComponent2(Show, {
        get when() {
          return memo2(() => cam.texture() != null)() && crop();
        },
        children: (c3) => (() => {
          var _el$2 = createElement("texture", {
            position: "absolute"
          });
          effect3(() => ({
            e: cam.texture(),
            t: c3().w,
            a: c3().h,
            o: c3().srcX,
            i: c3().srcY,
            n: c3().srcW,
            s: c3().srcH
          }), ({
            e: e3,
            t: t3,
            a: a3,
            o: o3,
            i: i3,
            n: n3,
            s: s2
          }, _p$) => {
            e3 !== _p$?.e && setProp(_el$2, "src", e3, _p$?.e);
            t3 !== _p$?.t && setProp(_el$2, "w", t3, _p$?.t);
            a3 !== _p$?.a && setProp(_el$2, "h", a3, _p$?.a);
            o3 !== _p$?.o && setProp(_el$2, "srcX", o3, _p$?.o);
            i3 !== _p$?.i && setProp(_el$2, "srcY", i3, _p$?.i);
            n3 !== _p$?.n && setProp(_el$2, "srcW", n3, _p$?.n);
            s2 !== _p$?.s && setProp(_el$2, "srcH", s2, _p$?.s);
          });
          return _el$2;
        })()
      }), createComponent2(View, {
        layout: {
          position: "absolute",
          width: "100%",
          height: "100%",
          justifyContent: "center",
          alignItems: "center"
        },
        get children() {
          return createComponent2(View, {
            get layout() {
              return {
                width: reticle().size,
                height: reticle().size
              };
            },
            get children() {
              var _el$ = createElement("d-path", {
                color: "white",
                drawStyle: "stroke",
                strokeWidth: 10,
                strokeCap: "round",
                strokeJoin: "round"
              });
              effect3(() => reticle().d, (_v$, _$p) => {
                setProp(_el$, "d", _v$, _$p);
              });
              return _el$;
            }
          });
        }
      }), createComponent2(View, {
        layout: {
          position: "absolute",
          width: "100%",
          height: "100%"
        },
        get children() {
          return createComponent2(SafeArea, {
            get children() {
              return createComponent2(View, {
                get layout() {
                  return {
                    flexGrow: 1,
                    padding: space("xl")
                  };
                },
                get children() {
                  return createComponent2(View, {
                    layout: {
                      flexDirection: "row"
                    },
                    get children() {
                      return createComponent2(Pressable, {
                        ref(r$) {
                          var _ref$ = closeNav.ref;
                          typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : closeNav.ref = r$;
                        },
                        get onPress() {
                          return props.onCancel;
                        },
                        layout: {
                          width: TAP_TARGET,
                          height: TAP_TARGET,
                          alignItems: "center",
                          justifyContent: "center"
                        },
                        style: (s2) => ({
                          backgroundColor: s2.hovered ? SCRIM_HOVER : SCRIM,
                          borderRadius: TAP_TARGET / 2,
                          ...navRing(closeNav.focused(), TAP_TARGET / 2)
                        }),
                        get children() {
                          return createComponent2(Icon, {
                            src: CLOSE_SVG,
                            size: 22,
                            color: "white"
                          });
                        }
                      });
                    }
                  });
                }
              });
            }
          });
        }
      })];
    }
  });
}

// lattice/launcher/parts/connect-screen.tsx
var DEFAULT_PORT = "34884";
function recentLabel(entry) {
  if (!entry.includes("|"))
    return entry;
  return "ticket " + entry.split("|")[0].slice(0, 8);
}
function ConnectScreen(props) {
  let hostDraft = "";
  let portDraft = DEFAULT_PORT;
  let submit = () => {
    let host = hostDraft.trim();
    if (!host)
      return;
    let port = portDraft.trim();
    props.onDial(port ? `${host}:${port}` : host);
  };
  return createComponent2(View, {
    layout: {
      flexGrow: 1,
      alignItems: "center"
    },
    get children() {
      return createComponent2(View, {
        get layout() {
          return {
            flexDirection: "column",
            gap: space("lg"),
            width: "100%",
            maxWidth: COLUMN_MAX_WIDTH,
            padding: space("xl"),
            paddingTop: 72
          };
        },
        get children() {
          return [createComponent2(Card, {
            get children() {
              return [createComponent2(Text, {
                variant: "title",
                children: "Connect to a dev server"
              }), createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "row",
                    gap: space("md")
                  };
                },
                get children() {
                  return [createComponent2(TextInput, {
                    layout: {
                      flexGrow: 1
                    },
                    placeholder: "IP address",
                    onInput: (v2) => hostDraft = v2,
                    onSubmit: submit
                  }), createComponent2(TextInput, {
                    layout: {
                      width: 96
                    },
                    placeholder: "port",
                    defaultValue: DEFAULT_PORT,
                    onInput: (v2) => portDraft = v2,
                    onSubmit: submit
                  })];
                }
              }), createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "row",
                    gap: space("md")
                  };
                },
                get children() {
                  return [createComponent2(NavButton, {
                    onPress: submit,
                    children: "Connect"
                  }), createComponent2(NavButton, {
                    variant: "ghost",
                    get onPress() {
                      return props.onCancel;
                    },
                    children: "Cancel"
                  })];
                }
              })];
            }
          }), createComponent2(Show, {
            get when() {
              return recentAddresses().length > 0;
            },
            get children() {
              return createComponent2(View, {
                get layout() {
                  return {
                    flexDirection: "column",
                    gap: space("sm")
                  };
                },
                get children() {
                  return [createComponent2(Text, {
                    variant: "body",
                    muted: true,
                    children: "Recent"
                  }), createComponent2(View, {
                    get layout() {
                      return {
                        flexDirection: "column",
                        gap: space("sm")
                      };
                    },
                    get children() {
                      return createComponent2(For, {
                        get each() {
                          return recentAddresses();
                        },
                        children: (entry) => createComponent2(NavButton, {
                          variant: "secondary",
                          onPress: () => props.onDial(entry),
                          get children() {
                            return recentLabel(entry);
                          }
                        })
                      });
                    }
                  })];
                }
              });
            }
          })];
        }
      });
    }
  });
}

// lattice/launcher/launcher.tsx
function App() {
  let [themeMode, setThemeMode] = createSignal("system");
  let dark = () => {
    let mode = themeMode();
    if (mode === "system")
      return env.systemTheme !== "light";
    return mode === "dark";
  };
  createEffect(() => dark(), (d2) => setTheme(d2 ? darkTheme : lightTheme));
  let [screen, setScreen] = createSignal("home");
  let [selectedId, setSelectedId] = createSignal(null);
  let [notice, setNotice] = createSignal(null);
  let [confirmExit, setConfirmExit] = createSignal(false);
  let dial = (addr) => {
    setNotice(null);
    setScreen("home");
    connect(addr);
  };
  onBack((e3) => {
    e3.preventDefault();
    if (confirmExit()) {
      setConfirmExit(false);
    } else if (screen() !== "home") {
      setScreen("home");
    } else {
      setConfirmExit(true);
    }
  });
  return createComponent2(Window, {
    title: "SolidRT",
    layout: {
      flexDirection: "column"
    },
    get style() {
      return {
        backgroundColor: theme.color.background
      };
    },
    get children() {
      return createComponent2(SafeArea, {
        get children() {
          return [createComponent2(Switch, {
            get children() {
              return [createComponent2(Match, {
                get when() {
                  return screen() === "scan";
                },
                get children() {
                  return createComponent2(ScanScreen, {
                    onScanned: (data) => dial(data),
                    onCancel: () => setScreen("home"),
                    onError: (m2) => {
                      setNotice(`Camera: ${m2}`);
                      setScreen("home");
                    }
                  });
                }
              }), createComponent2(Match, {
                get when() {
                  return screen() === "manual";
                },
                get children() {
                  return createComponent2(ConnectScreen, {
                    onDial: (addr) => dial(addr),
                    onCancel: () => setScreen("home")
                  });
                }
              }), createComponent2(Match, {
                get when() {
                  return screen() === "settings";
                },
                get children() {
                  return createComponent2(SettingsScreen, {
                    get mode() {
                      return themeMode();
                    },
                    onMode: setThemeMode,
                    onBack: () => setScreen("home")
                  });
                }
              }), createComponent2(Match, {
                get when() {
                  return screen() === "home";
                },
                get children() {
                  return createComponent2(HomeScreen, {
                    get selectedId() {
                      return selectedId();
                    },
                    setSelectedId,
                    get notice() {
                      return notice();
                    },
                    setNotice,
                    onScan: () => {
                      setNotice(null);
                      setScreen("scan");
                    },
                    onManual: () => setScreen("manual"),
                    onSettings: () => setScreen("settings")
                  });
                }
              })];
            }
          }), createComponent2(Show, {
            get when() {
              return confirmExit();
            },
            get children() {
              return createComponent2(Modal, {
                onClose: () => setConfirmExit(false),
                get children() {
                  return createComponent2(View, {
                    get layout() {
                      return {
                        width: "100%",
                        maxWidth: 380,
                        padding: space("xl")
                      };
                    },
                    get children() {
                      return createComponent2(Card, {
                        get layout() {
                          return {
                            gap: space("lg")
                          };
                        },
                        get children() {
                          return [createComponent2(Text, {
                            variant: "title",
                            children: "Exit SolidRT?"
                          }), createComponent2(View, {
                            get layout() {
                              return {
                                flexDirection: "row",
                                gap: space("md")
                              };
                            },
                            get children() {
                              return [createComponent2(NavButton, {
                                modal: true,
                                variant: "ghost",
                                onPress: () => setConfirmExit(false),
                                children: "Cancel"
                              }), createComponent2(NavButton, {
                                modal: true,
                                onPress: () => exit(),
                                children: "Exit"
                              })];
                            }
                          })];
                        }
                      });
                    }
                  });
                }
              });
            }
          })];
        }
      });
    }
  });
}
render(() => createComponent2(App, {}));
