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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.4/node_modules/@solidjs/signals/dist/prod/store/store.js
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
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createMemo;
var _createErrorBoundary;
var _createRenderEffect;
var latchedOnce = new WeakSet;
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
  let list = [...counts].map(([type, n]) => `<${type}> x${n}`).join(", ");
  console.warn(`Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` + `The usual cause is reading an element-valued prop more than once (every read ` + `builds a new subtree); read it once where it mounts, or resolve it with ` + `children(). If these nodes are intentionally kept for later mounting, ignore ` + `this. The next warning comes when a new element type joins the list.`);
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
    if (typeof node !== "object" || node.id === undefined) {
      let what = typeof node === "function" ? "a signal accessor" : `a ${typeof node}`;
      throw new Error(`insertNode received ${what} instead of an element under <${parent?.elementType ?? "?"}>; ` + `resolve the children with children() or return one root element from the component.`);
    }
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
