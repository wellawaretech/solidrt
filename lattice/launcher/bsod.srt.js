// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/error.js
class NotReadyError extends Error {
  source;
  constructor(r) {
    super();
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

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/constants.js
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

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/lanes.js
var signalLanes = new WeakMap;
var activeLanes = new Set;
function findLane(n) {
  while (n.an)
    n = n.an;
  return n;
}
function mergeLanes(n, e) {
  n = findLane(n);
  e = findLane(e);
  if (n === e)
    return n;
  e.an = n;
  for (const i of e.Pe)
    n.Pe.add(i);
  e.Pe.clear();
  n.tn[0].push(...e.tn[0]);
  n.tn[1].push(...e.tn[1]);
  e.tn[0].length = 0;
  e.tn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.Je;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  n.Je = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.fn) {
    const e = n.fn = currentTransition(n.fn);
    if (e.cn !== true)
      return e;
    n.fn = null;
  }
  return resolveLane(n)?.ve ?? n.ve;
}
function hasActiveOverride(n) {
  return !!(n.be !== undefined && n.be !== NOT_PENDING);
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const r = n.Je;
  if (r) {
    if (r.an) {
      n.Je = e;
      return;
    }
    const t = findLane(r);
    if (activeLanes.has(t)) {
      if (t !== i && !hasActiveOverride(n)) {
        if (i.sn && findLane(i.sn) === t) {
          n.Je = e;
        } else if (t.sn && findLane(t.sn) === i)
          ;
        else
          mergeLanes(i, t);
      }
      return;
    }
  }
  n.Je = e;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Le: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Le: 0,
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
  const t = e.N;
  return transitions.size === 0 && activeLanes.size === 0 && e.Qt.length === 0 && t.Be.length === 0 && t._.length === 0 && t.dn.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.p !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.ge !== NOT_PENDING)
      continue;
    if (e.be !== undefined && e.be !== NOT_PENDING)
      continue;
    if (e.M)
      continue;
    transientStoreNodes.delete(e);
    e.ct?.();
  }
}
function createBatch() {
  return {
    Ie: clock,
    yt: [],
    Lt: new Map,
    Be: [],
    _: [],
    dn: new Set,
    Te: [],
    Bt: {
      Mt: [[], []],
      Qt: []
    },
    cn: false,
    un: new Set
  };
}
function mergeTransitionState(e, t) {
  t.cn = e;
  e.Te.push(...t.Te);
  for (const i of activeLanes)
    if (i.ve === t)
      i.ve = e;
  if (t.Be.length) {
    e.Be.push(...t.Be);
    t.Be.length = 0;
  }
  if (t._.length) {
    e._.push(...t._);
    t._.length = 0;
  }
  for (const i of t.dn)
    e.dn.add(i);
  for (const [i, n] of t.Lt) {
    let t2 = e.Lt.get(i);
    if (!t2)
      e.Lt.set(i, t2 = new Set);
    for (const e2 of n)
      t2.add(e2);
  }
  for (const i of t.un)
    e.un.add(i);
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.Ut && !projectionWriteActive)
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
  Fe = null;
  Mt = [[], []];
  Qt = [];
  created = clock;
  addChild(e) {
    this.Qt.push(e);
    e.Fe = this;
  }
  removeChild(e) {
    const t = this.Qt.indexOf(e);
    if (t >= 0) {
      this.Qt.splice(t, 1);
      e.Fe = null;
    }
  }
  notify(e, t, i, n) {
    if (this.Fe)
      return this.Fe.notify(e, t, i, n);
    return false;
  }
  run(e) {
    if (this.Mt[e - 1].length) {
      const t = this.Mt[e - 1];
      this.Mt[e - 1] = [];
      runQueue(t, e);
    }
    for (let t = 0;t < this.Qt.length; t++)
      this.Qt[t].run?.(e);
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const i = findLane(currentOptimisticLane);
        i.tn[e - 1].push(t);
      } else {
        this.Mt[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.Mt[0].push(...this.Mt[0]);
    e.Mt[1].push(...this.Mt[1]);
    this.Mt = [[], []];
    for (let t = 0;t < this.Qt.length; t++) {
      let i = this.Qt[t];
      let n = e.Qt[t];
      if (!n) {
        n = {
          Mt: [[], []],
          Qt: []
        };
        e.Qt[t] = n;
      }
      i.stashQueues(n);
    }
  }
  restoreQueues(e) {
    this.Mt[0].push(...e.Mt[0]);
    this.Mt[1].push(...e.Mt[1]);
    for (let t = 0;t < e.Qt.length; t++) {
      const i = e.Qt[t];
      let n = this.Qt[t];
      if (n)
        n.restoreQueues(i);
    }
  }
}

