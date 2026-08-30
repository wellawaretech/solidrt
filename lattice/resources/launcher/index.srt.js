// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/error.js
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/constants.js
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
var REACTIVE_MISSED_WAKE = 1 << 12;
var CONFIG_OWNED_WRITE = 1 << 0;
var CONFIG_NO_SNAPSHOT = 1 << 1;
var CONFIG_TRANSPARENT = 1 << 2;
var CONFIG_IN_SNAPSHOT_SCOPE = 1 << 3;
var CONFIG_CHILDREN_FORBIDDEN = 1 << 4;
var CONFIG_AUTO_DISPOSE = 1 << 5;
var CONFIG_SYNC = 1 << 6;
var CONFIG_OPTIMISTIC = 1 << 7;
var CONFIG_HAS_COMPANIONS = 1 << 8;
var CONFIG_HAS_SNAPSHOT = 1 << 9;
var CONFIG_HAS_LANE = 1 << 10;
var CONFIG_CHILD_COMPANIONS = 1 << 11;
var CONFIG_FW_CHILDREN = 1 << 12;
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/lanes.js
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
  const e = n.o?.Be;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  if (n.o !== null)
    n.o.Be = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.o?.Nt) {
    const e = ext(n).Nt = currentTransition(n.o?.Nt);
    if (e.fn !== true)
      return e;
    if (n.o !== null)
      n.o.Nt = null;
  }
  return resolveLane(n)?._e ?? n._e;
}
function hasActiveOverride(n) {
  const e = n.o;
  return e !== null && e.De !== undefined && e.De !== NOT_PENDING;
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const t = n.o?.Be;
  if (t) {
    if (t.an) {
      ext(n).Be = e;
      n.T |= CONFIG_HAS_LANE;
      return;
    }
    const r = findLane(t);
    if (activeLanes.has(r)) {
      if (r !== i && !hasActiveOverride(n)) {
        if (i.sn && findLane(i.sn) === r) {
          ext(n).Be = e;
          n.T |= CONFIG_HAS_LANE;
        } else if (r.sn && findLane(r.sn) === i)
          ;
        else
          mergeLanes(i, r);
      }
      return;
    }
  }
  ext(n).Be = e;
  n.T |= CONFIG_HAS_LANE;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  xe: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  xe: 0,
  EE: 0
};
function cancelZombieRecompute(e) {
  if (e.ie & REACTIVE_IN_HEAP_HEIGHT)
    e.ie &= -12;
  else {
    deleteFromHeap(e, zombieQueue);
    e.ie &= -4;
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
  return transitions.size === 0 && activeLanes.size === 0 && e.Qt.length === 0 && t.Ke.length === 0 && t.A.length === 0 && t.Tn.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.u !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.Pe !== NOT_PENDING)
      continue;
    if (e.o?.De !== undefined && e.o?.De !== NOT_PENDING)
      continue;
    if (e.o?.t)
      continue;
    transientStoreNodes.delete(e);
    e.o?.ft?.();
  }
}
function setProjectionWriteActive(e) {
  projectionWriteActive = e;
}
function createBatch() {
  return {
    Te: clock,
    yt: [],
    Ne: new Map,
    Ke: [],
    A: [],
    Tn: new Set,
    ue: [],
    bt: {
      Lt: [[], []],
      Qt: []
    },
    fn: false,
    cn: new Set
  };
}
function mergeTransitionState(e, t) {
  t.fn = e;
  e.ue.push(...t.ue);
  for (const i2 of activeLanes)
    if (i2._e === t)
      i2._e = e;
  if (t.Ke.length) {
    e.Ke.push(...t.Ke);
    t.Ke.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const i2 of t.Tn)
    e.Tn.add(i2);
  const i = t.Mt;
  if (i !== undefined) {
    t.Mt = undefined;
    let n = e.Mt;
    if (n !== undefined)
      n.push(...i);
    else
      n = e.Mt = i;
    for (let e2 = 0;e2 < i.length; e2++) {
      const t2 = i[e2].pc;
      if (t2 !== undefined && t2.qe === i[e2])
        t2.qa = n;
    }
  }
  for (const [i2, n] of t.Ne) {
    let t2 = e.Ne.get(i2);
    if (!t2)
      e.Ne.set(i2, t2 = new Set);
    for (const e2 of n)
      t2.add(e2);
  }
  for (const i2 of t.cn)
    e.cn.add(i2);
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.En && !projectionWriteActive)
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
  Lt = [[], []];
  Qt = [];
  Vt = 0;
  created = clock;
  addChild(e) {
    this.Qt.push(e);
    e.ke = this;
  }
  removeChild(e) {
    const t = this.Qt.indexOf(e);
    if (t >= 0) {
      this.Qt.splice(t, 1);
      e.ke = null;
    }
  }
  notify(e, t, i, n) {
    if (this.ke)
      return this.ke.notify(e, t, i, n);
    return false;
  }
  run(e) {
    if (this.Lt[e - 1].length) {
      const t2 = this.Lt[e - 1];
      this.Lt[e - 1] = [];
      runQueue(t2, e);
    }
    const t = this.Qt;
    const i = ++queueRunToken;
    for (let n = 0;n < t.length; ) {
      const s = t[n];
      if (s.Vt !== i) {
        s.Vt = i;
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
        this.Lt[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.Lt[0].push(...this.Lt[0]);
    e.Lt[1].push(...this.Lt[1]);
    this.Lt = [[], []];
    for (let t = 0;t < this.Qt.length; t++) {
      let i = this.Qt[t];
      let n = e.Qt[t];
      if (!n) {
        n = {
          Lt: [[], []],
          Qt: []
        };
        e.Qt[t] = n;
      }
      i.stashQueues(n);
    }
  }
  restoreQueues(e) {
    this.Lt[0].push(...e.Lt[0]);
    this.Lt[1].push(...e.Lt[1]);
    for (let t = 0;t < e.Qt.length; t++) {
      const i = e.Qt[t];
      let n = this.Qt[t];
      if (n)
        n.restoreQueues(i);
    }
  }
}

class GlobalQueue extends Queue {
  En = false;
  m = createBatch();
  static Ce;
  static Fe;
  static tt;
  static Bt = null;
  static p = null;
  static G = null;
  static M = null;
  static N = null;
  static Rt = null;
  static Gt = null;
  static Oe = null;
  static de = null;
  static ye = null;
  static un = null;
  static Pt = null;
  static Dt = null;
  static Ht = null;
  static Je = null;
  static k = null;
  static wt = null;
  static jt = null;
  static kt = null;
  static dn = null;
  static In = null;
  static Nn = null;
  static _n = null;
  static ln = null;
  static Ft = null;
  static ht = null;
  static gt = null;
  static je = null;
  static $e = null;
  static ze = null;
  static An = null;
  flush() {
    if (this.En)
      return;
    if (activeTransition === null && dirtyQueue.EE < dirtyQueue.xe && this.Lt[0].length === 0 && this.Lt[1].length === 0 && this.Qt.length === 0 && canUseSimpleSyncFlush(this)) {
      this.En = true;
      try {
        sweepDormant();
        commitPendingNodes();
      } finally {
        this.En = false;
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.xe || this.Lt[0].length !== 0 || this.Lt[1].length !== 0 || this.m.yt.length !== 0;
      return;
    }
    this.En = true;
    try {
      if (false)
        ;
      sweepDormant();
      runHeap(dirtyQueue, GlobalQueue.Ce);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, this.m === e2 ? cancelZombieRecompute : GlobalQueue.Ce);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue._n(EFFECT_RENDER);
            GlobalQueue._n(EFFECT_USER);
          }
          this.stashQueues(e2.bt);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.xe || this.m.yt.length > 0;
          reassignPendingTransition(e2.yt);
          activeTransition = null;
          finalizePureQueue(null, true);
          return;
        }
        const t = activeTransition;
        const i = this.m;
        i !== t && i.yt.push(...t.yt);
        this.restoreQueues(t.bt);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(i.yt);
        finalizePureQueue(t);
        if (i === t) {
          const e2 = createBatch();
          e2.yt = i.yt;
          e2.Ke = i.Ke;
          e2.A = i.A;
          e2.Tn = i.Tn;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.xe) {
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
      scheduled = dirtyQueue.EE >= dirtyQueue.xe;
      activeLanes.size && GlobalQueue._n(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && GlobalQueue._n(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
      if (false)
        ;
      if (false)
        ;
    } finally {
      this.En = false;
    }
  }
  notify(e, t, i, n) {
    if (t & STATUS_PENDING) {
      if (i & STATUS_PENDING) {
        const t2 = n !== undefined ? n : e.o?._;
        if (t2?.l)
          return true;
        if (activeTransition && t2) {
          const i2 = t2.source;
          let n2 = activeTransition.Ne.get(i2);
          if (!n2)
            activeTransition.Ne.set(i2, n2 = new Set);
          const s = n2.size;
          n2.add(e);
          if (n2.size !== s) {
            schedule();
            GlobalQueue.jt?.(activeTransition);
          }
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
    if (!e && activeTransition && activeTransition.Te === clock)
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
    activeTransition.Te = clock;
    const t = this.m;
    if (t !== activeTransition) {
      for (let e2 = 0;e2 < t.yt.length; e2++) {
        const i = t.yt[e2];
        i._e = activeTransition;
        activeTransition.yt.push(i);
      }
      for (let e2 = 0;e2 < t.Ke.length; e2++) {
        const i = t.Ke[e2];
        i._e = activeTransition;
        activeTransition.Ke.push(i);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e2 of t.Tn)
        activeTransition.Tn.add(e2);
      if (t.cn.size) {
        for (const e2 of t.cn)
          activeTransition.cn.add(e2);
        t.cn.clear();
      }
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2._e)
        e2._e = activeTransition;
    }
  }
}
function queuePendingNode(e) {
  currentBatch.yt.push(e);
}
var reaskArmed = false;
var notifyEpoch = 0;
function bumpNotifyEpoch() {
  notifyEpoch++;
}
function insertSubs(e, t = false) {
  e._t = notifyEpoch;
  const i = e.T;
  const n = (i & CONFIG_HAS_LANE ? e.o?.Be : undefined) || currentOptimisticLane;
  const s = (i & CONFIG_HAS_SNAPSHOT) !== 0 && e.o?.Qe !== undefined;
  const o = reaskArmed;
  for (let i2 = e.u;i2 !== null; i2 = i2.ae) {
    const e2 = i2.ce;
    if (o)
      e2.ie &= ~REACTIVE_REASK;
    if (e2.ie & REACTIVE_RECOMPUTING_DEPS && i2.nn === e2.Ze && i2 !== e2.Ye)
      e2.ie |= REACTIVE_MISSED_WAKE;
    if (s && e2.T & CONFIG_IN_SNAPSHOT_SCOPE) {
      e2.ie |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && n) {
      e2.ie |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(e2, n);
    } else if (t) {
      e2.ie |= REACTIVE_OPTIMISTIC_DIRTY;
      if (e2.o)
        e2.o.Be = undefined;
    }
    enqueueSub(e2);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.oe) {
    if (e.Pe !== NOT_PENDING) {
      e.be = e.Pe;
      e.Pe = NOT_PENDING;
    }
    if (e.T & CONFIG_HAS_COMPANIONS)
      GlobalQueue.un(e);
    return;
  }
  if (e.Pe !== NOT_PENDING) {
    e.be = e.Pe;
    e.Pe = NOT_PENDING;
    if (e.Re && e.Re !== EFFECT_TRACKED)
      e.Xe = true;
  }
  t.Ie = false;
  t.ie &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  if (t.o != null && (t.o.qe !== null || t.o.We !== null))
    GlobalQueue.Fe(t, false, true);
  if (e.T & CONFIG_HAS_COMPANIONS)
    GlobalQueue.un(e);
}
var storeCommitHook = null;
function setStoreCommitHook(e) {
  storeCommitHook = e;
}
var patchCommitHook = null;
function commitPendingNodes() {
  const e = currentBatch.yt;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
  storeCommitHook?.();
  patchCommitHook?.(currentBatch);
}
function finalizePureQueue(e = null, t = false) {
  const i = !t;
  if (i)
    commitPendingNodes();
  if (!t && globalQueue.Qt.length)
    checkBoundaryChildren(globalQueue);
  const n = dirtyQueue.EE >= dirtyQueue.xe;
  if (n)
    runHeap(dirtyQueue, GlobalQueue.Ce);
  if (i) {
    if (n)
      commitPendingNodes();
    const t2 = e ?? globalQueue.m;
    if (t2.Ke.length)
      GlobalQueue.dn(t2.Ke);
    if (t2.cn.size) {
      for (const e2 of t2.cn) {
        if (e2.ie & REACTIVE_DISPOSED)
          continue;
        enqueueSub(e2);
      }
      t2.cn.clear();
      schedule();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.Qt.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.Tn.size)
      GlobalQueue.Bt(t2.Tn, e);
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.Nn(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.Qt) {
    t.se?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t]._e = activeTransition;
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
  if (globalQueue.En) {
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
  if (e.ie & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.o?.le?.has(t))
    return true;
  for (let i = e.nt;i; i = i.it) {
    let e2 = i.ut;
    while (e2) {
      if (e2 === t || e2.lt === t)
        return true;
      e2 = e2.o?.Et;
    }
  }
  return !!(e.S & STATUS_PENDING && e.o?._ instanceof NotReadyError && e.o?._.source === t);
}
function transitionComplete(e) {
  if (e.fn)
    return true;
  if (e.ue.length)
    return false;
  let t = true;
  for (const [i, n] of e.Ne) {
    let s = false;
    for (const e2 of n) {
      if (reporterBlocksSource(e2, i)) {
        s = true;
        break;
      }
      n.delete(e2);
    }
    if (!s)
      e.Ne.delete(i);
    else if (i.S & STATUS_PENDING && i.o?._?.source === i) {
      t = false;
      break;
    }
  }
  if (t && GlobalQueue.In?.(e))
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.ie & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  if (e.Re === EFFECT_TRACKED) {
    const E2 = e;
    if (!E2.Xe) {
      E2.Xe = true;
      E2.C.enqueue(EFFECT_USER, E2.Ut);
    }
    return;
  }
  const E = queueFor(e);
  if (E.xe > e.Le)
    E.xe = e.Le;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.ke?.Ct ? e.ke.Ot?.Le : e.ke?.Le) ?? -1;
  if (t >= e.Le)
    e.Le = t + 1;
  const n = e.Le;
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
  let t = e.ie;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (t & REACTIVE_CHECK) {
    e.ie = t & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else {
    e.ie = t | REACTIVE_IN_HEAP;
    if (E.tE && !(t & REACTIVE_DIRTY))
      E.tE = false;
  }
  if (!(t & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, E);
}
function insertIntoHeapHeight(e, E) {
  let t = e.ie;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.ie = t | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, E);
}
function deleteFromHeap(e, E) {
  const t = e.ie;
  if (!(t & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.ie = t & -25;
  const n = e.Le;
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
      if (t.ie & REACTIVE_IN_HEAP)
        markNode(t);
    }
  }
}
function markNode(e, E = REACTIVE_DIRTY) {
  const t = e.ie;
  if ((t & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= E)
    return;
  e.ie = t & -4 | E;
  for (let E2 = e.u;E2 !== null; E2 = E2.ae) {
    markNode(E2.ce, REACTIVE_CHECK);
  }
  if (e.T & CONFIG_FW_CHILDREN) {
    for (let E2 = e.o.i;E2 !== null; E2 = E2.Se) {
      for (let e2 = E2.u;e2 !== null; e2 = e2.ae) {
        markNode(e2.ce, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.xe = 0;e.xe <= e.EE; e.xe++) {
    let t = e.eE[e.xe];
    while (t !== undefined) {
      if (t.ie & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.xe];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.Le;
  for (let E2 = e.nt;E2; E2 = E2.it) {
    const e2 = E2.ut;
    const n = e2.lt || e2;
    if (n.oe && n.Le >= t)
      t = n.Le + 1;
  }
  if (e.Le !== t) {
    e.Le = t;
    for (let E2 = e.u;E2 !== null; E2 = E2.ae) {
      insertIntoHeapHeight(E2.ce, queueFor(E2.ce));
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/owner.js
var PENDING_OWNER = {};
function markDisposal(e) {
  let t = e.ve;
  while (t) {
    const e2 = t.ie;
    t.ie = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(t, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(t, zombieQueue);
      else
        insertIntoHeapHeight(t, zombieQueue);
    }
    markDisposal(t);
    t = t.Ve;
  }
}
function disposeChildren(e, t = false, n) {
  const i = e.ie;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t) {
    e.ie = i | REACTIVE_DISPOSED;
    const t2 = e;
    if (t2.o?.Ge || t2.o?.ge)
      GlobalQueue.un(t2);
  }
  if (t && e.oe && e.o !== null)
    e.o.Ee = null;
  let o = n ? e.o?.qe ?? null : e.ve;
  while (o) {
    const e2 = o.Ve;
    const t2 = o;
    t2.T &= ~CONFIG_AUTO_DISPOSE;
    deleteFromHeap(t2, queueFor(t2));
    clearDeps(t2);
    disposeChildren(o, true);
    o = e2;
  }
  if (n) {
    if (e.o !== null)
      e.o.qe = null;
  } else {
    e.ve = null;
    e.Me = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e.ke !== null && !(e.ke.ie & REACTIVE_DISPOSED)) {
    const t2 = e.ct;
    const n2 = e.Ve;
    if (t2 !== null)
      t2.Ve = n2;
    else
      e.ke.ve = n2;
    if (n2 !== null)
      n2.ct = t2;
    e.ct = null;
  }
  runDisposal(e, n);
  if (t && e.At) {
    const t2 = e.At;
    e.At = undefined;
    t2();
  }
}
function runDisposal(e, t) {
  let n = t ? e.o?.We : e.he;
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
  if (t) {
    if (e.o !== null)
      e.o.We = null;
  } else
    e.he = null;
}
function childId(e, t) {
  let n = e;
  while (n.T & CONFIG_TRANSPARENT && n.ke)
    n = n.ke;
  if (n.id != null)
    return formatId(n.id, t ? n.Me++ : n.Me);
  throw new Error("");
}
function getNextChildId(e) {
  return childId(e, true);
}
function inheritId(e, t, n) {
  return e?.id ?? (t ? n?.id : n?.id != null ? getNextChildId(n) : undefined);
}
function formatId(e, t) {
  const n = t.toString(36), i = n.length - 1;
  return e + (i ? String.fromCharCode(64 + i) : "") + n;
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
  const t = context;
  const n = e?.transparent ?? false;
  const i = {
    id: inheritId(e, n, t),
    T: n ? CONFIG_TRANSPARENT : 0,
    Ct: true,
    Ot: t?.Ct ? t.Ot : t,
    ve: null,
    Ve: null,
    ct: null,
    he: null,
    C: t?.C ?? globalQueue,
    we: t?.we || defaultContext,
    Me: 0,
    o: null,
    ke: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.ve;
    if (e2 === null) {
      t.ve = i;
    } else {
      i.Ve = e2;
      e2.ct = i;
      t.ve = i;
    }
  }
  return i;
}
function createRoot(e, t) {
  const n = createOwner(t);
  return runWithOwner(n, () => e(() => n.dispose()));
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(e) {
  const n = e.ut;
  const l = e.it;
  const o = e.ae;
  const u = e.en;
  if (o !== null)
    o.en = u;
  else
    n.rt = u;
  if (u !== null)
    u.ae = o;
  else {
    n.u = o;
    if (o === null) {
      n.o?.ft?.();
      const e2 = n;
      e2.oe && e2.T & CONFIG_AUTO_DISPOSE && !(e2.ie & REACTIVE_ZOMBIE) && !(e2.S & STATUS_PENDING) && unobserved(e2);
    }
  }
  return l;
}
function trimStaleDeps(e) {
  const n = e.Ye;
  let l = n !== null ? n.it : e.nt;
  if (l !== null) {
    do {
      l = unlinkSubs(l);
    } while (l !== null);
    if (n !== null)
      n.it = null;
    else
      e.nt = null;
  }
}
function clearDeps(e) {
  let n = e.nt;
  if (!n)
    return;
  do {
    n = unlinkSubs(n);
  } while (n !== null);
  e.nt = null;
  e.Ye = null;
}
function unobserved(e) {
  deleteFromHeap(e, queueFor(e));
  clearDeps(e);
  disposeChildren(e, true);
}
var dormantNodes = new Set;
function sweepDormant() {
  if (dormantNodes.size === 0)
    return;
  for (const e of dormantNodes) {
    if (!e.u && e.T & CONFIG_AUTO_DISPOSE && !(e.S & STATUS_PENDING) && !(e.ie & (REACTIVE_DISPOSED | REACTIVE_ZOMBIE))) {
      unobserved(e);
    }
  }
  dormantNodes.clear();
}
function link(e, n, l = false) {
  const o = n.Ye;
  if (o !== null && o.ut === e) {
    o.me &&= l;
    return;
  }
  let u = null;
  const t = n.ie & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    u = o !== null ? o.it : n.nt;
    if (u !== null && u.ut === e) {
      u.nn = n.Ze;
      n.Ye = u;
      u.me = l;
      return;
    }
  }
  const s = e.rt;
  if (s !== null && s.ce === n && (!t || s.nn === n.Ze)) {
    if (t)
      s.me &&= l;
    else
      s.me = l;
    return;
  }
  const r = n.Ye = e.rt = {
    ut: e,
    ce: n,
    it: u,
    en: s,
    ae: null,
    nn: n.Ze,
    me: l
  };
  if (o !== null)
    o.it = r;
  else
    n.nt = r;
  if (s !== null)
    s.ae = r;
  else
    e.u = r;
  bumpNotifyEpoch();
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.o?.le?.has(n))
    return false;
  (ext(e).le ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  if (!e.o?.le?.delete(n))
    return false;
  if (e.o?.le.size === 0) {
    if (e.o !== null)
      e.o.le = undefined;
  }
  return true;
}
function clearPendingSources(e) {
  e.o?.le?.clear();
  if (e.o !== null)
    e.o.le = undefined;
}
function parkLoadingWindow(e, n) {
  ext(e).fe = true;
  if (n.source)
    addPendingSource(e, n.source);
  if (!(e.S & STATUS_ERROR))
    setPendingError(e, n.source, n);
}
function setPendingError(e, n, t) {
  if (!n) {
    if (e.o !== null)
      e.o._ = null;
    return;
  }
  if (t instanceof NotReadyError && t.source === n) {
    ext(e)._ = t;
    return;
  }
  const r = e.o?._;
  if (!(r instanceof NotReadyError) || r.source !== n) {
    ext(e)._ = new NotReadyError(n);
  }
}
function forEachDependent(e, n) {
  for (let t = e.u;t !== null; t = t.ae)
    n(t.ce, t);
  for (let t = e.o?.i ?? null;t !== null; t = t.Se) {
    for (let e2 = t.u;e2 !== null; e2 = e2.ae)
      n(e2.ce, e2);
  }
}
function releaseIfSettledUnobserved(e) {
  e.oe && e.T & CONFIG_AUTO_DISPOSE && !e.u && !(e.ie & REACTIVE_ZOMBIE) && !(e.S & STATUS_PENDING) && unobserved(e);
}
function releaseSettledDependents(e) {
  let n;
  const t = new Set;
  const visit = (e2) => {
    if (t.has(e2))
      return;
    t.add(e2);
    if (!e2.u && e2.T & CONFIG_AUTO_DISPOSE)
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
    if (e2.o?._ === n) {
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
  const o = GlobalQueue.de;
  const settle = (l) => {
    if (r.has(l) || !removePendingSource(l, e))
      return;
    r.add(l);
    l.Te = clock;
    const i = l.o?.le?.values().next().value;
    const u = l.S & STATUS_ERROR;
    if (i) {
      if (!u)
        setPendingError(l, i);
      o !== null && o(l);
    } else {
      l.S &= ~STATUS_PENDING;
      if (!u)
        setPendingError(l);
      o !== null && o(l);
      if (l.o?.fe) {
        enqueueSub(l);
        n = true;
      }
      if (l.o !== null)
        l.o.fe = false;
      if (!l.u && l.T & CONFIG_AUTO_DISPOSE)
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
    if (e.o !== null)
      e.o.Ee = null;
    e.Ie = false;
    return n;
  }
  ext(e).Ee = n;
  let l;
  const settleTransition = () => {
    const n2 = resolveTransition(e);
    if (n2 && e.S & STATUS_UNINITIALIZED && !currentTransition(n2).Ne.has(e)) {
      e._e = null;
      return;
    }
    globalQueue.initTransition(n2);
  };
  const handleError = (t2) => {
    if (e.o?.Ee !== n)
      return;
    let r2 = t2 instanceof NotReadyError;
    if (r2 && e.Ie) {
      if (e.o !== null)
        e.o.Ee = null;
      parkLoadingWindow(e, t2);
      e.Te = clock;
      return;
    }
    settleTransition();
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    e.Te = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, o2) => {
    if (e.o?.Ee !== n)
      return;
    if (e.ie & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    settleTransition();
    const l2 = !!(e.S & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const i2 = resolveLane(e);
    if (i2)
      i2.Ae.delete(e);
    if (t) {
      t(r2);
      if (l2)
        clearStatus(e, true);
    } else if (e.o?.De !== undefined) {
      if (e.Pe === NOT_PENDING)
        queuePendingNode(e);
      e.Pe = r2;
      GlobalQueue.Oe !== null && GlobalQueue.Oe(e, r2);
      if (!hasActiveOverride(e)) {
        insertSubs(e);
      }
      e.Te = clock;
    } else if (i2) {
      const n2 = e.Re;
      const t2 = e.be;
      const o3 = e.Ue;
      try {
        if (!n2 && l2 || !o3 || !o3(r2, t2)) {
          e.be = r2;
          e.Te = clock;
          GlobalQueue.Oe !== null && GlobalQueue.Oe(e, r2);
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
    if (e.Pe === NOT_PENDING)
      e.Ie = false;
    settlePendingSource(e);
    schedule();
    flush();
    o2?.();
  };
  const settleAutodispose = () => {
    if (e.T & CONFIG_AUTO_DISPOSE && !e.u && !(e.S & STATUS_PENDING)) {
      unobserved(e);
      return true;
    }
    return false;
  };
  const consumeIterator = (t2, r2) => {
    const o2 = t2[Symbol.asyncIterator]();
    let i2 = false;
    let u = false;
    let s = !r2;
    const close = () => {
      if (u)
        return;
      u = true;
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
            u = true;
        } else if (e.o?.Ee !== n) {
          return;
        } else if (!r4.done) {
          i2 = true;
          asyncWrite(r4.value, iterateOrRelease);
        } else {
          u = true;
          if (i2) {
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
        } else if (e.o?.Ee === n) {
          u = true;
          handleError(t4);
          settleAutodispose();
        }
      });
      c = false;
      if (a) {
        u = true;
        handleError(r3);
        if (s)
          throw r3;
        return true;
      }
      if (f2 && !t3.done) {
        l = t3.value;
        i2 = true;
        return iterate();
      }
      return f2 && t3.done;
    };
    const f = iterate();
    s = false;
    return i2 || f;
  };
  let i = null;
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
      i = r2;
    return true;
  };
  if (o) {
    let t2 = false, r2 = false, o2, i2 = true;
    const registerDeferredClose = (n2) => {
      if (!e.he)
        e.he = n2;
      else if (Array.isArray(e.he))
        e.he.push(n2);
      else
        e.he = [e.he, n2];
    };
    n.then((r3) => {
      if (i2) {
        l = r3;
        t2 = true;
      } else if (e.o?.Ee === n && !(e.ie & REACTIVE_DISPOSED) && flattenIfIterable(r3, registerDeferredClose))
        ;
      else {
        asyncWrite(r3);
        settleAutodispose();
      }
    }, (e2) => {
      if (i2) {
        o2 = e2;
        r2 = true;
      } else {
        handleError(e2);
        settleAutodispose();
      }
    });
    i2 = false;
    if (r2) {
      handleError(o2);
      throw o2;
    } else if (!t2) {
      if (e.Ie)
        return e.be;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else if (!flattenIfIterable(l)) {
      e.Ie = false;
    }
  }
  if (r)
    flattenIfIterable(n);
  if (i !== null) {
    if (!i) {
      if (e.Ie)
        return e.be;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.Ie = false;
  }
  return l;
}
function clearStatus(e, n = false) {
  if (e.o?.le)
    clearPendingSources(e);
  if (e.o?.fe) {
    if (e.o !== null)
      e.o.fe = false;
  }
  if (e.o !== null)
    e.o.pe = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e.o?._)
    setPendingError(e);
  if (e.o?.Ge || e.o?.ge)
    GlobalQueue.de(e);
  if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.ye !== null)
    GlobalQueue.ye(e);
  const t = statusNotifierOf(e);
  if (t)
    t.call(e);
}
function notifyStatus(e, n, t, r, o) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const l = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const i = l === e;
  const u = n === STATUS_PENDING && e.o?.De !== undefined && !i;
  const s = u && hasActiveOverride(e);
  if (!r) {
    if (n === STATUS_PENDING && l) {
      addPendingSource(e, l);
      e.S = STATUS_PENDING | e.S & STATUS_UNINITIALIZED;
      setPendingError(e, l, t);
    } else {
      clearPendingSources(e);
      e.S = n | (n !== STATUS_ERROR ? e.S & STATUS_UNINITIALIZED : 0);
      ext(e)._ = t;
    }
    GlobalQueue.de !== null && GlobalQueue.de(e);
    if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.ye !== null)
      GlobalQueue.ye(e);
  }
  if (o && !r) {
    assignOrMergeLane(e, o);
  }
  const f = r || s;
  const a = r || u ? undefined : o;
  const c = statusNotifierOf(e);
  if (c) {
    if (r && n === STATUS_PENDING) {
      return;
    }
    if (f) {
      c.call(e, n, t);
    } else {
      c.call(e);
    }
    return;
  }
  forEachDependent(e, (e2, r2) => {
    e2.Te = clock;
    if (n === STATUS_PENDING && l && !e2.o?.le?.has(l) || n !== STATUS_PENDING && (e2.o?._ !== t || e2.o?.le)) {
      if (r2.me && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f && !e2._e)
        queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.Ce = recompute;
GlobalQueue.Fe = disposeChildren;
var tracking = false;
function setLatestReadActive(e) {
  latestReadActive = e;
}
var stale = false;
var pendingCheckActive = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.He)
      return true;
    e = e.ke;
  }
  return false;
}
function recompute(e, t = false) {
  bumpNotifyEpoch();
  const n = e.Re;
  if (!t) {
    if (e._e && (!n || activeTransition) && activeTransition !== e._e)
      globalQueue.initTransition(e._e);
    deleteFromHeap(e, queueFor(e));
    if (e.o !== null)
      e.o.Ee = null;
    if (e._e || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ve !== null || e.he !== null) {
      markDisposal(e);
      const t2 = ext(e);
      t2.We = e.he;
      t2.qe = e.ve;
      e.he = null;
      e.ve = null;
      e.Me = 0;
    }
  }
  let i = !!(e.ie & REACTIVE_OPTIMISTIC_DIRTY);
  const u = (e.T & CONFIG_OPTIMISTIC) !== 0 && e.o?.De !== NOT_PENDING && e.o?.De !== undefined;
  const l = !!(e.S & STATUS_UNINITIALIZED);
  const o = e.S & STATUS_ERROR ? e.o?._ : undefined;
  const s = (e.ie & REACTIVE_REASK) !== 0;
  const a = e.Ie;
  const r = context;
  context = e;
  e.Ye = null;
  e.Ze++;
  e.ie = REACTIVE_RECOMPUTING_DEPS;
  e.Te = clock;
  let c = e.Pe === NOT_PENDING ? e.be : e.Pe;
  let _ = e.Le;
  let f = false;
  let N = tracking;
  let E = currentOptimisticLane;
  tracking = true;
  const I = latestReadActive;
  latestReadActive = false;
  if (i) {
    const t2 = GlobalQueue.je(e, true);
    if (t2)
      currentOptimisticLane = t2;
    else if (t2 === false)
      i = false;
  } else if (activeTransition && !t && activeTransition.Ke.length) {
    const t2 = GlobalQueue.je(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const d = n && n !== EFFECT_USER;
  const T = stale;
  if (d)
    stale = true;
  try {
    if (e.T & CONFIG_SYNC) {
      c = e.oe(c);
      if (e.o !== null)
        e.o.Ee = null;
      e.Ie = false;
    } else {
      const t2 = e.o?.Ee;
      const n2 = e.oe(c);
      const i2 = typeof n2 === "object" && n2 !== null;
      const u2 = e.o?.Ee !== t2;
      c = u2 || !i2 ? n2 : handleAsync(e, n2);
      if (!u2 && !i2) {
        if (e.o !== null)
          e.o.Ee = null;
        e.Ie = false;
      }
    }
    if (e.S !== 0 || e.o !== null)
      clearStatus(e, t);
    if (e.T & CONFIG_HAS_LANE && e.o?.Be)
      GlobalQueue.ze(e);
  } catch (t2) {
    const n2 = t2 instanceof NotReadyError;
    if (n2 && e.Ie) {
      parkLoadingWindow(e, t2);
    } else {
      if (n2 && currentOptimisticLane)
        GlobalQueue.$e(e);
      let i2 = false;
      if (n2) {
        ext(e).fe = true;
        if (GlobalQueue.Je !== null)
          i2 = GlobalQueue.Je(e, s);
      }
      notifyStatus(e, n2 ? STATUS_PENDING : STATUS_ERROR, t2, undefined, n2 ? e.o?.Be : undefined);
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = N;
    latestReadActive = I;
    if (d)
      stale = T;
    f = (e.ie & REACTIVE_MISSED_WAKE) !== 0;
    e.ie = REACTIVE_NONE | (t ? e.ie & REACTIVE_SNAPSHOT_STALE : 0);
    context = r;
  }
  if (!e.o?._) {
    trimStaleDeps(e);
    const s2 = u ? unwrapOverride(e.o?.De) : e.Pe === NOT_PENDING ? e.be : e.Pe;
    let r2 = false;
    try {
      r2 = !n && l || !e.Ue || !e.Ue(s2, c);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && r2) {
      e.Xe = !e.o?._;
      if (!t)
        e.C.enqueue(n, e.et ??= GlobalQueue.tt.bind(null, e));
    }
    if (e.o?._)
      ;
    else if (r2) {
      const l2 = u ? e.o?.De : undefined;
      if (t || n && (activeTransition !== e._e || activeTransition === null) || i) {
        e.be = c;
        if (u && i) {
          ext(e).De = c === undefined ? OVERRIDE_UNDEFINED : c;
          e.Pe = NOT_PENDING;
        }
      } else {
        e.Pe = c;
        if (a)
          e.Ie = true;
        if ((activeTransition || e._e) && GlobalQueue.Oe !== null)
          GlobalQueue.Oe(e, c);
      }
      if (e.u !== null && (!u || i || e.o?.De !== l2))
        insertSubs(e, i || u);
    } else if (u) {
      if (e.Pe === NOT_PENDING)
        queuePendingNode(e);
      e.Pe = c;
      if (a)
        e.Ie = true;
    } else if (e.Le != _) {
      for (let t2 = e.u;t2 !== null; t2 = t2.ae) {
        insertIntoHeapHeight(t2.ce, queueFor(t2.ce));
      }
    }
    if (o !== undefined && !r2 && !e.o?._)
      settleErroredDependents(e, o);
  }
  currentOptimisticLane = E;
  const S = e.Pe !== NOT_PENDING || e.o !== null && (e.o.qe !== null || e.o.We !== null) || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  S && (!t || e.S & STATUS_PENDING) && (!e._e || u) && queuePendingNode(e);
  e._e && n && activeTransition !== e._e && runInTransition(e._e, () => recompute(e));
  if (f) {
    enqueueSub(e);
    schedule();
  }
}
function updateIfNecessary(e) {
  if (e.ie & REACTIVE_RECOMPUTING_DEPS)
    return;
  if (e.ie & REACTIVE_CHECK) {
    for (let t = e.nt;t; t = t.it) {
      const n = t.ut;
      const i = n.lt || n;
      if (i.oe) {
        updateIfNecessary(i);
      }
      if (e.ie & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.ie & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.o?._ && e.Te < clock && !e.o?.Ee) {
    recompute(e);
  }
  e.ie = e.ie & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = t !== null && typeof t === "object" && "loadingValue" in t;
  const u = {
    id: inheritId(t, n, context),
    T: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.H ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ue: t?.equals != null ? t.equals : isEqual,
    he: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    Me: 0,
    oe: e,
    be: i ? t.loadingValue : undefined,
    Le: 0,
    ot: undefined,
    st: null,
    nt: null,
    Ye: null,
    Ze: 0,
    u: null,
    rt: null,
    ke: context,
    Ve: null,
    ct: null,
    ve: null,
    ie: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: i ? 0 : STATUS_UNINITIALIZED,
    Te: clock,
    Pe: NOT_PENDING,
    _e: null,
    _t: -1,
    Ie: i,
    o: null
  };
  if (t?.unobserved)
    ext(u).ft = t.unobserved;
  setupComputedNode(u, t);
  return u;
}
function ext(e) {
  return e.o ??= {
    De: undefined,
    Nt: undefined,
    Be: undefined,
    Ge: undefined,
    ge: undefined,
    Et: undefined,
    t: 0,
    Ee: null,
    _: undefined,
    fe: undefined,
    le: undefined,
    h: undefined,
    pe: false,
    i: null,
    ft: undefined,
    Qe: undefined,
    We: null,
    qe: null,
    It: undefined
  };
}
function createEffectNode(e, t, n, i, u) {
  const l = u?.transparent ?? false;
  const o = {
    id: inheritId(u, l, context),
    T: (l ? CONFIG_TRANSPARENT : 0) | (u?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (u?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Ue: false,
    he: null,
    C: context?.C ?? globalQueue,
    we: context?.we ?? defaultContext,
    Me: 0,
    oe: e,
    be: undefined,
    Le: 0,
    ot: undefined,
    st: null,
    nt: null,
    Ye: null,
    Ze: 0,
    u: null,
    rt: null,
    ke: context,
    Ve: null,
    ct: null,
    ve: null,
    ie: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    Te: clock,
    Pe: NOT_PENDING,
    _e: null,
    _t: -1,
    Ie: false,
    Xe: false,
    dt: undefined,
    Tt: t,
    St: n,
    At: undefined,
    Re: i,
    o: null
  };
  if (u?.unobserved)
    ext(o).ft = u.unobserved;
  setupComputedNode(o, lazyOptions);
  return o;
}
var effectStatusNotify = null;
function setEffectStatusNotify(e) {
  effectStatusNotify = e;
}
function statusNotifierOf(e) {
  const t = e.o;
  const n = t !== null && t !== undefined ? t.h : undefined;
  if (n !== undefined)
    return n;
  return e.Re ? effectStatusNotify ?? undefined : undefined;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.st = e;
  const n = context?.Ct ? context.Ot : context;
  if (context) {
    const t2 = context.ve;
    if (t2 === null) {
      context.ve = e;
    } else {
      e.Ve = t2;
      t2.ct = e;
      context.ve = e;
    }
  }
  if (n)
    e.Le = n.Le + 1;
  if (GlobalQueue.Rt !== null)
    GlobalQueue.Rt(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      ext(e).Qe = e.be === undefined ? NO_SNAPSHOT : e.be;
      e.T |= CONFIG_HAS_SNAPSHOT;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    Ue: t?.equals != null ? t.equals : isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.H ? CONFIG_NO_SNAPSHOT : 0),
    be: e,
    u: null,
    rt: null,
    Te: clock,
    lt: n,
    Se: n?.o?.i || null,
    Pe: NOT_PENDING,
    _e: null,
    _t: -1,
    o: null
  };
  if (t?.unobserved)
    ext(i).ft = t.unobserved;
  if (n) {
    ext(n).i = i;
    n.T |= CONFIG_FW_CHILDREN;
  }
  if (snapshotCaptureActive && !(i.T & CONFIG_NO_SNAPSHOT) && !((n?.S ?? 0) & STATUS_PENDING)) {
    ext(i).Qe = e === undefined ? NO_SNAPSHOT : e;
    i.T |= CONFIG_HAS_SNAPSHOT;
    snapshotSources.add(i);
  }
  return i;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (GlobalQueue.Gt === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.Gt !== null)
      return GlobalQueue.Gt(e);
    return e();
  } finally {
    tracking = n;
  }
}
function prepareComputed(e, t) {
  if (e.ie & REACTIVE_LAZY) {
    e.ie &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.ie & REACTIVE_DISPOSED) {
    if (e.T & CONFIG_AUTO_DISPOSE)
      recompute(e, true);
  } else if (t) {
    updateIfNecessary(e);
  }
}
var READ_SLOW = Symbol("read-slow");
function readNodeFast(e) {
  if (latestReadActive || pendingCheckActive || e.oe || e.lt || e.o?.De !== undefined || e.o?.Qe !== undefined || activeTransition !== null || currentOptimisticLane !== null || snapshotCaptureActive || false)
    return READ_SLOW;
  let t = context;
  if (t?.Ct)
    t = t.Ot;
  if (t && tracking)
    link(e, t);
  return !t || e.Pe === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN ? e.be : e.Pe;
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.Pt(e);
  let t = context;
  if (t?.Ct)
    t = t.Ot;
  const n = e;
  const i = e.lt;
  const u = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Dt(e, t, u, i);
  } else if (typeof n.oe === "function") {
    prepareComputed(e, false);
  }
  if (!n.oe && u === e && e.o?.De === undefined && e.o?.Qe === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.Pe === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN ? e.be : e.Pe;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (u.oe) {
      const n2 = queueFor(e);
      if (u.Le >= n2.xe) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(u);
      }
      const i2 = u.Le;
      if (i2 >= t.Le && e.ke !== t) {
        t.Le = i2 + 1;
      }
    }
  }
  if (u.S & STATUS_PENDING) {
    if (t && !(stale && u._e && activeTransition !== u._e)) {
      if (currentOptimisticLane === null || GlobalQueue.ht(u)) {
        if (!tracking && e !== t)
          link(e, t);
        throw u.o?._;
      }
    } else if (t && u.S & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw u.o?._;
    } else if (!t && u.S & STATUS_UNINITIALIZED) {
      throw u.o?._;
    }
  }
  if (u.oe && u.S & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && u.Te < clock) {
      recompute(u);
      return read(e);
    } else
      throw u.o?._;
  }
  if (snapshotCaptureActive && t && t.T & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.o?.Qe;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const u2 = e.Pe !== NOT_PENDING ? e.Pe : e.be;
      if (u2 !== i2)
        t.ie |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.o?.De !== undefined && e.o?.De !== NOT_PENDING) {
    return unwrapOverride(e.o?.De);
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.Ft(e, u, t)) {
    return e.be;
  }
  const l = !t || currentOptimisticLane !== null && GlobalQueue.gt(e, u, t) || e.Pe === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && e._e && activeTransition !== e._e ? e.be : e.Pe;
  if (pendingCheckActive)
    GlobalQueue.Ht(e, l);
  if (!t && u === e && typeof n.oe === "function" && e.T & CONFIG_AUTO_DISPOSE && !(u.S & STATUS_PENDING) && !e.u) {
    dormantNodes.add(e);
    schedule();
  }
  return l;
}
function setSignal(e, t) {
  if (e._e && activeTransition !== e._e)
    globalQueue.initTransition(e._e);
  if (e.T & CONFIG_OPTIMISTIC && !projectionWriteActive)
    return GlobalQueue.kt(e, t);
  const n = e.Pe === NOT_PENDING ? e.be : e.Pe;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.Ue || !e.Ue(n, t);
  if (!i)
    return t;
  const u = e.Pe !== NOT_PENDING;
  if (!u)
    queuePendingNode(e);
  e.Pe = t;
  e.T & CONFIG_HAS_COMPANIONS && GlobalQueue.Oe !== null && GlobalQueue.Oe(e, t);
  if (e.oe !== undefined)
    e.Te = clock;
  if (u && e._t === notifyEpoch && currentOptimisticLane === null && !reaskArmed)
    return t;
  insertSubs(e);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.ie & REACTIVE_MANUAL_WRITE) && e.Pe === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.ie = e.ie & -4 | REACTIVE_MANUAL_WRITE;
  e.vt = clock;
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/context.js
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, e, E, f) {
  const r = !!f?.user;
  const R = createEffectNode(t, e, E, r ? EFFECT_USER : EFFECT_RENDER, f);
  recompute(R, true);
  !f?.defer && (R.Re === EFFECT_USER || f?.schedule ? R.C.enqueue(R.Re, runEffect.bind(null, R)) : runEffect(R));
}
function notifyEffectStatus(t, e) {
  const E = t !== undefined ? t : this.S;
  const f = e !== undefined ? e : this.o?._;
  if (E & STATUS_ERROR) {
    this.C.notify(this, STATUS_PENDING, 0);
    if (this.Re === EFFECT_USER) {
      if (this.S & STATUS_ERROR) {
        this.Xe = true;
        this.C.enqueue(this.Re, this.et ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.C.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(f));
      throw f;
    }
  } else if (this.Re === EFFECT_RENDER) {
    this.C.notify(this, STATUS_PENDING | STATUS_ERROR, E, f);
  }
}
function runEffect(t) {
  if (!t.Xe || t.ie & REACTIVE_DISPOSED)
    return;
  if (t.S & STATUS_ERROR && t.Re === EFFECT_USER) {
    const e2 = unwrapStatusError(t.o?._);
    t.dt = t.be;
    t.Xe = false;
    try {
      t.St ? t.St(e2, () => {
        const e3 = t.At;
        t.At = undefined;
        e3?.();
      }) : console.error(e2);
    } catch (e3) {
      if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(e3);
        throw e3;
      }
    }
    return;
  }
  const e = t.At;
  t.At = undefined;
  try {
    e?.();
    const E = t.Tt(t.be, t.dt);
    if (false)
      ;
    t.At = E;
  } catch (e2) {
    ext(t)._ = new StatusError(t, e2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(e2);
      throw e2;
    }
  } finally {
    t.dt = t.be;
    t.Xe = false;
  }
}
GlobalQueue.tt = runEffect;
function trackedEffect(t, e) {
  const run = () => {
    if (!E.Xe || E.ie & REACTIVE_DISPOSED)
      return;
    try {
      E.Xe = false;
      recompute(E);
    } finally {}
  };
  const E = computed(() => {
    const e2 = E.At;
    E.At = undefined;
    e2?.();
    const f = staleValues(t);
    E.At = f;
  }, {
    ...e,
    lazy: true
  });
  E.At = undefined;
  E.T = E.T & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  E.Xe = true;
  E.Re = EFFECT_TRACKED;
  E.Ut = run;
  E.C.enqueue(EFFECT_USER, run);
}
setEffectStatusNotify(notifyEffectStatus);

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/signals.js
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/next/target.js
var ownedRaw = new WeakSet;
var storeNextLookup = new WeakMap;
function devAssertNeverUserMutation(e) {
  return;
}
var optHooks = null;
function markDescendants(e) {
  let t = e;
  while (t && !t.d) {
    t.d = true;
    t = t.u;
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $AFFECTS = Symbol(0);
var STORE_VALUE = "v";
var STORE_NODE = "n";
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
    for (let t = 0, o = e.length;t < o; t++)
      markRawOne(e[t]);
  } else {
    for (const t in e)
      markRawOne(e[t]);
  }
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
  let o = wrappableProtos.get(t);
  if (o === undefined) {
    o = Object.prototype.toString.call(e) === "[object Object]" && (typeof Node === "undefined" || !(e instanceof Node));
    wrappableProtos.set(t, o);
  }
  return o;
}
var writeOverride = false;
function setWriteOverride(e) {
  writeOverride = e;
}
function getWriteOverride() {
  return writeOverride;
}
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
function inheritAffectsMarks(e, t, o) {
  for (const [r, s] of affectsScopes) {
    if (r.o?.t && s.scope.has(t) && (s.key === undefined || s.key === o)) {
      GlobalQueue.M(e);
      s.inherited.push(e);
    }
  }
}
var affectsScopes = new Map;
var nextAffectsNodeResolver = null;
function setNextAffectsNodeResolver(e) {
  nextAffectsNodeResolver = e;
}
function affectsScopesLive() {
  return affectsScopes.size > 0;
}
function witnessAffectsMark(e, t) {
  const o = e[STORE_NODE]?.[$AFFECTS];
  if (o?.o?.t)
    GlobalQueue.wt(o);
  if (affectsScopes.size) {
    let r = e[STORE_VALUE];
    for (const [e2, s] of affectsScopes) {
      if (e2 !== o && e2.o?.t && (s.key === undefined || s.key === t)) {
        let t2 = r;
        for (;; ) {
          if (s.scope.has(t2)) {
            GlobalQueue.wt(e2);
            break;
          }
          const o2 = t2?.[$TARGET];
          if (o2 === undefined)
            break;
          const r2 = o2.pb ?? o2[STORE_VALUE];
          if (r2 === t2)
            break;
          t2 = r2;
        }
      }
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/next/patch-hooks.js
var patchHooks = null;
var rowHooks = null;

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/next/store.js
function TargetShape() {
  this.v = undefined;
  this.ch = undefined;
  this.pb = undefined;
  this.n = undefined;
  this.h = undefined;
  this.k = undefined;
  this.dk = undefined;
  this.u = undefined;
  this.pk = undefined;
  this.px = undefined;
  this.d = undefined;
  this.a = undefined;
  this.sc = undefined;
  this.nc = undefined;
  this.adopted = undefined;
  this.fam = undefined;
  this.s = undefined;
  this.ovl = undefined;
  this.del = undefined;
  this.pc = undefined;
  this.hv = undefined;
  this.ht = undefined;
}
TargetShape.prototype = Object.prototype;
function pcOf(e) {
  return e.pc ?? (e.pc = {
    sp: null,
    p: null,
    ro: null,
    wk: null,
    qa: null,
    qe: null
  });
}
function createTarget(e, t, n, r = t?.fam ?? null) {
  const i = Array.isArray(e) ? [] : new TargetShape;
  i.v = e;
  i.ch = e[$TARGET] !== undefined;
  i.pb = null;
  i.n = null;
  i.h = null;
  i.k = null;
  i.dk = null;
  i.pc = null;
  i.u = t;
  i.pk = n;
  i.px = null;
  i.d = false;
  i.a = false;
  i.sc = false;
  i.nc = 0;
  i.adopted = false;
  i.fam = r;
  i.s = false;
  i.ovl = false;
  i.del = null;
  i.hv = null;
  i.ht = null;
  i.px = new Proxy(i, traps);
  i[$PROXY] = i.px;
  (r?.map ?? storeNextLookup).set(e, i);
  return i;
}
function wrapNext(e, t = null, n = null, r = t?.fam ?? null) {
  if (rawValuesUsed && isRawValue(e))
    return e;
  const i = (r?.map ?? storeNextLookup).get(e);
  if (i !== undefined)
    return i.px;
  const o = e[$TARGET];
  if (o !== undefined && o.px === e) {
    if (r === null || o.fam === r)
      return e;
    return createTarget(e, t, n, r).px;
  }
  return createTarget(e, t, n, r).px;
}
function unwrapValue(e) {
  if (e == null || typeof e !== "object")
    return e;
  const t = e[$TARGET];
  if (t !== undefined && t.px === e && t.v !== undefined) {
    if (t.ovl)
      materializePB(t);
    return t.pb ?? t.v;
  }
  return e;
}
function getNode(e, t, n) {
  const r = e.n ??= Object.create(null);
  let i = r[t];
  if (i === undefined) {
    const o = i = signal(n, {
      equals: (t2, n2) => isEqual(t2, n2) || sameLogicalSlot(e, t2, n2),
      unobserved() {
        if (o.o?.t)
          return;
        if (e.n && e.n[t] === o) {
          delete e.n[t];
          e.nc--;
        }
      }
    }, e.fam?.node ?? undefined);
    o.T |= CONFIG_OWNED_WRITE;
    o.acc = isOwnAccessor(e.pb ?? e.v, t);
    o.px = undefined;
    o.pxv = undefined;
    if (e.fam?.opt) {
      ext(o).De = NOT_PENDING;
      o.T |= CONFIG_OPTIMISTIC;
    }
    if (t !== $AFFECTS && affectsScopesLive())
      inheritAffectsMarks(o, e.v, t);
    r[t] = i;
    e.nc++;
    markDescendants(e);
  }
  return i;
}
function sameLogicalSlot(e, t, n) {
  if (t === null || typeof t !== "object" || n === null || typeof n !== "object")
    return false;
  const r = e.fam?.map ?? storeNextLookup;
  const i = r.get(t);
  return i !== undefined && i === r.get(n);
}
function getHasNode(e, t, n) {
  const r = e.h ??= Object.create(null);
  let i = r[t];
  if (i === undefined) {
    const o = i = signal(n, {
      equals: isEqual,
      unobserved() {
        if (o.o?.t)
          return;
        if (e.h && e.h[t] === o)
          delete e.h[t];
      }
    }, e.fam?.node ?? undefined);
    o.T |= CONFIG_OWNED_WRITE;
    if (e.fam?.opt) {
      ext(o).De = NOT_PENDING;
      o.T |= CONFIG_OPTIMISTIC;
    }
    if (affectsScopesLive())
      inheritAffectsMarks(o, e.v, t);
    r[t] = i;
    markDescendants(e);
  }
  return i;
}
function getKeySetNode(e) {
  let t = e.k;
  if (t === null) {
    const n = t = signal(0, {
      equals: false,
      unobserved() {
        if (e.k === n)
          e.k = null;
      }
    }, e.fam?.node ?? undefined);
    n.T |= CONFIG_OWNED_WRITE;
    if (e.fam?.opt) {
      ext(n).De = NOT_PENDING;
      n.T |= CONFIG_OPTIMISTIC;
    }
    e.k = t;
    markDescendants(e);
  }
  return t;
}
function bumpDeep(e) {
  if (e.dk !== null)
    setSignal(e.dk, 1);
}
var foldOlds = new Map;
var hookInstalled = false;
function cloneRaw(e, t) {
  const n = Object.getOwnPropertyDescriptors(e);
  for (const r of Reflect.ownKeys(n)) {
    const i = n[r];
    if (r === "length" && Array.isArray(e))
      continue;
    i.configurable = true;
    if (!i.get && !i.set)
      i.writable = true;
    else if (t)
      t.a = true;
  }
  return Array.isArray(e) ? Object.defineProperties([], n) : Object.create(Object.getPrototypeOf(e), n);
}
function scanAccessorsOnce(e) {
  const t = e.v;
  for (const n of Reflect.ownKeys(t)) {
    if (lookupGetter.call(t, n) !== undefined || lookupSetter.call(t, n) !== undefined) {
      e.a = true;
      break;
    }
  }
  e.sc = true;
  return !e.a;
}
function materializePB(e) {
  if (!e.ovl)
    return;
  const t = e.pb;
  const n = cloneRaw(e.v, e);
  for (const e2 of Reflect.ownKeys(t)) {
    const r2 = Object.getOwnPropertyDescriptor(t, e2);
    if (r2.get || r2.set || !r2.enumerable || !r2.writable || !r2.configurable)
      Object.defineProperty(n, e2, r2);
    else
      n[e2] = r2.value;
  }
  if (e.del !== null) {
    for (const t2 of e.del)
      delete n[t2];
    e.del = null;
  }
  const r = e.fam?.map ?? storeNextLookup;
  r.delete(t);
  ownedRaw.add(n);
  r.set(n, e);
  e.pb = n;
  e.ovl = false;
}
function ensurePB(e) {
  if (activeTransition !== null)
    foldBatches.set(e, activeTransition);
  let t = e.pb;
  if (t === null) {
    if (e.fam === null && !Array.isArray(e.v) && (e.sc ? !e.a : scanAccessorsOnce(e))) {
      t = e.pb = Object.create(e.v);
      e.ovl = true;
    } else
      t = e.pb = cloneRaw(e.v, e);
    if (e.fam?.opt && !projectionWriteActive && !getWriteOverride()) {
      const n = e.n;
      if (n !== null) {
        for (const e2 of Reflect.ownKeys(n)) {
          const r2 = n[e2];
          if (hasActiveOverride2(r2))
            t[e2] = unwrapOverride(r2.o?.De);
        }
      }
      const r = e.h;
      if (r !== null) {
        for (const e2 of Reflect.ownKeys(r)) {
          const n2 = r[e2];
          if (hasActiveOverride2(n2) && !unwrapOverride(n2.o?.De))
            delete t[e2];
        }
      }
    }
    ownedRaw.add(t);
    (e.fam?.map ?? storeNextLookup).set(t, e);
    queueFold(e);
  }
  return t;
}
var PLAIN_HOLD = Symbol("plainHold");
var latestPullActive = false;
function heldMaskView(e) {
  const t = e.ht;
  if (t === null)
    return null;
  if (t !== PLAIN_HOLD && currentTransition(t)?.fn === true)
    return e.ht = e.hv = null;
  return e.hv;
}
function adoptPB(e, t, n = false) {
  if (!n) {
    queueFold(e);
    e.adopted = true;
    if (e.fam?.opt !== true) {
      if (getWriteOverride()) {
        e.ht = e.hv = null;
      } else if (activeTransition !== null || latestPullActive) {
        if (heldMaskView(e) === null)
          e.hv = e.v;
        e.ht = activeTransition ?? PLAIN_HOLD;
      }
    }
  }
  e.pb = null;
  e.ovl = false;
  e.del = null;
  e.sc = false;
  e.a = false;
  if (e.pc !== null)
    e.pc.wk = null;
  e.v = t;
  e.ch = t[$TARGET] !== undefined;
  (e.fam?.map ?? storeNextLookup).set(t, e);
}
var WK_ALL = new Set;
var plainProto = (e) => {
  const t = Object.getPrototypeOf(e);
  return t === Object.prototype || t === Array.prototype || t === null;
};
function queueFold(e) {
  if (foldOlds.has(e))
    return;
  if (!hookInstalled) {
    hookInstalled = true;
    setStoreCommitHook(drainFolds);
  }
  schedule();
  foldOlds.set(e, e.v);
}
var foldBatches = new WeakMap;
function privatizeCommitted(e) {
  if (ownedRaw.has(e.v))
    return;
  const t = cloneRaw(e.v, e);
  ownedRaw.add(t);
  storeNextLookup.set(t, e);
  e.v = t;
  e.ch = false;
  if (e.u) {
    privatizeCommitted(e.u);
    devAssertNeverUserMutation(e.u.v);
    e.u.v[e.pk] = e.v;
  }
}
function drainFolds() {
  if (foldOlds.size === 0)
    return;
  const e = [...foldOlds];
  foldOlds.clear();
  for (const [t, n] of e) {
    if (t.ht === PLAIN_HOLD)
      t.ht = t.hv = null;
    const e2 = t.pb === null;
    if (t.pb !== null) {
      const e3 = foldBatches.get(t);
      if (e3 !== undefined) {
        if (currentTransition(e3).fn === false) {
          foldOlds.set(t, n);
          continue;
        }
        foldBatches.delete(t);
      }
      let r = false;
      const i = t.pb;
      const o = t.n;
      if (o !== null) {
        const e4 = t.pc !== null ? t.pc.wk : null;
        const n2 = e4 === null || e4 === WK_ALL || t.a === true || !plainProto(t.ovl ? t.v : i) ? Reflect.ownKeys(o) : e4;
        for (const e5 of n2) {
          const t2 = o[e5];
          if (t2 !== undefined && t2.Pe !== NOT_PENDING) {
            r = true;
            break;
          }
        }
      }
      if (r) {
        foldOlds.set(t, n);
        continue;
      }
      if (t.ovl) {
        privatizeCommitted(t);
        const e4 = t.v;
        for (const t2 of Reflect.ownKeys(i)) {
          const n2 = Object.getOwnPropertyDescriptor(i, t2);
          if (n2.get || n2.set || !n2.enumerable || !n2.writable || !n2.configurable)
            Object.defineProperty(e4, t2, n2);
          else
            e4[t2] = n2.value;
        }
        if (t.del !== null) {
          for (const n2 of t.del)
            delete e4[n2];
          t.del = null;
        }
        (t.fam?.map ?? storeNextLookup).delete(i);
        t.pb = null;
        t.ovl = false;
        if (t.pc !== null)
          t.pc.wk = null;
      } else {
        if (t.pc !== null && t.pc.ro !== null && !t.adopted && t.fam?.opt !== true && Array.isArray(i) && Array.isArray(t.v))
          rowHooks.emitSetterRowOps(t, t.v, i);
        t.v = i;
        t.ch = false;
        t.pb = null;
        if (t.pc !== null)
          t.pc.wk = null;
      }
    }
    if (t.v === n) {
      t.adopted = false;
      continue;
    }
    if (t.pc !== null && (t.fam !== null || t.adopted)) {
      if (t.pc.ro !== null && t.fam?.opt !== true && (t.fam !== null ? e2 && !t.adopted : t.adopted) && Array.isArray(t.v) && Array.isArray(n))
        rowHooks.emitSetterRowOps(t, n, t.v);
      if (t.pc.p !== null) {
        patchHooks.emitPatchLocal(t, t.v, n);
      }
    }
    if (t.u && t.u.v[t.pk] === n) {
      privatizeCommitted(t.u);
      devAssertNeverUserMutation(t.u.v);
      t.u.v[t.pk] = t.v;
    }
    if (t.adopted) {
      t.adopted = false;
      notifyFold(t, n, t.v);
    }
  }
}
function notifyWrites(e) {
  let t = e.pb;
  if (t === null)
    return;
  if (e.fam?.opt) {
    if (!projectionWriteActive && !getWriteOverride()) {
      optHooks.notifyOptimisticWrites(e, t);
      return;
    }
    if (!projectionWriteActive) {
      setProjectionWriteActive(true);
      try {
        notifyWrites(e);
      } finally {
        setProjectionWriteActive(false);
      }
      return;
    }
  }
  const n = e.v;
  const r = e.n;
  const i = e.pc !== null ? e.pc.wk : null;
  const o = i === WK_ALL || e.a === true || !plainProto(e.ovl ? e.v : t) ? null : i;
  if (r !== null) {
    const i2 = o ?? Reflect.ownKeys(r);
    for (const o2 of i2) {
      const i3 = r[o2];
      if (i3 === undefined)
        continue;
      if (i3.acc === true || hasOwn.call(t, o2) && lookupGetter.call(t, o2) !== undefined) {
        i3.acc = isOwnAccessor(t, o2);
        const e2 = Object.getOwnPropertyDescriptor(n, o2);
        const r2 = Object.getOwnPropertyDescriptor(t, o2);
        if (e2 && (e2.get || e2.set) || r2 && (r2.get || r2.set)) {
          if (e2?.get !== r2?.get || e2?.set !== r2?.set || e2?.value !== r2?.value)
            setSignal(i3, () => FORCE);
          continue;
        }
        if (!isEqual(e2?.value, r2?.value))
          setSignal(i3, () => r2?.value);
        continue;
      }
      const f2 = e.del !== null && e.del.has(o2) ? undefined : t[o2];
      setSignal(i3, () => f2);
    }
  }
  const f = e.h;
  if (f !== null) {
    const n2 = o ?? Reflect.ownKeys(f);
    for (const r2 of n2) {
      const n3 = f[r2];
      if (n3 !== undefined)
        setSignal(n3, r2 in t && !(e.del !== null && e.del.has(r2)));
    }
  }
  if (e.dk !== null) {
    if (e.del !== null && e.del.size !== 0)
      bumpDeep(e);
    else
      for (const r2 of o ?? Reflect.ownKeys(t)) {
        const i2 = t[r2];
        const o2 = n[r2];
        if (i2 !== null && typeof i2 === "object" ? !targetsEqual(o2, i2) : !isEqual(o2, i2)) {
          bumpDeep(e);
          break;
        }
      }
  }
  if (e.k !== null) {
    let r2;
    if (e.ovl) {
      r2 = e.del !== null && e.del.size !== 0;
      if (!r2) {
        for (const e2 of Reflect.ownKeys(t)) {
          if (!hasOwn.call(n, e2)) {
            r2 = true;
            break;
          }
        }
      }
    } else {
      r2 = Array.isArray(t) && Array.isArray(n) ? arrayStructureChanged(n, t) : membershipChanged(n, t);
    }
    if (r2)
      setSignal(e.k, (e2) => e2 + 1);
  }
  if (e.fam === null && patchHooks !== null && patchHooks.hasPatches())
    patchHooks.emitPatch(e, t, n);
  if (e.fam !== null && e.pb !== null && getWriteOverride()) {
    if (e.ht !== null)
      e.ht = e.hv = null;
    const n2 = e.v;
    e.pb = null;
    e.v = t;
    e.ch = false;
    if (e.u && e.u.v[e.pk] === n2) {
      privatizeCommitted(e.u);
      devAssertNeverUserMutation(e.u.v);
      e.u.v[e.pk] = t;
    }
  }
}
var FORCE = Symbol();
function targetsEqual(e, t) {
  if (e === null || typeof e !== "object")
    return false;
  const n = storeNextLookup.get(e);
  return n !== undefined && n === storeNextLookup.get(t);
}
function arrayStructureChanged(e, t) {
  if (e.length !== t.length)
    return true;
  for (let n = 0;n < t.length; n++) {
    const r = e[n];
    const i = t[n];
    if (!isEqual(r, i) && !targetsEqual(r, i))
      return true;
  }
  return false;
}
function membershipChanged(e, t) {
  const n = Reflect.ownKeys(t);
  if (Reflect.ownKeys(e).length !== n.length)
    return true;
  for (const t2 of n)
    if (!(t2 in e))
      return true;
  return false;
}
function notifyKeyDiff(e, t, n, r, i = true) {
  if (e.acc === true || i && hasOwn.call(r, t) && lookupGetter.call(r, t) !== undefined) {
    e.acc = isOwnAccessor(r, t);
    const i2 = Object.getOwnPropertyDescriptor(n, t);
    const o = Object.getOwnPropertyDescriptor(r, t);
    if (i2 && (i2.get || i2.set) || o && (o.get || o.set)) {
      if (i2?.get !== o?.get || i2?.set !== o?.set || i2?.value !== o?.value)
        setSignal(e, () => FORCE);
      return;
    }
    const f = i2?.value;
    const l = o?.value;
    if (!isEqual(f, l) && !targetsEqual(f, l))
      setSignal(e, typeof l === "function" ? () => l : l);
  } else {
    const i2 = n[t];
    const o = r[t];
    if (!isEqual(i2, o) && !targetsEqual(i2, o))
      setSignal(e, typeof o === "function" ? () => o : o);
  }
}
function hasAccessorFlag(e) {
  return e.acc === true;
}
function notifyKeyValue(e, t, n, r, i, o) {
  if (e.acc === true) {
    notifyKeyDiff(e, t, i, o, false);
    return;
  }
  if (!isEqual(n, r) && !targetsEqual(n, r))
    setSignal(e, typeof r === "function" ? () => r : r);
}
function notifyFoldTail(e, t, n) {
  const r = e.h;
  if (r !== null) {
    for (const e2 of Reflect.ownKeys(r))
      setSignal(r[e2], e2 in n);
  }
  if (e.k !== null) {
    const r2 = Array.isArray(n) && Array.isArray(t) ? arrayStructureChanged(t, n) : membershipChanged(t, n);
    if (r2)
      setSignal(e.k, (e2) => e2 + 1);
  }
}
function notifyFold(e, t, n) {
  if (e.dk !== null && t !== n)
    bumpDeep(e);
  if (e.fam?.opt && !projectionWriteActive) {
    setProjectionWriteActive(true);
    try {
      notifyFold(e, t, n);
    } finally {
      setProjectionWriteActive(false);
    }
    return;
  }
  const r = e.n;
  if (r !== null) {
    for (const e2 of Reflect.ownKeys(r)) {
      notifyKeyDiff(r[e2], e2, t, n);
    }
  }
  const i = e.h;
  if (i !== null) {
    for (const e2 of Reflect.ownKeys(i))
      setSignal(i[e2], e2 in n);
  }
  if (e.k !== null) {
    const r2 = Array.isArray(n) && Array.isArray(t) ? arrayStructureChanged(t, n) : membershipChanged(t, n);
    if (r2)
      setSignal(e.k, (e2) => e2 + 1);
  }
}
var writing = 0;
var writeScopes = null;
function scopeKey(e) {
  if (e.fam !== null)
    return e.fam;
  let t = e;
  while (t.u !== null)
    t = t.u;
  return t;
}
function inDraft(e) {
  return writeScopes !== null && writeScopes.has(scopeKey(e));
}
function serveShallow(e, t, n) {
  if (n !== null && typeof n === "object" && n[$TARGET] !== undefined)
    return draftServe(e, wrapNext(n, e, t));
  return n;
}
function draftServe(e, t) {
  if (writeScopes !== null && inDraft(e)) {
    const e2 = t?.[$TARGET];
    if (e2 !== undefined && e2.v !== undefined)
      writeScopes.add(scopeKey(e2));
  }
  return t;
}
var pendingNotify = new Set;
var UNSAFE_KEYS = new Set(["__proto__", "prototype", "constructor"]);
function inOwnerContext() {
  const e = getOwner();
  if (e === null)
    return false;
  const t = e.Ct ? e.Ot : e;
  return t != null && !(t.T & CONFIG_CHILDREN_FORBIDDEN);
}
function inForbiddenScope() {
  const e = getOwner();
  if (e === null)
    return false;
  const t = e.Ct ? e.Ot : e;
  return t != null && !!(t.T & CONFIG_CHILDREN_FORBIDDEN);
}
function foldHeld(e) {
  const t = e.n;
  if (t === null)
    return false;
  for (const e2 of Reflect.ownKeys(t)) {
    const n = t[e2];
    if (n.Pe !== NOT_PENDING && n._e != null && n._e.fn !== true)
      return true;
  }
  return false;
}
function readSource(e) {
  if (e.ht !== null && !latestReadActive && !inDraft(e) && !getWriteOverride() && !inOwnerContext()) {
    const t = heldMaskView(e);
    if (t !== null)
      return t;
  }
  if (e.pb !== null && (inDraft(e) || getWriteOverride() || inOwnerContext() || e.fam !== null && !foldHeld(e) && !inForbiddenScope()))
    return e.pb;
  return e.v;
}
var hasOwn = Object.prototype.hasOwnProperty;
var lookupGetter = Object.prototype.__lookupGetter__;
var lookupSetter = Object.prototype.__lookupSetter__;
function isOwnAccessor(e, t) {
  return hasOwn.call(e, t) && (lookupGetter.call(e, t) !== undefined || lookupSetter.call(e, t) !== undefined);
}
function hasActiveOverride2(e) {
  return e.o?.De !== undefined && e.o?.De !== NOT_PENDING;
}
function nodeValue(e, t) {
  const n = hasActiveOverride2(e) ? unwrapOverride(e.o?.De) : e.Pe !== NOT_PENDING && (latestReadActive || inOwnerContext()) ? e.Pe : t;
  return n === FORCE ? t : n;
}
function serveDataKey(e, t, n, r, i) {
  const o = e.ch && r === e.v;
  let f = n;
  if (t === "length" && e.fam?.opt === true && !o && Array.isArray(r)) {
    if (!inDraft(e)) {
      const r2 = e.n?.length;
      if (r2 !== undefined) {
        if (getObserver() !== null)
          read(r2);
      } else if (getObserver() !== null) {
        read(getNode(e, t, n));
      }
    }
    return optHooks.optimisticView(e, r).length;
  }
  if (inDraft(e)) {
    if (e.fam?.opt && e.pb === null) {
      const n2 = e.n?.[t];
      if (n2 !== undefined && hasActiveOverride2(n2))
        f = unwrapOverride(n2.o?.De);
    }
  } else {
    if (i !== undefined) {
      if (getObserver() !== null) {
        let e2 = readNodeFast(i);
        if (e2 === READ_SLOW)
          e2 = read(i);
        if (!o || hasActiveOverride2(i))
          f = e2 === FORCE ? n : e2;
      } else if (!o || hasActiveOverride2(i)) {
        f = nodeValue(i, n);
      }
    } else if (getObserver() !== null) {
      read(getNode(e, t, n));
    }
  }
  if (e.s)
    return serveShallow(e, t, f);
  if (i !== undefined) {
    if (i.pxv === f && f !== undefined)
      return draftServe(e, i.px);
    if (!isWrappable(f))
      return f;
    const n2 = wrapNext(f, e, t);
    i.px = n2;
    i.pxv = f;
    return draftServe(e, n2);
  }
  if (!isWrappable(f))
    return f;
  return draftServe(e, wrapNext(f, e, t));
}
function firewallGate(e) {
  if (projectionWriteActive || getWriteOverride())
    return;
  const t = e.fam?.node;
  if (t != null && t.S & (STATUS_UNINITIALIZED | STATUS_ERROR))
    read(t);
}
function pullProjectionForLatest(e) {
  const t = e.fam.node;
  if (t == null)
    return;
  const n = latestReadActive;
  setLatestReadActive(false);
  const r = latestPullActive;
  latestPullActive = true;
  try {
    prepareComputed(t, true);
  } finally {
    latestPullActive = r;
    setLatestReadActive(n);
  }
}
var traps = {
  get(e, t, n) {
    if (typeof t !== "string") {
      if (t === $TARGET)
        return e;
      if (t === $PROXY)
        return n;
      if (t === $REFRESH)
        return e.fam?.node ?? undefined;
      if (t === $TRACK) {
        if (pendingCheckActive)
          witnessAffectsMark(e, t);
        if (e.fam !== null && getObserver() === null && !inDraft(e))
          firewallGate(e);
        if (!inDraft(e) && getObserver() !== null) {
          read(getKeySetNode(e));
          const t2 = readSource(e);
          if (t2[$TARGET] !== undefined)
            t2[$TRACK];
        }
        return;
      }
    }
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    if (e.fam !== null && getObserver() === null && !inDraft(e))
      firewallGate(e);
    if (e.fam !== null && latestReadActive && !inDraft(e) && !getWriteOverride())
      pullProjectionForLatest(e);
    const r = readSource(e);
    if (e.del !== null && r === e.pb && e.del.has(t)) {
      if (!inDraft(e) && getObserver() !== null)
        read(getNode(e, t, undefined));
      return;
    }
    if (e.ch === false && writeScopes === null) {
      const n2 = e.n?.[t];
      if (n2 !== undefined && n2.acc !== true && getObserver() !== null) {
        let r2 = readNodeFast(n2);
        if (r2 === READ_SLOW)
          r2 = read(n2);
        if (r2 === null || typeof r2 !== "object")
          return r2;
        if (e.s)
          return serveShallow(e, t, r2);
        if (n2.pxv === r2)
          return n2.px;
        if (isWrappable(r2)) {
          const i2 = wrapNext(r2, e, t);
          n2.px = i2;
          n2.pxv = r2;
          return i2;
        }
        return r2;
      }
    }
    const i = e.n?.[t];
    {
      const o2 = i !== undefined ? i.acc === true : !inDraft(e) && getObserver() !== null && isOwnAccessor(r, t);
      if (o2) {
        if (!inDraft(e) && getObserver() !== null)
          read(i ?? getNode(e, t, undefined));
        const o3 = Reflect.get(r, t, n);
        if (e.s)
          return serveShallow(e, t, o3);
        return isWrappable(o3) ? draftServe(e, wrapNext(o3, e, t)) : o3;
      }
    }
    const o = e.ovl && r === e.pb;
    if ((t === "constructor" || t === "__proto__" || t === "prototype") && !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)))
      return;
    let f = r[t];
    if (f === undefined ? !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)) : false) {
      f = Reflect.get(r, t, n);
      if (typeof f === "function")
        return f;
      if (f === undefined && !inDraft(e)) {
        if (getObserver() !== null)
          read(getNode(e, t, undefined));
        const n2 = e.n?.[t];
        if (n2) {
          const r2 = nodeValue(n2, undefined);
          if (e.s)
            return serveShallow(e, t, r2);
          return isWrappable(r2) ? draftServe(e, wrapNext(r2, e, t)) : r2;
        }
      } else if (f === undefined && inDraft(e) && e.fam?.opt && e.pb === null) {
        const n2 = e.n?.[t];
        if (n2 !== undefined && hasActiveOverride2(n2))
          f = unwrapOverride(n2.o?.De);
      }
      if (e.s)
        return serveShallow(e, t, f);
      return isWrappable(f) ? draftServe(e, wrapNext(f, e, t)) : f;
    }
    if (typeof f === "function" && !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)))
      return f;
    return serveDataKey(e, t, f, r, i);
  },
  has(e, t) {
    if (t === $TARGET || t === $PROXY || t === $TRACK)
      return true;
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    if (e.fam !== null && getObserver() === null && !inDraft(e))
      firewallGate(e);
    const n = readSource(e);
    let r = t in n;
    if (r && e.del !== null && n === e.pb && e.del.has(t))
      r = false;
    if (!inDraft(e)) {
      if (getObserver() !== null) {
        const n2 = getHasNode(e, t, r);
        const i = read(n2);
        if (hasActiveOverride2(n2))
          r = !!i;
      } else {
        const n2 = e.h?.[t];
        if (n2 !== undefined && hasActiveOverride2(n2))
          r = !!unwrapOverride(n2.o?.De);
      }
    } else if (e.fam?.opt && e.pb === null) {
      const n2 = e.h?.[t];
      if (n2 !== undefined && hasActiveOverride2(n2))
        r = !!unwrapOverride(n2.o?.De);
    }
    return r;
  },
  ownKeys(e) {
    if (pendingCheckActive)
      witnessAffectsMark(e);
    if (e.fam !== null && getObserver() === null && !inDraft(e))
      firewallGate(e);
    if (!inDraft(e) && getObserver() !== null)
      read(getKeySetNode(e));
    const t = readSource(e);
    let n;
    if (e.ovl && t === e.pb) {
      n = Reflect.ownKeys(e.v);
      const r = e.del;
      if (r !== null && r.size !== 0)
        n = n.filter((e2) => !r.has(e2));
      for (const r2 of Reflect.ownKeys(t)) {
        if (!hasOwn.call(e.v, r2))
          n.push(r2);
      }
    } else
      n = Reflect.ownKeys(t);
    if (e.fam?.opt && e.h !== null && (!inDraft(e) || e.pb === null)) {
      let t2 = null;
      for (const r of Reflect.ownKeys(e.h)) {
        const i = e.h[r];
        if (!hasActiveOverride2(i))
          continue;
        t2 ??= new Set(n);
        if (unwrapOverride(i.o?.De))
          t2.add(r);
        else
          t2.delete(r);
      }
      if (t2 !== null)
        return [...t2];
    }
    return n;
  },
  getOwnPropertyDescriptor(e, t) {
    const n = readSource(e);
    let r = Object.getOwnPropertyDescriptor(n, t);
    if (e.ovl && n === e.pb) {
      if (e.del !== null && e.del.has(t))
        return;
      if (r === undefined)
        r = Object.getOwnPropertyDescriptor(e.v, t);
    }
    if (e.fam?.opt && !inDraft(e)) {
      const n2 = e.h?.[t];
      if (n2 !== undefined && hasActiveOverride2(n2)) {
        if (!unwrapOverride(n2.o?.De))
          return;
        if (r === undefined) {
          const n3 = e.n?.[t];
          return {
            value: n3 !== undefined ? nodeValue(n3, undefined) : undefined,
            writable: true,
            enumerable: true,
            configurable: true
          };
        }
      }
    }
    if (r === undefined)
      return;
    if (!(t === "length" && Array.isArray(e)))
      r.configurable = true;
    return r;
  },
  set(e, t, n) {
    const r = inDraft(e);
    const i = !r && getWriteOverride();
    if (!r && !i)
      return true;
    if (t === "__proto__")
      return true;
    const o = e.s ? n : unwrapValue(n);
    const f = ensurePB(e);
    pendingNotify.add(e);
    const l = pcOf(e);
    if (Array.isArray(f)) {
      if (t === "length")
        l.wk = WK_ALL;
      else if (l.wk !== WK_ALL) {
        const e2 = l.wk ??= new Set;
        e2.add(t);
        e2.add("length");
      }
    } else if (l.wk !== WK_ALL)
      (l.wk ??= new Set).add(t);
    if (UNSAFE_KEYS.has(t)) {
      Object.defineProperty(f, t, {
        value: o,
        writable: true,
        enumerable: true,
        configurable: true
      });
      if (e.del !== null)
        e.del.delete(t);
      return true;
    }
    if (e.ovl && !hasOwn.call(f, t)) {
      Object.defineProperty(f, t, {
        value: o,
        writable: true,
        enumerable: true,
        configurable: true
      });
    } else
      f[t] = o;
    if (e.del !== null)
      e.del.delete(t);
    if (e.s && o !== null && typeof o === "object")
      markRawOne(o);
    if (i)
      notifyWrites(e);
    return true;
  },
  defineProperty(e, t, n) {
    const r = inDraft(e);
    const i = !r && getWriteOverride();
    if (!r && !i)
      return true;
    if (t === "__proto__")
      return true;
    if (n.get || n.set) {
      e.a = true;
      if (e.pc !== null && e.pc.p !== null)
        patchHooks.demoteToEffects(e);
    }
    if ("value" in n)
      n = {
        ...n,
        value: unwrapValue(n.value)
      };
    const o = ensurePB(e);
    pendingNotify.add(e);
    const f = pcOf(e);
    if (f.wk !== WK_ALL)
      (f.wk ??= new Set).add(t);
    Object.defineProperty(o, t, n);
    if (e.del !== null)
      e.del.delete(t);
    if (i)
      notifyWrites(e);
    return true;
  },
  deleteProperty(e, t) {
    const n = inDraft(e);
    const r = !n && getWriteOverride();
    if (!n && !r)
      return true;
    const i = ensurePB(e);
    pendingNotify.add(e);
    const o = pcOf(e);
    if (o.wk !== WK_ALL)
      (o.wk ??= new Set).add(t);
    delete i[t];
    if (e.ovl && hasOwn.call(e.v, t))
      (e.del ??= new Set).add(t);
    if (r)
      notifyWrites(e);
    return true;
  }
};
function storeSetterNext(e, t, n = true) {
  const r = e[$TARGET];
  const i = writeScopes;
  writeScopes = new Set;
  writeScopes.add(scopeKey(r));
  writing++;
  let o;
  try {
    o = t(e);
  } finally {
    writing--;
    writeScopes = i;
    if (writing === 0 && pendingNotify.size) {
      const e2 = [...pendingNotify];
      pendingNotify.clear();
      for (const t2 of e2)
        notifyWrites(t2);
    }
  }
  if (o !== undefined && o !== e && isWrappable(o)) {
    if (r.fam?.opt && !projectionWriteActive && !getWriteOverride()) {
      optHooks.notifyOptimisticWrites(r, unwrapValue(o));
    } else {
      adoptPB(r, unwrapValue(o));
    }
  }
}
setNextAffectsNodeResolver((e, t) => t === $AFFECTS ? getNode(e, $AFFECTS, undefined) : getNode(e, t, (e.pb ?? e.v)[t]));
function createStoreNext(e, t = false) {
  const n = wrapNext(e);
  if (t) {
    n[$TARGET].s = true;
    markRawIngest(e);
  }
  const setter = (e2) => storeSetterNext(n, e2);
  return [n, setter];
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/map.js
function mapArray(t, s, i) {
  const e = typeof i?.keyed === "function" ? i.keyed : undefined;
  const r = s.length > 1;
  const n = s;
  const h = {
    Wt: createOwner(),
    xt: 0,
    Kt: t,
    $t: [],
    qt: n,
    zt: [],
    Jt: [],
    Xt: e,
    Yt: e || i?.keyed === false ? [] : undefined,
    Zt: r && i?.keyed !== false ? [] : undefined,
    ts: i?.keyed === false,
    ss: i?.fallback
  };
  const o = computed(updateKeyedMap.bind(h));
  h.Wt.Ot = o;
  o.T &= ~CONFIG_AUTO_DISPOSE;
  return accessor(o);
}
var pureOptions = {
  ownedWrite: true
};
function updateKeyedMap() {
  const t = this.Kt() || [], s = t.length;
  t[$TRACK];
  runWithOwner(this.Wt, () => {
    let i, e, r, n, h = this.Yt ? this.ts ? () => {
      r[e] = signal(t[e], pureOptions);
      return this.qt(accessor(r[e]), e);
    } : () => {
      r[e] = signal(t[e], pureOptions);
      n && (n[e] = signal(e, pureOptions));
      return this.qt(accessor(r[e]), n ? accessor(n[e]) : undefined);
    } : this.Zt ? () => {
      const s2 = t[e];
      n[e] = signal(e, pureOptions);
      return this.qt(s2, accessor(n[e]));
    } : () => {
      const s2 = t[e];
      return this.qt(s2);
    };
    if (s === 0) {
      if (this.xt !== 0) {
        this.Wt.dispose(false);
        this.Jt = [];
        this.$t = [];
        this.zt = [];
        this.xt = 0;
        this.Yt && (this.Yt = []);
        this.Zt && (this.Zt = []);
      }
      if (this.ss && !this.zt[0]) {
        this.Jt[0]?.dispose();
        this.zt[0] = runWithOwner(this.Jt[0] = createOwner(), this.ss);
      }
    } else if (this.xt === 0) {
      const o = new Array(s);
      const c = new Array(s);
      r = this.Yt && new Array(s);
      n = this.Zt && new Array(s);
      try {
        for (e = 0;e < s; e++)
          o[e] = runWithOwner(c[e] = createOwner(), h);
      } catch (t2) {
        for (i = 0;i <= e; i++)
          c[i]?.dispose();
        throw t2;
      }
      if (this.Jt[0])
        this.Jt[0].dispose();
      this.zt = o;
      this.Jt = c;
      r && (this.Yt = r);
      n && (this.Zt = n);
      this.$t = t.slice(0);
      this.xt = s;
    } else {
      let o, c, a, f, u, p, w, l, d;
      for (o = 0, c = Math.min(this.xt, s);o < c && (this.$t[o] === t[o] || this.Yt && compare(this.Xt, this.$t[o], t[o])); o++) {
        if (this.Yt)
          setSignal(this.Yt[o], t[o]);
      }
      for (c = this.xt - 1, a = s - 1;c >= o && a >= o && (this.$t[c] === t[a] || this.Yt && compare(this.Xt, this.$t[c], t[a])); c--, a--)
        ;
      if (o === s && this.xt === s) {
        this.$t = t.slice(0);
        return;
      }
      const O = s - this.xt;
      const m = new Array(s);
      const _ = new Array(s);
      r = this.Yt ? new Array(s) : undefined;
      n = this.Zt ? new Array(s) : undefined;
      p = new Map;
      w = new Array(a + 1);
      for (e = a;e >= o; e--) {
        f = t[e];
        u = this.Xt ? this.Xt(f) : f;
        i = p.get(u);
        w[e] = i === undefined ? -1 : i;
        p.set(u, e);
      }
      for (i = o;i <= c; i++) {
        f = this.$t[i];
        u = this.Xt ? this.Xt(f) : f;
        e = p.get(u);
        if (e !== undefined && e !== -1) {
          m[e] = this.zt[i];
          _[e] = this.Jt[i];
          r && (r[e] = this.Yt[i]);
          n && (n[e] = this.Zt[i]);
          e = w[e];
          p.set(u, e);
        } else
          (l ??= []).push(this.Jt[i]);
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
        m[i] = this.zt[i];
        _[i] = this.Jt[i];
        r && (r[i] = this.Yt[i]);
        n && (n[i] = this.Zt[i]);
      }
      for (e = o;e <= a; e++) {
        if (r)
          setSignal(r[e], t[e]);
        if (n)
          setSignal(n[e], e);
      }
      for (e = a + 1;e < s; e++) {
        m[e] = this.zt[e - O];
        _[e] = this.Jt[e - O];
        if (r) {
          r[e] = this.Yt[e - O];
          setSignal(r[e], t[e]);
        }
        if (n) {
          n[e] = this.Zt[e - O];
          if (O !== 0)
            setSignal(n[e], e);
        }
      }
      this.zt = m;
      this.Jt = _;
      r && (this.Yt = r);
      n && (this.Zt = n);
      this.xt = s;
      this.$t = t.slice(0);
      if (l)
        for (i = 0;i < l.length; i++)
          l[i].dispose();
    }
  });
  return this.zt;
}
function compare(t, s, i) {
  return t ? t(s) === t(i) : true;
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/next/reconcile.js
function reconcileNextState(e, n, t, o = false) {
  if (n == null)
    throw new Error("");
  const l = n?.[$TARGET];
  if (l === undefined || l.px !== n)
    throw new Error("");
  if (l.ovl)
    materializePB(l);
  let i = t === null ? null : typeof t === "string" ? (e2) => e2?.[t] : t;
  if (o && e !== n && e?.[$TARGET] !== undefined) {
    const n2 = l.pb ?? l.v;
    if (n2 === e)
      return;
    adoptPB(l, e);
    return;
  }
  const u = unwrapValue(e);
  if (i) {
    const e2 = l.pb ?? l.v;
    const n2 = i(e2);
    if (n2 !== undefined && !sameKey(i(u), n2)) {
      if (!o)
        throw new Error("");
      (l.fam?.map ?? storeNextLookup).delete(l.pb ?? l.v);
      adoptPB(l, u);
      return;
    }
  }
  if (l.fam?.opt === true && !projectionWriteActive && !getWriteOverride()) {
    optHooks.applyTentative(l, u, i);
    return;
  }
  applyAdopt(l, u, i, o);
}
function applyAdopt(e, n, t, o = false) {
  const l = e.pb ?? e.v;
  if (n === l && !ownedRaw.has(l))
    return;
  const i = e.fam;
  const u = i?.opt === true ? optHooks.optimisticView(e, l) : l;
  const f = Array.isArray(n);
  const s = i === null;
  const r = e.s === true;
  const c = e.v;
  adoptPB(e, n, s);
  if (patchHooks !== null && s && e.pc !== null && e.pc.p !== null) {
    {
      patchHooks.emitPatchLocal(e, n, c);
    }
  }
  if (r)
    markRawIngest(n);
  if (Array.isArray(u) !== f) {
    if (s)
      notifyFold(e, c, n);
    return;
  }
  if (f) {
    const l2 = u;
    const f2 = n;
    const a = s ? e.n : null;
    let p = 0;
    if (t && !r) {
      const u2 = l2.length;
      const s2 = f2.length;
      let r2 = false;
      let d = 0;
      for (const y2 = Math.min(u2, s2);d < y2; d++) {
        const u3 = f2[d];
        const s3 = l2[d];
        if (s3 !== u3 && !(s3 !== null && typeof s3 === "object" && u3 !== null && typeof u3 === "object" && sameKey(t(s3), t(u3))))
          break;
        if ((s3 !== u3 || u3 !== null && typeof u3 === "object" && ownedRaw.has(u3)) && u3 !== null && typeof u3 === "object")
          descend(unwrapValue(s3), u3, t, i, o);
        if (e.dk !== null && !r2 && !(u3 !== null && typeof u3 === "object" ? targetsEqual(s3, u3) : isEqual(s3, u3))) {
          bumpDeep(e);
          r2 = true;
        }
        if (a !== null) {
          const e2 = a[d];
          if (e2 !== undefined) {
            p++;
            notifyKeyValue(e2, d, c[d], u3, c, n);
          }
        }
      }
      if (e.dk !== null && !r2 && d < f2.length)
        bumpDeep(e);
      const y = d;
      let w = null;
      for (;d < f2.length; d++) {
        const e2 = f2[d];
        if (e2 !== null && typeof e2 === "object") {
          const n2 = t(e2);
          let u3;
          if (n2 !== undefined) {
            if (w === null) {
              w = new Map;
              for (let e4 = y;e4 < l2.length; e4++) {
                const n3 = unwrapValue(l2[e4]);
                if (n3 !== null && typeof n3 === "object") {
                  const o2 = t(n3);
                  if (o2 === undefined)
                    continue;
                  const l3 = w.get(o2);
                  if (l3 === undefined)
                    w.set(o2, e4);
                  else if (Array.isArray(l3))
                    l3.push(e4);
                  else
                    w.set(o2, [l3, e4]);
                }
              }
            }
            const e3 = w.get(n2);
            if (e3 === undefined)
              u3 = undefined;
            else if (Array.isArray(e3)) {
              u3 = unwrapValue(l2[e3.shift()]);
              if (e3.length === 1)
                w.set(n2, e3[0]);
            } else {
              u3 = unwrapValue(l2[e3]);
              w.delete(n2);
            }
          } else {
            u3 = unwrapValue(l2[d]);
          }
          descend(u3, e2, t, i, o);
        }
        if (a !== null) {
          const e3 = a[d];
          if (e3 !== undefined) {
            p++;
            notifyKeyDiff(e3, d, c, n, false);
          }
        }
      }
      if (rowHooks !== null && e.pc !== null && e.pc.ro !== null && (y < s2 || u2 !== s2))
        buildAndEmitRowOps(e, l2, f2, y, t);
    } else {
      const u2 = Math.min(l2.length, f2.length);
      const s2 = f2.length;
      let d = false;
      const y = rowHooks !== null && e.pc !== null ? e.pc.sp : null;
      const w = rowHooks !== null && e.pc !== null ? e.pc.ro : null;
      let b = t !== null && (w !== null || y !== null);
      let m = 0;
      for (let w2 = 0;w2 < s2; w2++) {
        const s3 = f2[w2];
        if (b && w2 < u2) {
          const e2 = l2[w2];
          if (e2 !== null && typeof e2 === "object" && s3 !== null && typeof s3 === "object" && sameKey(t(e2), t(s3)))
            m++;
          else
            b = false;
        }
        if (y !== null && w2 < u2 && (t === null || b)) {
          const n2 = l2[w2];
          if (n2 !== s3)
            rowHooks.emitSlotPatch(e, w2, s3, n2);
        }
        if (!r && w2 < u2 && s3 !== null && typeof s3 === "object")
          descend(unwrapValue(l2[w2]), s3, t, i, o);
        if (e.dk !== null && !d && !(s3 !== null && typeof s3 === "object" ? targetsEqual(l2[w2], s3) : isEqual(l2[w2], s3))) {
          bumpDeep(e);
          d = true;
        }
        if (a !== null) {
          const e2 = a[w2];
          if (e2 !== undefined) {
            p++;
            notifyKeyDiff(e2, w2, c, n, false);
          }
        }
      }
      if (w !== null) {
        const n2 = l2.length;
        if (t !== null) {
          if (m < s2 || n2 !== s2)
            buildAndEmitRowOps(e, l2, f2, m, t);
        } else if (n2 !== s2) {
          buildAndEmitRowOps(e, l2, f2, u2, null);
        }
      }
    }
    if (s) {
      if (a !== null && p < e.nc) {
        for (const e2 of Reflect.ownKeys(a)) {
          const t2 = typeof e2 === "string" ? +e2 : NaN;
          if (!(t2 >= 0 && t2 < f2.length))
            notifyKeyDiff(a[e2], e2, c, n, false);
        }
      }
      notifyFoldTail(e, c, n);
    }
    return;
  } else {
    if (e.pc !== null && e.pc.p !== null && s && e.n === null && e.h === null && e.k === null && e.dk === null && i === null) {
      return;
    }
    const l2 = s ? e.n : null;
    let f2 = 0;
    let a = false;
    for (const s2 in n) {
      const p2 = n[s2];
      const d = c[s2];
      const y = p2 !== null && typeof p2 === "object";
      if (d === p2 && (!y || !ownedRaw.has(p2)) && (l2 === null || l2[s2] === undefined || !hasAccessorFlag(l2[s2]))) {
        if (l2 !== null && l2[s2] !== undefined)
          f2++;
        continue;
      }
      if (y && !r)
        descend(unwrapValue(u[s2]), p2, t, i, o);
      if (e.dk !== null && !a && !(y ? targetsEqual(d, p2) : isEqual(d, p2))) {
        bumpDeep(e);
        a = true;
      }
      if (l2 !== null) {
        const e2 = l2[s2];
        if (e2 !== undefined) {
          f2++;
          notifyKeyValue(e2, s2, d, p2, c, n);
        }
      }
    }
    const p = Object.getOwnPropertySymbols(n);
    for (let e2 = 0;e2 < p.length; e2++) {
      const s2 = p[e2];
      const a2 = n[s2];
      if (!r && a2 !== null && typeof a2 === "object")
        descend(unwrapValue(u[s2]), a2, t, i, o);
      if (l2 !== null) {
        const e3 = l2[s2];
        if (e3 !== undefined) {
          f2++;
          notifyKeyValue(e3, s2, c[s2], a2, c, n);
        }
      }
    }
    if (s) {
      if (l2 !== null && f2 < e.nc) {
        for (const e2 of Reflect.ownKeys(l2)) {
          if (!hasOwnP.call(n, e2))
            notifyKeyDiff(l2[e2], e2, c, n, false);
        }
      }
      notifyFoldTail(e, c, n);
    }
    return;
  }
}
var hasOwnP = Object.prototype.hasOwnProperty;
function sameKey(e, n) {
  return e === n || e !== e && n !== n;
}
function buildAndEmitRowOps(e, n, t, o, l) {
  rowHooks.emitRowOps(e, t, buildRowOps(n, t, o, l));
}
function buildRowOps(e, n, t, o) {
  const l = e.length;
  const i = n.length;
  const u = new Array(i - t);
  let f = null;
  if (o !== null && t < l) {
    f = new Map;
    for (let n2 = t;n2 < l; n2++) {
      const t2 = unwrapValue(e[n2]);
      if (t2 !== null && typeof t2 === "object") {
        const e2 = o(t2);
        if (e2 === undefined)
          continue;
        const l2 = f.get(e2);
        if (l2 === undefined)
          f.set(e2, n2);
        else if (Array.isArray(l2))
          l2.push(n2);
        else
          f.set(e2, [l2, n2]);
      }
    }
  }
  const s = f !== null ? new Set : null;
  for (let e2 = t;e2 < i; e2++) {
    const l2 = n[e2];
    let i2 = -1;
    if (l2 !== null && typeof l2 === "object" && f !== null) {
      const e3 = o(l2);
      if (e3 !== undefined) {
        const n2 = f.get(e3);
        if (n2 !== undefined) {
          if (Array.isArray(n2)) {
            i2 = n2.shift();
            if (n2.length === 1)
              f.set(e3, n2[0]);
          } else {
            i2 = n2;
            f.delete(e3);
          }
          s.add(i2);
        }
      }
    }
    u[e2 - t] = i2;
  }
  const r = [];
  for (let n2 = t;n2 < l; n2++) {
    if (s === null || !s.has(n2))
      r.push(unwrapValue(e[n2]));
  }
  return {
    prefix: t,
    sources: u,
    removed: r
  };
}
function descend(e, n, t, o, l = false) {
  if (e === null || typeof e !== "object" || n === null || typeof n !== "object")
    return;
  const i = (o?.map ?? storeNextLookup).get(e);
  if (i === undefined)
    return;
  if (!isWrappable(n))
    return;
  if (rawValuesUsed && isRawValue(n))
    return;
  n = unwrapValue(n);
  if (Array.isArray(e) !== Array.isArray(n))
    return;
  if (t) {
    const o2 = t(e);
    const l2 = t(n);
    if (o2 !== undefined && l2 !== undefined && !sameKey(o2, l2))
      return;
  }
  if (!l && t !== null && !i.d)
    return;
  applyAdopt(i, n, t, l);
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/next/projection.js
function wrapDraft(e, t, r) {
  const i = {
    get(i2, o) {
      let n;
      const c = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        n = e[o];
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(c);
      }
      if (o === $TARGET)
        return n;
      return typeof n === "object" && n !== null ? wrapDraft(n, t, r) : n;
    },
    has(t2, r2) {
      let i2;
      const o = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        i2 = r2 in e;
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(o);
      }
      return i2;
    },
    set(i2, o, n) {
      if (t && !t())
        return true;
      const c = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        e[o] = n;
        r?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(c);
      }
      return true;
    },
    deleteProperty(i2, o) {
      if (t && !t())
        return true;
      const n = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        delete e[o];
        r?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(n);
      }
      return true;
    },
    ownKeys() {
      const t2 = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        return Reflect.ownKeys(e);
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(t2);
      }
    },
    getOwnPropertyDescriptor(t2, r2) {
      let i2;
      const o = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        i2 = Reflect.getOwnPropertyDescriptor(e, r2);
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(o);
      }
      if (i2)
        i2.configurable = true;
      return i2;
    },
    defineProperty(i2, o, n) {
      if (t && !t())
        return true;
      const c = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        Reflect.defineProperty(e, o, n);
        r?.();
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(c);
      }
      return true;
    }
  };
  return new Proxy(Array.isArray(e) ? [] : {}, i);
}
function createProjectionNextInternal(e, t, r) {
  const i = {
    map: new WeakMap,
    node: null,
    shallow: !!r?.shallow
  };
  const o = wrapNext(t, null, null, i);
  if (i.shallow) {
    o[$TARGET].s = true;
    markRawIngest(t);
  }
  let n;
  if (r?.seedLoadingValue)
    n = {
      loadingValue: undefined
    };
  const c = computed(() => {
    if (!i.node)
      i.node = getOwner();
    runProjectionComputedNext(o, e, r?.key === undefined ? "id" : r.key);
  }, n);
  c.T &= ~CONFIG_AUTO_DISPOSE;
  i.node = c;
  return {
    store: o,
    node: c
  };
}
function createStoreDerivedNext(e, t, r) {
  const { store: i, node: o } = createProjectionNextInternal(e, t, r);
  return [i, (e2) => {
    suppressComputedRecompute(o);
    storeSetterNext(i, e2);
  }];
}
function runProjectionComputedNext(e, t, r, i, o) {
  const n = getOwner();
  let c = false;
  let s;
  const u = n.Ie ? JSON.parse(JSON.stringify(e[$TARGET][STORE_VALUE])) : null;
  const l = wrapDraft(e, () => !c || n.o?.Ee === s, o);
  storeSetterNext(l, (o2) => {
    s = t(u ?? o2);
    c = true;
    const commit = (t2) => {
      if (u && (t2 === undefined || t2 === u))
        t2 = JSON.parse(JSON.stringify(u));
      if (t2 === o2 || t2 === undefined)
        return;
      const write = () => storeSetterNext(e, (e2) => reconcileNextState(t2, e2, r, true), false);
      i ? i(write) : write();
    };
    const l2 = handleAsync(n, s, commit);
    if (!n.Ie)
      commit(l2);
  }, false);
  return n;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/index.js
function createStore(e, t, r) {
  if (typeof e === "function")
    return createStoreDerivedNext(e, t, r);
  return createStoreNext(e, !!t?.shallow);
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/boundaries.js
function boundaryComputed(e, t) {
  const r = computed(e, {
    lazy: true
  });
  ext(r).h = (e2, t2) => {
    const n = e2 !== undefined ? e2 : r.S;
    const s = t2 !== undefined ? t2 : r.o?._;
    r.S &= ~r.R;
    const i = r.C.notify(r, STATUS_PENDING | STATUS_ERROR, n, s);
    const o = n & ~r.R & (STATUS_PENDING | STATUS_ERROR);
    if (o) {
      r.S &= ~o;
      if (r.o?._ === s && !(r.S & (STATUS_PENDING | STATUS_ERROR))) {
        if (r.o !== null)
          r.o._ = undefined;
      }
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
  return isRevealController(e) ? e.O() : e.v.size === 0 && !e.U;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.I() : isSlotReady(e);
}
function setSlotState(e, t, r, n) {
  setSignal(e.D, r);
  setSignal(e.P, n);
  if (isRevealController(e)) {
    if (!r && e.j === t)
      e.j = undefined;
    return e.B(r, n);
  }
  if (!r && e.W === t && e.L)
    e.W = undefined;
}

class RevealController {
  F;
  q;
  V = [];
  j;
  D = signal(false, {
    ownedWrite: true,
    H: true
  });
  P = signal(false, {
    ownedWrite: true,
    H: true
  });
  J = true;
  K = true;
  X = false;
  constructor(e, t) {
    this.F = e;
    this.q = t;
  }
  Y(e) {
    for (let t = 0;t < this.V.length; t++) {
      const r = this.V[t];
      if ((isRevealController(r) ? r.j : r.W) !== this)
        continue;
      if (e(r) === false)
        return false;
    }
    return true;
  }
  O() {
    return this.Y(isSlotReady);
  }
  I() {
    const e = untrack(this.F);
    if (e === "together")
      return this.Y(isSlotMinimallyReady);
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.Y((r) => {
        e2 = true;
        if (isSlotMinimallyReady(r)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.Y((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  Z(e) {
    if (this.V.includes(e))
      return;
    this.V.push(e);
    const t = untrack(this.F);
    setSignal(e.D, true), setSignal(e.P, t === "sequential" ? !!untrack(this.q) : false);
    untrack(() => this.B());
  }
  $(e) {
    const t = this.V.indexOf(e);
    if (t >= 0)
      this.V.splice(t, 1);
    untrack(() => this.B());
  }
  B(e, t) {
    if (this.X)
      return;
    this.X = true;
    const r = this.J;
    const n = this.K;
    try {
      const r2 = e ?? read(this.D), n2 = untrack(this.F), s = n2 === "sequential" && !!untrack(this.q), i = t ?? s;
      if (r2) {
        this.Y((e2) => setSlotState(e2, this, true, i));
      } else if (n2 === "natural") {
        this.Y((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.P, false);
            setSignal(e2.D, false);
            e2.B(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (n2 === "together") {
        const e2 = this.Y(isSlotMinimallyReady);
        this.Y((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.Y((t2) => {
          if (e2)
            return setSlotState(t2, this, true, s);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.P, false);
            setSignal(t2.D, false);
            t2.B(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.J = this.O();
      this.K = this.I();
      this.X = false;
    }
    if (this.j && (r !== this.J || n !== this.K))
      this.j.B();
  }
}

class CollectionQueue extends Queue {
  ee;
  v = new Set;
  te;
  U = true;
  D = signal(false, {
    ownedWrite: true,
    H: true
  });
  _;
  P = signal(false, {
    ownedWrite: true,
    H: true
  });
  W;
  L = false;
  re;
  ne = ON_INIT;
  constructor(e) {
    super();
    this.ee = e;
  }
  run(e) {
    if (!e || read(this.D) && (!_revealUsed || read(this.P)))
      return;
    return super.run(e);
  }
  notify(e, t, r, n) {
    if (!(t & this.ee))
      return super.notify(e, t, r, n);
    if (this.L && this.re) {
      const e2 = untrack(() => {
        try {
          return this.re();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.ne) {
        this.ne = e2;
        this.L = false;
        this.v.clear();
      }
    }
    if (this.ee & STATUS_PENDING && this.L)
      return super.notify(e, t, r, n);
    if (r & this.ee) {
      this.U = true;
      const t2 = n?.source || e.o?._?.source;
      if (t2) {
        const e2 = this.v.size === 0;
        this.v.add(t2);
        if (e2)
          setSignal(this.D, true);
        if (this.ee & STATUS_ERROR) {
          setSignal(this._, unwrapStatusError(t2.o?._));
        }
      }
    }
    t &= ~this.ee;
    return t ? super.notify(e, t, r, n) : true;
  }
  se() {
    for (const e of this.v) {
      if (e.ie & REACTIVE_DISPOSED || !e.o?.t && !(e.S & this.ee) && !(this.ee & STATUS_ERROR && e.S & STATUS_PENDING))
        this.v.delete(e);
    }
    if (!this.v.size) {
      if (this.ee & STATUS_PENDING && this.U && !this.L && this.te) {
        this.U = !!(this.te.S & this.ee);
      } else {
        this.U = false;
      }
      if (!this.U) {
        setSignal(this.D, false);
        if (this.re) {
          try {
            this.ne = untrack(() => this.re());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this.W?.B();
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
      H: true
    });
  if (n)
    i.re = n;
  const o = i.te = createBoundChildren(s, t, i, e);
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
    i.U = t2 || !!(o.S & e) || o.o?._ instanceof NotReadyError;
  });
  const l = _revealUsed && e === STATUS_PENDING ? getContext(RevealControllerContext) : null;
  if (l) {
    i.W = l;
    l.Z(i);
    cleanup(() => l.$(i));
  }
  return accessor(computed(() => {
    if (!read(i.D)) {
      const e2 = read(o);
      if (!untrack(() => read(i.D)))
        return i.L = true, e2;
    }
    if (_revealUsed && read(i.P))
      return;
    return r(i);
  }, {
    H: true
  }));
}
function createErrorBoundary(e, t) {
  return createCollectionBoundary(STATUS_ERROR, e, (e2) => t(accessor(e2._), () => {
    for (const t2 of e2.v) {
      if (t2.oe !== undefined)
        recompute(t2);
    }
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/utils.js
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
// ../../node_modules/.bun/solid-js@2.0.0-rc.4/node_modules/solid-js/dist/solid.js
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
function useContext(context2) {
  return getContext(context2);
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
var _createSignal;
var _createErrorBoundary;
var _createRenderEffect;
var latchedOnce = new WeakSet;
var LIVE_SOURCE = Symbol.for("solid.LiveSource");
var createMemo2 = (...args) => {
  return (_createMemo || createMemo)(...args);
};
var createSignal2 = (...args) => {
  return (_createSignal || createSignal)(...args);
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
var narrowedError = (name) => `Stale read from <${name}>.`;
function For(props) {
  const options = "fallback" in props ? {
    keyed: props.keyed,
    fallback: () => props.fallback
  } : {
    keyed: props.keyed
  };
  const owner = getOwner();
  let mapped;
  const list = () => {
    if (mapped === undefined)
      mapped = runWithOwner(owner, () => mapArray(() => props.each, props.children, options));
    return mapped();
  };
  if (props.keyed !== false && !("fallback" in props) && props.children.length < 2)
    list.$ll = {
      each: () => props.each,
      row: props.children,
      keyed: props.keyed
    };
  return list;
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

// ../../node_modules/.bun/@solidjs+universal@2.0.0-rc.4+4eb2d79516b92ebc/node_modules/@solidjs/universal/dist/universal.js
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
function createRenderer({
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
    const isLive = (n) => n && getParentNode(n) === parentNode;
    while (aStart < aEnd || bStart < bEnd) {
      if (a[aStart] === b[bStart] && isLive(a[aStart])) {
        aStart++;
        bStart++;
        continue;
      }
      while (a[aEnd - 1] === b[bEnd - 1] && isLive(a[aEnd - 1])) {
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
          const tree = code();
          insert(element, () => tree, undefined, undefined, {
            schedule: true,
            onUpdate(value) {
              mounted = collectMounted(element, value);
            }
          });
        });
        flush();
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
    ref,
    patchDriver(subject, body) {
      effect2(() => body(subject, subject, false), () => body(subject, undefined, true));
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

// ../../packages/core/src/window.ts
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
var layoutHandlers = [];
var layoutSubscribed = false;
function runLayoutHandlers() {
  for (let fn of [...layoutHandlers]) {
    try {
      fn();
    } catch (err) {
      console.error("Error in onLayout handler:", err);
    }
  }
  try {
    flush();
  } catch (err) {
    console.error("Error in reactive flush:", err);
  }
}
function onLayout(fn) {
  if (!layoutSubscribed) {
    layoutSubscribed = true;
    on2("postLayout", runLayoutHandlers);
  }
  layoutHandlers.push(fn);
  let unsubscribe = () => {
    let i = layoutHandlers.indexOf(fn);
    if (i >= 0)
      layoutHandlers.splice(i, 1);
  };
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
      if (anchor)
        tree2.insertNode(parent.id, node.id, anchor.id);
      else
        tree2.insertNode(parent.id, node.id);
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
function guard(fn, describe, nested, empty) {
  let last = empty;
  let failing = false;
  return (prev) => {
    try {
      let value = fn(prev === SKIP ? undefined : prev);
      if (failing) {
        failing = false;
        console.warn(`Recovered: ${describe()} computes again`);
      }
      if (nested && typeof value === "function" && value.length === 0) {
        let inner = guard(value, describe, true, empty);
        value = () => inner();
      }
      last = value;
      return value;
    } catch (e) {
      if (e instanceof NotReadyError)
        throw e;
      if (!failing) {
        failing = true;
        console.error(`Contained error: ${describe()} threw and keeps its last value until it computes again.`, e);
      }
      return last;
    }
  };
}
var effectRaw = rawEffect;
var insertRaw = rawInsert;
var effect3 = (fn, effectFn, options) => effectRaw(guard(fn, () => "an element's prop expression", false, SKIP), effectFn && ((value, prev) => value === SKIP ? undefined : effectFn(value, prev === SKIP ? undefined : prev)), options);
var insert = (parent, accessor2, marker, initial, options) => insertRaw(parent, typeof accessor2 === "function" ? guard(accessor2, () => `a child expression of <${parent.elementType}> ${getNodePath(parent.id).join("/")}`, true, undefined) : accessor2, marker, initial, options);
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
function createPortal(node, mount) {
  let target = mount ?? windowRoot;
  if (!target) {
    throw new Error("createPortal: no mount target (portals cannot mount during the initial render; open them after mount)");
  }
  if (node === null || typeof node !== "object" || Array.isArray(node)) {
    throw new Error("createPortal: node must be a single built element");
  }
  insertNode2(target, node);
  onCleanup(() => {
    if (nodes.has(node.id))
      removeNode(target, node);
  });
  return null;
}
// ../../packages/core/src/color.ts
import * as tree3 from "flux:rendertree";
function parseColor2(color) {
  return tree3.parseColor(color);
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
// ../../packages/core/src/environment.ts
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
// ../../packages/core/src/gamepad.ts
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
// ../../packages/core/src/capabilities.ts
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
function parseSvg(src, opts) {
  if (opts?.color != null)
    return fluxParseSvg(src, {
      color: parseColor2(opts.color)
    });
  return fluxParseSvg(src);
}
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
var clamp = (x) => x < 0 ? 0 : x > 1 ? 1 : x;
var ease = (t) => 1 - (1 - t) * (1 - t);
var byte = (x) => Math.round(clamp(x) * 255).toString(16).padStart(2, "0");
function Logo(props) {
  let size = () => props.size ?? 100;
  let mode = () => props.animation ?? "none";
  let [clock2, setClock] = createSignal2(0);
  let Animate = () => {
    let start = -1;
    let stop = onFrame((tick) => {
      if (start < 0)
        start = tick;
      let t = tick - start;
      if (mode() === "loop")
        setClock(t % CYCLE);
      else if (t < IN_DONE)
        setClock(t);
      else {
        setClock(IN_DONE);
        stop();
      }
    });
    return null;
  };
  let alpha = (seg) => {
    if (mode() === "none")
      return 1;
    let t = clock2();
    if (t < seg.base)
      return 0;
    let fadeIn = clamp((t - seg.base) / FADE);
    if (mode() === "once")
      return ease(fadeIn);
    let end = IN_DONE + seg.base + FADE;
    if (t >= end)
      return 0;
    return ease(Math.min(fadeIn, clamp((end - t) / FADE)));
  };
  let fill = (seg) => {
    let a = byte(alpha(seg));
    return createLinearGradient(0, 0, 1, 1, [{
      offset: 0,
      color: seg.light + a
    }, {
      offset: 1,
      color: seg.dark + a
    }]);
  };
  var _el$ = createElement("view", {
    designSize: [100, 100]
  });
  insert(_el$, createComponent2(Show, {
    get when() {
      return mode() !== "none";
    },
    get children() {
      return createComponent2(Animate, {});
    }
  }), null);
  insert(_el$, createComponent2(For, {
    each: SEGMENTS,
    children: (seg) => (() => {
      var _el$2 = createElement("d-path");
      effect3(() => ({
        e: seg.d,
        t: fill(seg)
      }), ({
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "d", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "color", t, _p$?.t);
      });
      return _el$2;
    })()
  }), null);
  effect3(() => ({
    e: size(),
    t: size()
  }), ({
    e,
    t
  }, _p$) => {
    e !== _p$?.e && setProp(_el$, "width", e, _p$?.e);
    t !== _p$?.t && setProp(_el$, "height", t, _p$?.t);
  });
  return _el$;
}
// ../../packages/core/src/scroll.ts
function createScroll(viewport, content, options = {}) {
  let axis = options.axis ?? "vertical";
  let canX = axis === "horizontal" || axis === "both";
  let canY = axis === "vertical" || axis === "both";
  let [offset, setOffset] = createSignal({
    x: 0,
    y: 0
  });
  let [range, setRange] = createSignal({
    x: 0,
    y: 0
  });
  let [behavior, setBehavior] = createSignal("auto");
  let lastBehavior = "auto";
  let origin = new Error().stack ?? "";
  let warnedCollapsed = false;
  let maxX = 0;
  let maxY = 0;
  let clamp2 = (x, y) => ({
    x: canX ? Math.max(0, Math.min(x, maxX)) : 0,
    y: canY ? Math.max(0, Math.min(y, maxY)) : 0
  });
  let set = (x, y, b = "auto") => {
    let cur = offset();
    let next = clamp2(x, y);
    if (next.x !== cur.x || next.y !== cur.y)
      setOffset(next);
    if (b !== lastBehavior) {
      lastBehavior = b;
      setBehavior(b);
    }
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
    let r = range();
    let rx = canX ? maxX : 0;
    let ry = canY ? maxY : 0;
    if (r.x !== rx || r.y !== ry)
      setRange({
        x: rx,
        y: ry
      });
    let cur = offset();
    let next = clamp2(cur.x, cur.y);
    if (next.x !== cur.x || next.y !== cur.y)
      setOffset(next);
  });
  return {
    offset,
    range,
    behavior,
    scrollTo: (o) => {
      let cur = offset();
      set(o.x ?? cur.x, o.y ?? cur.y, o.behavior);
    },
    scrollBy: (o) => {
      let cur = offset();
      set(cur.x + (o.x ?? 0), cur.y + (o.y ?? 0), o.behavior);
    }
  };
}
// ../../packages/core/src/arena.ts
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
// ../../packages/core/src/pan.ts
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
// ../../packages/core/src/transform.ts
import { on as on5 } from "srt:events";
// ../../packages/components/src/window.tsx
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
// ../../packages/components/src/types.ts
var STYLE_TO_BACKGROUND = {
  backgroundColor: "color",
  borderRadius: "radius"
};
var STYLE_TO_BORDER = {
  borderColor: "color",
  borderWidth: "strokeWidth",
  borderRadius: "radius"
};
function splitTransition(t) {
  if (t == null || typeof t === "string")
    return {
      root: t,
      background: t,
      border: t
    };
  let root = {};
  let background = {};
  let border = {};
  for (let [key, value] of Object.entries(t)) {
    if (key === "all" || key === "stagger") {
      root[key] = value;
      background[key] = value;
      border[key] = value;
    } else if (key in STYLE_TO_BACKGROUND || key in STYLE_TO_BORDER) {
      if (key in STYLE_TO_BACKGROUND)
        background[STYLE_TO_BACKGROUND[key]] = value;
      if (key in STYLE_TO_BORDER)
        border[STYLE_TO_BORDER[key]] = value;
    } else {
      root[key] = value;
    }
  }
  let pick = (o) => Object.keys(o).length ? o : undefined;
  return {
    root: pick(root),
    background: pick(background),
    border: pick(border)
  };
}
function transitionEndFor(node, handler) {
  if (!handler)
    return;
  return (e) => {
    let name = e.property;
    if (node === "background")
      name = e.property === "color" ? "backgroundColor" : "borderRadius";
    if (node === "border")
      name = e.property === "color" ? "borderColor" : e.property === "strokeWidth" ? "borderWidth" : "borderRadius";
    handler({
      property: name
    });
  };
}

// ../../packages/components/src/view.tsx
function View(props) {
  let hasBackground = () => props.style?.backgroundColor != null || props.style?.borderRadius != null;
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0;
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view");
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
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
        e: split().background,
        t: transitionEndFor("background", props.onTransitionEnd),
        a: props.style?.backgroundColor ?? "transparent",
        o: props.style?.borderRadius
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
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
        e: split().border,
        t: transitionEndFor("border", props.onTransitionEnd),
        a: props.style?.borderColor ?? "transparent",
        o: props.style?.borderWidth,
        i: props.style?.borderRadius
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$3, "strokeWidth", o, _p$?.o);
        i !== _p$?.i && setProp(_el$3, "radius", i, _p$?.i);
      });
      return _el$3;
    })() : null;
  })(), null);
  return _el$;
}
// ../../packages/components/src/theme.ts
var SPACING_BASE = 4;
function deriveSpacing(base) {
  return {
    sm: base,
    md: base * 2,
    lg: base * 4,
    xl: base * 5
  };
}
var RADIUS_BASE = 8;
var RADIUS_FULL = 9999;
function deriveRadius(base) {
  return {
    sm: Math.round(base / 2),
    md: base,
    lg: Math.round(base * 1.5),
    full: RADIUS_FULL
  };
}
var BORDER_WIDTH = {
  sm: 1,
  focus: 2
};
var SIZE = {
  navRail: 72,
  navSidebar: 220,
  splitViewList: 320,
  menuMinWidth: 120,
  slider: 200
};
var ROLE_DEFAULTS = {
  caption: {
    step: -1,
    lineHeight: 1.3,
    weight: 400
  },
  label: {
    step: 0,
    lineHeight: 1.5,
    weight: 600
  },
  body: {
    step: 0,
    lineHeight: 1.5,
    weight: 400
  },
  title: {
    step: 1,
    lineHeight: 1.4,
    weight: 700
  },
  heading: {
    step: 2,
    lineHeight: 1.3,
    weight: 700
  }
};
function defineTheme(def, scheme) {
  let color = {};
  for (let key in def.color) {
    let k = key;
    let value = def.color[k];
    if (value == null)
      continue;
    if (Array.isArray(value)) {
      if (!scheme)
        throw new Error(`Theme color "${key}" is a [light, dark] pair; pass a scheme to defineTheme`);
      color[k] = value[scheme === "light" ? 0 : 1];
    } else
      color[k] = value;
  }
  if (def.color.ring == null)
    color.ring = color.text;
  let base = def.text?.base ?? 14;
  let ratio = def.text?.ratio ?? 1.26;
  let role = (name) => {
    let d = ROLE_DEFAULTS[name];
    return {
      size: Math.round(base * ratio ** d.step),
      lineHeight: d.lineHeight,
      weight: d.weight,
      ...def.text?.roles?.[name]
    };
  };
  return {
    text: {
      fontFamily: def.text?.fontFamily ?? "sans",
      monoFamily: def.text?.monoFamily ?? "mono",
      caption: role("caption"),
      label: role("label"),
      body: role("body"),
      title: role("title"),
      heading: role("heading")
    },
    color,
    spacing: typeof def.spacing === "number" ? deriveSpacing(def.spacing) : {
      ...deriveSpacing(SPACING_BASE),
      ...def.spacing
    },
    radius: typeof def.radius === "number" ? deriveRadius(def.radius) : {
      ...deriveRadius(RADIUS_BASE),
      ...def.radius
    },
    borderWidth: {
      ...BORDER_WIDTH,
      ...def.borderWidth
    },
    size: {
      ...SIZE,
      ...def.size
    },
    icons: def.icons ?? {},
    components: def.components ?? {}
  };
}
var DEFAULT = {
  color: {
    background: ["#ffffff", "#0b0f17"],
    surface: ["#f6f8fa", "#161b22"],
    surfaceAlt: ["#eaeef2", "#21262d"],
    text: ["#1f2328", "#b1bac4"],
    textMuted: ["#707376", "#828993"],
    border: ["rgba(0,0,0,0.15)", "rgba(255,255,255,0.14)"],
    primary: "#547ebf",
    onPrimary: "#ffffff",
    secondary: "#2b5696",
    onSecondary: "#ffffff",
    danger: ["#cf222e", "#f85149"],
    scrim: ["rgba(0,0,0,0.4)", "rgba(0,0,0,0.6)"],
    overlayHover: ["rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)"],
    overlayPressed: ["rgba(0,0,0,0.14)", "rgba(255,255,255,0.14)"]
  },
  text: {
    roles: {
      caption: {
        size: 12
      }
    }
  }
};
var darkTheme = defineTheme(DEFAULT, "dark");
var lightTheme = defineTheme(DEFAULT, "light");
var [themeStore, setThemeStore] = createStore({
  ...darkTheme
});
var theme = themeStore;
function setTheme(partial) {
  setThemeStore((s) => {
    for (let key in partial) {
      let k = key;
      Object.assign(s[k], partial[k]);
    }
  });
}

// ../../packages/components/src/policy.ts
function defaultPolicyResolver(caps) {
  let interaction = caps.touch && caps.precisePointer ? "hybrid" : caps.touch ? "touch" : caps.precisePointer ? "desktop" : "hybrid";
  let layout = caps.windowSizeClass === "expanded" ? "twoPane" : "singlePane";
  return {
    interaction,
    density: interaction === "desktop" ? "compact" : "comfortable",
    motion: "normal",
    focusRing: caps.keyboardNav || gamepads().some((p) => p != null),
    textScale: env.textScale,
    textWeightDelta: env.displayScale < 1.5 ? 100 : 0,
    navigation: layout === "twoPane" ? "sidebar" : "bottomTabs",
    layout
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

// ../../packages/components/src/typography.ts
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

// ../../packages/components/src/text.tsx
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
  let split = () => {
    let t = splitTransition(props.transition);
    if (t.root == null || typeof t.root === "string")
      return {
        root: t.root,
        text: t.root
      };
    let {
      color: color2,
      ...rest
    } = t.root;
    let text = {};
    if (color2 !== undefined)
      text.color = color2;
    if (rest.all !== undefined)
      text.all = rest.all;
    return {
      root: Object.keys(rest).length ? rest : undefined,
      text: Object.keys(text).length ? text : undefined
    };
  };
  var _el$ = createElement("view"), _el$2 = createElement("text");
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    }
  }, box, {
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
    e: split().text,
    t: transitionEndFor("root", props.onTransitionEnd),
    a: color(),
    o: props.layout?.fontFamily ?? theme.text.fontFamily,
    i: size(),
    n: props.layout?.lineHeight ?? role().lineHeight,
    s: props.layout?.fontStyle,
    h: typeWeight(props.layout?.fontWeight ?? role().weight, size()),
    r: props.layout?.textAlign,
    d: props.layout?.maxLines
  }), ({
    e,
    t,
    a,
    o,
    i,
    n,
    s,
    h,
    r,
    d
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "fontFamily", o, _p$?.o);
    i !== _p$?.i && setProp(_el$2, "fontSize", i, _p$?.i);
    n !== _p$?.n && setProp(_el$2, "lineHeight", n, _p$?.n);
    s !== _p$?.s && setProp(_el$2, "fontStyle", s, _p$?.s);
    h !== _p$?.h && setProp(_el$2, "fontWeight", h, _p$?.h);
    r !== _p$?.r && setProp(_el$2, "textAlign", r, _p$?.r);
    d !== _p$?.d && setProp(_el$2, "maxLines", d, _p$?.d);
  });
  return _el$;
}
// ../../packages/components/src/safe-area.tsx
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
// ../../packages/core/src/text-input.ts
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
      wrap,
      caretWidth = 0
    } = input();
    let width = wrap ? Math.max(0, viewportSize().width - caretWidth) : Infinity;
    let units = wrap ? splitWide(prepared(), width) : prepared();
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
      wrap
    } = input();
    let ls = lines();
    let contentWidth = ls.reduce((w, l) => Math.max(w, l.width), 0);
    let last = ls[ls.length - 1];
    let contentHeight = last.y + last.height;
    let c = caret();
    setScrollX(wrap ? 0 : follow(scrollX(), c.x, caretWidth, vw, contentWidth + caretWidth));
    setScrollY(follow(scrollY(), c.y, c.height, vh, contentHeight));
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

// ../../packages/components/src/focus-nav.ts
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

// ../../packages/components/src/density.tsx
var DensityContext = createContext2(() => {
  return;
});
var DENSITY_SCALE = {
  comfortable: 1,
  compact: 0.85,
  dense: 0.7
};
function densityScale() {
  return DENSITY_SCALE[useContext(DensityContext)() ?? policy.density];
}

// ../../packages/components/src/spacing.ts
function space(token) {
  return Math.round(theme.spacing[token] * densityScale());
}

// ../../packages/components/src/editor-field.tsx
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
  let ring = () => focused() && policy.focusRing;
  let borderColor = () => props.style?.borderColor ?? (ring() ? theme.color.ring : theme.color.border);
  let borderWidth = () => props.style?.borderWidth ?? (ring() ? theme.borderWidth.focus : theme.borderWidth.sm);
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.md;
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
  let split = () => splitTransition(props.transition);
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
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    },
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
      return space("md");
    },
    get paddingBottom() {
      return space("md");
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
    e: split().background,
    t: transitionEndFor("background", props.onTransitionEnd),
    a: surfaceColor(),
    o: borderRadius(),
    i: split().border,
    n: transitionEndFor("border", props.onTransitionEnd),
    s: borderColor(),
    h: borderWidth(),
    r: borderRadius(),
    d: viewportHeight(),
    l: props.multiline ? "stretch" : undefined,
    u: editor.scrollX(),
    c: editor.scrollY()
  }), ({
    e,
    t,
    a,
    o,
    i,
    n,
    s,
    h,
    r,
    d,
    l,
    u,
    c
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
    i !== _p$?.i && setProp(_el$3, "transition", i, _p$?.i);
    n !== _p$?.n && setProp(_el$3, "onTransitionEnd", n, _p$?.n);
    s !== _p$?.s && setProp(_el$3, "color", s, _p$?.s);
    h !== _p$?.h && setProp(_el$3, "strokeWidth", h, _p$?.h);
    r !== _p$?.r && setProp(_el$3, "radius", r, _p$?.r);
    d !== _p$?.d && setProp(_el$4, "height", d, _p$?.d);
    l !== _p$?.l && setProp(_el$4, "alignSelf", l, _p$?.l);
    u !== _p$?.u && setProp(_el$4, "scrollX", u, _p$?.u);
    c !== _p$?.c && setProp(_el$4, "scrollY", c, _p$?.c);
  });
  return _el$;
}

// ../../packages/components/src/text-input.tsx
function TextInput(props) {
  let value = () => "";
  return createComponent2(EditorField, {
    get transition() {
      return props.transition;
    },
    get onTransitionEnd() {
      return props.onTransitionEnd;
    },
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
      return {
        ...theme.components.textInput,
        ...props.style
      };
    }
  });
}
// ../../packages/components/src/scroll-view.tsx
var SCROLL_SPRING = {
  duration: 250
};
function ScrollView(props) {
  let viewport;
  let content;
  let [dragging, setDragging] = createSignal(false);
  let scroll = createScroll(() => viewport, () => content, {
    axis: props.horizontal ? "horizontal" : "vertical"
  });
  onSettled(() => {
    untrack(() => props.scrollRef)?.(scroll);
  });
  let pan = createPan({
    axis: props.horizontal ? "horizontal" : "vertical",
    onPanStart: () => setDragging(true),
    onPanMove: (dx, dy) => scroll.scrollBy({
      x: -dx,
      y: -dy
    }),
    onPanEnd: () => setDragging(false)
  });
  let onWheel = (e) => {
    if (props.horizontal)
      scroll.scrollBy({
        x: e.deltaX || e.deltaY
      });
    else
      scroll.scrollBy({
        x: e.deltaX,
        y: e.deltaY
      });
  };
  let split = () => {
    let t = splitTransition(props.transition);
    if (t.root == null || typeof t.root === "string")
      return {
        ...t,
        viewport: t.root
      };
    let {
      scrollX,
      scrollY,
      ...rest
    } = t.root;
    let viewport2 = {};
    if (scrollX !== undefined)
      viewport2.scrollX = scrollX;
    if (scrollY !== undefined)
      viewport2.scrollY = scrollY;
    if (rest.all !== undefined)
      viewport2.all = rest.all;
    return {
      ...t,
      root: Object.keys(rest).length ? rest : undefined,
      viewport: Object.keys(viewport2).length ? viewport2 : undefined
    };
  };
  let viewportTransition = () => {
    let user = split().viewport;
    let entries = typeof user === "string" ? {
      all: user
    } : {
      ...user ?? {}
    };
    if (dragging() || scroll.behavior() === "instant") {
      let {
        scrollX,
        scrollY,
        all,
        ...rest
      } = entries;
      if (all !== undefined)
        rest.clipRadius = all;
      return Object.keys(rest).length ? rest : null;
    }
    return {
      scrollX: SCROLL_SPRING,
      scrollY: SCROLL_SPRING,
      ...entries
    };
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
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
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
        e: split().background,
        t: transitionEndFor("background", props.onTransitionEnd),
        a: props.style?.backgroundColor ?? "transparent",
        o: props.style?.borderRadius
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$4, "radius", o, _p$?.o);
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
    get transition() {
      return viewportTransition();
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
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
        e: split().border,
        t: transitionEndFor("border", props.onTransitionEnd),
        a: props.style?.borderColor ?? "transparent",
        o: props.style?.borderWidth,
        i: props.style?.borderRadius
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$5, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$5, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$5, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$5, "strokeWidth", o, _p$?.o);
        i !== _p$?.i && setProp(_el$5, "radius", i, _p$?.i);
      });
      return _el$5;
    })() : null;
  })(), null);
  effect3(() => direction(), (_v$, _$p) => {
    setProp(_el$3, "flexDirection", _v$, _$p);
  });
  return _el$;
}
// ../../packages/components/src/press.ts
function createPress(options) {
  let [pressed, setPressed] = createSignal(false);
  let [hovered, setHovered] = createSignal(false);
  let node = null;
  let unregisterNav = null;
  let [pending, setPending] = createSignal(false);
  let inflight = false;
  let activate = () => {
    if (options.disabled || inflight)
      return;
    let result = options.onPress?.();
    if (result && typeof result.then === "function") {
      inflight = true;
      setPending(true);
      result.finally(() => {
        inflight = false;
        setPending(false);
      });
    }
  };
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
    },
    get pending() {
      return pending();
    }
  };
  let state = () => live;
  let ref2 = (n) => {
    node = n;
    unregisterNav?.();
    unregisterNav = registerNavAction(n.id, activate);
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
          activate();
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
        activate();
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
    pending,
    state,
    ref: ref2,
    handlers: handlers2,
    cancel
  };
}

// ../../packages/components/src/pressable.tsx
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
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view");
  ref(() => (n) => {
    press.ref(n);
    props.ref?.(n);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    }
  }, () => props.layout, {
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
        e: split().background,
        t: transitionEndFor("background", props.onTransitionEnd),
        a: style()?.backgroundColor ?? "transparent",
        o: style()?.borderRadius
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
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
        e: split().border,
        t: transitionEndFor("border", props.onTransitionEnd),
        a: style()?.borderColor ?? "transparent",
        o: style()?.borderWidth,
        i: style()?.borderRadius
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$3, "strokeWidth", o, _p$?.o);
        i !== _p$?.i && setProp(_el$3, "radius", i, _p$?.i);
      });
      return _el$3;
    })() : null;
  })(), null);
  return _el$;
}
// ../../packages/components/src/spinner.tsx
var SIZE2 = 24;
var THICKNESS = 3;
function Spinner(props) {
  let size = () => props.size ?? SIZE2;
  let thickness = () => props.thickness ?? THICKNESS;
  let styled = () => ({
    ...theme.components.spinner,
    ...props.style
  });
  let color = () => styled().color ?? theme.color.primary;
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
    get transition() {
      return splitTransition(props.transition).root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    },
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
      return styled().x;
    },
    get y() {
      return styled().y;
    },
    get opacity() {
      return styled().opacity;
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

// ../../packages/components/src/button.tsx
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
          label: c.onSecondary
        };
      case "ghost":
        return {
          fill: "transparent",
          label: c.text
        };
      case "danger":
        return {
          fill: c.danger,
          label: c.onPrimary
        };
      default:
        return {
          fill: c.primary,
          label: c.onPrimary
        };
    }
  };
  let styled = () => ({
    ...theme.components.button,
    ...props.style
  });
  let idleFill = () => props.disabled ? props.variant === "ghost" ? "transparent" : theme.color.surface : colors().fill;
  let bg = () => styled().backgroundColor ?? idleFill();
  let overlay = (s) => s.hovered && !props.disabled && policy.interaction !== "touch" ? theme.color.overlayHover : "transparent";
  let radius = () => styled().borderRadius ?? theme.radius.md;
  let label = () => props.disabled ? theme.color.textMuted : colors().label;
  let resolved2 = children(() => props.children);
  let isText = () => typeof resolved2() === "string" || typeof resolved2() === "number";
  let labelOnDark = () => lightOnDark(label(), bg());
  let press = createPress(props);
  let style = () => ({
    ...styled(),
    ...press.focused() && policy.focusRing ? {
      borderWidth: theme.borderWidth.focus,
      borderColor: theme.color.ring
    } : {},
    backgroundColor: bg(),
    borderRadius: radius(),
    scale: (styled().scale ?? 1) * (press.pressed() && policy.motion !== "none" ? 0.97 : 1)
  });
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view"), _el$2 = createElement("d-rect"), _el$3 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  insertNode2(_el$, _el$3);
  ref(() => (n) => {
    press.ref(n);
    props.ref?.(n);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "alignItems", "center");
  setProp(_el$, "justifyContent", "center");
  setProp(_el$, "position", "relative");
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    },
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
      var _el$4 = createElement("text");
      spread(_el$4, mergeProps({
        get color() {
          return memo2(() => !!press.pending())() ? "transparent" : label();
        }
      }, () => typeStyle("body", labelOnDark())), true);
      insert(_el$4, resolved2);
      return _el$4;
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return press.pending();
    },
    get children() {
      var _el$5 = createElement("view", {
        position: "absolute",
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        alignItems: "center",
        justifyContent: "center"
      });
      insert(_el$5, createComponent2(Spinner, {
        size: 16,
        thickness: 2,
        get style() {
          return {
            color: label()
          };
        }
      }));
      return _el$5;
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return (style().borderWidth ?? 0) > 0;
    },
    get children() {
      var _el$6 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: split().border,
        t: transitionEndFor("border", props.onTransitionEnd),
        a: style().borderColor ?? "transparent",
        o: style().borderWidth,
        i: style().borderRadius
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$6, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$6, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$6, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$6, "strokeWidth", o, _p$?.o);
        i !== _p$?.i && setProp(_el$6, "radius", i, _p$?.i);
      });
      return _el$6;
    }
  }), null);
  effect3(() => ({
    e: split().background,
    t: transitionEndFor("background", props.onTransitionEnd),
    a: style().backgroundColor ?? "transparent",
    o: style().borderRadius,
    i: overlay(press.state()),
    n: style().borderRadius
  }), ({
    e,
    t,
    a,
    o,
    i,
    n
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
    i !== _p$?.i && setProp(_el$3, "color", i, _p$?.i);
    n !== _p$?.n && setProp(_el$3, "radius", n, _p$?.n);
  });
  return _el$;
}
// ../../packages/components/src/icon.tsx
var SIZE3 = 24;
function Icon(props) {
  let size = () => props.size ?? SIZE3;
  let doc = createMemo(() => parseSvg(props.src, {
    color: props.color ?? theme.color.text
  }));
  var _el$ = createElement("view");
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "pointerEvents", "all");
  spread(_el$, mergeProps({
    get transition() {
      return splitTransition(props.transition).root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    },
    get width() {
      return size();
    },
    get height() {
      return size();
    },
    get designSize() {
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
// ../../packages/components/src/radio.tsx
var RadioContext = createContext2();
// ../../packages/components/src/card.tsx
function Card(props) {
  let styled = () => ({
    ...theme.components.card,
    ...props.style
  });
  let bg = () => styled().backgroundColor ?? theme.color.surface;
  let radius = () => styled().borderRadius ?? theme.radius.lg;
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null;
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "column");
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    },
    get gap() {
      return space("lg");
    },
    get padding() {
      return space("xl");
    }
  }, () => props.layout, {
    get x() {
      return styled().x;
    },
    get y() {
      return styled().y;
    },
    get scale() {
      return styled().scale;
    },
    get rotate() {
      return styled().rotate;
    },
    get opacity() {
      return styled().opacity;
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
        e: split().border,
        t: transitionEndFor("border", props.onTransitionEnd),
        a: styled().borderColor ?? theme.color.border,
        o: styled().borderWidth ?? theme.borderWidth.sm,
        i: radius()
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$4, "strokeWidth", o, _p$?.o);
        i !== _p$?.i && setProp(_el$4, "radius", i, _p$?.i);
      });
      return _el$4;
    }
  }), null);
  effect3(() => ({
    e: split().background,
    t: transitionEndFor("background", props.onTransitionEnd),
    a: bg(),
    o: radius()
  }), ({
    e,
    t,
    a,
    o
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
  });
  return _el$;
}
// ../../packages/components/src/modal.tsx
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
// ../../packages/components/src/segmented-control.tsx
function SegmentedControl(props) {
  let [internal, setInternal] = createSignal(props.defaultValue);
  let value = () => props.value !== undefined ? props.value : internal();
  let select = (v) => {
    if (props.value === undefined)
      setInternal(() => v);
    props.onChange?.(v);
  };
  let styled = () => ({
    ...theme.components.segmentedControl,
    ...props.style
  });
  let radius = () => {
    let r = styled().borderRadius;
    return typeof r === "number" ? r : theme.radius.md;
  };
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
  let idleFill = () => styled().backgroundColor ?? theme.color.surfaceAlt;
  let activeFill = () => props.disabled ? theme.color.surface : theme.color.primary;
  let label = (active) => props.disabled ? theme.color.textMuted : active ? theme.color.onPrimary : theme.color.text;
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "gap", 0);
  spread(_el$, mergeProps({
    get transition() {
      return split().root;
    },
    get onTransitionEnd() {
      return transitionEndFor("root", props.onTransitionEnd);
    }
  }, () => props.layout, {
    get x() {
      return styled().x;
    },
    get y() {
      return styled().y;
    },
    get scale() {
      return styled().scale;
    },
    get rotate() {
      return styled().rotate;
    },
    get opacity() {
      return styled().opacity;
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
      let fill = () => active() ? activeFill() : idleFill();
      let overlay = () => press.hovered() && !props.disabled && policy.interaction !== "touch" ? theme.color.overlayHover : "transparent";
      var _el$3 = createElement("view"), _el$4 = createElement("d-rect"), _el$5 = createElement("d-rect"), _el$7 = createElement("text");
      insertNode2(_el$3, _el$4);
      insertNode2(_el$3, _el$5);
      insertNode2(_el$3, _el$7);
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
        get focusable() {
          return !props.disabled;
        },
        get pointerEvents() {
          return props.disabled ? "none" : undefined;
        }
      }), true);
      insert(_el$3, createComponent2(Show, {
        get when() {
          return memo2(() => !!press.focused())() ? policy.focusRing : press.focused();
        },
        get children() {
          var _el$6 = createElement("d-rect", {
            drawStyle: "stroke"
          });
          effect3(() => ({
            e: theme.color.ring,
            t: theme.borderWidth.focus,
            a: corners(i())
          }), ({
            e,
            t,
            a
          }, _p$) => {
            e !== _p$?.e && setProp(_el$6, "color", e, _p$?.e);
            t !== _p$?.t && setProp(_el$6, "strokeWidth", t, _p$?.t);
            a !== _p$?.a && setProp(_el$6, "radius", a, _p$?.a);
          });
          return _el$6;
        }
      }), _el$7);
      spread(_el$7, mergeProps({
        get color() {
          return label(active());
        }
      }, () => typeStyle("body", active() ? lightOnDark(label(true), activeFill()) : undefined)), true);
      insert(_el$7, () => opt.label);
      effect3(() => ({
        e: fill(),
        t: corners(i()),
        a: overlay(),
        o: corners(i())
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "radius", t, _p$?.t);
        a !== _p$?.a && setProp(_el$5, "color", a, _p$?.a);
        o !== _p$?.o && setProp(_el$5, "radius", o, _p$?.o);
      });
      return _el$3;
    }
  }), null);
  effect3(() => ({
    e: split().background,
    t: transitionEndFor("background", props.onTransitionEnd),
    a: theme.color.border,
    o: radius()
  }), ({
    e,
    t,
    a,
    o
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "radius", o, _p$?.o);
  });
  return _el$;
}
// ../../packages/components/src/split-view.tsx
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
      effect3(() => props.listWidth ?? theme.size.splitViewList, (_v$, _$p) => {
        setProp(_el$2, "width", _v$, _$p);
      });
      return _el$;
    }
  });
}
// ../../node_modules/.bun/qrcode-generator@2.0.4/node_modules/qrcode-generator/dist/qrcode.mjs
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
// src/parts/home-screen.tsx
import { stop } from "srt:dev";
import { available as appsAvailable, list, launch, remove, info, clearCache } from "srt:apps";

