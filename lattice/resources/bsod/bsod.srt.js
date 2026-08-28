// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/error.js
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

class ContextNotFoundError extends Error {
  constructor() {
    super("");
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/constants.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/lanes.js
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
  for (const i of e.Ae)
    n.Ae.add(i);
  e.Ae.clear();
  n.rn[0].push(...e.rn[0]);
  n.rn[1].push(...e.rn[1]);
  e.rn[0].length = 0;
  e.rn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.Ke;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  n.Ke = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.sn) {
    const e = n.sn = currentTransition(n.sn);
    if (e.fn !== true)
      return e;
    n.sn = null;
  }
  return resolveLane(n)?.Ne ?? n.Ne;
}
function hasActiveOverride(n) {
  return !!(n.De !== undefined && n.De !== NOT_PENDING);
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const r = n.Ke;
  if (r) {
    if (r.tn) {
      n.Ke = e;
      return;
    }
    const t = findLane(r);
    if (activeLanes.has(t)) {
      if (t !== i && !hasActiveOverride(n)) {
        if (i.an && findLane(i.an) === t) {
          n.Ke = e;
        } else if (t.an && findLane(t.an) === i)
          ;
        else
          mergeLanes(i, t);
      }
      return;
    }
  }
  n.Ke = e;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Ve: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Ve: 0,
  EE: 0
};
function cancelZombieRecompute(e) {
  if (e.se & REACTIVE_IN_HEAP_HEIGHT)
    e.se &= -12;
  else {
    deleteFromHeap(e, zombieQueue);
    e.se &= -4;
  }
}
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
  return transitions.size === 0 && activeLanes.size === 0 && e.vt.length === 0 && t.Me.length === 0 && t.A.length === 0 && t.dn.size === 0 && transientStoreNodes.size === 0;
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
    if (e.De !== undefined && e.De !== NOT_PENDING)
      continue;
    if (e.t)
      continue;
    transientStoreNodes.delete(e);
    e.ut?.();
  }
}
function createBatch() {
  return {
    de: clock,
    Qt: [],
    Ie: new Map,
    Me: [],
    A: [],
    dn: new Set,
    ie: [],
    yt: {
      bt: [[], []],
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
    if (i.Ne === t)
      i.Ne = e;
  if (t.Me.length) {
    e.Me.push(...t.Me);
    t.Me.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const i of t.dn)
    e.dn.add(i);
  for (const [i, n] of t.Ie) {
    let t2 = e.Ie.get(i);
    if (!t2)
      e.Ie.set(i, t2 = new Set);
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
  if (!syncDepth && !globalQueue.cn && !projectionWriteActive)
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
var queueRunToken = 0;

class Queue {
  Fe = null;
  bt = [[], []];
  vt = [];
  kt = 0;
  created = clock;
  addChild(e) {
    this.vt.push(e);
    e.Fe = this;
  }
  removeChild(e) {
    const t = this.vt.indexOf(e);
    if (t >= 0) {
      this.vt.splice(t, 1);
      e.Fe = null;
    }
  }
  notify(e, t, i, n) {
    if (this.Fe)
      return this.Fe.notify(e, t, i, n);
    return false;
  }
  run(e) {
    if (this.bt[e - 1].length) {
      const t2 = this.bt[e - 1];
      this.bt[e - 1] = [];
      runQueue(t2, e);
    }
    const t = this.vt;
    const i = ++queueRunToken;
    for (let n = 0;n < t.length; ) {
      const s = t[n];
      if (s.kt !== i) {
        s.kt = i;
        s.run?.(e);
        if (t[n] !== s) {
          n = 0;
          continue;
        }
      }
      n++;
    }
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const i = findLane(currentOptimisticLane);
        i.rn[e - 1].push(t);
      } else {
        this.bt[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.bt[0].push(...this.bt[0]);
    e.bt[1].push(...this.bt[1]);
    this.bt = [[], []];
    for (let t = 0;t < this.vt.length; t++) {
      let i = this.vt[t];
      let n = e.vt[t];
      if (!n) {
        n = {
          bt: [[], []],
          vt: []
        };
        e.vt[t] = n;
      }
      i.stashQueues(n);
    }
  }
  restoreQueues(e) {
    this.bt[0].push(...e.bt[0]);
    this.bt[1].push(...e.bt[1]);
    for (let t = 0;t < e.vt.length; t++) {
      const i = e.vt[t];
      let n = this.vt[t];
      if (n)
        n.restoreQueues(i);
    }
  }
}

class GlobalQueue extends Queue {
  cn = false;
  m = createBatch();
  static Ce;
  static me;
  static et;
  static gt = null;
  static p = null;
  static G = null;
  static M = null;
  static h = null;
  static dt = null;
  static St = null;
  static Pe = null;
  static Se = null;
  static Ge = null;
  static un = null;
  static At = null;
  static Ct = null;
  static Pt = null;
  static $e = null;
  static k = null;
  static Lt = null;
  static Gt = null;
  static En = null;
  static Tn = null;
  static In = null;
  static Nn = null;
  static Rt = null;
  static Ot = null;
  static Dt = null;
  static je = null;
  static ze = null;
  static Be = null;
  static Qn = null;
  flush() {
    if (this.cn)
      return;
    this.cn = true;
    try {
      if (false)
        ;
      runHeap(dirtyQueue, GlobalQueue.Ce);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, this.m === e2 ? cancelZombieRecompute : GlobalQueue.Ce);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue.Nn(EFFECT_RENDER);
            GlobalQueue.Nn(EFFECT_USER);
          }
          this.stashQueues(e2.yt);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.Ve || this.m.Qt.length > 0;
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
          e2.Me = i.Me;
          e2.A = i.A;
          e2.dn = i.dn;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.Ve) {
            runHeap(dirtyQueue, GlobalQueue.Ce);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.Ce);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.Ve;
      activeLanes.size && GlobalQueue.Nn(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && GlobalQueue.Nn(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
      if (false)
        ;
      if (false)
        ;
    } finally {
      this.cn = false;
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
          let n2 = activeTransition.Ie.get(i2);
          if (!n2)
            activeTransition.Ie.set(i2, n2 = new Set);
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
    if (!e && activeTransition && activeTransition.de === clock)
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
    activeTransition.de = clock;
    const t = this.m;
    if (t !== activeTransition) {
      for (let e2 = 0;e2 < t.Qt.length; e2++) {
        const i = t.Qt[e2];
        i.Ne = activeTransition;
        activeTransition.Qt.push(i);
      }
      for (let e2 = 0;e2 < t.Me.length; e2++) {
        const i = t.Me[e2];
        i.Ne = activeTransition;
        activeTransition.Me.push(i);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e2 of t.dn)
        activeTransition.dn.add(e2);
      if (t.ln.size) {
        for (const e2 of t.ln)
          activeTransition.ln.add(e2);
        t.ln.clear();
      }
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.Ne)
        e2.Ne = activeTransition;
    }
  }
}
function queuePendingNode(e) {
  currentBatch.Qt.push(e);
}
var reaskArmed = false;
function insertSubs(e, t = false) {
  const i = e.Ke || currentOptimisticLane;
  const n = e.xe !== undefined;
  const s = reaskArmed;
  for (let r = e.o;r !== null; r = r.ue) {
    if (s)
      r.fe.se &= ~REACTIVE_REASK;
    if (n && r.fe.T & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.fe.se |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && i) {
      r.fe.se |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.fe, i);
    } else if (t) {
      r.fe.se |= REACTIVE_OPTIMISTIC_DIRTY;
      r.fe.Ke = undefined;
    }
    enqueueSub(r.fe);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.ce) {
    if (e._e !== NOT_PENDING) {
      e.be = e._e;
      e._e = NOT_PENDING;
    }
    if (e.Oe || e.ge)
      GlobalQueue.un(e);
    return;
  }
  if (e._e !== NOT_PENDING) {
    e.be = e._e;
    e._e = NOT_PENDING;
    if (e.Re && e.Re !== EFFECT_TRACKED)
      e.Je = true;
  }
  t.Ee = false;
  t.se &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  if (t.We !== null || t.Qe !== null)
    GlobalQueue.me(t, false, true);
  if (e.Oe || e.ge)
    GlobalQueue.un(e);
}
var storeCommitHook = null;
function commitPendingNodes() {
  const e = currentBatch.Qt;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
  storeCommitHook?.();
}
function finalizePureQueue(e = null, t = false) {
  const i = !t;
  if (i)
    commitPendingNodes();
  if (!t && globalQueue.vt.length)
    checkBoundaryChildren(globalQueue);
  const n = dirtyQueue.EE >= dirtyQueue.Ve;
  if (n)
    runHeap(dirtyQueue, GlobalQueue.Ce);
  if (i) {
    if (n)
      commitPendingNodes();
    const t2 = e ?? globalQueue.m;
    if (t2.Me.length)
      GlobalQueue.En(t2.Me);
    if (t2.ln.size) {
      for (const e2 of t2.ln) {
        if (e2.se & REACTIVE_DISPOSED)
          continue;
        enqueueSub(e2);
      }
      t2.ln.clear();
      schedule();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.vt.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.dn.size)
      GlobalQueue.gt(t2.dn, e);
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.In(e);
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
    e[t].Ne = activeTransition;
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
  if (globalQueue.cn) {
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
  for (let i = e.tt;i; i = i.nt) {
    let e2 = i.it;
    while (e2) {
      if (e2 === t || e2.lt === t)
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
  for (const [i, n] of e.Ie) {
    let s = false;
    for (const e2 of n) {
      if (reporterBlocksSource(e2, i)) {
        s = true;
        break;
      }
      n.delete(e2);
    }
    if (!s)
      e.Ie.delete(i);
    else if (i.S & STATUS_PENDING && i._?.source === i) {
      t = false;
      break;
    }
  }
  if (t && GlobalQueue.Tn?.(e))
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.se & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  if (e.Re === EFFECT_TRACKED) {
    const E2 = e;
    if (!E2.Je) {
      E2.Je = true;
      E2.C.enqueue(EFFECT_USER, E2.ht);
    }
    return;
  }
  const E = queueFor(e);
  if (E.Ve > e.Le)
    E.Ve = e.Le;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.Fe?.Tt ? e.Fe.It?.Le : e.Fe?.Le) ?? -1;
  if (t >= e.Le)
    e.Le = t + 1;
  const n = e.Le;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.ot;
    E2.st = e;
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
  const n = e.Le;
  if (e.ot === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.st;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.ot.st = t2;
    o.ot = e.ot;
  }
  e.ot = e;
  e.st = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.st) {
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
    markNode(E2.fe, REACTIVE_CHECK);
  }
  if (e.u !== null) {
    for (let E2 = e.u;E2 !== null; E2 = E2.ae) {
      for (let e2 = E2.o;e2 !== null; e2 = e2.ue) {
        markNode(e2.fe, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.Ve = 0;e.Ve <= e.EE; e.Ve++) {
    let t = e.eE[e.Ve];
    while (t !== undefined) {
      if (t.se & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.Ve];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.Le;
  for (let E2 = e.tt;E2; E2 = E2.nt) {
    const e2 = E2.it;
    const n = e2.lt || e2;
    if (n.ce && n.Le >= t)
      t = n.Le + 1;
  }
  if (e.Le !== t) {
    e.Le = t;
    for (let E2 = e.o;E2 !== null; E2 = E2.ue) {
      insertIntoHeapHeight(E2.fe, queueFor(E2.fe));
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/owner.js
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
    n = n.He;
  }
}
function disposeChildren(e, n = false, t) {
  const i = e.se;
  if (i & REACTIVE_DISPOSED)
    return;
  if (n) {
    e.se = i | REACTIVE_DISPOSED;
    const n2 = e;
    if (n2.Oe || n2.ge)
      GlobalQueue.un(n2);
  }
  if (n && e.ce)
    e.Te = null;
  let l = t ? e.We : e.ke;
  while (l) {
    const e2 = l.He;
    const n2 = l;
    if (n2.se & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT))
      deleteFromHeap(n2, queueFor(n2));
    if (n2.tt) {
      let e3 = n2.tt;
      do {
        e3 = unlinkSubs(e3);
      } while (e3 !== null);
      n2.tt = null;
      n2.Ye = null;
    }
    disposeChildren(l, true);
    l = e2;
  }
  if (t) {
    e.We = null;
  } else {
    e.ke = null;
    e.qe = 0;
  }
  if (n && !t && !(i & REACTIVE_ZOMBIE) && e.Fe !== null && !(e.Fe.se & REACTIVE_DISPOSED)) {
    const n2 = e.ct;
    const t2 = e.He;
    if (n2 !== null)
      n2.He = t2;
    else
      e.Fe.ke = t2;
    if (t2 !== null)
      t2.ct = n2;
    e.ct = null;
  }
  runDisposal(e, t);
  if (n && e.Nt) {
    const n2 = e.Nt;
    e.Nt = undefined;
    n2();
  }
}
function runDisposal(e, n) {
  let t = n ? e.Qe : e.he;
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
  n ? e.Qe = null : e.he = null;
}
function childId(e, n) {
  let t = e;
  while (t.T & CONFIG_TRANSPARENT && t.Fe)
    t = t.Fe;
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
  if (!context.he)
    context.he = e;
  else if (Array.isArray(context.he))
    context.he.push(e);
  else
    context.he = [context.he, e];
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
    Tt: true,
    It: n?.Tt ? n.It : n,
    ke: null,
    He: null,
    ct: null,
    he: null,
    C: n?.C ?? globalQueue,
    we: n?.we || defaultContext,
    qe: 0,
    Qe: null,
    We: null,
    Fe: n,
    dispose: disposeRootSelf
  };
  if (n) {
    const e2 = n.ke;
    if (e2 === null) {
      n.ke = i;
    } else {
      i.He = e2;
      e2.ct = i;
      n.ke = i;
    }
  }
  return i;
}
function createRoot(e, n) {
  const t = createOwner(n);
  return runWithOwner(t, () => e(() => t.dispose()));
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(l) {
  const n = l.it;
  const e = l.nt;
  const u = l.ue;
  const s = l.ll;
  if (u !== null)
    u.ll = s;
  else
    n.rt = s;
  if (s !== null)
    s.ue = u;
  else {
    n.o = u;
    if (u === null) {
      n.ut?.();
      const l2 = n;
      l2.ce && l2.T & CONFIG_AUTO_DISPOSE && !(l2.se & REACTIVE_ZOMBIE) && !(l2.S & STATUS_PENDING) && unobserved(l2);
    }
  }
  return e;
}
function trimStaleDeps(l) {
  const n = l.Ye;
  let e = n !== null ? n.nt : l.tt;
  if (e !== null) {
    do {
      e = unlinkSubs(e);
    } while (e !== null);
    if (n !== null)
      n.nt = null;
    else
      l.tt = null;
  }
}
function unobserved(l) {
  deleteFromHeap(l, queueFor(l));
  let n = l.tt;
  while (n !== null) {
    n = unlinkSubs(n);
  }
  l.tt = null;
  l.Ye = null;
  disposeChildren(l, true);
}
function link(l, n, e = false) {
  const u = n.Ye;
  if (u !== null && u.it === l) {
    u.ye &&= e;
    return;
  }
  let s = null;
  const t = n.se & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = u !== null ? u.nt : n.tt;
    if (s !== null && s.it === l) {
      s.nl = n.Ze;
      n.Ye = s;
      s.ye = e;
      return;
    }
  }
  const i = l.rt;
  if (i !== null && i.fe === n && (!t || i.nl === n.Ze)) {
    if (t)
      i.ye &&= e;
    else
      i.ye = e;
    return;
  }
  const o = n.Ye = l.rt = {
    it: l,
    fe: n,
    nt: s,
    ll: i,
    ue: null,
    nl: n.Ze,
    ye: e
  };
  if (u !== null)
    u.nt = o;
  else
    n.tt = o;
  if (i !== null)
    i.ue = o;
  else
    l.o = o;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/async.js
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
function parkLoadingWindow(e, n) {
  e.le = true;
  if (n.source)
    addPendingSource(e, n.source);
  if (!(e.S & STATUS_ERROR))
    setPendingError(e, n.source, n);
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
    n(t.fe, t);
  for (let t = e.u ?? null;t !== null; t = t.ae) {
    for (let e2 = t.o;e2 !== null; e2 = e2.ue)
      n(e2.fe, e2);
  }
}
function releaseIfSettledUnobserved(e) {
  e.ce && e.T & CONFIG_AUTO_DISPOSE && !e.o && !(e.se & REACTIVE_ZOMBIE) && !(e.S & STATUS_PENDING) && unobserved(e);
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
function settleErroredDependents(e, n) {
  let t = false;
  const r = new Set;
  const visit = (e2) => {
    if (r.has(e2))
      return;
    r.add(e2);
    if (e2._ === n) {
      enqueueSub(e2);
      t = true;
    }
    forEachDependent(e2, visit);
  };
  forEachDependent(e, visit);
  if (t)
    schedule();
}
function settlePendingSource(e) {
  let n = false;
  let t;
  const r = new Set;
  const o = GlobalQueue.Se;
  const settle = (l) => {
    if (r.has(l) || !removePendingSource(l, e))
      return;
    r.add(l);
    l.de = clock;
    const u = l.oe?.values().next().value;
    const i = l.S & STATUS_ERROR;
    if (u) {
      if (!i)
        setPendingError(l, u);
      o !== null && o(l);
    } else {
      l.S &= ~STATUS_PENDING;
      if (!i)
        setPendingError(l);
      o !== null && o(l);
      if (l.le) {
        enqueueSub(l);
        n = true;
      }
      l.le = false;
      if (!l.o && l.T & CONFIG_AUTO_DISPOSE)
        (t ??= []).push(l);
    }
    forEachDependent(l, settle);
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
    e.Ee = false;
    return n;
  }
  e.Te = n;
  let l;
  const settleTransition = () => {
    const n2 = resolveTransition(e);
    if (n2 && e.S & STATUS_UNINITIALIZED && !currentTransition(n2).Ie.has(e)) {
      e.Ne = null;
      return;
    }
    globalQueue.initTransition(n2);
  };
  const handleError = (t2) => {
    if (e.Te !== n)
      return;
    let r2 = t2 instanceof NotReadyError;
    if (r2 && e.Ee) {
      e.Te = null;
      parkLoadingWindow(e, t2);
      e.de = clock;
      return;
    }
    settleTransition();
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    e.de = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, o2) => {
    if (e.Te !== n)
      return;
    if (e.se & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    settleTransition();
    const l2 = !!(e.S & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const u2 = resolveLane(e);
    if (u2)
      u2.Ae.delete(e);
    if (t) {
      t(r2);
      if (l2)
        clearStatus(e, true);
    } else if (e.De !== undefined) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = r2;
      GlobalQueue.Pe !== null && GlobalQueue.Pe(e, r2);
      if (!hasActiveOverride(e))
        insertSubs(e);
      e.de = clock;
    } else if (u2) {
      const n2 = e.Re;
      const t2 = e.be;
      const o3 = e.Ue;
      try {
        if (!n2 && l2 || !o3 || !o3(r2, t2)) {
          e.be = r2;
          e.de = clock;
          GlobalQueue.Pe !== null && GlobalQueue.Pe(e, r2);
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
    if (e._e === NOT_PENDING)
      e.Ee = false;
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
  const consumeIterator = (t2, r2) => {
    const o2 = t2[Symbol.asyncIterator]();
    let u2 = false;
    let i = false;
    let s = !r2;
    const close = () => {
      if (i)
        return;
      i = true;
      try {
        const e2 = o2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    };
    r2 ? r2(close) : cleanup(close);
    const iterateOrRelease = () => {
      if (!settleAutodispose())
        iterate();
    };
    const iterate = () => {
      let t3, r3, f2 = false, a = false, c = true;
      const S = o2.next();
      const d = isThenable(S) ? S : {
        then: (e2) => void e2(S)
      };
      d.then((r4) => {
        if (c && s) {
          t3 = r4;
          f2 = true;
          if (r4.done)
            i = true;
        } else if (e.Te !== n) {
          return;
        } else if (!r4.done) {
          u2 = true;
          asyncWrite(r4.value, iterateOrRelease);
        } else {
          i = true;
          if (u2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
          settleAutodispose();
        }
      }, (t4) => {
        if (c && s) {
          r3 = t4;
          a = true;
        } else if (e.Te === n) {
          i = true;
          handleError(t4);
          settleAutodispose();
        }
      });
      c = false;
      if (a) {
        i = true;
        handleError(r3);
        if (s)
          throw r3;
        return true;
      }
      if (f2 && !t3.done) {
        l = t3.value;
        u2 = true;
        return iterate();
      }
      return f2 && t3.done;
    };
    const f = iterate();
    s = false;
    return u2 || f;
  };
  let u = null;
  const flattenIfIterable = (e2, n2) => {
    let t2 = false;
    if (typeof e2 === "object" && e2 !== null) {
      untrack(() => {
        t2 = e2[Symbol.asyncIterator];
      });
    }
    if (!t2)
      return false;
    const r2 = consumeIterator(e2, n2);
    if (!n2)
      u = r2;
    return true;
  };
  if (o) {
    let t2 = false, r2 = false, o2, u2 = true;
    const registerDeferredClose = (n2) => {
      if (!e.he)
        e.he = n2;
      else if (Array.isArray(e.he))
        e.he.push(n2);
      else
        e.he = [e.he, n2];
    };
    n.then((r3) => {
      if (u2) {
        l = r3;
        t2 = true;
      } else if (e.Te === n && !(e.se & REACTIVE_DISPOSED) && flattenIfIterable(r3, registerDeferredClose))
        ;
      else {
        asyncWrite(r3);
        settleAutodispose();
      }
    }, (e2) => {
      if (u2) {
        o2 = e2;
        r2 = true;
      } else {
        handleError(e2);
        settleAutodispose();
      }
    });
    u2 = false;
    if (r2) {
      handleError(o2);
      throw o2;
    } else if (!t2) {
      if (e.Ee)
        return e.be;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else if (!flattenIfIterable(l)) {
      e.Ee = false;
    }
  }
  if (r)
    flattenIfIterable(n);
  if (u !== null) {
    if (!u) {
      if (e.Ee)
        return e.be;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.Ee = false;
  }
  return l;
}
function clearStatus(e, n = false) {
  if (e.oe)
    clearPendingSources(e);
  if (e.le)
    e.le = false;
  e.pe = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e._)
    setPendingError(e);
  if (e.Oe || e.ge)
    GlobalQueue.Se(e);
  if (e.u && GlobalQueue.Ge !== null)
    GlobalQueue.Ge(e);
  if (e.i)
    e.i();
}
function notifyStatus(e, n, t, r, o) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const l = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const u = l === e;
  const i = n === STATUS_PENDING && e.De !== undefined && !u;
  const s = i && hasActiveOverride(e);
  if (!r) {
    if (n === STATUS_PENDING && l) {
      addPendingSource(e, l);
      e.S = STATUS_PENDING | e.S & STATUS_UNINITIALIZED;
      setPendingError(e, l, t);
    } else {
      clearPendingSources(e);
      e.S = n | (n !== STATUS_ERROR ? e.S & STATUS_UNINITIALIZED : 0);
      e._ = t;
    }
    GlobalQueue.Se !== null && GlobalQueue.Se(e);
    if (e.u && GlobalQueue.Ge !== null)
      GlobalQueue.Ge(e);
  }
  if (o && !r) {
    assignOrMergeLane(e, o);
  }
  const f = r || s;
  const a = r || i ? undefined : o;
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
    e2.de = clock;
    if (n === STATUS_PENDING && l && !e2.oe?.has(l) || n !== STATUS_PENDING && (e2._ !== t || e2.oe)) {
      if (r2.ye && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f && !e2.Ne)
        queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.Ce = recompute;
GlobalQueue.me = disposeChildren;
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
    if (e.ve)
      return true;
    e = e.Fe;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.Re;
  if (!t) {
    if (e.Ne && (!n || activeTransition) && activeTransition !== e.Ne)
      globalQueue.initTransition(e.Ne);
    deleteFromHeap(e, queueFor(e));
    e.Te = null;
    if (e.Ne || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ke !== null || e.he !== null) {
      markDisposal(e);
      e.Qe = e.he;
      e.We = e.ke;
      e.he = null;
      e.ke = null;
      e.qe = 0;
    }
  }
  let i = !!(e.se & REACTIVE_OPTIMISTIC_DIRTY);
  const l = e.De !== undefined && e.De !== NOT_PENDING;
  const u = !!(e.S & STATUS_UNINITIALIZED);
  const s = e.S & STATUS_ERROR ? e._ : undefined;
  const o = (e.se & REACTIVE_REASK) !== 0;
  const a = e.Ee;
  const r = context;
  context = e;
  e.Ye = null;
  e.Ze++;
  e.se = REACTIVE_RECOMPUTING_DEPS;
  e.de = clock;
  let c = e._e === NOT_PENDING ? e.be : e._e;
  let _ = e.Le;
  let f = tracking;
  let E = currentOptimisticLane;
  tracking = true;
  const N = latestReadActive;
  latestReadActive = false;
  if (i) {
    const t2 = GlobalQueue.je(e, true);
    if (t2)
      currentOptimisticLane = t2;
    else if (t2 === false)
      i = false;
  } else if (activeTransition && !t && activeTransition.Me.length) {
    const t2 = GlobalQueue.je(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const T = n && n !== EFFECT_USER;
  const I = stale;
  if (T)
    stale = true;
  try {
    if (e.T & CONFIG_SYNC) {
      c = e.ce(c);
      e.Te = null;
      e.Ee = false;
    } else {
      const t2 = e.Te;
      const n2 = e.ce(c);
      const i2 = typeof n2 === "object" && n2 !== null;
      const l2 = e.Te !== t2;
      c = l2 || !i2 ? n2 : handleAsync(e, n2);
      if (!l2 && !i2) {
        e.Te = null;
        e.Ee = false;
      }
    }
    if (e.S !== 0 || e.i !== undefined || e._ || e.pe || e.le || e.oe !== undefined || e.Oe !== undefined || e.ge !== undefined || e.u !== null)
      clearStatus(e, t);
    if (e.Ke)
      GlobalQueue.Be(e);
  } catch (t2) {
    const n2 = t2 instanceof NotReadyError;
    if (n2 && e.Ee) {
      parkLoadingWindow(e, t2);
    } else {
      if (n2 && currentOptimisticLane)
        GlobalQueue.ze(e);
      let i2 = false;
      if (n2) {
        e.le = true;
        if (GlobalQueue.$e !== null)
          i2 = GlobalQueue.$e(e, o);
      }
      notifyStatus(e, n2 ? STATUS_PENDING : STATUS_ERROR, t2, undefined, n2 ? e.Ke : undefined);
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = f;
    latestReadActive = N;
    if (T)
      stale = I;
    e.se = REACTIVE_NONE | (t ? e.se & REACTIVE_SNAPSHOT_STALE : 0);
    context = r;
  }
  if (!e._) {
    trimStaleDeps(e);
    const o2 = l ? unwrapOverride(e.De) : e._e === NOT_PENDING ? e.be : e._e;
    let r2 = false;
    try {
      r2 = !n && u || !e.Ue || !e.Ue(o2, c);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && r2) {
      e.Je = !e._;
      if (!t)
        e.C.enqueue(n, e.Xe ??= GlobalQueue.et.bind(null, e));
    }
    if (e._)
      ;
    else if (r2) {
      const u2 = l ? e.De : undefined;
      if (t || n && (activeTransition !== e.Ne || activeTransition === null) || i) {
        e.be = c;
        if (l && i) {
          e.De = c === undefined ? OVERRIDE_UNDEFINED : c;
          e._e = NOT_PENDING;
        }
      } else {
        e._e = c;
        if (a)
          e.Ee = true;
        if ((activeTransition || e.Ne) && GlobalQueue.Pe !== null)
          GlobalQueue.Pe(e, c);
      }
      if (e.o !== null && (!l || i || e.De !== u2))
        insertSubs(e, i || l);
    } else if (l) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = c;
      if (a)
        e.Ee = true;
    } else if (e.Le != _) {
      for (let t2 = e.o;t2 !== null; t2 = t2.ue) {
        insertIntoHeapHeight(t2.fe, queueFor(t2.fe));
      }
    }
    if (s !== undefined && !r2 && !e._)
      settleErroredDependents(e, s);
  }
  currentOptimisticLane = E;
  const d = e._e !== NOT_PENDING || e.We !== null || e.Qe !== null || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  d && (!t || e.S & STATUS_PENDING) && (!e.Ne || l) && queuePendingNode(e);
  e.Ne && n && activeTransition !== e.Ne && runInTransition(e.Ne, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.se & REACTIVE_CHECK) {
    for (let t = e.tt;t; t = t.nt) {
      const n = t.it;
      const i = n.lt || n;
      if (i.ce) {
        updateIfNecessary(i);
      }
      if (e.se & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.se & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e._ && e.de < clock && !e.Te) {
    recompute(e);
  }
  e.se = e.se & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = t !== null && typeof t === "object" && "loadingValue" in t;
  const l = {
    id: inheritId(t, n, context),
    T: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.V ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ue: t?.equals != null ? t.equals : isEqual,
    ut: t?.unobserved,
    he: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ce: e,
    be: i ? t.loadingValue : undefined,
    Le: 0,
    u: null,
    st: undefined,
    ot: null,
    tt: null,
    Ye: null,
    Ze: 0,
    o: null,
    rt: null,
    Fe: context,
    He: null,
    ct: null,
    ke: null,
    se: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: i ? 0 : STATUS_UNINITIALIZED,
    de: clock,
    _e: NOT_PENDING,
    Qe: null,
    We: null,
    Te: null,
    Ne: null,
    pe: false,
    Ee: i
  };
  setupComputedNode(l, t);
  return l;
}
function createEffectNode(e, t, n, i, l, u) {
  const s = u?.transparent ?? false;
  const o = {
    id: inheritId(u, s, context),
    T: (s ? CONFIG_TRANSPARENT : 0) | (u?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (u?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ue: false,
    ut: u?.unobserved,
    he: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ce: e,
    be: undefined,
    Le: 0,
    u: null,
    st: undefined,
    ot: null,
    tt: null,
    Ye: null,
    Ze: 0,
    o: null,
    rt: null,
    Fe: context,
    He: null,
    ct: null,
    ke: null,
    se: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    de: clock,
    _e: NOT_PENDING,
    Qe: null,
    We: null,
    Te: null,
    Ne: null,
    pe: false,
    Ee: false,
    Je: false,
    _t: undefined,
    ft: t,
    Et: n,
    Nt: undefined,
    Re: i,
    i: l
  };
  setupComputedNode(o, lazyOptions);
  return o;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.ot = e;
  const n = context?.Tt ? context.It : context;
  if (context) {
    const t2 = context.ke;
    if (t2 === null) {
      context.ke = e;
    } else {
      e.He = t2;
      t2.ct = e;
      context.ke = e;
    }
  }
  if (n)
    e.Le = n.Le + 1;
  if (GlobalQueue.dt !== null)
    GlobalQueue.dt(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      e.xe = e.be === undefined ? NO_SNAPSHOT : e.be;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    Ue: t?.equals != null ? t.equals : isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.V ? CONFIG_NO_SNAPSHOT : 0),
    ut: t?.unobserved,
    be: e,
    o: null,
    rt: null,
    de: clock,
    lt: n,
    ae: n?.u || null,
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
  if (GlobalQueue.St === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.St !== null)
      return GlobalQueue.St(e);
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
    return GlobalQueue.At(e);
  let t = context;
  if (t?.Tt)
    t = t.It;
  const n = e;
  const i = e.lt;
  const l = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Ct(e, t, l, i);
  } else if (typeof n.ce === "function") {
    prepareComputed(e, false);
  }
  if (!n.ce && l === e && e.De === undefined && e.xe === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e._e === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN ? e.be : e._e;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (l.ce) {
      const n2 = queueFor(e);
      if (l.Le >= n2.Ve) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(l);
      }
      const i2 = l.Le;
      if (i2 >= t.Le && e.Fe !== t) {
        t.Le = i2 + 1;
      }
    }
  }
  if (l.S & STATUS_PENDING) {
    if (t && !(stale && l.Ne && activeTransition !== l.Ne)) {
      if (currentOptimisticLane === null || GlobalQueue.Ot(l)) {
        if (!tracking && e !== t)
          link(e, t);
        throw l._;
      }
    } else if (t && l !== e && l.S & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw l._;
    } else if (!t && l.S & STATUS_UNINITIALIZED) {
      throw l._;
    }
  }
  if (l.ce && l.S & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && l.de < clock) {
      recompute(l);
      return read(e);
    } else
      throw l._;
  }
  if (snapshotCaptureActive && t && t.T & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.xe;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const l2 = e._e !== NOT_PENDING ? e._e : e.be;
      if (l2 !== i2)
        t.se |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.De !== undefined && e.De !== NOT_PENDING) {
    return unwrapOverride(e.De);
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.Rt(e, l, t)) {
    return e.be;
  }
  const u = !t || currentOptimisticLane !== null && GlobalQueue.Dt(e, l, t) || e._e === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && e.Ne && activeTransition !== e.Ne ? e.be : e._e;
  if (pendingCheckActive)
    GlobalQueue.Pt(e, u);
  if (!t && l === e && typeof n.ce === "function" && e.T & CONFIG_AUTO_DISPOSE && !(l.S & STATUS_PENDING) && !e.o) {
    unobserved(e);
  }
  return u;
}
function setSignal(e, t) {
  if (e.Ne && activeTransition !== e.Ne)
    globalQueue.initTransition(e.Ne);
  if (e.De !== undefined && !projectionWriteActive)
    return GlobalQueue.Gt(e, t);
  const n = e._e === NOT_PENDING ? e.be : e._e;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.Ue || !e.Ue(n, t);
  if (!i)
    return t;
  if (e._e === NOT_PENDING)
    queuePendingNode(e);
  e._e = t;
  (e.Oe !== undefined || e.ge !== undefined) && GlobalQueue.Pe !== null && GlobalQueue.Pe(e, t);
  e.de = clock;
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/context.js
function createContext(e, t) {
  return {
    id: Symbol(t),
    defaultValue: e
  };
}
function getContext(e, t = getOwner()) {
  if (!t) {
    throw new NoOwnerError;
  }
  const n = hasContext(e, t) ? t.we[e.id] : e.defaultValue;
  if (isUndefined(n)) {
    throw new ContextNotFoundError;
  }
  return n;
}
function setContext(e, t, n = getOwner()) {
  if (!n) {
    throw new NoOwnerError;
  }
  n.we = {
    ...n.we,
    [e.id]: isUndefined(t) ? e.defaultValue : t
  };
}
function hasContext(e, t) {
  return !isUndefined(t?.we[e.id]);
}
function isUndefined(e) {
  return typeof e === "undefined";
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, E, e, R) {
  const r = !!R?.user;
  const f = createEffectNode(t, E, e, r ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, R);
  recompute(f, true);
  !R?.defer && (f.Re === EFFECT_USER || R?.schedule ? f.C.enqueue(f.Re, runEffect.bind(null, f)) : runEffect(f));
}
function notifyEffectStatus(t, E) {
  const e = t !== undefined ? t : this.S;
  const R = E !== undefined ? E : this._;
  if (e & STATUS_ERROR) {
    this.C.notify(this, STATUS_PENDING, 0);
    if (this.Re === EFFECT_USER) {
      if (this.S & STATUS_ERROR) {
        this.Je = true;
        this.C.enqueue(this.Re, this.Xe ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.C.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(R));
      throw R;
    }
  } else if (this.Re === EFFECT_RENDER) {
    this.C.notify(this, STATUS_PENDING | STATUS_ERROR, e, R);
  }
}
function runEffect(t) {
  if (!t.Je || t.se & REACTIVE_DISPOSED)
    return;
  if (t.S & STATUS_ERROR && t.Re === EFFECT_USER) {
    const E2 = unwrapStatusError(t._);
    t._t = t.be;
    t.Je = false;
    try {
      t.Et ? t.Et(E2, () => {
        const E3 = t.Nt;
        t.Nt = undefined;
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
  const E = t.Nt;
  t.Nt = undefined;
  try {
    E?.();
    const e = t.ft(t.be, t._t);
    if (false)
      ;
    t.Nt = e;
  } catch (E2) {
    t._ = new StatusError(t, E2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(E2);
      throw E2;
    }
  } finally {
    t._t = t.be;
    t.Je = false;
  }
}
GlobalQueue.et = runEffect;
function trackedEffect(t, E) {
  const run = () => {
    if (!e.Je || e.se & REACTIVE_DISPOSED)
      return;
    try {
      e.Je = false;
      recompute(e);
    } finally {}
  };
  const e = computed(() => {
    const E2 = e.Nt;
    e.Nt = undefined;
    E2?.();
    const R = staleValues(t);
    e.Nt = R;
  }, {
    ...E,
    lazy: true
  });
  e.Nt = undefined;
  e.T = e.T & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  e.Je = true;
  e.Re = EFFECT_TRACKED;
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
  e.ht = run;
  e.C.enqueue(EFFECT_USER, run);
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/signals.js
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $AFFECTS = Symbol(0);
var rawValues = new WeakSet;
var OBJECT_PROTO = Object.prototype;
var wrappableProtos = new WeakMap;
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
var affectsScopes = new Map;

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/boundaries.js
function boundaryComputed(e, t) {
  const r = computed(e, {
    lazy: true
  });
  r.i = (e2, t2) => {
    const n = e2 !== undefined ? e2 : r.S;
    const s = t2 !== undefined ? t2 : r._;
    r.S &= ~r.R;
    const i = r.C.notify(r, STATUS_PENDING | STATUS_ERROR, n, s);
    const o = n & ~r.R & (STATUS_PENDING | STATUS_ERROR);
    if (o) {
      r.S &= ~o;
      if (r._ === s && !(r.S & (STATUS_PENDING | STATUS_ERROR)))
        r._ = undefined;
    }
    if (!i && n & STATUS_ERROR) {
      haltReactivity(unwrapStatusError(s));
      throw s;
    }
  };
  r.R = t;
  r.T &= ~CONFIG_AUTO_DISPOSE;
  recompute(r, true);
  return r;
}
function createBoundChildren(e, t, r, n) {
  const s = e.C;
  s.addChild(e.C = r);
  cleanup(() => s.removeChild(e.C));
  return runWithOwner(e, () => {
    const e2 = computed(t);
    return boundaryComputed(() => flatten(read(e2)), n);
  });
}
var ON_INIT = Symbol();
var RevealControllerContext = /* @__PURE__ */ createContext(null);
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
function createCollectionBoundary(e, t, r, n) {
  const s = createOwner();
  if (_revealUsed)
    setContext(RevealControllerContext, null, s);
  const i = new CollectionQueue(e);
  if (e === STATUS_ERROR)
    i._ = signal(undefined, {
      ownedWrite: true,
      V: true
    });
  if (n)
    i.te = n;
  const o = i.ee = createBoundChildren(s, t, i, e);
  untrack(() => {
    let t2 = false;
    try {
      read(o);
    } catch (e2) {
      if (e2 instanceof NotReadyError)
        t2 = true;
      else
        throw e2;
    }
    i.N = t2 || !!(o.S & e) || o._ instanceof NotReadyError;
  });
  const l = _revealUsed && e === STATUS_PENDING ? getContext(RevealControllerContext) : null;
  if (l) {
    i.B = l;
    l.Y(i);
    cleanup(() => l.Z(i));
  }
  return accessor(computed(() => {
    if (!read(i.I)) {
      const e2 = read(o);
      if (!untrack(() => read(i.I)))
        return i.W = true, e2;
    }
    if (_revealUsed && read(i.D))
      return;
    return r(i);
  }, {
    V: true
  }));
}
function createErrorBoundary(e, t) {
  return createCollectionBoundary(STATUS_ERROR, e, (e2) => t(accessor(e2._), () => {
    for (const t2 of e2.v)
      recompute(t2);
    schedule();
  }));
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.1/node_modules/@solidjs/signals/dist/prod/store/utils.js
function trueFn() {
  return true;
}
var propTraps = {
  get(e, r, t) {
    if (r === $PROXY)
      return t;
    return e.get(r);
  },
  has(e, r) {
    if (r === $PROXY)
      return true;
    return e.has(r);
  },
  set: trueFn,
  deleteProperty: trueFn,
  getOwnPropertyDescriptor(e, r) {
    return {
      configurable: true,
      enumerable: true,
      get() {
        return e.get(r);
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
  let r = false;
  const t = [];
  for (let n2 = 0;n2 < e.length; n2++) {
    const o2 = e[n2];
    r = r || !!o2 && $PROXY in o2;
    const s2 = !!o2 && o2[$SOURCES];
    if (s2) {
      for (let e2 = 0;e2 < s2.length; e2++)
        t.push(s2[e2]);
    } else
      t.push(typeof o2 === "function" ? (r = true, createMemo(o2)) : o2);
  }
  if (SUPPORTS_PROXY && r) {
    return new Proxy({
      get(e2) {
        if (e2 === $SOURCES)
          return t;
        for (let r2 = t.length - 1;r2 >= 0; r2--) {
          const n2 = resolveSource(t[r2]);
          if (e2 in n2)
            return n2[e2];
        }
      },
      has(e2) {
        for (let r2 = t.length - 1;r2 >= 0; r2--) {
          if (e2 in resolveSource(t[r2]))
            return true;
        }
        return false;
      },
      keys() {
        const e2 = new Set;
        for (let r2 = 0;r2 < t.length; r2++) {
          const n2 = ownEnumerableKeys(resolveSource(t[r2]));
          for (let r3 = 0;r3 < n2.length; r3++)
            e2.add(n2[r3]);
        }
        return [...e2];
      }
    }, propTraps);
  }
  const n = Object.create(null);
  let o = false;
  let s = t.length - 1;
  for (let e2 = s;e2 >= 0; e2--) {
    const r2 = t[e2];
    if (!r2) {
      e2 === s && s--;
      continue;
    }
    const u2 = Object.getOwnPropertyNames(r2);
    for (let t2 = u2.length - 1;t2 >= 0; t2--) {
      const c2 = u2[t2];
      if (c2 === "__proto__" || c2 === "constructor")
        continue;
      if (!n[c2]) {
        o = o || e2 !== s;
        const t3 = Object.getOwnPropertyDescriptor(r2, c2);
        n[c2] = t3.get ? {
          enumerable: true,
          configurable: true,
          get: t3.get.bind(r2)
        } : t3;
      }
    }
  }
  if (!o)
    return t[s];
  const u = {};
  const c = Object.keys(n);
  for (let e2 = c.length - 1;e2 >= 0; e2--) {
    const r2 = c[e2], t2 = n[r2];
    if (t2.get)
      Object.defineProperty(u, r2, t2);
    else
      u[r2] = t2.value;
  }
  u[$SOURCES] = t;
  return u;
}
// ../../node_modules/.bun/solid-js@2.0.0-rc.1/node_modules/solid-js/dist/solid.js
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createMemo;
var _createErrorBoundary;
var _createRenderEffect;
var LIVE_SOURCE = Symbol.for("solid.LiveSource");
var createMemo2 = (...args) => {
  return (_createMemo || createMemo)(...args);
};
var createErrorBoundary2 = (...args) => (_createErrorBoundary || createErrorBoundary)(...args);
var createRenderEffect2 = (...args) => (_createRenderEffect || createRenderEffect)(...args);
var _fragments = new Map;
var _truncated = new Set;
var _revealSubs = new Set;
var _truncationRejectors = new Map;
function createComponent(Comp, props) {
  return untrack(() => Comp(props || {}));
}

// ../../node_modules/.bun/@solidjs+universal@2.0.0-rc.1+8dd5f48cc8d92621/node_modules/@solidjs/universal/dist/universal.js
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

// ../../packages/core/src/renderer.ts
import * as tree2 from "flux:rendertree";

// ../../packages/core/src/window.ts
import { requestFrame, setPointerLock } from "flux:rendertree";
import { renderFrame } from "srt:render";
import { on as on2, once } from "srt:events";
import { exit } from "srt:app";

// ../../packages/core/src/core.ts
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

// ../../packages/core/src/window.ts
var animationFrames = new Map;
var refreshRate = 60;
var backHandlers = [];
var windowRootId = 0;
function setWindowRoot(nodeId) {
  windowRootId = nodeId;
  setInterestRoot(nodeId);
}
function attachWindow(nodeId) {
  setWindowRoot(nodeId);
  let unsubscribe = null;
  let unsubDown = null;
  let unsubUp = null;
  let unsubMove = null;
  let unsubEnter = null;
  let unsubLeave = null;
  let unsubWheel = null;
  let unsubTransitionEnd = null;
  let unsubKeyDown = null;
  let unsubKeyUp = null;
  let unsubBack = null;
  let unsubTextInput = null;
  let unsubKeyboardVisibility = null;
  let unsubRefreshRate = null;
  let unsubFirstResize = null;
  function runFrame(t, frame, bootstrap = false) {
    if (!bootstrap && animationFrames.size > 0) {
      let frames = animationFrames;
      animationFrames = new Map;
      for (let fn of frames.values())
        fn(t, frame, refreshRate);
    }
    try {
      flush();
    } catch (err) {
      console.error("Error in reactive flush:", err);
    }
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
        try {
          getEventHandler(targets[i], handler)?.(e);
        } catch (err) {
          console.error(`Error in ${handler} handler:`, err);
        }
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
    unsubTransitionEnd = on2("transitionEnd", (raw) => {
      try {
        getEventHandler(raw.target, "onTransitionEnd")?.({
          property: raw.property
        });
      } catch (err) {
        console.error("Error in onTransitionEnd handler:", err);
      }
    });
    let dispatchKey = (raw, handler) => {
      let target = focusedNode() ?? windowRootId;
      let stopped = false;
      let e = {
        ...raw,
        target,
        stopPropagation: () => stopped = true
      };
      let path = getNodePath(target);
      if (path[path.length - 1] !== windowRootId)
        path.push(windowRootId);
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
      queueMicrotask(() => runFrame(0, 0, true));
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
    if (unsubTransitionEnd)
      unsubTransitionEnd();
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

// ../../packages/core/src/renderer.ts
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
  let cleanup2 = (n) => {
    for (let child of n.children)
      if (child.parent === n)
        cleanup2(child);
    if (n.id === focusedNode())
      setFocus(null);
    nodes.delete(n.id);
    cleanupNode(n.id);
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
  let list = fresh.map(([type, n]) => `<${type}> x${n}`).join(", ");
  console.warn(`Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` + `The usual cause is reading an element-valued prop more than once (every read ` + `builds a new subtree); read it once where it mounts, or resolve it with ` + `children(). If these nodes are intentionally kept for later mounting, ignore ` + `this. Element types already reported are not reported again.`);
}
var warnedRejectedProps = new Set;
function setTreeProperty(node, name, value) {
  try {
    tree2.setProperty(node.id, name, value);
  } catch (e) {
    let message = String(e);
    if (!message.includes("Unknown property") && !message.includes("Detached-only"))
      throw e;
    let key = node.elementType + "." + name;
    if (warnedRejectedProps.has(key))
      return;
    warnedRejectedProps.add(key);
    let stack = new Error().stack ?? "";
    console.warn(`Ignoring property '${name}' on <${node.elementType}>: ${message}
${stack}`);
  }
}
var ROUTE_TREE = 0;
var ROUTE_EVENT = 1;
var ROUTE_FOCUSABLE = 2;
var ROUTE_HINTS = 3;
var propRoutes = new Map;
function routeFor(name) {
  let route = propRoutes.get(name);
  if (route === undefined) {
    route = /^on[A-Z]/.test(name) ? ROUTE_EVENT : name === "focusable" ? ROUTE_FOCUSABLE : name === "textInputHints" ? ROUTE_HINTS : ROUTE_TREE;
    propRoutes.set(name, route);
  }
  return route;
}
function applyProp(node, name, value) {
  if (!node)
    return;
  switch (routeFor(name)) {
    case ROUTE_EVENT:
      if (value == null || typeof value === "function") {
        setEventHandler(node.id, name, value);
        return;
      }
      break;
    case ROUTE_FOCUSABLE:
      setFocusable(node.id, value === true);
      return;
    case ROUTE_HINTS:
      setTextInputHints(node.id, value);
      return;
  }
  setTreeProperty(node, name, value);
}
var renderer = createRenderer({
  createElement: (elementType, props) => {
    let proxy = createProxyNode(elementType);
    if (elementType === "window")
      tree2.createRoot(proxy.id);
    else
      tree2.createNode(proxy.id, elementType);
    if (props) {
      for (let name in props) {
        applyProp(proxy, name, props[name]);
      }
    }
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode("#text");
    tree2.createNode(proxy.id, "#text");
    tree2.setProperty(proxy.id, "text", "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    tree2.setProperty(node.id, "text", "" + value);
  },
  isTextNode: (node) => node?.elementType === "#text",
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
var {
  memo: memo2,
  createComponent: createComponent2,
  createElement,
  createTextNode,
  insertNode: insertNode2,
  spread,
  setProp,
  mergeProps,
  applyRef,
  ref
} = renderer;
var {
  effect: rawEffect,
  insert: rawInsert
} = renderer;
var SKIP = Symbol("skip");
var windowRoot;
var rendered = false;
var errorWindows = new Set;
function render(code) {
  if (rendered) {
    throw new Error("render() already called; an app has exactly one render()");
  }
  rendered = true;
  createRoot(() => {
    let root = createErrorBoundary2(() => {
      let win = code();
      if (!win || win.elementType !== "window") {
        throw new Error("render() root must be a <window> element");
      }
      return win;
    }, (error, reset) => {
      let err = error();
      console.error("Uncaught error: the app is replaced by the error window until reset or reload.", err);
      let win = errorWindow(err, reset);
      errorWindows.add(win.id);
      return win;
    });
    rawEffect(() => root(), (win, prev) => swapRoot(win, prev));
  });
}
function swapRoot(win, prev) {
  windowRoot = win;
  if (prev === undefined) {
    attachWindow(win.id);
    return;
  }
  if (!errorWindows.has(win.id))
    tree2.setRoot(win.id);
  setWindowRoot(win.id);
  setFocus(null);
  if (errorWindows.has(prev.id) || !errorWindows.has(win.id)) {
    errorWindows.delete(prev.id);
    destroyNode2(prev);
  }
}
function errorWindow(err, reset) {
  let message = err instanceof Error ? err.message : String(err);
  let stack = err instanceof Error && err.stack ? err.stack : "";
  let text = (content, props) => {
    let node = createElement("text", props);
    insertNode2(node, createTextNode(content));
    return node;
  };
  let win = createElement("window", {
    title: "Application error"
  });
  insertNode2(win, createElement("d-rect", {
    color: "#1144bb"
  }));
  let column = createElement("view", {
    flexGrow: 1,
    flexDirection: "column",
    padding: 40,
    gap: 12
  });
  insertNode2(column, text(":(", {
    color: "white",
    fontSize: 64,
    fontWeight: 700
  }));
  insertNode2(column, text("Something went wrong", {
    color: "white",
    fontSize: 22
  }));
  insertNode2(column, text(message, {
    color: "white",
    fontSize: 16
  }));
  if (stack)
    insertNode2(column, text(stack, {
      color: "#aac2ff",
      fontSize: 12,
      fontFamily: "mono"
    }));
  insertNode2(column, text("Fix the error and save to reload, or reset to retry the failed computations.", {
    color: "#aac2ff",
    fontSize: 14
  }));
  let button = createElement("view", {
    alignSelf: "flex-start",
    padding: 12,
    onPointerDown: () => reset()
  });
  insertNode2(button, createElement("d-rect", {
    color: "white",
    radius: 6
  }));
  insertNode2(button, text("Reset", {
    color: "#1144bb",
    fontSize: 16,
    fontWeight: 600
  }));
  insertNode2(column, button);
  insertNode2(win, column);
  return win;
}
// ../../packages/core/src/color.ts
import * as tree3 from "flux:rendertree";
// ../../packages/core/src/environment.ts
import { on as on3 } from "srt:events";
// ../../packages/core/src/gamepad.ts
import { on as on4 } from "srt:events";
// ../../packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { depthTexture, destroyTexture as destroyTexture2, endBufferWrite, resizeTexture, setTargetParams as setTargetParams2, setTargetRect, setTargetSize as setTargetSize2, setTargetTextures, uploadTexture } from "flux:gpu";
import { copyTexture, destroyBuffer as destroyBuffer2, renderTarget, setDraw } from "flux:gpu";
import { addDraw, removeDraw, setDrawBuffers, setDrawOrder, setDrawParams, setDrawRange, setDrawTextures } from "flux:gpu";
import { limits } from "flux:gpu";
import { compileShader, createRenderPipeline, destroyProgram, destroyRenderPipeline, destroyShader, linkProgram, programAttributes } from "flux:gpu";
import { captureSnapshot, readTexture } from "flux:gpu";
var glsl = String.raw;
// ../../packages/core/src/image.ts
import { decodeImage } from "flux:image";
import { decodeImage as decodeImage2, encodeImage } from "flux:image";
var imageCache = new Map;
// ../../packages/core/src/svg.ts
import { parseSvg as fluxParseSvg } from "flux:svg";
var svg = String.raw;
// ../../packages/core/src/logo.tsx
var SEGMENTS = [{
  base: 0,
  light: "#3f5494",
  dark: "#162b6c",
  d: "M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z"
}, {
  base: 90,
  light: "#547ebf",
  dark: "#2b5696",
  d: "M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z"
}, {
  base: 180,
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z"
}, {
  base: 270,
  light: "#547ebf",
  dark: "#2b5696",
  d: "M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z"
}, {
  base: 360,
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z"
}, {
  base: 450,
  light: "#3f5494",
  dark: "#162b6c",
  d: "M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z"
}, {
  base: 540,
  light: "#7ea9ea",
  dark: "#5681c1",
  d: "M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z"
}];
var FADE = 360;
var LAST = SEGMENTS[SEGMENTS.length - 1].base;
var IN_DONE = LAST + FADE;
var CYCLE = IN_DONE + LAST + FADE;
// ../../packages/core/src/arena.ts
var claims = new Map;
// ../../packages/core/src/transform.ts
import { on as on5 } from "srt:events";
// src/bsod.tsx
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