class GlobalQueue extends Queue {
  Ut = false;
  N = createBatch();
  static Ce;
  static Oe;
  static ut;
  static Vt = null;
  static T = null;
  static j = null;
  static h = null;
  static D = null;
  static F = null;
  static $ = null;
  static I = null;
  static Ot = null;
  static Rt = null;
  static _e = null;
  static P = null;
  static me = null;
  static R = null;
  static Gt = null;
  static Pt = null;
  static kt = null;
  static tt = null;
  static nt = null;
  static wt = null;
  static bt = null;
  static En = null;
  static Tn = null;
  static In = null;
  static Nn = null;
  static On = null;
  static Dt = null;
  static gt = null;
  static ht = null;
  static vt = null;
  static $e = null;
  static et = null;
  static Xe = null;
  static mn = null;
  flush() {
    if (this.Ut)
      return;
    this.Ut = true;
    try {
      if (false)
        ;
      runHeap(dirtyQueue, GlobalQueue.Ce);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, GlobalQueue.Ce);
          currentBatch = this.N = createBatch();
          if (activeLanes.size) {
            GlobalQueue.On(EFFECT_RENDER);
            GlobalQueue.On(EFFECT_USER);
          }
          this.stashQueues(e2.Bt);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.Le;
          reassignPendingTransition(e2.yt);
          activeTransition = null;
          if (!e2.Te.length && !e2.Lt.size && e2.Be.length) {
            GlobalQueue.Tn(e2);
          } else {
            finalizePureQueue(null, true);
          }
          return;
        }
        const t = activeTransition;
        const i = this.N;
        i !== t && i.yt.push(...t.yt);
        this.restoreQueues(t.Bt);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(i.yt);
        finalizePureQueue(t);
        if (i === t) {
          const e2 = createBatch();
          e2.yt = i.yt;
          e2.Be = i.Be;
          e2._ = i._;
          e2.dn = i.dn;
          currentBatch = this.N = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.Le) {
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
      scheduled = dirtyQueue.EE >= dirtyQueue.Le;
      activeLanes.size && GlobalQueue.On(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && GlobalQueue.On(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
      if (false)
        ;
      if (false)
        ;
    } finally {
      this.Ut = false;
    }
  }
  notify(e, t, i, n) {
    if (t & STATUS_PENDING) {
      if (i & STATUS_PENDING) {
        const t2 = n !== undefined ? n : e.k;
        if (activeTransition && t2) {
          const i2 = t2.source;
          let n2 = activeTransition.Lt.get(i2);
          if (!n2)
            activeTransition.Lt.set(i2, n2 = new Set);
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
    if (!e && activeTransition && activeTransition.Ie === clock)
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
    activeTransition.Ie = clock;
    const t = this.N;
    if (t !== activeTransition) {
      for (let e2 = 0;e2 < t.yt.length; e2++) {
        const i = t.yt[e2];
        i.ve = activeTransition;
        activeTransition.yt.push(i);
      }
      for (let e2 = 0;e2 < t.Be.length; e2++) {
        const i = t.Be[e2];
        i.ve = activeTransition;
        activeTransition.Be.push(i);
      }
      if (t._.length)
        activeTransition._.push(...t._);
      for (const e2 of t.dn)
        activeTransition.dn.add(e2);
      currentBatch = this.N = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.ve)
        e2.ve = activeTransition;
    }
  }
}
function queuePendingNode(e) {
  currentBatch.yt.push(e);
}
var reaskArmed = false;
function insertSubs(e, t = false) {
  const i = e.Je || currentOptimisticLane;
  const n = e.Ye !== undefined;
  const s = reaskArmed;
  for (let r = e.p;r !== null; r = r.de) {
    if (s)
      r.Ee.u &= ~REACTIVE_REASK;
    if (n && r.Ee.U & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.Ee.u |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && i) {
      r.Ee.u |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.Ee, i);
    } else if (t) {
      r.Ee.u |= REACTIVE_OPTIMISTIC_DIRTY;
      r.Ee.Je = undefined;
    }
    enqueueSub(r.Ee);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.xe) {
    if (e.ge !== NOT_PENDING) {
      e.Ue = e.ge;
      e.ge = NOT_PENDING;
    }
    if (e.ye || e.pe)
      GlobalQueue.R(e);
    return;
  }
  if (e.ge !== NOT_PENDING) {
    e.Ue = e.ge;
    e.ge = NOT_PENDING;
    if (e.De && e.De !== EFFECT_TRACKED)
      e.it = true;
  }
  t.u &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.i & STATUS_PENDING))
    t.i &= ~STATUS_UNINITIALIZED;
  if (t.Ze !== null || t.We !== null)
    GlobalQueue.Oe(t, false, true);
  if (e.ye || e.pe)
    GlobalQueue.R(e);
}
function commitPendingNodes() {
  const e = currentBatch.yt;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
}
function finalizePureQueue(e = null, t = false) {
  const i = !t;
  if (i)
    commitPendingNodes();
  if (!t && globalQueue.Qt.length)
    checkBoundaryChildren(globalQueue);
  const n = dirtyQueue.EE >= dirtyQueue.Le;
  if (n)
    runHeap(dirtyQueue, GlobalQueue.Ce);
  if (i) {
    if (n)
      commitPendingNodes();
    const t2 = e ?? globalQueue.N;
    if (t2.Be.length)
      GlobalQueue.En(t2.Be);
    if (e && e.un.size) {
      for (const t3 of e.un) {
        if (t3.u & REACTIVE_DISPOSED)
          continue;
        enqueueSub(t3);
      }
      e.un.clear();
    }
    if (t2._.length)
      GlobalQueue.h(t2._);
    if (t2.dn.size)
      GlobalQueue.Vt(t2.dn, e);
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.Nn(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.Qt) {
    t.Se?.();
    checkBoundaryChildren(t);
  }
}
var activeAffectsMarks = 0;
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].ve = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
var currentBatch = globalQueue.N;
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
  if (globalQueue.Ut) {
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
  if (e.u & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.m?.has(t))
    return true;
  for (let i = e.S;i; i = i.st) {
    let e2 = i.ot;
    while (e2) {
      if (e2 === t || e2.rt === t)
        return true;
      e2 = e2.en;
    }
  }
  return !!(e.i & STATUS_PENDING && e.k instanceof NotReadyError && e.k.source === t);
}
function transitionComplete(e) {
  if (e.cn)
    return true;
  if (e.Te.length)
    return false;
  let t = true;
  for (const [i, n] of e.Lt) {
    let s = false;
    for (const e2 of n) {
      if (reporterBlocksSource(e2, i)) {
        s = true;
        break;
      }
      n.delete(e2);
    }
    if (!s)
      e.Lt.delete(i);
    else if (i.i & STATUS_PENDING && i.k?.source === i) {
      t = false;
      break;
    }
  }
  if (t && e.Be.length && GlobalQueue.In(e))
    t = false;
  t && (e.cn = true);
  return t;
}
function currentTransition(e) {
  while (e.cn && typeof e.cn === "object")
    e = e.cn;
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

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.u & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  if (e.De === EFFECT_TRACKED) {
    const E2 = e;
    if (!E2.it) {
      E2.it = true;
      E2.v.enqueue(EFFECT_USER, E2.Ft);
    }
    return;
  }
  const E = queueFor(e);
  if (E.Le > e.qe)
    E.Le = e.qe;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.Fe?.At ? e.Fe.Ct?.qe : e.Fe?.qe) ?? -1;
  if (t >= e.qe)
    e.qe = t + 1;
  const n = e.qe;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.ft;
    E2._t = e;
    e.ft = E2;
    I.ft = e;
  }
  if (n > E.EE)
    E.EE = n;
}
function insertIntoHeap(e, E) {
  let t = e.u;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (t & REACTIVE_CHECK) {
    e.u = t & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else
    e.u = t | REACTIVE_IN_HEAP;
  if (!(t & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, E);
}
function insertIntoHeapHeight(e, E) {
  let t = e.u;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.u = t | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, E);
}
function deleteFromHeap(e, E) {
  const t = e.u;
  if (!(t & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.u = t & -25;
  const n = e.qe;
  if (e.ft === e)
    E.eE[n] = undefined;
  else {
    const t2 = e._t;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.ft._t = t2;
    o.ft = e.ft;
  }
  e.ft = e;
  e._t = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t._t) {
      if (t.u & REACTIVE_IN_HEAP)
        markNode(t);
    }
  }
}
function markNode(e, E = REACTIVE_DIRTY) {
  const t = e.u;
  if ((t & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= E)
    return;
  e.u = t & -4 | E;
  for (let E2 = e.p;E2 !== null; E2 = E2.de) {
    markNode(E2.Ee, REACTIVE_CHECK);
  }
  if (e.G !== null) {
    for (let E2 = e.G;E2 !== null; E2 = E2.Ne) {
      for (let e2 = E2.p;e2 !== null; e2 = e2.de) {
        markNode(e2.Ee, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.Le = 0;e.Le <= e.EE; e.Le++) {
    let t = e.eE[e.Le];
    while (t !== undefined) {
      if (t.u & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.Le];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.qe;
  for (let E2 = e.S;E2; E2 = E2.st) {
    const e2 = E2.ot;
    const n = e2.rt || e2;
    if (n.xe && n.qe >= t)
      t = n.qe + 1;
  }
  if (e.qe !== t) {
    e.qe = t;
    for (let E2 = e.p;E2 !== null; E2 = E2.de) {
      insertIntoHeapHeight(E2.Ee, queueFor(E2.Ee));
    }
  }
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/owner.js
function markDisposal(e) {
  let n = e.He;
  while (n) {
    const e2 = n.u;
    n.u = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(n, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(n, zombieQueue);
      else
        insertIntoHeapHeight(n, zombieQueue);
    }
    markDisposal(n);
    n = n.Ve;
  }
}
function disposeChildren(e, n = false, t) {
  const i = e.u;
  if (i & REACTIVE_DISPOSED)
    return;
  if (n) {
    e.u = i | REACTIVE_DISPOSED;
    const n2 = e;
    if (n2.ye || n2.pe)
      GlobalQueue.R(n2);
  }
  if (n && e.xe)
    e.Ae = null;
  let l = t ? e.Ze : e.He;
  while (l) {
    const e2 = l.Ve;
    if (l.S) {
      const e3 = l;
      deleteFromHeap(e3, queueFor(e3));
      let n2 = e3.S;
      do {
        n2 = unlinkSubs(n2);
      } while (n2 !== null);
      e3.S = null;
      e3.Ke = null;
    }
    disposeChildren(l, true);
    l = e2;
  }
  if (t) {
    e.Ze = null;
  } else {
    e.He = null;
    e.je = 0;
  }
  if (n && !t && !(i & REACTIVE_ZOMBIE) && e.Fe !== null && !(e.Fe.u & REACTIVE_DISPOSED)) {
    const n2 = e.Nt;
    const t2 = e.Ve;
    if (n2 !== null)
      n2.Ve = t2;
    else
      e.Fe.He = t2;
    if (t2 !== null)
      t2.Nt = n2;
    e.Nt = null;
  }
  runDisposal(e, t);
  if (n && e.dt) {
    const n2 = e.dt;
    e.dt = undefined;
    n2();
  }
}
function runDisposal(e, n) {
  let t = n ? e.We : e.Me;
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
  n ? e.We = null : e.Me = null;
}
function childId(e, n) {
  let t = e;
  while (t.U & CONFIG_TRANSPARENT && t.Fe)
    t = t.Fe;
  if (t.id != null)
    return formatId(t.id, n ? t.je++ : t.je);
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
  if (!context.Me)
    context.Me = e;
  else if (Array.isArray(context.Me))
    context.Me.push(e);
  else
    context.Me = [context.Me, e];
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
    U: t ? CONFIG_TRANSPARENT : 0,
    At: true,
    Ct: n?.At ? n.Ct : n,
    He: null,
    Ve: null,
    Nt: null,
    Me: null,
    v: n?.v ?? globalQueue,
    we: n?.we || defaultContext,
    je: 0,
    We: null,
    Ze: null,
    Fe: n,
    dispose: disposeRootSelf
  };
  if (n) {
    const e2 = n.He;
    if (e2 === null) {
      n.He = i;
    } else {
      i.Ve = e2;
      e2.Nt = i;
      n.He = i;
    }
  }
  return i;
}
function createRoot(e, n) {
  const t = createOwner(n);
  return runWithOwner(t, () => e(() => t.dispose()));
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(n) {
  const l = n.ot;
  const e = n.st;
  const u = n.de;
  const s = n.nn;
  if (u !== null)
    u.nn = s;
  else
    l.Et = s;
  if (s !== null)
    s.de = u;
  else {
    l.p = u;
    if (u === null) {
      l.ct?.();
      const n2 = l;
      n2.xe && n2.U & CONFIG_AUTO_DISPOSE && !(n2.u & REACTIVE_ZOMBIE) && unobserved(n2);
    }
  }
  return e;
}
function trimStaleDeps(n) {
  const l = n.Ke;
  let e = l !== null ? l.st : n.S;
  if (e !== null) {
    do {
      e = unlinkSubs(e);
    } while (e !== null);
    if (l !== null)
      l.st = null;
    else
      n.S = null;
  }
}
function unobserved(n) {
  deleteFromHeap(n, queueFor(n));
  let l = n.S;
  while (l !== null) {
    l = unlinkSubs(l);
  }
  n.S = null;
  n.Ke = null;
  disposeChildren(n, true);
}
function link(n, l, e = false) {
  const u = l.Ke;
  if (u !== null && u.ot === n) {
    u.Qe = e;
    return;
  }
  let s = null;
  const t = l.u & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = u !== null ? u.st : l.S;
    if (s !== null && s.ot === n) {
      s.ln = l.ze;
      l.Ke = s;
      s.Qe = e;
      return;
    }
  }
  const i = n.Et;
  if (i !== null && i.Ee === l && (!t || i.ln === l.ze)) {
    i.Qe = e;
    return;
  }
  const o = l.Ke = n.Et = {
    ot: n,
    Ee: l,
    st: s,
    nn: i,
    de: null,
    ln: l.ze,
    Qe: e
  };
  if (u !== null)
    u.st = o;
  else
    l.S = o;
  if (i !== null)
    i.de = o;
  else
    n.p = o;
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.m?.has(n))
    return false;
  (e.m ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  if (!e.m?.delete(n))
    return false;
  if (e.m.size === 0)
    e.m = undefined;
  return true;
}
function clearPendingSources(e) {
  e.m?.clear();
  e.m = undefined;
}
function setPendingError(e, n, r) {
  if (!n) {
    e.k = null;
    return;
  }
  if (r instanceof NotReadyError && r.source === n) {
    e.k = r;
    return;
  }
  const t = e.k;
  if (!(t instanceof NotReadyError) || t.source !== n) {
    e.k = new NotReadyError(n);
  }
}
function forEachDependent(e, n) {
  for (let r = e.p;r !== null; r = r.de)
    n(r.Ee, r);
  for (let r = e.G ?? null;r !== null; r = r.Ne) {
    for (let e2 = r.p;e2 !== null; e2 = e2.de)
      n(e2.Ee, e2);
  }
}
function settlePendingSource(e, n = e, r = false) {
  let t = false;
  const o = new Set;
  const u = r ? GlobalQueue.R : GlobalQueue.P;
  const settle = (e2) => {
    if (o.has(e2) || !removePendingSource(e2, n))
      return;
    o.add(e2);
    e2.Ie = clock;
    const r2 = e2.m?.values().next().value;
    if (r2) {
      setPendingError(e2, r2);
      u !== null && u(e2);
    } else {
      e2.i &= ~STATUS_PENDING;
      setPendingError(e2);
      u !== null && u(e2);
      if (e2.Re) {
        enqueueSub(e2);
        t = true;
      }
      e2.Re = false;
    }
    forEachDependent(e2, settle);
  };
  forEachDependent(e, settle);
  if (t)
    schedule();
}
function isThenable(e) {
  return e != null && typeof e === "object" && typeof e.then === "function";
}
function handleAsync(e, n, r) {
  let t = false;
  let o = false;
  if (typeof n === "object" && n !== null) {
    untrack(() => {
      t = n[Symbol.asyncIterator];
      o = !t && isThenable(n);
    });
  }
  if (!o && !t) {
    e.Ae = null;
    return n;
  }
  e.Ae = n;
  let u;
  const handleError = (r2) => {
    if (e.Ae !== n)
      return;
    globalQueue.initTransition(resolveTransition(e));
    notifyStatus(e, r2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, r2);
    e.Ie = clock;
  };
  const asyncWrite = (t2, o2) => {
    if (e.Ae !== n)
      return;
    if (e.u & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    globalQueue.initTransition(resolveTransition(e));
    const u2 = !!(e.i & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const l = resolveLane(e);
    if (l)
      l.Pe.delete(e);
    if (r) {
      r(t2);
      if (u2)
        clearStatus(e, true);
    } else if (e.be !== undefined) {
      if (e.ge === NOT_PENDING)
        queuePendingNode(e);
      e.ge = t2;
      GlobalQueue._e !== null && GlobalQueue._e(e, t2);
      if (!hasActiveOverride(e))
        insertSubs(e);
      e.Ie = clock;
    } else if (l) {
      const n2 = e.De;
      const r2 = e.Ue;
      const o3 = e.Ge;
      try {
        if (!n2 && u2 || !o3 || !o3(t2, r2)) {
          e.Ue = t2;
          e.Ie = clock;
          GlobalQueue._e !== null && GlobalQueue._e(e, t2);
          insertSubs(e, true);
        }
      } catch (n3) {
        notifyStatus(e, STATUS_ERROR, n3);
      }
    } else {
      try {
        setSignal(e, () => t2);
      } catch (n2) {
        notifyStatus(e, STATUS_ERROR, n2);
      }
    }
    settlePendingSource(e);
    schedule();
    flush();
    o2?.();
  };
  if (o) {
    let r2 = false, t2 = false, o2, l = true;
    n.then((e2) => {
      if (l) {
        u = e2;
        r2 = true;
      } else
        asyncWrite(e2);
    }, (e2) => {
      if (l) {
        o2 = e2;
        t2 = true;
      } else
        handleError(e2);
    });
    l = false;
    if (t2) {
      handleError(o2);
      throw o2;
    } else if (!r2) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  if (t) {
    const r2 = n[Symbol.asyncIterator]();
    let t2 = false;
    let o2 = false;
    let l = true;
    cleanup(() => {
      if (o2)
        return;
      o2 = true;
      try {
        const e2 = r2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    });
    const iterate = () => {
      let i2, s, f = false, a = false, c = true;
      r2.next().then((r3) => {
        if (c) {
          i2 = r3;
          f = true;
          if (r3.done)
            o2 = true;
        } else if (e.Ae !== n) {
          return;
        } else if (!r3.done) {
          t2 = true;
          asyncWrite(r3.value, iterate);
        } else {
          o2 = true;
          if (t2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
        }
      }, (r3) => {
        if (c) {
          s = r3;
          a = true;
        } else if (e.Ae === n) {
          o2 = true;
          handleError(r3);
        }
      });
      c = false;
      if (a) {
        o2 = true;
        handleError(s);
        if (l)
          throw s;
        return true;
      }
      if (f && !i2.done) {
        u = i2.value;
        t2 = true;
        return iterate();
      }
      return f && i2.done;
    };
    const i = iterate();
    l = false;
    if (!t2 && !i) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  return u;
}
function clearStatus(e, n = false) {
  if (e.m)
    clearPendingSources(e);
  if (e.Re)
    e.Re = false;
  e.A = false;
  e.i = n ? 0 : e.i & STATUS_UNINITIALIZED;
  if (e.k)
    setPendingError(e);
  if (e.ye || e.pe)
    GlobalQueue.P(e);
  if (e.G && GlobalQueue.me !== null)
    GlobalQueue.me(e);
  if (e.C)
    e.C();
}
function notifyStatus(e, n, r, t, o) {
  if (n === STATUS_ERROR && !(r instanceof StatusError) && !(r instanceof NotReadyError))
    r = new StatusError(e, r);
  const u = n === STATUS_PENDING && r instanceof NotReadyError ? r.source : undefined;
  const l = u?.l !== undefined;
  if (l && e.i & STATUS_ERROR)
    return;
  const i = u === e;
  const s = n === STATUS_PENDING && e.be !== undefined && !i;
  const f = s && hasActiveOverride(e);
  if (!t) {
    if (n === STATUS_PENDING && u) {
      addPendingSource(e, u);
      e.i = STATUS_PENDING | e.i & STATUS_UNINITIALIZED;
      setPendingError(e, u, r);
    } else {
      clearPendingSources(e);
      e.i = n | (n !== STATUS_ERROR ? e.i & STATUS_UNINITIALIZED : 0);
      e.k = r;
    }
    GlobalQueue.P !== null && GlobalQueue.P(e);
    if (e.G && GlobalQueue.me !== null)
      GlobalQueue.me(e);
  }
  if (o && !t) {
    assignOrMergeLane(e, o);
  }
  const a = t || f;
  const c = t || s ? undefined : o;
  if (e.C) {
    if (t && n === STATUS_PENDING) {
      return;
    }
    if (a) {
      e.C(n, r);
    } else {
      e.C();
    }
    return;
  }
  forEachDependent(e, (e2, t2) => {
    e2.Ie = clock;
    if (n === STATUS_PENDING && u && !e2.m?.has(u) || n !== STATUS_PENDING && (e2.k !== r || e2.m)) {
      if (t2.Qe && n !== STATUS_PENDING && !(r instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!a && !l && !e2.ve)
        queuePendingNode(e2);
      notifyStatus(e2, n, r, a, c);
    }
  });
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.Ce = recompute;
GlobalQueue.Oe = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var affectsReads = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.ke)
      return true;
    e = e.Fe;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.De;
  if (!t) {
    if (e.ve && (!n || activeTransition) && activeTransition !== e.ve)
      globalQueue.initTransition(e.ve);
    deleteFromHeap(e, queueFor(e));
    e.Ae = null;
    if (e.ve || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.He !== null || e.Me !== null) {
      markDisposal(e);
      e.We = e.Me;
      e.Ze = e.He;
      e.Me = null;
      e.He = null;
      e.je = 0;
    }
  }
  let i = !!(e.u & REACTIVE_OPTIMISTIC_DIRTY);
  const l = e.be !== undefined && e.be !== NOT_PENDING;
  const u = !!(e.i & STATUS_UNINITIALIZED);
  const s = (e.u & REACTIVE_REASK) !== 0;
  const o = context;
  context = e;
  e.Ke = null;
  e.ze++;
  e.u = REACTIVE_RECOMPUTING_DEPS;
  e.Ie = clock;
  let a = e.ge === NOT_PENDING ? e.Ue : e.ge;
  let r = e.qe;
  let c = tracking;
  const _ = affectsReads;
  affectsReads = null;
  let f = null;
  let E = currentOptimisticLane;
  tracking = true;
  if (i) {
    const t2 = GlobalQueue.$e(e, true);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.Be.length) {
    const t2 = GlobalQueue.$e(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const N = n && n !== EFFECT_USER;
  const T = stale;
  if (N)
    stale = true;
  try {
    if (e.U & CONFIG_SYNC) {
      a = e.xe(a);
      e.Ae = null;
    } else {
      const t2 = e.Ae;
      const n2 = e.xe(a);
      const i2 = typeof n2 === "object" && n2 !== null;
      const l2 = e.Ae !== t2;
      a = l2 || !i2 ? n2 : handleAsync(e, n2);
      if (!l2 && !i2)
        e.Ae = null;
    }
    clearStatus(e, t);
    if (e.Je)
      GlobalQueue.Xe(e);
  } catch (t2) {
    if (t2 instanceof NotReadyError && currentOptimisticLane)
      GlobalQueue.et(e);
    let n2 = false;
    if (t2 instanceof NotReadyError) {
      e.Re = true;
      if (GlobalQueue.tt !== null)
        n2 = GlobalQueue.tt(e, s);
    }
    notifyStatus(e, t2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, t2, undefined, t2 instanceof NotReadyError ? e.Je : undefined);
    if (n2)
      GlobalQueue.nt(e);
  } finally {
    tracking = c;
    if (N)
      stale = T;
    e.u = REACTIVE_NONE | (t ? e.u & REACTIVE_SNAPSHOT_STALE : 0);
    context = o;
    f = affectsReads;
    affectsReads = _;
  }
  if (!e.k) {
    trimStaleDeps(e);
    const s2 = l ? unwrapOverride(e.be) : e.ge === NOT_PENDING ? e.Ue : e.ge;
    let o2 = false;
    try {
      o2 = !n && u || !e.Ge || !e.Ge(s2, a);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && o2) {
      e.it = !e.k;
      if (!t)
        e.v.enqueue(n, e.lt ??= GlobalQueue.ut.bind(null, e));
    }
    if (e.k)
      ;
    else if (o2) {
      const u2 = l ? e.be : undefined;
      if (t || n && activeTransition !== e.ve || i) {
        e.Ue = a;
        if (l && i) {
          e.be = a === undefined ? OVERRIDE_UNDEFINED : a;
          e.ge = NOT_PENDING;
        }
      } else {
        e.ge = a;
        if ((activeTransition || e.ve) && GlobalQueue._e !== null)
          GlobalQueue._e(e, a);
      }
      if (!l || i || e.be !== u2)
        insertSubs(e, i || l);
    } else if (l) {
      if (e.ge === NOT_PENDING)
        queuePendingNode(e);
      e.ge = a;
    } else if (e.qe != r) {
      for (let t2 = e.p;t2 !== null; t2 = t2.de) {
        insertIntoHeapHeight(t2.Ee, queueFor(t2.Ee));
      }
    }
  }
  currentOptimisticLane = E;
  if (f && !(e.i & STATUS_ERROR))
    GlobalQueue.j(e, f);
  const I = e.i & (STATUS_PENDING | STATUS_UNINITIALIZED);
  const p = e.ge !== NOT_PENDING || e.Ze !== null || e.We !== null || I !== 0 && (I !== STATUS_PENDING || activeAffectsMarks === 0 || !GlobalQueue.$(e));
  p && (!t || e.i & STATUS_PENDING) && (!e.ve || l) && queuePendingNode(e);
  e.ve && n && activeTransition !== e.ve && runInTransition(e.ve, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.u & REACTIVE_CHECK) {
    for (let t = e.S;t; t = t.st) {
      const n = t.ot;
      const i = n.rt || n;
      if (i.xe) {
        updateIfNecessary(i);
      }
      if (e.u & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.u & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.k && e.Ie < clock && !e.Ae) {
    recompute(e);
  }
  e.u = e.u & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = {
    id: inheritId(t, n, context),
    U: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.re ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ge: t?.equals != null ? t.equals : isEqual,
    ct: t?.unobserved,
    Me: null,
    v: context?.v ?? globalQueue,
    we: context?.we ?? defaultContext,
    je: 0,
    xe: e,
    Ue: undefined,
    qe: 0,
    G: null,
    _t: undefined,
    ft: null,
    S: null,
    Ke: null,
    ze: 0,
    p: null,
    Et: null,
    Fe: context,
    Ve: null,
    Nt: null,
    He: null,
    u: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    i: STATUS_UNINITIALIZED,
    Ie: clock,
    ge: NOT_PENDING,
    We: null,
    Ze: null,
    Ae: null,
    ve: null,
    A: false
  };
  setupComputedNode(i, t);
  return i;
}
function createEffectNode(e, t, n, i, l, u) {
  const s = u?.transparent ?? false;
  const o = {
    id: inheritId(u, s, context),
    U: (s ? CONFIG_TRANSPARENT : 0) | (u?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (u?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ge: false,
    ct: u?.unobserved,
    Me: null,
    v: context?.v ?? globalQueue,
    we: context?.we ?? defaultContext,
    je: 0,
    xe: e,
    Ue: undefined,
    qe: 0,
    G: null,
    _t: undefined,
    ft: null,
    S: null,
    Ke: null,
    ze: 0,
    p: null,
    Et: null,
    Fe: context,
    Ve: null,
    Nt: null,
    He: null,
    u: REACTIVE_LAZY,
    i: STATUS_UNINITIALIZED,
    Ie: clock,
    ge: NOT_PENDING,
    We: null,
    Ze: null,
    Ae: null,
    ve: null,
    A: false,
    it: false,
    Tt: undefined,
    It: t,
    St: n,
    dt: undefined,
    De: i,
    C: l
  };
  setupComputedNode(o, lazyOptions);
  return o;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.ft = e;
  const n = context?.At ? context.Ct : context;
  if (context) {
    const t2 = context.He;
    if (t2 === null) {
      context.He = e;
    } else {
      e.Ve = t2;
      t2.Nt = e;
      context.He = e;
    }
  }
  if (n)
    e.qe = n.qe + 1;
  if (GlobalQueue.Ot !== null)
    GlobalQueue.Ot(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.i & STATUS_PENDING) && !(e.U & CONFIG_NO_SNAPSHOT)) {
      e.Ye = e.Ue === undefined ? NO_SNAPSHOT : e.Ue;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    Ge: t?.equals != null ? t.equals : isEqual,
    U: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.re ? CONFIG_NO_SNAPSHOT : 0),
    ct: t?.unobserved,
    Ue: e,
    p: null,
    Et: null,
    Ie: clock,
    rt: n,
    Ne: n?.G || null,
    ge: NOT_PENDING
  };
  n && (n.G = i);
  if (snapshotCaptureActive && !(i.U & CONFIG_NO_SNAPSHOT) && !((n?.i ?? 0) & STATUS_PENDING)) {
    i.Ye = e === undefined ? NO_SNAPSHOT : e;
    snapshotSources.add(i);
  }
  return i;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (GlobalQueue.Rt === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.Rt !== null)
      return GlobalQueue.Rt(e);
    return e();
  } finally {
    tracking = n;
  }
}
function prepareComputed(e, t) {
  if (e.u & REACTIVE_LAZY) {
    e.u &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.u & REACTIVE_DISPOSED) {
    recompute(e, true);
  } else if (t) {
    updateIfNecessary(e);
  }
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.Gt(e);
  let t = context;
  if (t?.At)
    t = t.Ct;
  const n = e;
  const i = e.rt;
  const l = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Pt(e, t, l, i);
  } else if (typeof n.xe === "function") {
    prepareComputed(e, false);
  }
  if (!n.xe && l === e && e.be === undefined && e.Ye === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking) {
      link(e, t);
      if (activeAffectsMarks !== 0 && e.M && !pendingCheckActive)
        (affectsReads ??= []).push(e);
    }
    return !t || e.ge === NOT_PENDING ? e.Ue : e.ge;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (activeAffectsMarks !== 0 && !pendingCheckActive) {
      if (e.M)
        (affectsReads ??= []).push(e);
      if (l.i & STATUS_PENDING)
        GlobalQueue.I(l, affectsReads ??= []);
    }
    if (l.xe) {
      const n2 = queueFor(e);
      if (l.qe >= n2.Le) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(l);
      }
      const i2 = l.qe;
      if (i2 >= t.qe && e.Fe !== t) {
        t.qe = i2 + 1;
      }
    }
  }
  if (l.i & STATUS_PENDING && !(activeAffectsMarks !== 0 && GlobalQueue.$(l))) {
    if (t && !(stale && l.ve && activeTransition !== l.ve)) {
      if (currentOptimisticLane === null || GlobalQueue.ht(l)) {
        if (!tracking && e !== t)
          link(e, t);
        throw l.k;
      }
    } else if (t && l !== e && l.i & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw l.k;
    } else if (!t && l.i & STATUS_UNINITIALIZED) {
      throw l.k;
    }
  }
  if (e.xe && e.i & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && e.Ie < clock) {
      recompute(e);
      return read(e);
    } else
      throw e.k;
  }
  if (snapshotCaptureActive && t && t.U & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.Ye;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const l2 = e.ge !== NOT_PENDING ? e.ge : e.Ue;
      if (l2 !== i2)
        t.u |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.be !== undefined && e.be !== NOT_PENDING) {
    if (t && stale && GlobalQueue.Dt(e))
      return e.Ue;
    return unwrapOverride(e.be);
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.gt(e, l, t)) {
    return e.Ue;
  }
  const u = !t || currentOptimisticLane !== null && GlobalQueue.vt(e, l, t) || e.ge === NOT_PENDING || stale && e.ve && activeTransition !== e.ve ? e.Ue : e.ge;
  if (pendingCheckActive)
    GlobalQueue.kt(e, u);
  if (!t && l === e && typeof n.xe === "function" && e.U & CONFIG_AUTO_DISPOSE && !(l.i & STATUS_PENDING) && !e.p) {
    unobserved(e);
  }
  return u;
}
function setSignal(e, t) {
  if (e.ve && activeTransition !== e.ve)
    globalQueue.initTransition(e.ve);
  if (e.be !== undefined && !projectionWriteActive)
    return GlobalQueue.bt(e, t);
  const n = e.ge === NOT_PENDING ? e.Ue : e.ge;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.i & STATUS_UNINITIALIZED) || !e.Ge || !e.Ge(n, t);
  if (!i)
    return t;
  if (e.ge === NOT_PENDING)
    queuePendingNode(e);
  e.ge = t;
  GlobalQueue._e !== null && GlobalQueue._e(e, t);
  e.Ie = clock;
  insertSubs(e);
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
function staleValues(e, t = true) {
  const n = stale;
  stale = t;
  try {
    return e();
  } finally {
    stale = n;
  }
}
// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, E, e, R) {
  const r = !!R?.user;
  const f = createEffectNode(t, E, e, r ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, R);
  recompute(f, true);
  !R?.defer && (f.De === EFFECT_USER || R?.schedule ? f.v.enqueue(f.De, runEffect.bind(null, f)) : runEffect(f));
}
function notifyEffectStatus(t, E) {
  const e = t !== undefined ? t : this.i;
  const R = E !== undefined ? E : this.k;
  if (e & STATUS_ERROR) {
    this.v.notify(this, STATUS_PENDING, 0);
    if (this.De === EFFECT_USER) {
      if (this.i & STATUS_ERROR) {
        this.it = true;
        this.v.enqueue(this.De, this.lt ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.v.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(R));
      throw R;
    }
  } else if (this.De === EFFECT_RENDER) {
    this.v.notify(this, STATUS_PENDING | STATUS_ERROR, e, R);
  }
}
function runEffect(t) {
  if (!t.it || t.u & REACTIVE_DISPOSED)
    return;
  if (t.i & STATUS_ERROR && t.De === EFFECT_USER) {
    const E2 = unwrapStatusError(t.k);
    t.Tt = t.Ue;
    t.it = false;
    try {
      t.St ? t.St(E2, () => {
        const E3 = t.dt;
        t.dt = undefined;
        E3?.();
      }) : console.error(E2);
    } catch (E3) {
      if (!t.v.notify(t, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(E3);
        throw E3;
      }
    }
    return;
  }
  const E = t.dt;
  t.dt = undefined;
  try {
    E?.();
    const e = t.It(t.Ue, t.Tt);
    if (false)
      ;
    t.dt = e;
  } catch (E2) {
    t.k = new StatusError(t, E2);
    t.i |= STATUS_ERROR;
    if (!t.v.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(E2);
      throw E2;
    }
  } finally {
    t.Tt = t.Ue;
    t.it = false;
  }
}
GlobalQueue.ut = runEffect;
function trackedEffect(t, E) {
  const run = () => {
    if (!e.it || e.u & REACTIVE_DISPOSED)
      return;
    try {
      e.it = false;
      recompute(e);
    } finally {}
  };
  const e = computed(() => {
    const E2 = e.dt;
    e.dt = undefined;
    E2?.();
    const R = staleValues(t);
    e.dt = R;
  }, {
    ...E,
    lazy: true
  });
  e.dt = undefined;
  e.U = e.U & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  e.it = true;
  e.De = EFFECT_TRACKED;
  e.C = (t2, E2) => {
    const R = t2 !== undefined ? t2 : e.i;
    if (R & STATUS_ERROR) {
      e.v.notify(e, STATUS_PENDING, 0);
      const t3 = E2 !== undefined ? E2 : e.k;
      if (!e.v.notify(e, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(unwrapStatusError(t3));
        throw t3;
      }
    }
  };
  e.Ft = run;
  e.v.enqueue(EFFECT_USER, run);
}

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/signals.js
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
  t && !(t.U & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    e();
  });
}
// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var $AFFECTS = Symbol(0);
var STORE_SELF_PENDING = Symbol(0);
var storeLookup = new WeakMap;
var symbolKeyedRecords = new WeakSet;
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
var affectsScopes = new Map;

// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/store/utils.js
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
// node_modules/.bun/@solidjs+signals@2.0.0-beta.20/node_modules/@solidjs/signals/dist/prod/boundaries.js
var ON_INIT = Symbol();
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.B() : e.W.size === 0 && !e.L;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.q() : isSlotReady(e);
}
function setSlotState(e, t, r, n) {
  setSignal(e.V, r);
  setSignal(e.H, n);
  if (isRevealController(e)) {
    if (!r && e.J === t)
      e.J = undefined;
    return e.K(r, n);
  }
  if (!r && e.X === t && e.Y)
    e.X = undefined;
}

class RevealController {
  Z;
  ee;
  te = [];
  J;
  V = signal(false, {
    ownedWrite: true,
    re: true
  });
  H = signal(false, {
    ownedWrite: true,
    re: true
  });
  ne = true;
  se = true;
  ie = false;
  constructor(e, t) {
    this.Z = e;
    this.ee = t;
  }
  oe(e) {
    for (let t = 0;t < this.te.length; t++) {
      const r = this.te[t];
      if ((isRevealController(r) ? r.J : r.X) !== this)
        continue;
      if (e(r) === false)
        return false;
    }
    return true;
  }
  B() {
    return this.oe(isSlotReady);
  }
  q() {
    const e = untrack(this.Z);
    if (e === "together")
      return this.oe(isSlotMinimallyReady);
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.oe((r) => {
        e2 = true;
        if (isSlotMinimallyReady(r)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.oe((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  le(e) {
    if (this.te.includes(e))
      return;
    this.te.push(e);
    const t = untrack(this.Z);
    setSignal(e.V, true), setSignal(e.H, t === "sequential" ? !!untrack(this.ee) : false);
    untrack(() => this.K());
  }
  ae(e) {
    const t = this.te.indexOf(e);
    if (t >= 0)
      this.te.splice(t, 1);
    untrack(() => this.K());
  }
  K(e, t) {
    if (this.ie)
      return;
    this.ie = true;
    const r = this.ne;
    const n = this.se;
    try {
      const r2 = e ?? read(this.V), n2 = untrack(this.Z), s = n2 === "sequential" && !!untrack(this.ee), i = t ?? s;
      if (r2) {
        this.oe((e2) => setSlotState(e2, this, true, i));
      } else if (n2 === "natural") {
        this.oe((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.H, false);
            setSignal(e2.V, false);
            e2.K(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (n2 === "together") {
        const e2 = this.oe(isSlotMinimallyReady);
        this.oe((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.oe((t2) => {
          if (e2)
            return setSlotState(t2, this, true, s);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.H, false);
            setSignal(t2.V, false);
            t2.K(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.ne = this.B();
      this.se = this.q();
      this.ie = false;
    }
    if (this.J && (r !== this.ne || n !== this.se))
      this.J.K();
  }
}

class CollectionQueue extends Queue {
  ue;
  W = new Set;
  ce;
  L = true;
  V = signal(false, {
    ownedWrite: true,
    re: true
  });
  k;
  H = signal(false, {
    ownedWrite: true,
    re: true
  });
  X;
  Y = false;
  fe;
  he = ON_INIT;
  constructor(e) {
    super();
    this.ue = e;
  }
  run(e) {
    if (!e || read(this.V) && (!_revealUsed || read(this.H)))
      return;
    return super.run(e);
  }
  notify(e, t, r, n) {
    if (!(t & this.ue))
      return super.notify(e, t, r, n);
    if (this.Y && this.fe) {
      const e2 = untrack(() => {
        try {
          return this.fe();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.he) {
        this.he = e2;
        this.Y = false;
        this.W.clear();
      }
    }
    if (this.ue & STATUS_PENDING && this.Y)
      return super.notify(e, t, r, n);
    if (r & this.ue) {
      this.L = true;
      const t2 = n?.source || e.k?.source;
      if (t2) {
        const e2 = this.W.size === 0;
        this.W.add(t2);
        if (e2)
          setSignal(this.V, true);
        if (this.ue & STATUS_ERROR) {
          setSignal(this.k, unwrapStatusError(t2.k));
        }
      }
    }
    t &= ~this.ue;
    return t ? super.notify(e, t, r, n) : true;
  }
  Se() {
    for (const e of this.W) {
      if (e.u & REACTIVE_DISPOSED || !(e.i & this.ue) && !(this.ue & STATUS_ERROR && e.i & STATUS_PENDING))
        this.W.delete(e);
    }
    if (!this.W.size) {
      if (this.ue & STATUS_PENDING && this.L && !this.Y && this.ce) {
        this.L = !!(this.ce.i & this.ue);
      } else {
        this.L = false;
      }
      if (!this.L) {
        setSignal(this.V, false);
        if (this.fe) {
          try {
            this.he = untrack(() => this.fe());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this.X?.K();
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
// node_modules/.bun/solid-js@2.0.0-beta.20/node_modules/solid-js/dist/solid.js
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

// node_modules/.bun/@solidjs+universal@2.0.0-beta.20+96ee2375bb65ef1e/node_modules/@solidjs/universal/dist/universal.js
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

// packages/core/src/window.ts
var animationFrames = new Map;
var refreshRate = 60;
var backHandlers = [];
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
// packages/core/src/environment.ts
import { on as on2 } from "srt:events";
// packages/core/src/gamepad.ts
import { on as on3 } from "srt:events";
// packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
import { destroyTexture as destroyTexture2, resizeTexture, setShaderParams as setShaderParams2, setShaderSize as setShaderSize2, setShaderTextures, uploadTexture } from "flux:gpu";
import { destroyBuffer as destroyBuffer2, setDrawCount } from "flux:gpu";
import { captureSnapshot, readTexture } from "flux:gpu";
// packages/core/src/image.ts
var imageCache = new Map;
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