// src/parts/app-icon.tsx
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
        a !== _p$?.a && setProp(_el$2, "designSize", a, _p$?.a);
      });
      return _el$2;
    })()
  });
}

// src/parts/detail-card.tsx
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

// src/parts/types.ts
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

// src/parts/back-button.tsx
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
      backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
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

// src/parts/scan-button.tsx
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
      backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
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

// src/parts/settings-panel.tsx
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
                      backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
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

// src/parts/dev-connection.ts
import { on as on6 } from "srt:events";
import { available as devAvailable, connect as devConnect, launchAddress } from "srt:dev";
var available = devAvailable;
var [state, setState] = createSignal("idle");
var [address, setAddress] = createSignal(null);
var [tunneled, setTunneled] = createSignal(false);
var [recents, setRecents] = createSignal([]);
if (available) {
  on6("dev", (e) => {
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

// src/parts/connect-panel.tsx
var DEFAULT_PORT = "34884";
function recentLabel(entry) {
  if (!entry.includes("|"))
    return entry;
  return "ticket " + entry.split("|")[0].slice(0, 8);
}
function ConnectPanel(props) {
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
              }), createComponent2(ScanButton, {
                get onPress() {
                  return props.onScan;
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

// src/parts/home-screen.tsx
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
          backgroundColor: props.active ? theme.color.surfaceAlt : s.hovered ? theme.color.surfaceAlt : theme.color.surface
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
              return memo2(() => !!(ps.pressed || ps.hovered))() ? theme.color.text : theme.color.primary;
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
                          return [createComponent2(Logo, {
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
                              backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
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
                              return available && !isConnected();
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
                  return createComponent2(Logo, {
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

// ../../packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on7 } from "srt:events";
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

// src/parts/scan-screen.tsx
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

// src/index.tsx
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
