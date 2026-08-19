// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/error.js
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/constants.js
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/lanes.js
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
  return !!(n._e !== undefined && n._e !== NOT_PENDING);
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
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
function registerTransientStoreNode(e) {
  transientStoreNodes.add(e);
}
function canUseSimpleSyncFlush(e) {
  const t = e.m;
  return transitions.size === 0 && activeLanes.size === 0 && e.vt.length === 0 && t.Me.length === 0 && t.A.length === 0 && t.cn.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.o !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.De !== NOT_PENDING)
      continue;
    if (e._e !== undefined && e._e !== NOT_PENDING)
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
    de: clock,
    Qt: [],
    Ie: new Map,
    Me: [],
    A: [],
    cn: new Set,
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
  for (const i of t.cn)
    e.cn.add(i);
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
  if (!syncDepth && !globalQueue.gt && !projectionWriteActive)
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
  ke = null;
  bt = [[], []];
  vt = [];
  kt = 0;
  created = clock;
  addChild(e) {
    this.vt.push(e);
    e.ke = this;
  }
  removeChild(e) {
    const t = this.vt.indexOf(e);
    if (t >= 0) {
      this.vt.splice(t, 1);
      e.ke = null;
    }
  }
  notify(e, t, i, n) {
    if (this.ke)
      return this.ke.notify(e, t, i, n);
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
  gt = false;
  m = createBatch();
  static Ce;
  static me;
  static et;
  static Dt = null;
  static p = null;
  static G = null;
  static M = null;
  static h = null;
  static It = null;
  static St = null;
  static Pe = null;
  static Se = null;
  static Oe = null;
  static un = null;
  static At = null;
  static Ct = null;
  static ht = null;
  static Be = null;
  static k = null;
  static Lt = null;
  static Gt = null;
  static En = null;
  static Tn = null;
  static dn = null;
  static In = null;
  static Rt = null;
  static Ot = null;
  static Pt = null;
  static je = null;
  static $e = null;
  static ze = null;
  static Nn = null;
  flush() {
    if (this.gt)
      return;
    this.gt = true;
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
            GlobalQueue.In(EFFECT_RENDER);
            GlobalQueue.In(EFFECT_USER);
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
          e2.cn = i.cn;
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
      this.gt = false;
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
      for (const e2 of t.cn)
        activeTransition.cn.add(e2);
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
  const n = e.Le !== undefined;
  const s = reaskArmed;
  for (let r = e.o;r !== null; r = r.le) {
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
    if (e.De !== NOT_PENDING) {
      e.Ue = e.De;
      e.De = NOT_PENDING;
    }
    if (e.ge || e.pe)
      GlobalQueue.un(e);
    return;
  }
  if (e.De !== NOT_PENDING) {
    e.Ue = e.De;
    e.De = NOT_PENDING;
    if (e.Re && e.Re !== EFFECT_TRACKED)
      e.Je = true;
  }
  t.Ee = false;
  t.se &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  if (t.We !== null || t.Qe !== null)
    GlobalQueue.me(t, false, true);
  if (e.ge || e.pe)
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
    if (t2.cn.size)
      GlobalQueue.Dt(t2.cn, e);
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.dn(e);
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
  if (globalQueue.gt) {
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.se & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  if (e.Re === EFFECT_TRACKED) {
    const E2 = e;
    if (!E2.Je) {
      E2.Je = true;
      E2.C.enqueue(EFFECT_USER, E2.Ft);
    }
    return;
  }
  const E = queueFor(e);
  if (E.Ve > e.xe)
    E.Ve = e.xe;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.ke?.Nt ? e.ke.dt?.xe : e.ke?.xe) ?? -1;
  if (t >= e.xe)
    e.xe = t + 1;
  const n = e.xe;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.st;
    E2.ot = e;
    e.st = E2;
    I.st = e;
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
  const n = e.xe;
  if (e.st === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.ot;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.st.ot = t2;
    o.st = e.st;
  }
  e.st = e;
  e.ot = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.ot) {
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
  for (let E2 = e.o;E2 !== null; E2 = E2.le) {
    markNode(E2.fe, REACTIVE_CHECK);
  }
  if (e.u !== null) {
    for (let E2 = e.u;E2 !== null; E2 = E2.ae) {
      for (let e2 = E2.o;e2 !== null; e2 = e2.le) {
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
  let t = e.xe;
  for (let E2 = e.tt;E2; E2 = E2.nt) {
    const e2 = E2.it;
    const n = e2.lt || e2;
    if (n.ce && n.xe >= t)
      t = n.xe + 1;
  }
  if (e.xe !== t) {
    e.xe = t;
    for (let E2 = e.o;E2 !== null; E2 = E2.le) {
      insertIntoHeapHeight(E2.fe, queueFor(E2.fe));
    }
  }
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/owner.js
var PENDING_OWNER = {};
function markDisposal(e) {
  let n = e.Fe;
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
    if (n2.ge || n2.pe)
      GlobalQueue.un(n2);
  }
  if (n && e.ce)
    e.Te = null;
  let l = t ? e.We : e.Fe;
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
    e.Fe = null;
    e.qe = 0;
  }
  if (n && !t && !(i & REACTIVE_ZOMBIE) && e.ke !== null && !(e.ke.se & REACTIVE_DISPOSED)) {
    const n2 = e.ct;
    const t2 = e.He;
    if (n2 !== null)
      n2.He = t2;
    else
      e.ke.Fe = t2;
    if (t2 !== null)
      t2.ct = n2;
    e.ct = null;
  }
  runDisposal(e, t);
  if (n && e.Tt) {
    const n2 = e.Tt;
    e.Tt = undefined;
    n2();
  }
}
function runDisposal(e, n) {
  let t = n ? e.Qe : e.ye;
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
  n ? e.Qe = null : e.ye = null;
}
function childId(e, n) {
  let t = e;
  while (t.T & CONFIG_TRANSPARENT && t.ke)
    t = t.ke;
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
  if (!context.ye)
    context.ye = e;
  else if (Array.isArray(context.ye))
    context.ye.push(e);
  else
    context.ye = [context.ye, e];
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
    dt: n?.Nt ? n.dt : n,
    Fe: null,
    He: null,
    ct: null,
    ye: null,
    C: n?.C ?? globalQueue,
    we: n?.we || defaultContext,
    qe: 0,
    Qe: null,
    We: null,
    ke: n,
    dispose: disposeRootSelf
  };
  if (n) {
    const e2 = n.Fe;
    if (e2 === null) {
      n.Fe = i;
    } else {
      i.He = e2;
      e2.ct = i;
      n.Fe = i;
    }
  }
  return i;
}
function createRoot(e, n) {
  const t = createOwner(n);
  return runWithOwner(t, () => e(() => t.dispose()));
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(l) {
  const n = l.it;
  const e = l.nt;
  const u = l.le;
  const s = l.ll;
  if (u !== null)
    u.ll = s;
  else
    n.rt = s;
  if (s !== null)
    s.le = u;
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
    u.Ge &&= e;
    return;
  }
  let s = null;
  const t = n.se & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = u !== null ? u.nt : n.tt;
    if (s !== null && s.it === l) {
      s.nl = n.Ze;
      n.Ye = s;
      s.Ge = e;
      return;
    }
  }
  const i = l.rt;
  if (i !== null && i.fe === n && (!t || i.nl === n.Ze)) {
    if (t)
      i.Ge &&= e;
    else
      i.Ge = e;
    return;
  }
  const o = n.Ye = l.rt = {
    it: l,
    fe: n,
    nt: s,
    ll: i,
    le: null,
    nl: n.Ze,
    Ge: e
  };
  if (u !== null)
    u.nt = o;
  else
    n.tt = o;
  if (i !== null)
    i.le = o;
  else
    l.o = o;
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/async.js
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
  e.ue = true;
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
  for (let t = e.o;t !== null; t = t.le)
    n(t.fe, t);
  for (let t = e.u ?? null;t !== null; t = t.ae) {
    for (let e2 = t.o;e2 !== null; e2 = e2.le)
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
  const settle = (u) => {
    if (r.has(u) || !removePendingSource(u, e))
      return;
    r.add(u);
    u.de = clock;
    const i = u.oe?.values().next().value;
    const l = u.S & STATUS_ERROR;
    if (i) {
      if (!l)
        setPendingError(u, i);
      o !== null && o(u);
    } else {
      u.S &= ~STATUS_PENDING;
      if (!l)
        setPendingError(u);
      o !== null && o(u);
      if (u.ue) {
        enqueueSub(u);
        n = true;
      }
      u.ue = false;
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
    e.Ee = false;
    return n;
  }
  e.Te = n;
  let u;
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
    const u2 = !!(e.S & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const i = resolveLane(e);
    if (i)
      i.Ae.delete(e);
    if (t) {
      t(r2);
      if (u2)
        clearStatus(e, true);
    } else if (e._e !== undefined) {
      if (e.De === NOT_PENDING)
        queuePendingNode(e);
      e.De = r2;
      GlobalQueue.Pe !== null && GlobalQueue.Pe(e, r2);
      if (!hasActiveOverride(e))
        insertSubs(e);
      e.de = clock;
    } else if (i) {
      const n2 = e.Re;
      const t2 = e.Ue;
      const o3 = e.be;
      try {
        if (!n2 && u2 || !o3 || !o3(r2, t2)) {
          e.Ue = r2;
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
    if (e.De === NOT_PENDING)
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
  if (o) {
    let t2 = false, r2 = false, o2, i = true;
    n.then((e2) => {
      if (i) {
        u = e2;
        t2 = true;
      } else {
        asyncWrite(e2);
        settleAutodispose();
      }
    }, (e2) => {
      if (i) {
        o2 = e2;
        r2 = true;
      } else {
        handleError(e2);
        settleAutodispose();
      }
    });
    i = false;
    if (r2) {
      handleError(o2);
      throw o2;
    } else if (!t2) {
      if (e.Ee)
        return e.Ue;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else {
      e.Ee = false;
    }
  }
  if (r) {
    const t2 = n[Symbol.asyncIterator]();
    let r2 = false;
    let o2 = false;
    let i = true;
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
      let l2, s, f = false, a = false, c = true;
      const S = t2.next();
      const d = isThenable(S) ? S : {
        then: (e2) => void e2(S)
      };
      d.then((t3) => {
        if (c && i) {
          l2 = t3;
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
          s = t3;
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
        handleError(s);
        if (i)
          throw s;
        return true;
      }
      if (f && !l2.done) {
        u = l2.value;
        r2 = true;
        return iterate();
      }
      return f && l2.done;
    };
    const l = iterate();
    i = false;
    if (!r2 && !l) {
      if (e.Ee)
        return e.Ue;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.Ee = false;
  }
  return u;
}
function clearStatus(e, n = false) {
  if (e.oe)
    clearPendingSources(e);
  if (e.ue)
    e.ue = false;
  e.he = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e._)
    setPendingError(e);
  if (e.ge || e.pe)
    GlobalQueue.Se(e);
  if (e.u && GlobalQueue.Oe !== null)
    GlobalQueue.Oe(e);
  if (e.i)
    e.i();
}
function notifyStatus(e, n, t, r, o) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const u = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const i = u === e;
  const l = n === STATUS_PENDING && e._e !== undefined && !i;
  const s = l && hasActiveOverride(e);
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
    GlobalQueue.Se !== null && GlobalQueue.Se(e);
    if (e.u && GlobalQueue.Oe !== null)
      GlobalQueue.Oe(e);
  }
  if (o && !r) {
    assignOrMergeLane(e, o);
  }
  const f = r || s;
  const a = r || l ? undefined : o;
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
    if (n === STATUS_PENDING && u && !e2.oe?.has(u) || n !== STATUS_PENDING && (e2._ !== t || e2.oe)) {
      if (r2.Ge && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/core.js
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
    e = e.ke;
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
    else if (e.Fe !== null || e.ye !== null) {
      markDisposal(e);
      e.Qe = e.ye;
      e.We = e.Fe;
      e.ye = null;
      e.Fe = null;
      e.qe = 0;
    }
  }
  let i = !!(e.se & REACTIVE_OPTIMISTIC_DIRTY);
  const l = e._e !== undefined && e._e !== NOT_PENDING;
  const u = !!(e.S & STATUS_UNINITIALIZED);
  const o = e.S & STATUS_ERROR ? e._ : undefined;
  const s = (e.se & REACTIVE_REASK) !== 0;
  const a = e.Ee;
  const r = context;
  context = e;
  e.Ye = null;
  e.Ze++;
  e.se = REACTIVE_RECOMPUTING_DEPS;
  e.de = clock;
  let c = e.De === NOT_PENDING ? e.Ue : e.De;
  let _ = e.xe;
  let f = tracking;
  let E = currentOptimisticLane;
  tracking = true;
  const T = latestReadActive;
  latestReadActive = false;
  if (i) {
    const t2 = GlobalQueue.je(e, true);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.Me.length) {
    const t2 = GlobalQueue.je(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const N = n && n !== EFFECT_USER;
  const d = stale;
  if (N)
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
    if (e.S !== 0 || e.i !== undefined || e._ || e.he || e.ue || e.oe !== undefined || e.ge !== undefined || e.pe !== undefined || e.u !== null)
      clearStatus(e, t);
    if (e.Ke)
      GlobalQueue.ze(e);
  } catch (t2) {
    const n2 = t2 instanceof NotReadyError;
    if (n2 && e.Ee) {
      parkLoadingWindow(e, t2);
    } else {
      if (n2 && currentOptimisticLane)
        GlobalQueue.$e(e);
      let i2 = false;
      if (n2) {
        e.ue = true;
        if (GlobalQueue.Be !== null)
          i2 = GlobalQueue.Be(e, s);
      }
      notifyStatus(e, n2 ? STATUS_PENDING : STATUS_ERROR, t2, undefined, n2 ? e.Ke : undefined);
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = f;
    latestReadActive = T;
    if (N)
      stale = d;
    e.se = REACTIVE_NONE | (t ? e.se & REACTIVE_SNAPSHOT_STALE : 0);
    context = r;
  }
  if (!e._) {
    trimStaleDeps(e);
    const s2 = l ? unwrapOverride(e._e) : e.De === NOT_PENDING ? e.Ue : e.De;
    let r2 = false;
    try {
      r2 = !n && u || !e.be || !e.be(s2, c);
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
      const u2 = l ? e._e : undefined;
      if (t || n && (activeTransition !== e.Ne || activeTransition === null) || i) {
        e.Ue = c;
        if (l && i) {
          e._e = c === undefined ? OVERRIDE_UNDEFINED : c;
          e.De = NOT_PENDING;
        }
      } else {
        e.De = c;
        if (a)
          e.Ee = true;
        if ((activeTransition || e.Ne) && GlobalQueue.Pe !== null)
          GlobalQueue.Pe(e, c);
      }
      if (e.o !== null && (!l || i || e._e !== u2))
        insertSubs(e, i || l);
    } else if (l) {
      if (e.De === NOT_PENDING)
        queuePendingNode(e);
      e.De = c;
      if (a)
        e.Ee = true;
    } else if (e.xe != _) {
      for (let t2 = e.o;t2 !== null; t2 = t2.le) {
        insertIntoHeapHeight(t2.fe, queueFor(t2.fe));
      }
    }
    if (o !== undefined && !r2 && !e._)
      settleErroredDependents(e, o);
  }
  currentOptimisticLane = E;
  const I = e.De !== NOT_PENDING || e.We !== null || e.Qe !== null || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  I && (!t || e.S & STATUS_PENDING) && (!e.Ne || l) && queuePendingNode(e);
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
    be: t?.equals != null ? t.equals : isEqual,
    ut: t?.unobserved,
    ye: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ce: e,
    Ue: i ? t.loadingValue : undefined,
    xe: 0,
    u: null,
    ot: undefined,
    st: null,
    tt: null,
    Ye: null,
    Ze: 0,
    o: null,
    rt: null,
    ke: context,
    He: null,
    ct: null,
    Fe: null,
    se: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: i ? 0 : STATUS_UNINITIALIZED,
    de: clock,
    De: NOT_PENDING,
    Qe: null,
    We: null,
    Te: null,
    Ne: null,
    he: false,
    Ee: i
  };
  setupComputedNode(l, t);
  return l;
}
function createEffectNode(e, t, n, i, l, u) {
  const o = u?.transparent ?? false;
  const s = {
    id: inheritId(u, o, context),
    T: (o ? CONFIG_TRANSPARENT : 0) | (u?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (u?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    be: false,
    ut: u?.unobserved,
    ye: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    qe: 0,
    ce: e,
    Ue: undefined,
    xe: 0,
    u: null,
    ot: undefined,
    st: null,
    tt: null,
    Ye: null,
    Ze: 0,
    o: null,
    rt: null,
    ke: context,
    He: null,
    ct: null,
    Fe: null,
    se: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    de: clock,
    De: NOT_PENDING,
    Qe: null,
    We: null,
    Te: null,
    Ne: null,
    he: false,
    Ee: false,
    Je: false,
    _t: undefined,
    ft: t,
    Et: n,
    Tt: undefined,
    Re: i,
    i: l
  };
  setupComputedNode(s, lazyOptions);
  return s;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.st = e;
  const n = context?.Nt ? context.dt : context;
  if (context) {
    const t2 = context.Fe;
    if (t2 === null) {
      context.Fe = e;
    } else {
      e.He = t2;
      t2.ct = e;
      context.Fe = e;
    }
  }
  if (n)
    e.xe = n.xe + 1;
  if (GlobalQueue.It !== null)
    GlobalQueue.It(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      e.Le = e.Ue === undefined ? NO_SNAPSHOT : e.Ue;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    be: t?.equals != null ? t.equals : isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.V ? CONFIG_NO_SNAPSHOT : 0),
    ut: t?.unobserved,
    Ue: e,
    o: null,
    rt: null,
    de: clock,
    lt: n,
    ae: n?.u || null,
    De: NOT_PENDING
  };
  n && (n.u = i);
  if (snapshotCaptureActive && !(i.T & CONFIG_NO_SNAPSHOT) && !((n?.S ?? 0) & STATUS_PENDING)) {
    i.Le = e === undefined ? NO_SNAPSHOT : e;
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
function readNodeFast(e) {
  if (latestReadActive || pendingCheckActive || e.ce || e.lt || e._e !== undefined || e.Le !== undefined || activeTransition !== null || currentOptimisticLane !== null || snapshotCaptureActive || false)
    return READ_SLOW;
  let t = context;
  if (t?.Nt)
    t = t.dt;
  if (t && tracking)
    link(e, t);
  return !t || e.De === NOT_PENDING ? e.Ue : e.De;
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.At(e);
  let t = context;
  if (t?.Nt)
    t = t.dt;
  const n = e;
  const i = e.lt;
  const l = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Ct(e, t, l, i);
  } else if (typeof n.ce === "function") {
    prepareComputed(e, false);
  }
  if (!n.ce && l === e && e._e === undefined && e.Le === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.De === NOT_PENDING ? e.Ue : e.De;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (l.ce) {
      const n2 = queueFor(e);
      if (l.xe >= n2.Ve) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(l);
      }
      const i2 = l.xe;
      if (i2 >= t.xe && e.ke !== t) {
        t.xe = i2 + 1;
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
    const n2 = e.Le;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const l2 = e.De !== NOT_PENDING ? e.De : e.Ue;
      if (l2 !== i2)
        t.se |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e._e !== undefined && e._e !== NOT_PENDING) {
    return unwrapOverride(e._e);
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.Rt(e, l, t)) {
    return e.Ue;
  }
  const u = !t || currentOptimisticLane !== null && GlobalQueue.Pt(e, l, t) || e.De === NOT_PENDING || stale && e.Ne && activeTransition !== e.Ne ? e.Ue : e.De;
  if (pendingCheckActive)
    GlobalQueue.ht(e, u);
  if (!t && l === e && typeof n.ce === "function" && e.T & CONFIG_AUTO_DISPOSE && !(l.S & STATUS_PENDING) && !e.o) {
    unobserved(e);
  }
  return u;
}
function setSignal(e, t) {
  if (e.Ne && activeTransition !== e.Ne)
    globalQueue.initTransition(e.Ne);
  if (e._e !== undefined && !projectionWriteActive)
    return GlobalQueue.Gt(e, t);
  const n = e.De === NOT_PENDING ? e.Ue : e.De;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.be || !e.be(n, t);
  if (!i)
    return t;
  if (e.De === NOT_PENDING)
    queuePendingNode(e);
  e.De = t;
  (e.ge !== undefined || e.pe !== undefined) && GlobalQueue.Pe !== null && GlobalQueue.Pe(e, t);
  e.de = clock;
  insertSubs(e);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.se & REACTIVE_MANUAL_WRITE) && e.De === NOT_PENDING) {
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
// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/context.js
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
// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/core/effect.js
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
    t._t = t.Ue;
    t.Je = false;
    try {
      t.Et ? t.Et(E2, () => {
        const E3 = t.Tt;
        t.Tt = undefined;
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
  const E = t.Tt;
  t.Tt = undefined;
  try {
    E?.();
    const e = t.ft(t.Ue, t._t);
    if (false)
      ;
    t.Tt = e;
  } catch (E2) {
    t._ = new StatusError(t, E2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(E2);
      throw E2;
    }
  } finally {
    t._t = t.Ue;
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
    const E2 = e.Tt;
    e.Tt = undefined;
    E2?.();
    const R = staleValues(t);
    e.Tt = R;
  }, {
    ...E,
    lazy: true
  });
  e.Tt = undefined;
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
  e.Ft = run;
  e.C.enqueue(EFFECT_USER, run);
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/signals.js
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
// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/store/reconcile.js
function nodeKeys(e) {
  const t = Object.keys(e);
  if (symbolKeyedRecords.has(e)) {
    const n = Object.getOwnPropertySymbols(e);
    for (let e2 = 0, r = n.length;e2 < r; e2++) {
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
function getOverrideValue(e, t, n, r) {
  if (r && n in r)
    return r[n];
  return t && n in t ? t[n] : e[n];
}
function addEnumSymbols(e, t, n) {
  for (let r = 0, a = t.length;r < a; r++) {
    if (Object.prototype.propertyIsEnumerable.call(e, t[r]))
      n.add(t[r]);
  }
}
function getAllKeys(e, t, n) {
  const r = getKeys(e, t);
  const a = Object.keys(n);
  const i = e[$TARGET] ? untrack(() => Object.getOwnPropertySymbols(e)) : Object.getOwnPropertySymbols(e);
  const l = Object.getOwnPropertySymbols(n);
  if (i.length === 0 && l.length === 0) {
    if (r.length === a.length) {
      let e3 = true;
      for (let t2 = 0;t2 < r.length; t2++) {
        if (r[t2] !== a[t2]) {
          e3 = false;
          break;
        }
      }
      if (e3)
        return r;
    }
    const e2 = new Set(r);
    for (let t2 = 0;t2 < a.length; t2++)
      e2.add(a[t2]);
    return Array.from(e2);
  }
  const s = new Set(r);
  addEnumSymbols(e, i, s);
  if (t) {
    for (const e2 of Reflect.ownKeys(t)) {
      t[e2] === $DELETED ? s.delete(e2) : s.add(e2);
    }
  }
  for (let e2 = 0;e2 < a.length; e2++)
    s.add(a[e2]);
  addEnumSymbols(n, l, s);
  return Array.from(s);
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
function recursablePair(e, t) {
  return isWrappable(e) && isWrappable(t) && !(rawValuesUsed && (isRawValue(e) || isRawValue(t))) && Array.isArray(e) === Array.isArray(t);
}
function syncArrayNodeMembership(e, t) {
  let n = e[STORE_NODE];
  if (n) {
    if (symbolKeyedRecords.has(n)) {
      const e2 = nodeKeys(n);
      for (let r = 0, a = e2.length;r < a; r++) {
        e2[r] in t || setSignal(n[e2[r]], undefined);
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
      for (let r = 0, a = e2.length;r < a; r++) {
        setSignal(n[e2[r]], e2[r] in t);
      }
    } else {
      for (const e2 in n) {
        setSignal(n[e2], e2 in t);
      }
    }
  }
}
function applyStateChild(e, t, n, r) {
  if (n[STORE_WRAP] !== undefined) {
    applyState(e, wrap(t, n), r);
    return;
  }
  const a = t[$TARGET] ?? storeLookup.get(t);
  if (a === undefined)
    return;
  e = unwrap(e);
  if (a[STORE_SHALLOW]) {
    applyStateShallow(e, a);
  } else if (a[STORE_OVERRIDE] || a[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(e, a, r);
  } else {
    applyStateFast(e, a, r);
  }
}
function applyArrayItem(e, t, n, r, a) {
  if (recursablePair(t, e)) {
    const i = wrap(t, n);
    r && setSignal(r, i);
    applyState(e, i, a);
  } else
    r && setSignal(r, wrapValue(e, n));
}
function applyDescendants(e, t, n, r, a, i, l) {
  const s = n[STORE_LOOKUP] || storeLookup;
  if (i) {
    const n2 = getKeys(e, i).concat(getStoreSymbols(e, i));
    for (let o2 = 0, f = n2.length;o2 < f; o2++) {
      const f2 = n2[o2];
      if (r?.[f2])
        continue;
      const u = unwrap(getOverrideValue(e, i, f2, l));
      if (!isWrappable(u))
        continue;
      descendInto(u, t[f2], s, a);
    }
    return;
  }
  for (const n2 in e) {
    if (r?.[n2])
      continue;
    const i2 = unwrap(e[n2]);
    if (!isWrappable(i2))
      continue;
    descendInto(i2, t[n2], s, a);
  }
  const o = Object.getOwnPropertySymbols(e);
  for (let n2 = 0, i2 = o.length;n2 < i2; n2++) {
    if (Object.prototype.propertyIsEnumerable.call(e, o[n2])) {
      if (r?.[o[n2]])
        continue;
      const i3 = unwrap(e[o[n2]]);
      if (!isWrappable(i3))
        continue;
      descendInto(i3, t[o[n2]], s, a);
    }
  }
}
function descendInto(e, t, n, r) {
  const a = lookupTarget(e, n);
  if (!a?.[STORE_DESC])
    return;
  const i = unwrap(t);
  if (e === i || !isWrappable(i) || Array.isArray(e) !== Array.isArray(i) || r(e) != null && r(e) !== r(i))
    return;
  if (a[STORE_SHALLOW]) {
    applyStateShallow(i, a);
  } else if (a[STORE_OVERRIDE] || a[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(i, a, r);
  } else {
    applyStateFast(i, a, r);
  }
}
function applyState(e, t, n) {
  e = unwrap(e);
  const r = t?.[$TARGET];
  if (!r)
    return;
  if (r[STORE_SHALLOW]) {
    applyStateShallow(e, r);
  } else if (r[STORE_OVERRIDE] || r[STORE_OPTIMISTIC_OVERRIDE]) {
    applyStateSlow(e, r, n);
  } else {
    applyStateFast(e, r, n);
  }
}
function shallowDiffNodes(e, t, n, r) {
  let a = false;
  for (const i in e) {
    if (r && i === "length")
      continue;
    if (i in t) {
      const r2 = t[i];
      if (r2 !== n(i)) {
        a = true;
        setSignal(e[i], r2);
      }
    } else {
      a = true;
      setSignal(e[i], undefined);
    }
  }
  return a;
}
function applyStateShallow(e, t, n) {
  const r = t[STORE_VALUE];
  const a = t[STORE_OVERRIDE];
  const i = t[STORE_OPTIMISTIC_OVERRIDE];
  if (e === r && !a && !i)
    return;
  const prevAt = (e2) => {
    const t2 = getOverrideValue(r, a, e2, i);
    return t2 === $DELETED ? undefined : t2;
  };
  t[STORE_OVERRIDE] = undefined;
  const l = t[STORE_LOOKUP];
  l !== undefined ? l.set(e, t[$PROXY]) : storeLookup.set(e, t);
  t[STORE_VALUE] = e;
  markRawIngest(e);
  const s = t[STORE_NODE];
  const o = s && s[$TRACK];
  let f = false;
  if (Array.isArray(r)) {
    const n2 = a?.length ?? i?.length ?? r.length;
    if (s) {
      f = shallowDiffNodes(s, e, prevAt, true);
      if (s.length && n2 !== e.length)
        setSignal(s.length, e.length);
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
    if (s) {
      f = shallowDiffNodes(s, e, prevAt, false);
    }
    if (!f && (o || t[STORE_HAS]))
      f = true;
  }
  let u = t[STORE_HAS];
  if (u) {
    for (const t2 in u) {
      setSignal(u[t2], t2 in e);
    }
  }
  f && notifySelf(t);
}
function applyStateFast(e, t, n) {
  const r = t[STORE_VALUE];
  if (e === r)
    return;
  const a = t[STORE_NODE];
  {
    const n2 = t[STORE_LOOKUP];
    n2 !== undefined ? n2.set(e, t[$PROXY]) : storeLookup.set(e, t);
  }
  t[STORE_VALUE] = e;
  if (Array.isArray(r)) {
    let i2 = false;
    const l2 = r.length;
    if (e.length && l2 && isWrappable(e[0]) && n(e[0]) != null) {
      let s, o, f, u, p, c, S, E;
      for (f = 0, u = Math.min(l2, e.length);f < u && keyedMatch(c = r[f], e[f], n); f++) {
        if (c !== e[f]) {
          if (!recursablePair(c, e[f])) {
            a?.[f] && setSignal(a[f], wrapValue(e[f], t));
          } else
            applyStateChild(e[f], c, t, n);
        }
      }
      if (f === e.length && f === l2)
        return;
      const O = new Array(e.length), R = new Map;
      for (u = l2 - 1, p = e.length - 1;u >= f && p >= f && keyedMatch(c = r[u], e[p], n); u--, p--) {
        O[p] = c;
      }
      if (f > p || f > u) {
        for (o = f;o <= p; o++) {
          i2 = true;
          a?.[o] && setSignal(a[o], wrapValue(e[o], t));
        }
        for (;o < e.length; o++) {
          i2 = true;
          applyArrayItem(e[o], O[o], t, a?.[o], n);
        }
        syncArrayNodeMembership(t, e);
        (i2 || l2 !== e.length) && notifySelf(t);
        l2 !== e.length && a?.length && setSignal(a.length, e.length);
        return;
      }
      S = new Array(p + 1);
      for (o = p;o >= f; o--) {
        c = e[o];
        E = itemKey(c, n);
        s = R.get(E);
        S[o] = s === undefined ? -1 : s;
        R.set(E, o);
      }
      for (s = f;s <= u; s++) {
        c = r[s];
        E = itemKey(c, n);
        o = R.get(E);
        if (o !== undefined && o !== -1) {
          O[o] = c;
          o = S[o];
          R.set(E, o);
        }
      }
      for (o = f;o < e.length; o++) {
        if (o in O) {
          applyArrayItem(e[o], O[o], t, a?.[o], n);
        } else
          a?.[o] && setSignal(a[o], wrapValue(e[o], t));
      }
      if (f < e.length)
        i2 = true;
    } else if (e.length) {
      for (let l3 = 0, s = e.length;l3 < s; l3++) {
        const s2 = r[l3];
        if (recursablePair(s2, e[l3])) {
          if (s2 !== e[l3])
            applyStateChild(e[l3], s2, t, n);
        } else {
          if (s2 !== e[l3])
            i2 = true;
          a?.[l3] && setSignal(a[l3], wrapValue(e[l3], t));
        }
      }
    }
    syncArrayNodeMembership(t, e);
    if (l2 !== e.length) {
      i2 = true;
      a?.length && setSignal(a.length, e.length);
    }
    i2 && notifySelf(t);
    return;
  }
  let i = t[STORE_NODE];
  let l;
  if (i) {
    l = i[$TRACK];
    if (l || symbolKeyedRecords.has(i)) {
      const a2 = l ? getAllKeys(r, undefined, e) : nodeKeys(i);
      for (let s = 0, o = a2.length;s < o; s++) {
        const o2 = a2[s];
        const f = i[o2];
        const u = unwrap(r[o2]);
        const p = unwrap(e[o2]);
        if (u === p)
          continue;
        if (!u || !isWrappable(u) || !isWrappable(p) || rawValuesUsed && (isRawValue(u) || isRawValue(p)) || Array.isArray(u) !== Array.isArray(p) || n(u) != null && n(u) !== n(p)) {
          l && setSignal(l, undefined);
          f && setSignal(f, isWrappable(p) ? wrap(p, t) : p);
        } else
          applyStateChild(p, u, t, n);
      }
    } else {
      for (const a2 in i) {
        const s = i[a2];
        const o = unwrap(r[a2]);
        const f = unwrap(e[a2]);
        if (o === f)
          continue;
        if (!o || !isWrappable(o) || !isWrappable(f) || rawValuesUsed && (isRawValue(o) || isRawValue(f)) || Array.isArray(o) !== Array.isArray(f) || n(o) != null && n(o) !== n(f)) {
          l && setSignal(l, undefined);
          s && setSignal(s, isWrappable(f) ? wrap(f, t) : f);
        } else
          applyStateChild(f, o, t, n);
      }
    }
  }
  if (!l && t[STORE_DESC])
    applyDescendants(r, e, t, i, n);
  if (i = t[STORE_HAS]) {
    const t2 = nodeKeys(i);
    for (let n2 = 0, r2 = t2.length;n2 < r2; n2++) {
      const r3 = t2[n2];
      setSignal(i[r3], r3 in e);
    }
  }
}
function applyStateSlow(e, t, n) {
  const r = t[STORE_VALUE];
  const a = t[STORE_OVERRIDE];
  const i = t[STORE_OPTIMISTIC_OVERRIDE];
  let l = t[STORE_NODE];
  {
    const n2 = t[STORE_LOOKUP];
    n2 !== undefined ? n2.set(e, t[$PROXY]) : storeLookup.set(e, t);
  }
  t[STORE_VALUE] = e;
  t[STORE_OVERRIDE] = undefined;
  if (Array.isArray(r)) {
    let s2 = false;
    const o = getOverrideValue(r, a, "length", i);
    if (e.length && o && isWrappable(e[0]) && n(e[0]) != null) {
      let f2, u, p, c, S, E, O, R;
      for (p = 0, c = Math.min(o, e.length);p < c && keyedMatch(E = getOverrideValue(r, a, p, i), e[p], n); p++) {
        if (E !== e[p] && isWrappable(E) && isWrappable(e[p])) {
          if (!recursablePair(E, e[p])) {
            l?.[p] && setSignal(l[p], wrapValue(e[p], t));
          } else
            applyState(e[p], wrap(E, t), n);
        }
      }
      const d = new Array(e.length), y = new Map;
      for (c = o - 1, S = e.length - 1;c >= p && S >= p && keyedMatch(E = getOverrideValue(r, a, c, i), e[S], n); c--, S--) {
        d[S] = E;
      }
      if (p > S || p > c) {
        for (u = p;u <= S; u++) {
          s2 = true;
          l?.[u] && setSignal(l[u], wrapValue(e[u], t));
        }
        for (;u < e.length; u++) {
          s2 = true;
          applyArrayItem(e[u], d[u], t, l?.[u], n);
        }
        const r2 = e.length;
        syncArrayNodeMembership(t, e);
        (s2 || o !== r2) && notifySelf(t);
        o !== r2 && l?.length && setSignal(l.length, r2);
        return;
      }
      O = new Array(S + 1);
      for (u = S;u >= p; u--) {
        E = e[u];
        R = itemKey(E, n);
        f2 = y.get(R);
        O[u] = f2 === undefined ? -1 : f2;
        y.set(R, u);
      }
      for (f2 = p;f2 <= c; f2++) {
        E = getOverrideValue(r, a, f2, i);
        R = itemKey(E, n);
        u = y.get(R);
        if (u !== undefined && u !== -1) {
          d[u] = E;
          u = O[u];
          y.set(R, u);
        }
      }
      for (u = p;u < e.length; u++) {
        if (u in d) {
          applyArrayItem(e[u], d[u], t, l?.[u], n);
        } else
          l?.[u] && setSignal(l[u], wrapValue(e[u], t));
      }
      if (p < e.length)
        s2 = true;
    } else if (e.length) {
      for (let o2 = 0, f2 = e.length;o2 < f2; o2++) {
        const f3 = getOverrideValue(r, a, o2, i);
        if (recursablePair(f3, e[o2])) {
          if (f3 !== e[o2])
            applyState(e[o2], wrap(f3, t), n);
        } else {
          if (f3 !== e[o2])
            s2 = true;
          l?.[o2] && setSignal(l[o2], wrapValue(e[o2], t));
        }
      }
    }
    const f = e.length;
    syncArrayNodeMembership(t, e);
    if (o !== f) {
      s2 = true;
      l?.length && setSignal(l.length, f);
    }
    s2 && notifySelf(t);
    return;
  }
  let s;
  if (l) {
    s = l[$TRACK];
    const o = s ? getAllKeys(r, a, e) : nodeKeys(l);
    for (let f = 0, u = o.length;f < u; f++) {
      const u2 = o[f];
      const p = l[u2];
      const c = unwrap(getOverrideValue(r, a, u2, i));
      let S = unwrap(e[u2]);
      if (c === S)
        continue;
      if (!c || !isWrappable(c) || !isWrappable(S) || rawValuesUsed && (isRawValue(c) || isRawValue(S)) || Array.isArray(c) !== Array.isArray(S) || n(c) != null && n(c) !== n(S)) {
        s && setSignal(s, undefined);
        p && setSignal(p, isWrappable(S) ? wrap(S, t) : S);
      } else
        applyState(S, wrap(c, t), n);
    }
  }
  if (!s && t[STORE_DESC])
    applyDescendants(r, e, t, l, n, a, i);
  if (l = t[STORE_HAS]) {
    const t2 = nodeKeys(l);
    for (let n2 = 0, r2 = t2.length;n2 < r2; n2++) {
      const r3 = t2[n2];
      setSignal(l[r3], r3 in e);
    }
  }
}
var NOKEY = () => null;
var IDENTITY = (e) => e;
function reconcileState(e, t, n, r) {
  if (t == null)
    throw new Error("");
  let a;
  const i = r ? t[$TARGET] : undefined;
  if (i !== undefined) {
    if (e?.[$TARGET] !== undefined && e[$TARGET] !== i && !i[STORE_SHALLOW]) {
      if (i[STORE_VALUE] === e)
        return;
      a = e;
    }
    while (i[STORE_VALUE]?.[$TARGET] !== undefined)
      i[STORE_VALUE] = unwrap(i[STORE_VALUE]);
  }
  if (n === null)
    applyState(e, t, NOKEY);
  else {
    let a2 = typeof n === "string" ? (e2) => e2[n] : n;
    const i2 = a2(t);
    if (i2 !== undefined && a2(e) !== i2) {
      if (!r)
        throw new Error("");
      const n2 = t[$TARGET];
      if (n2 && n2[STORE_VALUE] !== unwrap(e))
        n2[STORE_LOOKUP]?.delete(n2[STORE_VALUE]);
      a2 = IDENTITY;
    }
    applyState(e, t, a2);
  }
  if (a !== undefined)
    i[STORE_VALUE] = a;
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/store/projection.js
function createProjectionInternal(e, t, r) {
  let o;
  const n = new WeakMap;
  const i = !!r?.shallow;
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
    const t2 = createStoreProxy(e2, storeTraps, wrapper);
    n.set(e2, t2);
    return t2;
  };
  const c = wrapProjection(t);
  let s;
  if (r?.seedLoadingValue)
    s = {
      loadingValue: undefined
    };
  o = computed(() => {
    if (!o)
      o = getOwner();
    runProjectionComputed(c, e, r?.key === undefined ? "id" : r.key);
  }, s);
  o.T &= ~CONFIG_AUTO_DISPOSE;
  return {
    store: c,
    node: o
  };
}
function runProjectionComputed(e, t, r, o, n) {
  const i = getOwner();
  let c = false;
  let s;
  const u = i.Ee ? JSON.parse(JSON.stringify(e[$TARGET][STORE_VALUE])) : null;
  const a = new Proxy(e, createWriteTraps(() => !c || i.Te === s, n));
  storeSetter(a, (n2) => {
    s = t(u ?? n2);
    c = true;
    const commit = (t2) => {
      if (u && (t2 === undefined || t2 === u))
        t2 = JSON.parse(JSON.stringify(u));
      if (t2 === n2 || t2 === undefined)
        return;
      const write = () => storeSetter(e, (e2) => reconcileState(t2, e2, r, true));
      o ? o(write) : write();
    };
    const a2 = handleAsync(i, s, commit);
    if (!i.Ee)
      commit(a2);
  });
  return i;
}
function createWriteTraps(e, t) {
  const r = {
    get(e2, t2) {
      let o;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        o = e2[t2];
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      if (t2 === $TARGET)
        return o;
      return typeof o === "object" && o !== null ? new Proxy(o, r) : o;
    },
    has(e2, t2) {
      let r2;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        r2 = t2 in e2;
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return r2;
    },
    set(r2, o, n) {
      if (e && !e())
        return true;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        r2[o] = n;
        t?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return true;
    },
    deleteProperty(r2, o) {
      if (e && !e())
        return true;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        delete r2[o];
        t?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(false);
      }
      return true;
    }
  };
  return r;
}

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/store/store.js
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
var OBJECT_PROTO = Object.prototype;
var wrappableProtos = new WeakMap;
function isWrappable(e) {
  if (e == null || typeof e !== "object" || Object.isFrozen(e))
    return false;
  const t = Object.getPrototypeOf(e);
  if (t === OBJECT_PROTO || t === null)
    return true;
  if (Array.isArray(e))
    return true;
  let r = wrappableProtos.get(t);
  if (r === undefined) {
    r = Object.prototype.toString.call(e) === "[object Object]" && (typeof Node === "undefined" || !(e instanceof Node));
    wrappableProtos.set(t, r);
  }
  return r;
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
  return e._e !== undefined && e._e !== NOT_PENDING ? unwrapOverride(e._e) : e.De !== NOT_PENDING ? e.De : e.Ue;
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
    O._e = NOT_PENDING;
  }
  if (i && r in i) {
    const e2 = i[r];
    O.Le = e2 === undefined ? NO_SNAPSHOT : e2;
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
    GlobalQueue.Lt(r);
  if (affectsScopes.size) {
    const n = e[STORE_VALUE];
    for (const [e2, o] of affectsScopes) {
      if (e2 !== r && e2.t && o.scope.has(n) && (o.key === undefined || o.key === t))
        GlobalQueue.Lt(e2);
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
    if (t2?.Ne) {
      globalQueue.initTransition(t2.Ne);
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
function throwIfUnreadable(e) {
  const t = e[STORE_FIREWALL];
  if (!t)
    return;
  const r = t.S;
  if (r & STATUS_ERROR || r & STATUS_UNINITIALIZED && r & STATUS_PENDING)
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
    const T = s ?? e[STORE_VALUE];
    if (!i) {
      const n2 = Object.getOwnPropertyDescriptor(T, t);
      if (n2 && n2.get)
        return n2.get.call(r);
      if (!n2 && !E && e[STORE_CUSTOM_PROTO]) {
        const e2 = unwrapStoreValue(T);
        if (hasInheritedAccessor(e2, t)) {
          return Reflect.get(T, t, r);
        }
      }
    }
    if (writeOnly(r)) {
      if (isPrototypePollutionKey(t) && !hasOwnStoreProperty(e, t))
        return;
      let r2 = i && (E || !S) ? visibleNodeValue(i) : T[t];
      r2 === $DELETED && (r2 = undefined);
      if (!isWrappable(r2))
        return r2;
      if (e[STORE_SHALLOW])
        return r2;
      const n2 = wrap(r2, e);
      Writing?.add(n2);
      return n2;
    }
    let c = i ? E || !S ? read(o[t]) : (read(o[t]), T[t]) : T[t];
    c === $DELETED && (c = undefined);
    if (!i) {
      if (!E && typeof c === "function" && !Object.prototype.hasOwnProperty.call(T, t)) {
        let t2;
        return !Array.isArray(e[STORE_VALUE]) && (t2 = Object.getPrototypeOf(e[STORE_VALUE])) && t2 !== Object.prototype ? c.bind(T) : c;
      } else if (getObserver() && !n) {
        return read(getNode(e, o, t, isWrappable(c) ? wrap(c, e) : c, isEqual, e[STORE_SNAPSHOT_PROPS]));
      }
    }
    if (!n && !getObserver())
      throwIfUnreadable(e);
    return isWrappable(c) ? wrap(c, e) : c;
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
    throwIfUnreadable(e);
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
        const T = !!e[STORE_SHALLOW] && r?.[$TARGET] !== undefined;
        const c = T ? r : unwrapStoreValue(r);
        if (e[STORE_SHALLOW] && !T && isWrappable(c)) {
          rawValuesUsed = true;
          rawValues.add(c);
        }
        const f = typeof t === "string" ? Number(t) : -1;
        const R = Array.isArray(O) && Number.isInteger(f) && f >= 0 && f < 4294967295 && String(f) === t;
        const u = R ? f + 1 : 0;
        const a = R && (getOverlayLayer(e, "length") ?? O).length;
        const l = R && u > a ? u : undefined;
        if (E === c && l === undefined)
          return true;
        armOptimisticStoreWrite(e, n);
        if (c !== undefined && c === o && l === undefined) {
          delete e[i]?.[t];
          if (i === STORE_OPTIMISTIC_OVERRIDE)
            delete e[STORE_OPTIMISTIC_OWNERS]?.[t];
        } else {
          const r2 = e[i] || (e[i] = Object.create(null));
          r2[t] = c;
          stampOptimisticOwner(e, i, t);
          if (l !== undefined) {
            r2.length = l;
            stampOptimisticOwner(e, i, "length");
          }
        }
        notifyStoreProperty(e, t, "set", c, E, S);
        if (Array.isArray(O) && t === "length" && typeof c === "number" && typeof E === "number" && c < E) {
          const t2 = e[i] || (e[i] = Object.create(null));
          for (let r2 = c;r2 < E; r2++) {
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
        throwIfUnreadable(e);
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

// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/map.js
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
    Ut: [],
    $t: e,
    qt: e || i?.keyed === false ? [] : undefined,
    zt: r && i?.keyed !== false ? [] : undefined,
    Bt: i?.keyed === false,
    Ht: i?.fallback
  };
  const o = computed(updateKeyedMap.bind(h));
  h.wt.dt = o;
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
    let i, e, r, n, h = this.qt ? this.Bt ? () => {
      r[e] = signal(t[e], pureOptions);
      return this.Kt(accessor(r[e]), e);
    } : () => {
      r[e] = signal(t[e], pureOptions);
      n && (n[e] = signal(e, pureOptions));
      return this.Kt(accessor(r[e]), n ? accessor(n[e]) : undefined);
    } : this.zt ? () => {
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
        this.Ut = [];
        this.Mt = [];
        this.xt = [];
        this.jt = 0;
        this.qt && (this.qt = []);
        this.zt && (this.zt = []);
      }
      if (this.Ht && !this.xt[0]) {
        this.Ut[0]?.dispose();
        this.xt[0] = runWithOwner(this.Ut[0] = createOwner(), this.Ht);
      }
    } else if (this.jt === 0) {
      const o = new Array(s);
      const c = new Array(s);
      r = this.qt && new Array(s);
      n = this.zt && new Array(s);
      try {
        for (e = 0;e < s; e++)
          o[e] = runWithOwner(c[e] = createOwner(), h);
      } catch (t2) {
        for (i = 0;i <= e; i++)
          c[i]?.dispose();
        throw t2;
      }
      if (this.Ut[0])
        this.Ut[0].dispose();
      this.xt = o;
      this.Ut = c;
      r && (this.qt = r);
      n && (this.zt = n);
      this.Mt = t.slice(0);
      this.jt = s;
    } else {
      let o, c, a, f, u, p, w, l, d;
      for (o = 0, c = Math.min(this.jt, s);o < c && (this.Mt[o] === t[o] || this.qt && compare(this.$t, this.Mt[o], t[o])); o++) {
        if (this.qt)
          setSignal(this.qt[o], t[o]);
      }
      for (c = this.jt - 1, a = s - 1;c >= o && a >= o && (this.Mt[c] === t[a] || this.qt && compare(this.$t, this.Mt[c], t[a])); c--, a--)
        ;
      if (o === s && this.jt === s) {
        this.Mt = t.slice(0);
        return;
      }
      const O = s - this.jt;
      const m = new Array(s);
      const _ = new Array(s);
      r = this.qt ? new Array(s) : undefined;
      n = this.zt ? new Array(s) : undefined;
      p = new Map;
      w = new Array(a + 1);
      for (e = a;e >= o; e--) {
        f = t[e];
        u = this.$t ? this.$t(f) : f;
        i = p.get(u);
        w[e] = i === undefined ? -1 : i;
        p.set(u, e);
      }
      for (i = o;i <= c; i++) {
        f = this.Mt[i];
        u = this.$t ? this.$t(f) : f;
        e = p.get(u);
        if (e !== undefined && e !== -1) {
          m[e] = this.xt[i];
          _[e] = this.Ut[i];
          r && (r[e] = this.qt[i]);
          n && (n[e] = this.zt[i]);
          e = w[e];
          p.set(u, e);
        } else
          (l ??= []).push(this.Ut[i]);
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
        _[i] = this.Ut[i];
        r && (r[i] = this.qt[i]);
        n && (n[i] = this.zt[i]);
      }
      for (e = o;e <= a; e++) {
        if (r)
          setSignal(r[e], t[e]);
        if (n)
          setSignal(n[e], e);
      }
      for (e = a + 1;e < s; e++) {
        m[e] = this.xt[e - O];
        _[e] = this.Ut[e - O];
        if (r) {
          r[e] = this.qt[e - O];
          setSignal(r[e], t[e]);
        }
        if (n) {
          n[e] = this.zt[e - O];
          if (O !== 0)
            setSignal(n[e], e);
        }
      }
      this.xt = m;
      this.Ut = _;
      r && (this.qt = r);
      n && (this.zt = n);
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
// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/store/utils.js
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
// node_modules/.bun/@solidjs+signals@2.0.0-rc.0/node_modules/@solidjs/signals/dist/prod/boundaries.js
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
// node_modules/.bun/solid-js@2.0.0-rc.0/node_modules/solid-js/dist/solid.js
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
var createMemo2 = (...args) => {
  return (_createMemo || createMemo)(...args);
};
var createRenderEffect2 = (...args) => (_createRenderEffect || createRenderEffect)(...args);
var _fragments = new Map;
var _truncated = new Set;
var _revealSubs = new Set;
var _truncationRejectors = new Map;
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

// node_modules/.bun/@solidjs+universal@2.0.0-rc.0+6b48b9f3356e564b/node_modules/@solidjs/universal/dist/universal.js
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
var globalMoveUnsub = null;
var interestRoot = null;
function onPointerMove(fn) {
  globalMoveSubs.add(fn);
  if (globalMoveSubs.size === 1) {
    globalMoveUnsub = on("pointerMove", (raw) => {
      let e = {
        clientX: raw.clientX,
        clientY: raw.clientY,
        target: raw.target,
        pointerId: raw.pointerId,
        pointerType: raw.pointerType,
        shiftKey: raw.shiftKey,
        ctrlKey: raw.ctrlKey,
        altKey: raw.altKey,
        metaKey: raw.metaKey
      };
      for (let sub of [...globalMoveSubs])
        sub(e);
    });
    if (interestRoot != null)
      syncInterest(interestRoot);
  }
  let cleanup2 = () => {
    if (!globalMoveSubs.delete(fn))
      return;
    if (globalMoveSubs.size === 0) {
      globalMoveUnsub?.();
      globalMoveUnsub = null;
      if (interestRoot != null)
        syncInterest(interestRoot);
    }
  };
  onCleanup(cleanup2);
  return cleanup2;
}
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
function getFocusables() {
  return [...focusables];
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
function startTextInput() {
  if (!textInputEligible()) {
    throw new Error("startTextInput: no focused node with an onTextInput handler");
  }
  syncTextInput(true);
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
function prepareText2(text, options) {
  return tree.prepareText(text, options);
}
function unitInk(units, index) {
  let ink = units[index].width;
  let advance = 0;
  for (let j = index + 1;j < units.length && units[j].glue; j++) {
    advance += units[j - 1].advance;
    ink = advance + units[j].width;
  }
  return ink;
}
function layoutNextLine(prepared, cursor, width) {
  let units = prepared.units;
  if (cursor >= units.length)
    return null;
  let pen = 0;
  let ascent = 0;
  let descent = 0;
  let i = cursor;
  while (i < units.length) {
    let unit = units[i];
    if (i > cursor && !unit.glue && pen + unitInk(units, i) > width)
      break;
    pen += unit.advance;
    if (unit.ascent > ascent)
      ascent = unit.ascent;
    if (unit.descent > descent)
      descent = unit.descent;
    i++;
    if (unit.hardBreak)
      break;
  }
  let last = units[i - 1];
  return {
    from: cursor,
    to: i,
    start: units[cursor].start,
    end: last.end,
    width: pen - last.advance + last.width,
    height: ascent + descent,
    ascent,
    hardBreak: last.hardBreak,
    cursor: i
  };
}

// packages/core/src/window.ts
var nextFrameId = 1;
var animationFrames = new Map;
var refreshRate = 60;
function onFrame(fn) {
  let frameId = null;
  let cancelled = false;
  let extendedFn = (tick, frame, rate) => {
    if (cancelled)
      return;
    frameId = nextFrameId++;
    animationFrames.set(frameId, extendedFn);
    requestFrame();
    try {
      fn(tick, frame, rate);
    } catch (err) {
      console.error("Error in onFrame callback:", err);
    }
  };
  frameId = nextFrameId++;
  animationFrames.set(frameId, extendedFn);
  requestFrame();
  let cleanup2 = () => {
    cancelled = true;
    animationFrames.delete(frameId);
  };
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
  on2("resize", (e) => {
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
    on2("windowFocus", () => setFocused(true));
    on2("windowBlur", () => setFocused(false));
    focusedAccessor = focused;
  }
  return focusedAccessor();
}
var keyboardHeightAccessor;
function keyboardHeight() {
  if (!keyboardHeightAccessor) {
    let [height, setHeight] = createSignal(0);
    on2("keyboardVisibility", ({
      height: h
    }) => setHeight(h ?? 0));
    keyboardHeightAccessor = height;
  }
  return keyboardHeightAccessor();
}
function onLayout(fn) {
  let unsubscribe = on2("postLayout", fn);
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
function attachWindow(nodeId) {
  setInterestRoot(nodeId);
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
  function runFrame(t, frame) {
    if (animationFrames.size > 0) {
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
// packages/core/src/color.ts
import * as tree3 from "flux:rendertree";
function parseColor2(color) {
  return tree3.parseColor(color);
}
function mixColors2(a, b, t) {
  return tree3.mixColors(a, b, t);
}
function brightness2(color) {
  return tree3.brightness(color);
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
function parseStops(stops) {
  return stops.map((s) => ({
    offset: s.offset,
    color: parseColor2(s.color)
  }));
}
// packages/core/src/environment.ts
import { on as on3 } from "srt:events";
var devicesAccessor;
function ensureDevicesState() {
  if (devicesAccessor)
    return;
  let [devices, setDevices] = createSignal(undefined, {
    ownedWrite: true
  });
  on3("inputDevices", (d) => {
    setDevices({
      keyboard: !!d.keyboard,
      mouse: !!d.mouse,
      touch: !!d.touch,
      screenKeyboard: !!d.screenKeyboard
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
  on3("systemTheme", (e) => setTheme(e.theme ?? "unknown"));
  systemThemeAccessor = theme;
}
var visibilityAccessor;
function ensureVisibilityState() {
  if (visibilityAccessor)
    return;
  let [visibility, setVisibility] = createSignal("visible", {
    ownedWrite: true
  });
  on3("visibility", (e) => setVisibility(e.state === "hidden" ? "hidden" : "visible"));
  visibilityAccessor = visibility;
}
var orientationAccessor;
function ensureOrientationState() {
  if (orientationAccessor)
    return;
  let [orientation, setOrientation] = createSignal("unknown", {
    ownedWrite: true
  });
  on3("displayOrientation", (e) => {
    setOrientation(e.orientation ?? "unknown");
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
  on3("textScale", (e) => {
    setScale(typeof e.scale === "number" && e.scale > 0 ? e.scale : 1);
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
  let unsubMove = null;
  let note = (e) => {
    if (e.pointerType === "mouse" && !sawMouse) {
      sawMouse = true;
      setMouse(true);
      unsubMove();
    } else if (e.pointerType === "touch" && !sawTouch) {
      sawTouch = true;
      setTouch(true);
    }
    if (sawMouse && sawTouch)
      for (let u of unsubs)
        u();
  };
  unsubMove = createRoot(() => onPointerMove(note));
  unsubs.push(unsubMove, on3("pointerDown", note));
  mouseSeenAccessor = mouse;
  touchSeenAccessor = touch;
}
var keyboardSeenAccessor;
function ensureKeyboardState() {
  if (keyboardSeenAccessor)
    return;
  let [keyboard, setKeyboard] = createSignal(false);
  let unsub = on3("keydown", () => {
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
import { on as on4 } from "srt:events";
var gamepadsAccessor;
function gamepads() {
  if (!gamepadsAccessor) {
    let [pads, setPads] = createSignal([], {
      ownedWrite: true
    });
    on4("gamepads", (e) => setPads(e.pads ?? []));
    gamepadsAccessor = pads;
  }
  return gamepadsAccessor();
}
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
    let w = env.windowSize.width;
    return w >= EXPANDED_MIN_WIDTH ? "expanded" : w >= MEDIUM_MIN_WIDTH ? "medium" : "compact";
  }
};
// packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { destroyTexture as destroyTexture2, resizeTexture, setTargetParams as setTargetParams2, setTargetSize as setTargetSize2, setTargetTextures, uploadTexture } from "flux:gpu";
import { copyTexture, destroyBuffer as destroyBuffer2, renderTarget, setDraw } from "flux:gpu";
import { addDraw, removeDraw, setDrawOrder, setDrawParams, setDrawRange, setDrawTextures } from "flux:gpu";
import { limits } from "flux:gpu";
import { compileShader, createRenderPipeline, destroyProgram, destroyRenderPipeline, destroyShader, linkProgram } from "flux:gpu";
import { captureSnapshot, readTexture } from "flux:gpu";
var glsl = String.raw;
// packages/core/src/image.ts
import { decodeImage } from "flux:image";
import { decodeImage as decodeImage2, encodeImage } from "flux:image";
var imageCache = new Map;
// packages/core/src/svg.ts
import { parseSvg as fluxParseSvg } from "flux:svg";
var svg = String.raw;
function parseSvg(src, opts) {
  if (opts?.color != null)
    return fluxParseSvg(src, {
      color: parseColor2(opts.color)
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
  let clamp = (x, y) => ({
    x: canX ? Math.max(0, Math.min(x, maxX)) : 0,
    y: canY ? Math.max(0, Math.min(y, maxY)) : 0
  });
  let set = (x, y) => {
    let cur = offset();
    let next = clamp(x, y);
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
    scrollTo: (x, y) => set(x, y)
  };
}
// packages/core/src/arena.ts
var claims = new Map;
var arena = {
  claim(pointerId, owner) {
    if (claims.has(pointerId))
      return false;
    claims.set(pointerId, {
      owner,
      resolved: false
    });
    return true;
  },
  steal(pointerId, owner) {
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
  },
  release(pointerId, owner) {
    if (claims.get(pointerId)?.owner === owner)
      claims.delete(pointerId);
  }
};
// packages/core/src/pan.ts
var PAN_SLOP = 8;
function createPan(options) {
  let origin = null;
  let active = null;
  let armed = null;
  let past = (e) => {
    if (!origin)
      return false;
    let dx = Math.abs(e.clientX - origin.x);
    let dy = Math.abs(e.clientY - origin.y);
    let axis = options.axis ?? "both";
    if (axis === "vertical")
      return dy >= PAN_SLOP;
    if (axis === "horizontal")
      return dx >= PAN_SLOP;
    return dx * dx + dy * dy >= PAN_SLOP * PAN_SLOP;
  };
  let reset = () => {
    if (active != null) {
      arena.release(active, owner);
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
    onPointerDown: (e) => {
      if (e.button != null && e.button !== 0)
        return;
      if (armed == null && active == null) {
        armed = e.pointerId;
        origin = {
          x: e.clientX,
          y: e.clientY
        };
      }
    },
    onPointerMove: (e) => {
      if (armed === e.pointerId && past(e)) {
        if (arena.steal(e.pointerId, owner)) {
          active = e.pointerId;
          armed = null;
          origin = {
            x: e.clientX,
            y: e.clientY
          };
          options.onPanStart?.();
        } else {
          reset();
        }
        return;
      }
      if (active === e.pointerId && origin) {
        options.onPanMove?.(e.clientX - origin.x, e.clientY - origin.y);
        origin = {
          x: e.clientX,
          y: e.clientY
        };
      }
    },
    onPointerUp: (e) => {
      if (active === e.pointerId) {
        reset();
        options.onPanEnd?.();
      } else if (armed === e.pointerId) {
        reset();
      }
    }
  };
  return {
    handlers: handlers2,
    cancel
  };
}
// packages/core/src/transform.ts
import { on as on5 } from "srt:events";
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
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
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
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "strokeWidth", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "radius", a, _p$?.a);
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
    textMuted: mixColors2("#e6edf3", "#0b0f17", 0.4),
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
    textMuted: mixColors2("#1f2328", "#ffffff", 0.4),
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
  setThemeStore((s) => {
    for (let key in partial) {
      let k = key;
      Object.assign(s[k], partial[k]);
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
    focusRing: caps.keyboardNav || gamepads().some((p) => p != null),
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
  return brightness2(text) > brightness2(fill);
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
    let l = props.layout;
    if (!l)
      return {};
    let out = {};
    for (let key in l) {
      if (!FONT_KEYS.includes(key))
        out[key] = l[key];
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
    e,
    t,
    a,
    o,
    i,
    n,
    s,
    h
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "fontFamily", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "fontSize", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "lineHeight", o, _p$?.o);
    i !== _p$?.i && setProp(_el$2, "fontStyle", i, _p$?.i);
    n !== _p$?.n && setProp(_el$2, "fontWeight", n, _p$?.n);
    s !== _p$?.s && setProp(_el$2, "textAlign", s, _p$?.s);
    h !== _p$?.h && setProp(_el$2, "maxLines", h, _p$?.h);
  });
  return _el$;
}
// packages/components/src/safe-area.tsx
function SafeArea(props) {
  let pad = (edge) => {
    let defaultOn = edge === "top" || edge === "bottom";
    let p = props[edge] ?? defaultOn;
    if (p === false)
      return 0;
    if (p === true)
      return safeArea()[edge];
    return Math.max(safeArea()[edge], p);
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
    e,
    t,
    a,
    o,
    i
  }, _p$) => {
    e !== _p$?.e && setProp(_el$, "position", e, _p$?.e);
    t !== _p$?.t && setProp(_el$, "marginTop", t, _p$?.t);
    a !== _p$?.a && setProp(_el$, "marginBottom", a, _p$?.a);
    o !== _p$?.o && setProp(_el$, "marginLeft", o, _p$?.o);
    i !== _p$?.i && setProp(_el$, "marginRight", i, _p$?.i);
  });
  return _el$;
}
// packages/core/src/text-input.ts
function createTextBuffer(options = {}) {
  let initial = options.defaultValue ?? "";
  let [internalValue, setInternalValue] = createSignal(initial);
  let initialCaret = untrack(() => options.value?.() ?? initial).length;
  let [selectionState, setSelectionState] = createSignal({
    anchor: initialCaret,
    focus: initialCaret
  });
  let value = () => options.value?.() ?? internalValue();
  let selection = () => {
    let len = value().length;
    let s = selectionState();
    return {
      anchor: Math.min(s.anchor, len),
      focus: Math.min(s.focus, len)
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
  let step = (text, offset, direction) => {
    if (options.step)
      return Math.max(0, Math.min(options.step(text, offset, direction), text.length));
    return direction === "left" ? Math.max(0, offset - 1) : Math.min(text.length, offset + 1);
  };
  let replace = (start, end, text) => {
    let v = value();
    let max = options.maxLength?.();
    if (max != null)
      text = text.slice(0, Math.max(0, max - (v.length - (end - start))));
    options.onReplace?.(start, end, text);
    let next = v.slice(0, start) + text + v.slice(end);
    if (options.value?.() == null)
      setInternalValue(next);
    setCaret(start + text.length);
    options.onInput?.(next);
    flush();
  };
  return {
    value,
    selection,
    caret: () => selection().focus,
    insertText: (text) => {
      let [start, end] = range();
      replace(start, end, text);
    },
    deleteBackward: () => {
      let [start, end] = range();
      if (start !== end)
        replace(start, end, "");
      else if (start > 0)
        replace(step(value(), start, "left"), start, "");
    },
    deleteForward: () => {
      let v = value();
      let [start, end] = range();
      if (start !== end)
        replace(start, end, "");
      else if (end < v.length)
        replace(end, step(v, end, "right"), "");
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
        flush();
        return;
      }
      let next = focus;
      if (direction === "left")
        next = step(value(), focus, "left");
      else if (direction === "right")
        next = step(value(), focus, "right");
      else if (direction === "start")
        next = 0;
      else if (direction === "end")
        next = len;
      setSelectionState({
        anchor: extend ? anchor : next,
        focus: next
      });
      flush();
    },
    setSelection: (anchor, focus) => {
      let len = value().length;
      setSelectionState({
        anchor: Math.min(anchor, len),
        focus: Math.min(focus, len)
      });
      flush();
    },
    setValue: (next) => replace(0, value().length, next),
    clear: () => replace(0, value().length, "")
  };
}
function createTextEditorLayout(viewport, input) {
  let [viewportSize, setViewportSize] = createSignal({
    width: 0,
    height: 0
  }, {
    equals: (a, b) => a.width === b.width && a.height === b.height
  });
  let [scrollX, setScrollX] = createSignal(0);
  let [scrollY, setScrollY] = createSignal(0);
  let prepared = createMemo(() => {
    let {
      text,
      font,
      runs
    } = input();
    return prepareText2(text, {
      ...font,
      runs,
      carets: true
    });
  });
  let placed = createMemo(() => {
    let {
      text,
      font,
      wrap: wrap2,
      caretWidth = 0
    } = input();
    let width = wrap2 ? Math.max(0, viewportSize().width - caretWidth) : Infinity;
    let units = wrap2 ? splitWide(prepared(), width) : prepared();
    let out = [];
    let y = 0;
    let cursor = 0;
    let line = layoutNextLine(units, cursor, width);
    let hardBreak = false;
    while (line) {
      out.push({
        start: line.start,
        end: line.end,
        y,
        height: line.height,
        width: line.width,
        from: line.from,
        to: line.to
      });
      y += line.height;
      hardBreak = line.hardBreak;
      line = layoutNextLine(units, line.cursor, width);
    }
    if (out.length === 0 || hardBreak) {
      let height = measureText2(" ", font).height;
      let n = units.units.length;
      out.push({
        start: text.length,
        end: text.length,
        y,
        height,
        width: 0,
        from: n,
        to: n
      });
    }
    return {
      units: units.units,
      lines: out
    };
  });
  let lines = createMemo(() => placed().lines);
  let lineStops = (index) => {
    let {
      units,
      lines: lines2
    } = placed();
    let line = lines2[index];
    if (!line)
      return [];
    let stops = [];
    let pen = 0;
    for (let u = line.from;u < line.to; u++) {
      let unit = units[u];
      for (let stop of unit.carets ?? []) {
        let x = pen + stop.x;
        if (stops.length && stops[stops.length - 1].offset === stop.offset)
          continue;
        stops.push({
          offset: stop.offset,
          x
        });
      }
      pen += unit.advance;
    }
    if (stops.length === 0)
      stops.push({
        offset: line.start,
        x: 0
      });
    return stops;
  };
  let lineOf = (offset) => {
    let ls = lines();
    for (let i = 0;i < ls.length; i++) {
      if (offset < ls[i].end)
        return i;
    }
    return ls.length - 1;
  };
  let caretLine = createMemo(() => lineOf(input().caret));
  let caret = createMemo(() => {
    let offset = input().caret;
    let index = caretLine();
    let line = lines()[index];
    let x = 0;
    for (let stop of lineStops(index)) {
      if (stop.offset > offset)
        break;
      x = stop.x;
    }
    return {
      x,
      y: line.y,
      height: line.height
    };
  });
  let offsetAtX = (index, x) => {
    let best = lines()[index]?.start ?? 0;
    let bestDistance = Infinity;
    for (let stop of lineStops(index)) {
      if (lineOf(stop.offset) !== index)
        continue;
      let d = Math.abs(stop.x - x);
      if (d < bestDistance) {
        best = stop.offset;
        bestDistance = d;
      }
    }
    return best;
  };
  let lineAtY = (y) => {
    let ls = lines();
    let index = 0;
    while (index + 1 < ls.length && ls[index + 1].y <= y)
      index++;
    return index;
  };
  let step = (offset, direction) => {
    let {
      units
    } = placed();
    let text = input().text;
    if (direction === "right") {
      for (let unit of units) {
        if (unit.end <= offset)
          continue;
        for (let stop of unit.carets ?? [])
          if (stop.offset > offset)
            return stop.offset;
        return unit.end;
      }
      return text.length;
    }
    for (let u = units.length - 1;u >= 0; u--) {
      let unit = units[u];
      if (unit.start >= offset)
        continue;
      let stops = unit.carets ?? [];
      for (let i = stops.length - 1;i >= 0; i--)
        if (stops[i].offset < offset)
          return stops[i].offset;
      return unit.start;
    }
    return 0;
  };
  onLayout(() => {
    let node = viewport();
    if (!node)
      return;
    let box = getBoundingBox2(node);
    setViewportSize({
      width: box?.width ?? 0,
      height: box?.height ?? 0
    });
    flush();
    let {
      width: vw,
      height: vh
    } = viewportSize();
    let {
      caretWidth = 0,
      wrap: wrap2
    } = input();
    let ls = lines();
    let contentWidth = ls.reduce((w, l) => Math.max(w, l.width), 0);
    let last = ls[ls.length - 1];
    let contentHeight = last.y + last.height;
    let c = caret();
    setScrollX(wrap2 ? 0 : follow(scrollX(), c.x, caretWidth, vw, contentWidth + caretWidth));
    setScrollY(follow(scrollY(), c.y, c.height, vh, contentHeight));
    flush();
  });
  return {
    lines,
    caret,
    caretLine,
    offsetAtX,
    lineAtY,
    step,
    scrollX,
    scrollY
  };
}
function splitWide(prepared, width) {
  let all = prepared.units;
  if (!all.some((u, i) => !u.glue && unitInk(all, i) > width))
    return prepared;
  let wide = false;
  let units = [];
  for (let u = 0;u < all.length; u++) {
    let unit = all[u];
    if (!unit.glue)
      wide = unitInk(all, u) > width;
    let stops = unit.carets;
    if (!wide || !stops || stops.length <= 2) {
      units.push(unit);
      continue;
    }
    for (let i = 1;i < stops.length; i++) {
      let a = stops[i - 1];
      let b = stops[i];
      let last = i === stops.length - 1;
      let advance = last ? unit.advance - a.x : b.x - a.x;
      units.push({
        text: prepared.text.slice(a.offset, b.offset),
        start: a.offset,
        end: last ? unit.end : b.offset,
        advance,
        width: Math.max(0, Math.min(b.x, unit.width) - a.x),
        ascent: unit.ascent,
        descent: unit.descent,
        hardBreak: last && unit.hardBreak,
        glue: i === 1 && unit.glue,
        run: unit.run,
        carets: [{
          offset: a.offset,
          x: 0
        }, {
          offset: b.offset,
          x: b.x - a.x
        }]
      });
    }
  }
  return {
    text: prepared.text,
    units
  };
}
function follow(current, pos, size, extent, content) {
  if (extent <= 0)
    return 0;
  let next = current;
  if (pos < current)
    next = pos;
  else if (pos + size > current + extent)
    next = pos + size - extent;
  return Math.max(0, Math.min(next, Math.max(0, content - extent)));
}

// packages/components/src/focus-nav.ts
var navActions = new Map;
function registerNavAction(nodeId, action2) {
  navActions.set(nodeId, action2);
  return () => {
    if (navActions.get(nodeId) === action2)
      navActions.delete(nodeId);
  };
}
var [scopeStack, setScopeStack] = createSignal([], {
  ownedWrite: true
});
function pushNavScope(node) {
  setScopeStack((s) => [...s, node]);
  return () => setScopeStack((s) => s.filter((n) => n !== node));
}
function createFocusNav(options) {
  let currentScope = () => options?.scope?.() ?? scopeStack()[scopeStack().length - 1];
  let reachable = () => {
    let scopeNode = currentScope();
    let placed = [];
    for (let id2 of getFocusables()) {
      if (scopeNode && !getNodePath(id2).includes(scopeNode.id))
        continue;
      let b = getBoundingBoxViewport2({
        id: id2
      });
      if (b)
        placed.push({
          id: id2,
          x: b.x + b.width / 2,
          y: b.y + b.height / 2
        });
    }
    return placed;
  };
  let ordered = (placed) => [...placed].sort((a, b) => Math.abs(a.y - b.y) <= 1 ? a.x - b.x : a.y - b.y);
  let lastPos = null;
  let focusCandidate = (p) => {
    lastPos = {
      x: p.x,
      y: p.y
    };
    setFocus(p.id);
  };
  let focusFirst = (placed) => {
    focusCandidate(ordered(placed)[0]);
  };
  let focusEntry = (placed) => {
    if (!lastPos)
      return focusFirst(placed);
    let {
      x,
      y
    } = lastPos;
    let best = placed.reduce((a, b) => (b.x - x) ** 2 + (b.y - y) ** 2 < (a.x - x) ** 2 + (a.y - y) ** 2 ? b : a);
    focusCandidate(best);
  };
  let move = (dir) => {
    let placed = reachable();
    if (placed.length === 0)
      return;
    let focused = focusedNode();
    let from = focused != null ? placed.find((p) => p.id === focused) : undefined;
    if (!from)
      return focusEntry(placed);
    let best = null;
    let bestScore = Infinity;
    for (let p of placed) {
      if (p === from)
        continue;
      let dx = p.x - from.x;
      let dy = p.y - from.y;
      let ahead = dir === "up" ? -dy : dir === "down" ? dy : dir === "left" ? -dx : dx;
      if (ahead <= 1)
        continue;
      let across = Math.abs(dir === "up" || dir === "down" ? dx : dy);
      let score = ahead + 2 * across;
      if (score < bestScore) {
        bestScore = score;
        best = p;
      }
    }
    if (best)
      focusCandidate(best);
  };
  let tab = (delta) => {
    let placed = reachable();
    if (placed.length === 0)
      return;
    let row = ordered(placed);
    let focused = focusedNode();
    let i = focused != null ? row.findIndex((p) => p.id === focused) : -1;
    if (i < 0) {
      if (lastPos)
        return focusEntry(placed);
      return focusCandidate(row[delta === 1 ? 0 : row.length - 1]);
    }
    focusCandidate(row[(i + delta + row.length) % row.length]);
  };
  let activate = () => {
    let placed = reachable();
    if (placed.length === 0)
      return;
    let focused = focusedNode();
    let hit = focused != null ? placed.find((p) => p.id === focused) : undefined;
    if (!hit)
      return focusEntry(placed);
    lastPos = {
      x: hit.x,
      y: hit.y
    };
    navActions.get(hit.id)?.();
  };
  let onKeyDown = (e) => {
    if (e.key === "ArrowUp")
      move("up");
    else if (e.key === "ArrowDown")
      move("down");
    else if (e.key === "ArrowLeft")
      move("left");
    else if (e.key === "ArrowRight")
      move("right");
    else if (e.key === "Tab")
      tab(e.shiftKey ? -1 : 1);
    else if ((e.key === "Enter" || e.code === "Select") && !e.repeat)
      activate();
  };
  let prevFocused = null;
  let refocusPending = false;
  createEffect(() => focusedNode(), (id2) => {
    let prev = prevFocused;
    prevFocused = id2;
    if (id2 != null || prev == null)
      return;
    refocusPending = getNodePath(prev).length === 0;
  });
  onLayout(() => {
    if (!refocusPending)
      return;
    refocusPending = false;
    if (focusedNode() != null)
      return;
    let placed = reachable();
    if (placed.length > 0)
      focusEntry(placed);
  });
  createEffect(() => currentScope(), (scopeNode) => {
    if (!scopeNode)
      return;
    let focused = focusedNode();
    if (focused != null && getNodePath(focused).includes(scopeNode.id))
      return;
    let placed = reachable();
    if (placed.length > 0)
      focusFirst(placed);
    else if (focused != null)
      setFocus(null);
  });
  let prevButtons = new Set;
  createEffect(() => gamepads(), (pads) => {
    let now = new Set;
    for (let pad of pads)
      for (let b of pad?.buttons ?? [])
        now.add(b);
    for (let b of now) {
      if (prevButtons.has(b))
        continue;
      if (b === "dpadUp")
        move("up");
      else if (b === "dpadDown")
        move("down");
      else if (b === "dpadLeft")
        move("left");
      else if (b === "dpadRight")
        move("right");
      else if (b === "south")
        activate();
    }
    prevButtons = now;
  });
  return {
    onKeyDown,
    move,
    tab,
    activate
  };
}

// packages/components/src/spacing.ts
function space(token) {
  return Math.round(theme.spacing[token] * densityScale());
}

// packages/components/src/editor-field.tsx
var CARET_WIDTH = 1;
function EditorField(props) {
  let [caretOn, setCaretOn] = createSignal(true);
  let node;
  let viewport;
  let blinkId = null;
  let focused = createMemo(() => {
    let id2 = focusedNode();
    return id2 != null && id2 === node?.id;
  });
  let buffer = untrack(() => props.buffer)((_text, offset, direction) => editor.step(offset, direction));
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
  let handleViewportPointerDown = (e) => {
    if (props.disabled)
      return;
    let line = editor.lineAtY(e.localY + editor.scrollY());
    let offset = editor.offsetAtX(line, e.localX + editor.scrollX());
    buffer.setSelection(offset, offset);
    setCaretOn(true);
  };
  let handleFocus = () => {
    setCaretOn(true);
    if (blinkId == null) {
      blinkId = setInterval(() => setCaretOn((v) => !v), 500);
    }
    props.onFocus?.();
  };
  let handleBlur = () => {
    if (blinkId != null) {
      clearInterval(blinkId);
      blinkId = null;
    }
    props.onBlur?.();
  };
  let handleKeyDown = (e) => {
    if (props.disabled)
      return;
    let consumed = true;
    if (e.key === "Backspace") {
      buffer.deleteBackward();
      setCaretOn(true);
    } else if (e.key === "Delete") {
      buffer.deleteForward();
      setCaretOn(true);
    } else if (e.key === "ArrowLeft") {
      buffer.move("left");
      setCaretOn(true);
    } else if (e.key === "ArrowRight") {
      buffer.move("right");
      setCaretOn(true);
    } else if (e.key === "Home" || e.key === "End") {
      if (props.multiline) {
        let offset = editor.offsetAtX(editor.caretLine(), e.key === "Home" ? 0 : 1e9);
        buffer.setSelection(offset, offset);
      } else {
        buffer.move(e.key === "Home" ? "start" : "end");
      }
      setCaretOn(true);
    } else if (props.multiline && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      moveLine(e.key === "ArrowUp" ? -1 : 1);
      setCaretOn(true);
    } else if (props.multiline && e.key === "Enter" && textInputActive()) {
      buffer.insertText(`
`);
      setCaretOn(true);
    } else if (e.key === "Enter" || e.code === "Select") {
      activateField();
    } else if (e.key === "Escape") {
      if (node)
        setFocus(null);
    } else {
      consumed = false;
    }
    if (consumed)
      e.stopPropagation();
  };
  let handleTextInput = (e) => {
    if (props.disabled)
      return;
    buffer.insertText(e.text ?? "");
    setCaretOn(true);
  };
  let moveLine = (delta) => {
    let target = editor.caretLine() + delta;
    let count = editor.lines().length;
    let offset = target < 0 ? 0 : target >= count ? value().length : editor.offsetAtX(target, editor.caret().x);
    buffer.setSelection(offset, offset);
  };
  let activateField = () => {
    if (props.disabled)
      return;
    if (!textInputActive()) {
      startTextInput();
    } else if (!props.multiline) {
      props.onSubmit?.(value());
      setFocus(null);
    }
  };
  let unregisterNav = null;
  onCleanup(() => {
    if (blinkId != null)
      clearInterval(blinkId);
    unregisterNav?.();
  });
  let textColor = () => props.style?.color ?? theme.color.text;
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface;
  let borderColor = () => props.style?.borderColor ?? (focused() && policy.focusRing ? theme.color.primary : theme.color.border);
  let borderWidth = () => props.style?.borderWidth ?? theme.borderWidth.sm;
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.sm;
  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0;
  let showCaret = () => focused() && caretOn() && !showPlaceholder();
  let fontSize = () => theme.text.body.size * policy.textScale;
  let font = () => ({
    fontSize: fontSize(),
    lineHeight: theme.text.body.lineHeight
  });
  let rowHeight = () => Math.round(fontSize() * theme.text.body.lineHeight);
  let editor = createTextEditorLayout(() => viewport, () => ({
    text: value(),
    font: font(),
    runs: props.runs?.(),
    caret: buffer.caret(),
    caretWidth: CARET_WIDTH,
    wrap: props.multiline ?? false
  }));
  let caret = editor.caret;
  let viewportHeight = () => {
    if (!props.multiline)
      return rowHeight();
    if (props.layout?.height != null)
      return;
    let lines = editor.lines();
    let last = lines[lines.length - 1];
    let content = Math.ceil(last.y + last.height);
    let max = props.maxRows != null ? props.maxRows * rowHeight() : Infinity;
    return Math.max(rowHeight(), Math.min(content, max));
  };
  var _el$ = createElement("view"), _el$2 = createElement("d-rect"), _el$3 = createElement("d-rect", {
    drawStyle: "stroke"
  }), _el$4 = createElement("view", {
    flex: 1,
    overflow: "hidden",
    onPointerDown: handleViewportPointerDown
  });
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  insertNode2(_el$, _el$4);
  ref(() => (n) => {
    node = n;
    unregisterNav?.();
    unregisterNav = registerNavAction(n.id, activateField);
    props.ref?.(n);
  }, _el$);
  setProp(_el$, "focusable", true);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "alignItems", "center");
  spread(_el$, mergeProps({
    get textInputHints() {
      return memo2(() => !!props.multiline)() ? {
        multiline: true,
        ...props.hints
      } : props.hints;
    },
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
  ref(() => (n) => viewport = n, _el$4);
  insert(_el$4, (() => {
    var _c$ = memo2(() => !!showPlaceholder());
    return () => _c$() ? (() => {
      var _el$5 = createElement("d-text");
      setProp(_el$5, "w", 1e9);
      spread(_el$5, mergeProps(font, {
        get color() {
          return theme.color.textMuted;
        },
        maxLines: 1
      }), true);
      insert(_el$5, () => props.placeholder ?? "");
      return _el$5;
    })() : [createComponent2(For, {
      get each() {
        return editor.lines();
      },
      keyed: false,
      children: (line) => props.renderLine({
        line,
        font,
        color: textColor
      })
    }), memo2(() => memo2(() => !!showCaret())() ? (() => {
      var _el$6 = createElement("d-rect", {
        w: 1
      });
      effect3(() => ({
        e: textColor(),
        t: caret().x,
        a: caret().y + (caret().height - fontSize()) / 2,
        o: fontSize()
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$6, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$6, "x", t, _p$?.t);
        a !== _p$?.a && setProp(_el$6, "y", a, _p$?.a);
        o !== _p$?.o && setProp(_el$6, "h", o, _p$?.o);
      });
      return _el$6;
    })() : null)];
  })());
  effect3(() => ({
    e: surfaceColor(),
    t: borderRadius(),
    a: borderColor(),
    o: borderWidth(),
    i: borderRadius(),
    n: viewportHeight(),
    s: props.multiline ? "stretch" : undefined,
    h: editor.scrollX(),
    r: editor.scrollY()
  }), ({
    e,
    t,
    a,
    o,
    i,
    n,
    s,
    h,
    r
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
    a !== _p$?.a && setProp(_el$3, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$3, "strokeWidth", o, _p$?.o);
    i !== _p$?.i && setProp(_el$3, "radius", i, _p$?.i);
    n !== _p$?.n && setProp(_el$4, "height", n, _p$?.n);
    s !== _p$?.s && setProp(_el$4, "alignSelf", s, _p$?.s);
    h !== _p$?.h && setProp(_el$4, "scrollX", h, _p$?.h);
    r !== _p$?.r && setProp(_el$4, "scrollY", r, _p$?.r);
  });
  return _el$;
}

// packages/components/src/text-input.tsx
function TextInput(props) {
  let value = () => "";
  return createComponent2(EditorField, {
    buffer: (step) => {
      let buffer = createTextBuffer({
        value: () => props.value,
        defaultValue: untrack(() => props.defaultValue),
        onInput: (v) => props.onInput?.(v),
        maxLength: () => props.maxLength,
        step
      });
      value = buffer.value;
      return buffer;
    },
    renderLine: ({
      line,
      font,
      color
    }) => (() => {
      var _el$ = createElement("d-text");
      spread(_el$, mergeProps({
        get y() {
          return line().y;
        },
        get w() {
          return line().width + 1;
        }
      }, font, {
        get color() {
          return color();
        },
        maxLines: 1
      }), true);
      insert(_el$, () => value().slice(line().start, line().end));
      return _el$;
    })(),
    get onSubmit() {
      return props.onSubmit;
    },
    get onFocus() {
      return props.onFocus;
    },
    get onBlur() {
      return props.onBlur;
    },
    get placeholder() {
      return props.placeholder;
    },
    get disabled() {
      return props.disabled;
    },
    get autoFocus() {
      return props.autoFocus;
    },
    get multiline() {
      return props.multiline;
    },
    get maxRows() {
      return props.maxRows;
    },
    get hints() {
      return props.hints;
    },
    ref(r$) {
      var _ref$ = props.ref;
      typeof _ref$ === "function" || Array.isArray(_ref$) ? applyRef(_ref$, r$) : props.ref = r$;
    },
    get layout() {
      return props.layout;
    },
    get style() {
      return props.style;
    }
  });
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
  let onWheel = (e) => {
    if (props.horizontal)
      scroll.scrollBy(e.deltaX || e.deltaY, 0);
    else
      scroll.scrollBy(e.deltaX, e.deltaY);
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
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "radius", t, _p$?.t);
      });
      return _el$4;
    })() : null;
  })(), _el$2);
  insertNode2(_el$2, _el$3);
  ref(() => (n) => viewport = n, _el$2);
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
  ref(() => (n) => content = n, _el$3);
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
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$5, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$5, "strokeWidth", t, _p$?.t);
        a !== _p$?.a && setProp(_el$5, "radius", a, _p$?.a);
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
  let unregisterNav = null;
  let focused = createMemo(() => {
    let id2 = focusedNode();
    return id2 != null && id2 === node?.id;
  });
  let active = null;
  let inside = false;
  let live = {
    get pressed() {
      return pressed();
    },
    get hovered() {
      return hovered();
    },
    get focused() {
      return focused();
    }
  };
  let state = () => live;
  let ref2 = (n) => {
    node = n;
    unregisterNav?.();
    unregisterNav = registerNavAction(n.id, () => {
      if (!options.disabled)
        options.onPress?.();
    });
  };
  let within = (e) => {
    let b = node && getBoundingBoxViewport2(node);
    if (!b)
      return true;
    return e.clientX >= b.x && e.clientX < b.x + b.width && e.clientY >= b.y && e.clientY < b.y + b.height;
  };
  let disengage = () => {
    if (active != null) {
      arena.release(active, owner);
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
  onSettled(() => () => {
    disengage();
    unregisterNav?.();
  });
  let handlers2 = {
    onPointerDown: (e) => {
      if (e.button != null && e.button !== 0)
        return;
      if (active == null && arena.claim(e.pointerId, owner)) {
        active = e.pointerId;
        inside = true;
        setPressed(true);
      }
      options.onPointerDown?.(e);
    },
    onPointerMove: (e) => {
      if (active === e.pointerId) {
        inside = within(e);
        setPressed(inside);
      }
      options.onPointerMove?.(e);
    },
    onPointerUp: (e) => {
      if (active === e.pointerId) {
        let fire = inside;
        cancel();
        if (fire)
          options.onPress?.();
      }
      options.onPointerUp?.(e);
    },
    onPointerEnter: (e) => {
      setHovered(true);
      options.onPointerEnter?.(e);
    },
    onPointerLeave: (e) => {
      setHovered(false);
      options.onPointerLeave?.(e);
    },
    onKeyDown: (e) => {
      if ((e.key === "Enter" || e.key === " " || e.code === "Select") && !e.repeat && !options.disabled) {
        e.stopPropagation();
        options.onPress?.();
      }
      options.onKeyDown?.(e);
    },
    onFocus: () => {
      options.onFocus?.();
    },
    onBlur: () => {
      options.onBlur?.();
    }
  };
  return {
    pressed,
    hovered,
    focused,
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
    let c = resolved2();
    return typeof c === "function" ? c(press.state()) : c;
  };
  let hasBackground = () => style()?.backgroundColor != null || style()?.borderRadius != null;
  let hasBorder = () => (style()?.borderWidth ?? 0) > 0;
  var _el$ = createElement("view");
  ref(() => (n) => {
    press.ref(n);
    props.ref?.(n);
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
      return press.handlers.onFocus;
    },
    get onBlur() {
      return press.handlers.onBlur;
    },
    get onKeyDown() {
      return press.handlers.onKeyDown;
    },
    get onKeyUp() {
      return props.onKeyUp;
    },
    get onTextInput() {
      return props.onTextInput;
    },
    get focusable() {
      return memo2(() => props.focusable === true)() && props.disabled !== true;
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
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
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
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "strokeWidth", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "radius", a, _p$?.a);
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
    let c = theme.color;
    switch (props.variant ?? "primary") {
      case "secondary":
        return {
          fill: c.secondary,
          hover: c.secondaryHover,
          label: c.onSecondary
        };
      case "ghost":
        return {
          fill: "transparent",
          hover: c.surfaceHover,
          label: c.text
        };
      case "danger":
        return {
          fill: c.danger,
          hover: c.dangerHover,
          label: c.onPrimary
        };
      default:
        return {
          fill: c.primary,
          hover: c.primaryHover,
          label: c.onPrimary
        };
    }
  };
  let idleFill = () => props.disabled ? props.variant === "ghost" ? "transparent" : theme.color.surface : colors().fill;
  let bg = (s) => props.style?.backgroundColor ?? (props.disabled ? idleFill() : s.hovered && policy.interaction !== "touch" ? colors().hover : colors().fill);
  let radius = () => props.style?.borderRadius ?? theme.radius.md;
  let label = () => props.disabled ? theme.color.textMuted : colors().label;
  let resolved2 = children(() => props.children);
  let isText = () => typeof resolved2() === "string" || typeof resolved2() === "number";
  let labelOnDark = () => lightOnDark(label(), props.style?.backgroundColor ?? idleFill());
  let press = createPress(props);
  let style = () => ({
    ...props.style,
    ...press.focused() && policy.focusRing ? {
      borderWidth: 2,
      borderColor: theme.color.text
    } : {},
    backgroundColor: bg(press.state()),
    borderRadius: radius(),
    scale: (props.style?.scale ?? 1) * (press.pressed() && policy.motion !== "none" ? 0.97 : 1)
  });
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  ref(() => (n) => {
    press.ref(n);
    props.ref?.(n);
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
    get focusable() {
      return memo2(() => !!(props.focusable ?? true))() ? props.disabled !== true : props.focusable ?? true;
    },
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
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "strokeWidth", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "radius", a, _p$?.a);
      });
      return _el$4;
    }
  }), null);
  effect3(() => ({
    e: style().backgroundColor ?? "transparent",
    t: style().borderRadius
  }), ({
    e,
    t
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
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
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "strokeWidth", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "radius", a, _p$?.a);
      });
      return _el$4;
    }
  }), null);
  effect3(() => ({
    e: bg(),
    t: radius()
  }), ({
    e,
    t
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
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
    let s = size();
    let r = (s - thickness()) / 2;
    let c = s / 2;
    return `M ${c} ${c - r} A ${r} ${r} 0 1 1 ${c - r} ${c}`;
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
    e,
    t,
    a
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "d", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "color", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "strokeWidth", a, _p$?.a);
  });
  return _el$;
}
// packages/components/src/modal.tsx
function Modal(props) {
  let dismiss = (_e) => {
    if (props.dismissable !== false)
      props.onClose?.();
  };
  let popNavScope = null;
  onCleanup(() => popNavScope?.());
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
    ref(() => (n) => {
      popNavScope = pushNavScope(n);
    }, _el$);
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
  let select = (v) => {
    if (props.value === undefined)
      setInternal(() => v);
    props.onChange?.(v);
  };
  let radius = () => typeof props.style?.borderRadius === "number" ? props.style.borderRadius : theme.radius.md;
  let corners = (i) => {
    let r = radius();
    let last = props.options.length - 1;
    if (last === 0)
      return r;
    if (i === 0)
      return [r, 0, 0, r];
    if (i === last)
      return [0, r, r, 0];
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
    children: (opt, i) => {
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
        t: corners(i())
      }), ({
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "radius", t, _p$?.t);
      });
      return _el$3;
    }
  }), null);
  effect3(() => ({
    e: theme.color.border,
    t: radius()
  }), ({
    e,
    t
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
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
    for (let r = -1;r <= 7; r += 1) {
      if (row + r <= -1 || _moduleCount <= row + r)
        continue;
      for (let c = -1;c <= 7; c += 1) {
        if (col + c <= -1 || _moduleCount <= col + c)
          continue;
        if (0 <= r && r <= 6 && (c == 0 || c == 6) || 0 <= c && c <= 6 && (r == 0 || r == 6) || 2 <= r && r <= 4 && 2 <= c && c <= 4) {
          _modules[row + r][col + c] = true;
        } else {
          _modules[row + r][col + c] = false;
        }
      }
    }
  };
  const getBestMaskPattern = function() {
    let minLostPoint = 0;
    let pattern = 0;
    for (let i = 0;i < 8; i += 1) {
      makeImpl(true, i);
      const lostPoint = QRUtil.getLostPoint(_this);
      if (i == 0 || minLostPoint > lostPoint) {
        minLostPoint = lostPoint;
        pattern = i;
      }
    }
    return pattern;
  };
  const setupTimingPattern = function() {
    for (let r = 8;r < _moduleCount - 8; r += 1) {
      if (_modules[r][6] != null) {
        continue;
      }
      _modules[r][6] = r % 2 == 0;
    }
    for (let c = 8;c < _moduleCount - 8; c += 1) {
      if (_modules[6][c] != null) {
        continue;
      }
      _modules[6][c] = c % 2 == 0;
    }
  };
  const setupPositionAdjustPattern = function() {
    const pos = QRUtil.getPatternPosition(_typeNumber);
    for (let i = 0;i < pos.length; i += 1) {
      for (let j = 0;j < pos.length; j += 1) {
        const row = pos[i];
        const col = pos[j];
        if (_modules[row][col] != null) {
          continue;
        }
        for (let r = -2;r <= 2; r += 1) {
          for (let c = -2;c <= 2; c += 1) {
            if (r == -2 || r == 2 || c == -2 || c == 2 || r == 0 && c == 0) {
              _modules[row + r][col + c] = true;
            } else {
              _modules[row + r][col + c] = false;
            }
          }
        }
      }
    }
  };
  const setupTypeNumber = function(test) {
    const bits = QRUtil.getBCHTypeNumber(_typeNumber);
    for (let i = 0;i < 18; i += 1) {
      const mod = !test && (bits >> i & 1) == 1;
      _modules[Math.floor(i / 3)][i % 3 + _moduleCount - 8 - 3] = mod;
    }
    for (let i = 0;i < 18; i += 1) {
      const mod = !test && (bits >> i & 1) == 1;
      _modules[i % 3 + _moduleCount - 8 - 3][Math.floor(i / 3)] = mod;
    }
  };
  const setupTypeInfo = function(test, maskPattern) {
    const data = _errorCorrectionLevel << 3 | maskPattern;
    const bits = QRUtil.getBCHTypeInfo(data);
    for (let i = 0;i < 15; i += 1) {
      const mod = !test && (bits >> i & 1) == 1;
      if (i < 6) {
        _modules[i][8] = mod;
      } else if (i < 8) {
        _modules[i + 1][8] = mod;
      } else {
        _modules[_moduleCount - 15 + i][8] = mod;
      }
    }
    for (let i = 0;i < 15; i += 1) {
      const mod = !test && (bits >> i & 1) == 1;
      if (i < 8) {
        _modules[8][_moduleCount - i - 1] = mod;
      } else if (i < 9) {
        _modules[8][15 - i - 1 + 1] = mod;
      } else {
        _modules[8][15 - i - 1] = mod;
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
        for (let c = 0;c < 2; c += 1) {
          if (_modules[row][col - c] == null) {
            let dark = false;
            if (byteIndex < data.length) {
              dark = (data[byteIndex] >>> bitIndex & 1) == 1;
            }
            const mask = maskFunc(row, col - c);
            if (mask) {
              dark = !dark;
            }
            _modules[row][col - c] = dark;
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
    for (let r = 0;r < rsBlocks.length; r += 1) {
      const dcCount = rsBlocks[r].dataCount;
      const ecCount = rsBlocks[r].totalCount - dcCount;
      maxDcCount = Math.max(maxDcCount, dcCount);
      maxEcCount = Math.max(maxEcCount, ecCount);
      dcdata[r] = new Array(dcCount);
      for (let i = 0;i < dcdata[r].length; i += 1) {
        dcdata[r][i] = 255 & buffer.getBuffer()[i + offset];
      }
      offset += dcCount;
      const rsPoly = QRUtil.getErrorCorrectPolynomial(ecCount);
      const rawPoly = qrPolynomial(dcdata[r], rsPoly.getLength() - 1);
      const modPoly = rawPoly.mod(rsPoly);
      ecdata[r] = new Array(rsPoly.getLength() - 1);
      for (let i = 0;i < ecdata[r].length; i += 1) {
        const modIndex = i + modPoly.getLength() - ecdata[r].length;
        ecdata[r][i] = modIndex >= 0 ? modPoly.getAt(modIndex) : 0;
      }
    }
    let totalCodeCount = 0;
    for (let i = 0;i < rsBlocks.length; i += 1) {
      totalCodeCount += rsBlocks[i].totalCount;
    }
    const data = new Array(totalCodeCount);
    let index = 0;
    for (let i = 0;i < maxDcCount; i += 1) {
      for (let r = 0;r < rsBlocks.length; r += 1) {
        if (i < dcdata[r].length) {
          data[index] = dcdata[r][i];
          index += 1;
        }
      }
    }
    for (let i = 0;i < maxEcCount; i += 1) {
      for (let r = 0;r < rsBlocks.length; r += 1) {
        if (i < ecdata[r].length) {
          data[index] = ecdata[r][i];
          index += 1;
        }
      }
    }
    return data;
  };
  const createData = function(typeNumber2, errorCorrectionLevel2, dataList) {
    const rsBlocks = QRRSBlock.getRSBlocks(typeNumber2, errorCorrectionLevel2);
    const buffer = qrBitBuffer();
    for (let i = 0;i < dataList.length; i += 1) {
      const data = dataList[i];
      buffer.put(data.getMode(), 4);
      buffer.put(data.getLength(), QRUtil.getLengthInBits(data.getMode(), typeNumber2));
      data.write(buffer);
    }
    let totalDataCount = 0;
    for (let i = 0;i < rsBlocks.length; i += 1) {
      totalDataCount += rsBlocks[i].dataCount;
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
        for (let i = 0;i < _dataList.length; i++) {
          const data = _dataList[i];
          buffer.put(data.getMode(), 4);
          buffer.put(data.getLength(), QRUtil.getLengthInBits(data.getMode(), typeNumber2));
          data.write(buffer);
        }
        let totalDataCount = 0;
        for (let i = 0;i < rsBlocks.length; i++) {
          totalDataCount += rsBlocks[i].dataCount;
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
    for (let r = 0;r < _this.getModuleCount(); r += 1) {
      qrHtml += "<tr>";
      for (let c = 0;c < _this.getModuleCount(); c += 1) {
        qrHtml += '<td style="';
        qrHtml += " border-width: 0px; border-style: none;";
        qrHtml += " border-collapse: collapse;";
        qrHtml += " padding: 0px; margin: 0px;";
        qrHtml += " width: " + cellSize + "px;";
        qrHtml += " height: " + cellSize + "px;";
        qrHtml += " background-color: ";
        qrHtml += _this.isDark(r, c) ? "#000000" : "#ffffff";
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
    let c, mc, r, mr, qrSvg = "", rect;
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
    for (r = 0;r < _this.getModuleCount(); r += 1) {
      mr = r * cellSize + margin;
      for (c = 0;c < _this.getModuleCount(); c += 1) {
        if (_this.isDark(r, c)) {
          mc = c * cellSize + margin;
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
    return createDataURL(size, size, function(x, y) {
      if (min <= x && x < max && min <= y && y < max) {
        const c = Math.floor((x - min) / cellSize);
        const r = Math.floor((y - min) / cellSize);
        return _this.isDark(r, c) ? 0 : 1;
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
  const escapeXml = function(s) {
    let escaped = "";
    for (let i = 0;i < s.length; i += 1) {
      const c = s.charAt(i);
      switch (c) {
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
          escaped += c;
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
    let y, x, r1, r2, p;
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
    for (y = 0;y < size; y += 2) {
      r1 = Math.floor((y - min) / cellSize);
      r2 = Math.floor((y + 1 - min) / cellSize);
      for (x = 0;x < size; x += 1) {
        p = "█";
        if (min <= x && x < max && min <= y && y < max && _this.isDark(r1, Math.floor((x - min) / cellSize))) {
          p = " ";
        }
        if (min <= x && x < max && min <= y + 1 && y + 1 < max && _this.isDark(r2, Math.floor((x - min) / cellSize))) {
          p += " ";
        } else {
          p += "█";
        }
        ascii += margin < 1 && y + 1 >= max ? blocksLastLineNoMargin[p] : blocks[p];
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
    let y, x, r, p;
    const white = Array(cellSize + 1).join("██");
    const black = Array(cellSize + 1).join("  ");
    let ascii = "";
    let line = "";
    for (y = 0;y < size; y += 1) {
      r = Math.floor((y - min) / cellSize);
      line = "";
      for (x = 0;x < size; x += 1) {
        p = 1;
        if (min <= x && x < max && min <= y && y < max && _this.isDark(r, Math.floor((x - min) / cellSize))) {
          p = 0;
        }
        line += p ? white : black;
      }
      for (r = 0;r < cellSize; r += 1) {
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
qrcode.stringToBytes = function(s) {
  const bytes = [];
  for (let i = 0;i < s.length; i += 1) {
    const c = s.charCodeAt(i);
    bytes.push(c & 255);
  }
  return bytes;
};
qrcode.createStringToBytes = function(unicodeData, numChars) {
  const unicodeMap = function() {
    const bin = base64DecodeInputStream(unicodeData);
    const read2 = function() {
      const b = bin.read();
      if (b == -1)
        throw "eof";
      return b;
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
      const k = String.fromCharCode(b0 << 8 | b1);
      const v = b2 << 8 | b3;
      unicodeMap2[k] = v;
      count += 1;
    }
    if (count != numChars) {
      throw count + " != " + numChars;
    }
    return unicodeMap2;
  }();
  const unknownChar = 63;
  return function(s) {
    const bytes = [];
    for (let i = 0;i < s.length; i += 1) {
      const c = s.charCodeAt(i);
      if (c < 128) {
        bytes.push(c);
      } else {
        const b = unicodeMap[s.charAt(i)];
        if (typeof b == "number") {
          if ((b & 255) == b) {
            bytes.push(b);
          } else {
            bytes.push(b >>> 8);
            bytes.push(b & 255);
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
    let d = data << 10;
    while (getBCHDigit(d) - getBCHDigit(G15) >= 0) {
      d ^= G15 << getBCHDigit(d) - getBCHDigit(G15);
    }
    return (data << 10 | d) ^ G15_MASK;
  };
  _this.getBCHTypeNumber = function(data) {
    let d = data << 12;
    while (getBCHDigit(d) - getBCHDigit(G18) >= 0) {
      d ^= G18 << getBCHDigit(d) - getBCHDigit(G18);
    }
    return data << 12 | d;
  };
  _this.getPatternPosition = function(typeNumber) {
    return PATTERN_POSITION_TABLE[typeNumber - 1];
  };
  _this.getMaskFunction = function(maskPattern) {
    switch (maskPattern) {
      case QRMaskPattern.PATTERN000:
        return function(i, j) {
          return (i + j) % 2 == 0;
        };
      case QRMaskPattern.PATTERN001:
        return function(i, j) {
          return i % 2 == 0;
        };
      case QRMaskPattern.PATTERN010:
        return function(i, j) {
          return j % 3 == 0;
        };
      case QRMaskPattern.PATTERN011:
        return function(i, j) {
          return (i + j) % 3 == 0;
        };
      case QRMaskPattern.PATTERN100:
        return function(i, j) {
          return (Math.floor(i / 2) + Math.floor(j / 3)) % 2 == 0;
        };
      case QRMaskPattern.PATTERN101:
        return function(i, j) {
          return i * j % 2 + i * j % 3 == 0;
        };
      case QRMaskPattern.PATTERN110:
        return function(i, j) {
          return (i * j % 2 + i * j % 3) % 2 == 0;
        };
      case QRMaskPattern.PATTERN111:
        return function(i, j) {
          return (i * j % 3 + (i + j) % 2) % 2 == 0;
        };
      default:
        throw "bad maskPattern:" + maskPattern;
    }
  };
  _this.getErrorCorrectPolynomial = function(errorCorrectLength) {
    let a = qrPolynomial([1], 0);
    for (let i = 0;i < errorCorrectLength; i += 1) {
      a = a.multiply(qrPolynomial([1, QRMath.gexp(i)], 0));
    }
    return a;
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
        for (let r = -1;r <= 1; r += 1) {
          if (row + r < 0 || moduleCount <= row + r) {
            continue;
          }
          for (let c = -1;c <= 1; c += 1) {
            if (col + c < 0 || moduleCount <= col + c) {
              continue;
            }
            if (r == 0 && c == 0) {
              continue;
            }
            if (dark == qrcode2.isDark(row + r, col + c)) {
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
  for (let i = 0;i < 8; i += 1) {
    EXP_TABLE[i] = 1 << i;
  }
  for (let i = 8;i < 256; i += 1) {
    EXP_TABLE[i] = EXP_TABLE[i - 4] ^ EXP_TABLE[i - 5] ^ EXP_TABLE[i - 6] ^ EXP_TABLE[i - 8];
  }
  for (let i = 0;i < 255; i += 1) {
    LOG_TABLE[EXP_TABLE[i]] = i;
  }
  const _this = {};
  _this.glog = function(n) {
    if (n < 1) {
      throw "glog(" + n + ")";
    }
    return LOG_TABLE[n];
  };
  _this.gexp = function(n) {
    while (n < 0) {
      n += 255;
    }
    while (n >= 256) {
      n -= 255;
    }
    return EXP_TABLE[n];
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
    for (let i = 0;i < num.length - offset; i += 1) {
      _num2[i] = num[i + offset];
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
  _this.multiply = function(e) {
    const num2 = new Array(_this.getLength() + e.getLength() - 1);
    for (let i = 0;i < _this.getLength(); i += 1) {
      for (let j = 0;j < e.getLength(); j += 1) {
        num2[i + j] ^= QRMath.gexp(QRMath.glog(_this.getAt(i)) + QRMath.glog(e.getAt(j)));
      }
    }
    return qrPolynomial(num2, 0);
  };
  _this.mod = function(e) {
    if (_this.getLength() - e.getLength() < 0) {
      return _this;
    }
    const ratio = QRMath.glog(_this.getAt(0)) - QRMath.glog(e.getAt(0));
    const num2 = new Array(_this.getLength());
    for (let i = 0;i < _this.getLength(); i += 1) {
      num2[i] = _this.getAt(i);
    }
    for (let i = 0;i < e.getLength(); i += 1) {
      num2[i] ^= QRMath.gexp(QRMath.glog(e.getAt(i)) + ratio);
    }
    return qrPolynomial(num2, 0).mod(e);
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
    for (let i = 0;i < length; i += 1) {
      const count = rsBlock[i * 3 + 0];
      const totalCount = rsBlock[i * 3 + 1];
      const dataCount = rsBlock[i * 3 + 2];
      for (let j = 0;j < count; j += 1) {
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
    for (let i = 0;i < length; i += 1) {
      _this.putBit((num >>> length - i - 1 & 1) == 1);
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
    let i = 0;
    while (i + 2 < data2.length) {
      buffer.put(strToNum(data2.substring(i, i + 3)), 10);
      i += 3;
    }
    if (i < data2.length) {
      if (data2.length - i == 1) {
        buffer.put(strToNum(data2.substring(i, i + 1)), 4);
      } else if (data2.length - i == 2) {
        buffer.put(strToNum(data2.substring(i, i + 2)), 7);
      }
    }
  };
  const strToNum = function(s) {
    let num = 0;
    for (let i = 0;i < s.length; i += 1) {
      num = num * 10 + chatToNum(s.charAt(i));
    }
    return num;
  };
  const chatToNum = function(c) {
    if ("0" <= c && c <= "9") {
      return c.charCodeAt(0) - 48;
    }
    throw "illegal char :" + c;
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
    const s = _data;
    let i = 0;
    while (i + 1 < s.length) {
      buffer.put(getCode(s.charAt(i)) * 45 + getCode(s.charAt(i + 1)), 11);
      i += 2;
    }
    if (i < s.length) {
      buffer.put(getCode(s.charAt(i)), 6);
    }
  };
  const getCode = function(c) {
    if ("0" <= c && c <= "9") {
      return c.charCodeAt(0) - 48;
    } else if ("A" <= c && c <= "Z") {
      return c.charCodeAt(0) - 65 + 10;
    } else {
      switch (c) {
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
          throw "illegal char :" + c;
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
    for (let i = 0;i < _bytes.length; i += 1) {
      buffer.put(_bytes[i], 8);
    }
  };
  return _this;
};
var qrKanji = function(data) {
  const _mode = QRMode.MODE_KANJI;
  const _data = data;
  const stringToBytes = qrcode.stringToBytes;
  (function(c, code) {
    const test = stringToBytes(c);
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
    let i = 0;
    while (i + 1 < data2.length) {
      let c = (255 & data2[i]) << 8 | 255 & data2[i + 1];
      if (33088 <= c && c <= 40956) {
        c -= 33088;
      } else if (57408 <= c && c <= 60351) {
        c -= 49472;
      } else {
        throw "illegal char at " + (i + 1) + "/" + c;
      }
      c = (c >>> 8 & 255) * 192 + (c & 255);
      buffer.put(c, 13);
      i += 2;
    }
    if (i < data2.length) {
      throw "illegal char at " + (i + 1);
    }
  };
  return _this;
};
var byteArrayOutputStream = function() {
  const _bytes = [];
  const _this = {};
  _this.writeByte = function(b) {
    _bytes.push(b & 255);
  };
  _this.writeShort = function(i) {
    _this.writeByte(i);
    _this.writeByte(i >>> 8);
  };
  _this.writeBytes = function(b, off, len) {
    off = off || 0;
    len = len || b.length;
    for (let i = 0;i < len; i += 1) {
      _this.writeByte(b[i + off]);
    }
  };
  _this.writeString = function(s) {
    for (let i = 0;i < s.length; i += 1) {
      _this.writeByte(s.charCodeAt(i));
    }
  };
  _this.toByteArray = function() {
    return _bytes;
  };
  _this.toString = function() {
    let s = "";
    s += "[";
    for (let i = 0;i < _bytes.length; i += 1) {
      if (i > 0) {
        s += ",";
      }
      s += _bytes[i];
    }
    s += "]";
    return s;
  };
  return _this;
};
var base64EncodeOutputStream = function() {
  let _buffer = 0;
  let _buflen = 0;
  let _length = 0;
  let _base64 = "";
  const _this = {};
  const writeEncoded = function(b) {
    _base64 += String.fromCharCode(encode(b & 63));
  };
  const encode = function(n) {
    if (n < 0) {
      throw "n:" + n;
    } else if (n < 26) {
      return 65 + n;
    } else if (n < 52) {
      return 97 + (n - 26);
    } else if (n < 62) {
      return 48 + (n - 52);
    } else if (n == 62) {
      return 43;
    } else if (n == 63) {
      return 47;
    } else {
      throw "n:" + n;
    }
  };
  _this.writeByte = function(n) {
    _buffer = _buffer << 8 | n & 255;
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
      for (let i = 0;i < padlen; i += 1) {
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
      const c = _str.charAt(_pos);
      _pos += 1;
      if (c == "=") {
        _buflen = 0;
        return -1;
      } else if (c.match(/^\s$/)) {
        continue;
      }
      _buffer = _buffer << 6 | decode(c.charCodeAt(0));
      _buflen += 6;
    }
    const n = _buffer >>> _buflen - 8 & 255;
    _buflen -= 8;
    return n;
  };
  const decode = function(c) {
    if (65 <= c && c <= 90) {
      return c - 65;
    } else if (97 <= c && c <= 122) {
      return c - 97 + 26;
    } else if (48 <= c && c <= 57) {
      return c - 48 + 52;
    } else if (c == 43) {
      return 62;
    } else if (c == 47) {
      return 63;
    } else {
      throw "c:" + c;
    }
  };
  return _this;
};
var gifImage = function(width, height) {
  const _width = width;
  const _height = height;
  const _data = new Array(width * height);
  const _this = {};
  _this.setPixel = function(x, y, pixel) {
    _data[y * _width + x] = pixel;
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
    for (let i = 0;i < clearCode; i += 1) {
      table.add(String.fromCharCode(i));
    }
    table.add(String.fromCharCode(clearCode));
    table.add(String.fromCharCode(endCode));
    const byteOut = byteArrayOutputStream();
    const bitOut = bitOutputStream(byteOut);
    bitOut.write(clearCode, bitLength);
    let dataIndex = 0;
    let s = String.fromCharCode(_data[dataIndex]);
    dataIndex += 1;
    while (dataIndex < _data.length) {
      const c = String.fromCharCode(_data[dataIndex]);
      dataIndex += 1;
      if (table.contains(s + c)) {
        s = s + c;
      } else {
        bitOut.write(table.indexOf(s), bitLength);
        if (table.size() < 4095) {
          if (table.size() == 1 << bitLength) {
            bitLength += 1;
          }
          table.add(s + c);
        }
        s = c;
      }
    }
    bitOut.write(table.indexOf(s), bitLength);
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
  for (let y = 0;y < height; y += 1) {
    for (let x = 0;x < width; x += 1) {
      gif.setPixel(x, y, getPixel(x, y));
    }
  }
  const b = byteArrayOutputStream();
  gif.write(b);
  const base64 = base64EncodeOutputStream();
  const bytes = b.toByteArray();
  for (let i = 0;i < bytes.length; i += 1) {
    base64.writeByte(bytes[i]);
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
  setProp(_el$, "pointerEvents", "all");
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
// apps/launcher/src/parts/home-screen.tsx
import { stop } from "srt:dev";

// packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on6 } from "srt:events";
var devicesAccessor2;
function cameraDevices() {
  if (!devicesAccessor2) {
    let [devices, setDevices] = createSignal(listCameras());
    on6("cameraDeviceChange", () => setDevices(listCameras()));
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
  }).catch((e) => setError(e instanceof Error ? e : new Error(String(e))));
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

// apps/launcher/src/parts/home-screen.tsx
import { available as appsAvailable, list, launch, remove, info, clearCache } from "srt:apps";

// apps/launcher/src/parts/puzzle.tsx
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
            e,
            t
          }, _p$) => {
            e !== _p$?.e && setProp(_el$, "d", e, _p$?.e);
            t !== _p$?.t && setProp(_el$, "color", t, _p$?.t);
          });
          return _el$;
        })()
      });
    }
  });
}

// apps/launcher/src/parts/app-icon.tsx
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
            e,
            t,
            a
          }, _p$) => {
            e !== _p$?.e && setProp(_el$, "color", e, _p$?.e);
            t !== _p$?.t && setProp(_el$, "fontFamily", t, _p$?.t);
            a !== _p$?.a && setProp(_el$, "fontSize", a, _p$?.a);
          });
          return _el$;
        }
      });
    },
    children: (d) => (() => {
      var _el$2 = createElement("view", {
        repaintBoundary: true,
        pointerEvents: "all",
        flexShrink: 0
      });
      insert(_el$2, createComponent2(For, {
        get each() {
          return d().draws;
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
        a: [d().width, d().height]
      }), ({
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "width", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "height", t, _p$?.t);
        a !== _p$?.a && setProp(_el$2, "viewBox", a, _p$?.a);
      });
      return _el$2;
    })()
  });
}

// apps/launcher/src/parts/detail-card.tsx
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

// apps/launcher/src/parts/types.ts
function focusRing(focused, radius) {
  if (!focused || !policy.focusRing)
    return {};
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md
  };
}
var LIST_GUTTER = 2;
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

// apps/launcher/src/parts/back-button.tsx
var ARROW_LEFT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12h-14"/></svg>`;
function BackButton(props) {
  return createComponent2(Pressable, {
    focusable: true,
    get onPress() {
      return props.onPress;
    },
    layout: {
      width: TAP_TARGET,
      height: TAP_TARGET,
      alignItems: "center",
      justifyContent: "center"
    },
    style: (s) => ({
      backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
      borderRadius: theme.radius.md,
      ...focusRing(s.focused)
    }),
    get children() {
      return createComponent2(Icon, {
        src: ARROW_LEFT_SVG,
        size: 22
      });
    }
  });
}

// apps/launcher/src/parts/scan-button.tsx
var QR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="5" height="5" x="3" y="3" rx="1"/><rect width="5" height="5" x="16" y="3" rx="1"/><rect width="5" height="5" x="3" y="16" rx="1"/><path d="M21 16h-3a2 2 0 0 0-2 2v3"/><path d="M21 21v.01"/><path d="M12 7v3a2 2 0 0 1-2 2H7"/><path d="M3 12h.01"/><path d="M12 3h.01"/><path d="M12 16v.01"/><path d="M16 12h1"/><path d="M21 12v.01"/><path d="M12 21v-1"/></svg>`;
function ScanButton(props) {
  return createComponent2(Pressable, {
    focusable: true,
    get onPress() {
      return props.onPress;
    },
    layout: {
      width: TAP_TARGET,
      height: TAP_TARGET,
      alignItems: "center",
      justifyContent: "center"
    },
    style: (s) => ({
      backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
      borderRadius: theme.radius.md,
      ...focusRing(s.focused)
    }),
    get children() {
      return createComponent2(Icon, {
        src: QR_SVG,
        size: 22
      });
    }
  });
}

// apps/launcher/src/parts/settings-panel.tsx
import { version as buildVersion, profile as buildProfile, platform as buildPlatform } from "srt:apps";
var MAXIMIZE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/></svg>`;
var MINIMIZE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3"/><path d="M21 8h-3a2 2 0 0 1-2-2V3"/><path d="M3 16h3a2 2 0 0 1 2 2v3"/><path d="M16 21v-3a2 2 0 0 1 2-2h3"/></svg>`;
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
function SettingsPanel(props) {
  let cycleMode = () => props.onMode(THEME_MODES[(THEME_MODES.indexOf(props.mode) + 1) % THEME_MODES.length]);
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
                width: "100%",
                maxWidth: DETAIL_MAX_WIDTH,
                padding: space("xl")
              };
            },
            get children() {
              return [createComponent2(View, {
                layout: {
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: "space-between"
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
                  }), createComponent2(Pressable, {
                    focusable: true,
                    onPress: () => props.onFullscreen(!props.fullscreen),
                    layout: {
                      width: TAP_TARGET,
                      height: TAP_TARGET,
                      alignItems: "center",
                      justifyContent: "center"
                    },
                    style: (s) => ({
                      backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
                      borderRadius: theme.radius.md,
                      ...focusRing(s.focused)
                    }),
                    get children() {
                      return createComponent2(Icon, {
                        get src() {
                          return props.fullscreen ? MINIMIZE_SVG : MAXIMIZE_SVG;
                        },
                        size: 22
                      });
                    }
                  })];
                }
              }), createComponent2(DetailCard, {
                title: "Appearance",
                get children() {
                  return createComponent2(Pressable, {
                    focusable: true,
                    onPress: cycleMode,
                    style: (s) => focusRing(s.focused),
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
                        onChange: (v) => props.onMode(v)
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

// apps/launcher/src/parts/dev-connection.ts
import { on as on7 } from "srt:events";
import { available as devAvailable, connect as devConnect, launchAddress } from "srt:dev";
var available = devAvailable;
var [state, setState] = createSignal("idle");
var [address, setAddress] = createSignal(null);
var [tunneled, setTunneled] = createSignal(false);
var [recents, setRecents] = createSignal([]);
if (available) {
  on7("dev", (e) => {
    setState(e.state);
    setAddress(e.address);
    setTunneled(e.tunneled);
    if (e.recents)
      setRecents(e.recents);
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

// apps/launcher/src/parts/connect-panel.tsx
var DEFAULT_PORT = "34884";
function recentLabel(entry) {
  if (!entry.includes("|"))
    return entry;
  return "ticket " + entry.split("|")[0].slice(0, 8);
}
function ConnectPanel(props) {
  let hostDraft = "";
  let portDraft = DEFAULT_PORT;
  let hasCamera = () => cameraDevices().length > 0;
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
                  return props.onClose;
                }
              }), createComponent2(Text, {
                variant: "heading",
                layout: {
                  flexGrow: 1
                },
                children: "Connect"
              }), createComponent2(Show, {
                get when() {
                  return hasCamera();
                },
                get children() {
                  return createComponent2(ScanButton, {
                    get onPress() {
                      return props.onScan;
                    }
                  });
                }
              })];
            }
          }), createComponent2(Card, {
            title: "Manual",
            get children() {
              return [createComponent2(View, {
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
                    hints: {
                      capitalize: "none",
                      autocorrect: false
                    },
                    onInput: (v) => hostDraft = v,
                    onSubmit: submit
                  }), createComponent2(TextInput, {
                    layout: {
                      width: 96
                    },
                    placeholder: "port",
                    defaultValue: DEFAULT_PORT,
                    hints: {
                      type: "number"
                    },
                    onInput: (v) => portDraft = v,
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
                  return createComponent2(Button, {
                    onPress: submit,
                    children: "Connect"
                  });
                }
              })];
            }
          }), createComponent2(Show, {
            get when() {
              return recentAddresses().length > 0;
            },
            get children() {
              return createComponent2(Card, {
                title: "Recent connections",
                get children() {
                  return createComponent2(View, {
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
                        children: (entry) => createComponent2(Button, {
                          variant: "secondary",
                          onPress: () => props.onDial(entry),
                          get children() {
                            return recentLabel(entry);
                          }
                        })
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

// apps/launcher/src/parts/home-screen.tsx
var GEAR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>`;
var PLAY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 3 20 12 6 21 6 3"/></svg>`;
function formatStamp(ms) {
  if (!ms)
    return "";
  let then = new Date(ms);
  let pad = (n) => String(n).padStart(2, "0");
  let time = `${pad(then.getHours())}:${pad(then.getMinutes())}`;
  let midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
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
  return createComponent2(Pressable, {
    focusable: true,
    get onPress() {
      return props.onPress;
    },
    style: (s) => focusRing(s.focused, theme.radius.lg),
    children: (s) => createComponent2(Card, {
      get layout() {
        return {
          flexDirection: "row",
          alignItems: "center",
          gap: space("lg")
        };
      },
      get style() {
        return {
          backgroundColor: props.active ? theme.color.surfaceAlt : s.hovered ? theme.color.surfaceHover : theme.color.surface
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
  for (let e of entries) {
    let k = key(e);
    let g = groups.get(k);
    if (!g)
      groups.set(k, g = {
        key: k,
        count: 0,
        size: 0
      });
    g.count += 1;
    g.size += e.size;
  }
  return [...groups.values()].sort((a, b) => b.size - a.size);
}
function cacheDomain(url) {
  let m = /^[a-z][a-z0-9+.-]*:\/\/([^/]+)/i.exec(url);
  return m?.[1] ?? "unknown";
}
function amount(count, size) {
  return `${count} file${count === 1 ? "" : "s"}, ${formatSize(size)}`;
}
function AppDetail(props) {
  let [confirming, setConfirming] = createSignal(false);
  createEffect(() => props.app.id, () => {
    setConfirming(false);
  });
  onBack((e) => {
    if (confirming()) {
      e.preventDefault();
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
                  return [createComponent2(BackButton, {
                    get onPress() {
                      return props.onBack;
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
                  return [createComponent2(Button, {
                    onPress: () => props.onLaunch(),
                    children: "Launch"
                  }), createComponent2(Button, {
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
                                  return [createComponent2(Button, {
                                    variant: "ghost",
                                    onPress: () => setConfirming(false),
                                    children: "Cancel"
                                  }), createComponent2(Button, {
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
                children: (d) => [createComponent2(DetailCard, {
                  title: "Storage",
                  get children() {
                    return [createComponent2(DetailRow, {
                      label: "App",
                      get value() {
                        return formatSize(d().installSize);
                      }
                    }), createComponent2(DetailRow, {
                      label: "Files",
                      get value() {
                        return amount(d().files.length, d().files.reduce((sum, f) => sum + f.size, 0));
                      }
                    }), createComponent2(DetailRow, {
                      label: "Data",
                      get value() {
                        return amount(d().data.length, d().dataSize);
                      }
                    }), createComponent2(DetailRow, {
                      label: "Cache",
                      get value() {
                        return amount(d().cache.length, d().cacheSize);
                      }
                    })];
                  }
                }), createComponent2(DetailCard, {
                  title: "Versions",
                  get children() {
                    return createComponent2(For, {
                      get each() {
                        return d().versions;
                      },
                      children: (v) => createComponent2(DetailRow, {
                        get label() {
                          return v.id.slice(0, 12) + (v.current ? " (current)" : "");
                        },
                        get value() {
                          return `${v.solidrtVersion}, ${formatSize(v.size)}`;
                        },
                        get mutedValue() {
                          return !v.current;
                        }
                      })
                    });
                  }
                }), createComponent2(DetailCard, {
                  title: "Files",
                  get children() {
                    return createComponent2(For, {
                      get each() {
                        return d().files;
                      },
                      children: (f) => createComponent2(DetailRow, {
                        get label() {
                          return f.path;
                        },
                        get value() {
                          return formatSize(f.size);
                        }
                      })
                    });
                  }
                }), createComponent2(DetailCard, {
                  title: "Data",
                  get children() {
                    return createComponent2(Show, {
                      get when() {
                        return d().data.length > 0;
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
                            return d().data;
                          },
                          children: (f) => createComponent2(DetailRow, {
                            get label() {
                              return f.path;
                            },
                            get value() {
                              return formatSize(f.size);
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
                        return d().cache.length > 0;
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
                            return groupCache(d().cache, (e) => e.type ?? "unknown");
                          },
                          children: (g) => createComponent2(DetailRow, {
                            get label() {
                              return g.key;
                            },
                            get value() {
                              return amount(g.count, g.size);
                            }
                          })
                        }), createComponent2(Text, {
                          variant: "body",
                          children: "By domain"
                        }), createComponent2(For, {
                          get each() {
                            return groupCache(d().cache, (e) => cacheDomain(e.url));
                          },
                          children: (g) => createComponent2(DetailRow, {
                            get label() {
                              return g.key;
                            },
                            get value() {
                              return amount(g.count, g.size);
                            }
                          })
                        })];
                      }
                    });
                  }
                }), createComponent2(Show, {
                  get when() {
                    return d().cache.length > 0;
                  },
                  get children() {
                    return createComponent2(Button, {
                      variant: "danger",
                      onPress: () => {
                        clearCache(props.app.id);
                        setDetailsGen((n) => n + 1);
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
            gap: space("md"),
            padding: LIST_GUTTER
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
              return createComponent2(Button, {
                variant: "secondary",
                get onPress() {
                  return props.onConnect;
                },
                children: "Connect"
              });
            }
          }), createComponent2(Show, {
            get when() {
              return props.busy;
            },
            get children() {
              return createComponent2(Button, {
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
              return createComponent2(Button, {
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
  let twoPane = () => policy.layout === "twoPane";
  let selectedApp = () => apps().find((a) => a.id === props.selectedId) ?? null;
  let status = () => isConnected() ? `Connected to ${serverAddress()}${isTunneled() ? " (tunneled)" : ""}` : props.notice ?? STATUS_TEXT[connectionState()];
  let doLaunch = (id2) => {
    try {
      launch(id2);
    } catch (e) {
      props.setNotice(e instanceof Error ? e.message : String(e));
    }
  };
  let doRemove = (id2) => {
    try {
      remove(id2);
    } catch (e) {
      props.setNotice(e instanceof Error ? e.message : String(e));
    }
    props.setSelectedId(null);
    setApps(appsAvailable ? list() : []);
  };
  onBack((e) => {
    if (!twoPane() && props.panel == null && selectedApp() != null) {
      e.preventDefault();
      props.setSelectedId(null);
    }
  });
  return createComponent2(SplitView, {
    layout: {
      flexGrow: 1
    },
    listWidth: 380,
    get showDetail() {
      return props.panel === "settings" || props.panel == null && selectedApp() != null;
    },
    get list() {
      return createComponent2(Show, {
        get when() {
          return props.panel !== "connect";
        },
        get fallback() {
          return createComponent2(ConnectPanel, {
            get onDial() {
              return props.onDial;
            },
            get onScan() {
              return props.onScan;
            },
            get onClose() {
              return props.onPanelClose;
            }
          });
        },
        get children() {
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
                      }), createComponent2(View, {
                        layout: {
                          flexDirection: "row",
                          alignItems: "center"
                        },
                        get children() {
                          return [createComponent2(Pressable, {
                            focusable: true,
                            get onPress() {
                              return props.onSettings;
                            },
                            layout: {
                              width: TAP_TARGET,
                              height: TAP_TARGET,
                              alignItems: "center",
                              justifyContent: "center"
                            },
                            style: (s) => ({
                              backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
                              borderRadius: theme.radius.md,
                              ...focusRing(s.focused)
                            }),
                            get children() {
                              return createComponent2(Icon, {
                                src: GEAR_SVG,
                                size: 22
                              });
                            }
                          }), createComponent2(Show, {
                            get when() {
                              return memo2(() => !!(available && cameraDevices().length > 0))() ? !isConnected() : available && cameraDevices().length > 0;
                            },
                            get children() {
                              return createComponent2(ScanButton, {
                                get onPress() {
                                  return props.onScan;
                                }
                              });
                            }
                          })];
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
                        onSelect: (id2) => {
                          if (props.panel === "settings")
                            props.onPanelClose();
                          props.setSelectedId(id2);
                        },
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
                        get onConnect() {
                          return props.onConnect;
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
    },
    get detail() {
      return createComponent2(Show, {
        get when() {
          return props.panel !== "settings";
        },
        get fallback() {
          return createComponent2(SettingsPanel, {
            get mode() {
              return props.themeMode;
            },
            get onMode() {
              return props.onThemeMode;
            },
            get fullscreen() {
              return props.fullscreen;
            },
            get onFullscreen() {
              return props.onFullscreen;
            },
            get onBack() {
              return props.onPanelClose;
            }
          });
        },
        get children() {
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
              onBack: () => props.setSelectedId(null)
            })
          });
        }
      });
    }
  });
}

// apps/launcher/src/parts/scan-screen.tsx
var RETICLE_STROKE = 10;
var RETICLE_RADIUS = 20;
var CLOSE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`;
var SCRIM = "rgba(0, 0, 0, 0.45)";
var SCRIM_HOVER = "rgba(0, 0, 0, 0.65)";
function ScanScreen(props) {
  let cam = createCamera(untrack(() => ({
    scan: ["qr"]
  })));
  createEffect(() => cam.barcode(), (b) => {
    if (b)
      props.onScanned(b.data);
  });
  createEffect(() => cam.error(), (e) => {
    if (e)
      props.onError(e.message);
  });
  let crop = () => {
    let cw = cam.width();
    let ch = cam.height();
    let {
      width: w,
      height: h
    } = env.windowSize;
    if (!cw || !ch || !w || !h)
      return null;
    let scale = Math.max(w / cw, h / ch);
    let srcW = w / scale;
    let srcH = h / scale;
    return {
      w,
      h,
      srcX: (cw - srcW) / 2,
      srcY: (ch - srcH) / 2,
      srcW,
      srcH
    };
  };
  let reticle = () => {
    let {
      width: w,
      height: h
    } = env.windowSize;
    let s = Math.round(Math.min(w, h) * 0.55);
    let l = Math.round(s * 0.18);
    let i = RETICLE_STROKE / 2;
    let r = RETICLE_RADIUS;
    return {
      size: s,
      d: `M${i} ${l} L${i} ${i + r} A ${r} ${r} 0 0 1 ${i + r} ${i} L${l} ${i} ` + `M${s - l} ${i} L${s - i - r} ${i} A ${r} ${r} 0 0 1 ${s - i} ${i + r} L${s - i} ${l} ` + `M${s - i} ${s - l} L${s - i} ${s - i - r} A ${r} ${r} 0 0 1 ${s - i - r} ${s - i} L${s - l} ${s - i} ` + `M${l} ${s - i} L${i + r} ${s - i} A ${r} ${r} 0 0 1 ${i} ${s - i - r} L${i} ${s - l}`
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
        children: (c) => (() => {
          var _el$2 = createElement("texture", {
            position: "absolute"
          });
          effect3(() => ({
            e: cam.texture(),
            t: c().w,
            a: c().h,
            o: c().srcX,
            i: c().srcY,
            n: c().srcW,
            s: c().srcH
          }), ({
            e,
            t,
            a,
            o,
            i,
            n,
            s
          }, _p$) => {
            e !== _p$?.e && setProp(_el$2, "src", e, _p$?.e);
            t !== _p$?.t && setProp(_el$2, "width", t, _p$?.t);
            a !== _p$?.a && setProp(_el$2, "height", a, _p$?.a);
            o !== _p$?.o && setProp(_el$2, "srcX", o, _p$?.o);
            i !== _p$?.i && setProp(_el$2, "srcY", i, _p$?.i);
            n !== _p$?.n && setProp(_el$2, "srcW", n, _p$?.n);
            s !== _p$?.s && setProp(_el$2, "srcH", s, _p$?.s);
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
                        focusable: true,
                        get onPress() {
                          return props.onCancel;
                        },
                        layout: {
                          width: TAP_TARGET,
                          height: TAP_TARGET,
                          alignItems: "center",
                          justifyContent: "center"
                        },
                        style: (s) => ({
                          backgroundColor: s.hovered ? SCRIM_HOVER : SCRIM,
                          borderRadius: TAP_TARGET / 2,
                          ...focusRing(s.focused, TAP_TARGET / 2)
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

// apps/launcher/src/index.tsx
function App() {
  let [themeMode, setThemeMode] = createSignal("system");
  let dark = () => {
    let mode = themeMode();
    if (mode === "system")
      return env.systemTheme !== "light";
    return mode === "dark";
  };
  createEffect(() => dark(), (d) => setTheme(d ? darkTheme : lightTheme));
  let [fullscreen, setFullscreen] = createSignal(false);
  let [screen, setScreen] = createSignal("home");
  let panel = () => {
    let s = screen();
    return s === "settings" || s === "connect" ? s : null;
  };
  let [selectedId, setSelectedId] = createSignal(null);
  let [notice, setNotice] = createSignal(null);
  let [confirmExit, setConfirmExit] = createSignal(false);
  let dial = (addr) => {
    setNotice(null);
    setScreen("home");
    connect(addr);
  };
  onBack((e) => {
    e.preventDefault();
    if (confirmExit()) {
      setConfirmExit(false);
    } else if (screen() !== "home") {
      setScreen("home");
    } else {
      setConfirmExit(true);
    }
  });
  let nav = createFocusNav();
  return createComponent2(Window, {
    title: "SolidRT",
    get fullscreen() {
      return fullscreen();
    },
    layout: {
      flexDirection: "column"
    },
    get style() {
      return {
        backgroundColor: theme.color.background
      };
    },
    get onKeyDown() {
      return nav.onKeyDown;
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
                    onCancel: () => setScreen("connect"),
                    onError: (m) => {
                      setNotice(`Camera: ${m}`);
                      setScreen("home");
                    }
                  });
                }
              }), createComponent2(Match, {
                get when() {
                  return screen() === "home" || panel() != null;
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
                    get panel() {
                      return panel();
                    },
                    get themeMode() {
                      return themeMode();
                    },
                    onThemeMode: setThemeMode,
                    get fullscreen() {
                      return fullscreen();
                    },
                    onFullscreen: setFullscreen,
                    onScan: () => {
                      setNotice(null);
                      setScreen("scan");
                    },
                    onConnect: () => setScreen("connect"),
                    onSettings: () => setScreen("settings"),
                    onPanelClose: () => setScreen("home"),
                    onDial: (addr) => dial(addr)
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
                              return [createComponent2(Button, {
                                variant: "ghost",
                                onPress: () => setConfirmExit(false),
                                children: "Cancel"
                              }), createComponent2(Button, {
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
