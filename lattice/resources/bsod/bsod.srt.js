// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/error.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/constants.js
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
var CONFIG_AUTHORITATIVE_READ = 1 << 13;
var CONFIG_AUTHORITATIVE_OBSERVED = 1 << 14;
var CONFIG_DIRECT_COMMIT = 1 << 15;
var CONFIG_FRESH_READ = 1 << 16;
var CONFIG_HELD_TRUTH = 1 << 17;
var CONFIG_SLOT_NODE = 1 << 18;
var CONFIG_OVERRIDE_SUPERSEDED = 1 << 19;
var CONFIG_INPUTS_PUBLISHED = 1 << 21;
var STATUS_PENDING = 1 << 0;
var STATUS_ERROR = 1 << 1;
var STATUS_UNINITIALIZED = 1 << 2;
var EFFECT_RENDER = 1;
var EFFECT_USER = 2;
var EFFECT_TRACKED = 3;
var LANE_RUN = 4;
var NOT_PENDING = {};
var NO_SNAPSHOT = {};
var OVERRIDE_UNDEFINED = {};
function unwrapOverride(E) {
  return E === OVERRIDE_UNDEFINED ? undefined : E;
}
var SUPPORTS_PROXY = typeof Proxy === "function";
var defaultContext = {};
var $REFRESH = Symbol("refresh");

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/lanes.js
var signalLanes = new WeakMap;
var activeLanes = new Set;
function findLane(n) {
  while (n.rn)
    n = n.rn;
  return n;
}
function mergeLanes(n, e) {
  n = findLane(n);
  e = findLane(e);
  if (n === e)
    return n;
  e.rn = n;
  for (const i of e.he)
    n.he.add(i);
  e.he.clear();
  n.tn[0].push(...e.tn[0]);
  n.tn[1].push(...e.tn[1]);
  e.tn[0].length = 0;
  e.tn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.o?.Oe;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  if (n.o !== null)
    n.o.Oe = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.o?.Ot) {
    const e = ext(n).Ot = currentTransition(n.o?.Ot);
    if (e.ft !== true)
      return e;
    if (n.o !== null)
      n.o.Ot = null;
  }
  return resolveLane(n)?.ge ?? n.ge;
}
function hasActiveOverride(n) {
  const e = n.o;
  return e !== null && e.be !== undefined && e.be !== NOT_PENDING;
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const t = n.o?.Oe;
  if (t) {
    if (t.rn) {
      ext(n).Oe = e;
      n.T |= CONFIG_HAS_LANE;
      return;
    }
    const r = findLane(t);
    if (activeLanes.has(r)) {
      if (r !== i && !hasActiveOverride(n)) {
        if (i.an && findLane(i.an) === r) {
          ext(n).Oe = e;
          n.T |= CONFIG_HAS_LANE;
        } else if (r.an && findLane(r.an) === i)
          ;
        else
          mergeLanes(i, r);
      }
      return;
    }
  }
  ext(n).Oe = e;
  n.T |= CONFIG_HAS_LANE;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Ke: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  Ke: 0,
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
var actionStepDepth = 0;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(e) {
  const t = e.m;
  return transitions.size === 0 && activeLanes.size === 0 && e.Xt.length === 0 && t.it.length === 0 && t.A.length === 0 && t.En.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.u !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.Ge !== NOT_PENDING)
      continue;
    if (e.o?.be !== undefined && e.o?.be !== NOT_PENDING)
      continue;
    if (e.o?.t)
      continue;
    transientStoreNodes.delete(e);
    if (e.T & CONFIG_SLOT_NODE)
      slotUnobservedHook(e);
    else
      e.o?.Ct?.();
  }
}
function createBatch() {
  return {
    _e: clock,
    $t: [],
    Re: new Map,
    it: [],
    A: [],
    En: new Set,
    ue: [],
    ei: {
      ti: [[], []],
      Xt: []
    },
    ft: false,
    lt: new Set,
    Et: null
  };
}
function mergeTransitionState(e, t) {
  t.ft = e;
  e.ue.push(...t.ue);
  for (const i of activeLanes)
    if (i.ge === t)
      i.ge = e;
  if (t.it.length) {
    e.it.push(...t.it);
    t.it.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const i of t.En)
    e.En.add(i);
  for (const [i, n] of t.Re) {
    let t2 = e.Re.get(i);
    if (!t2)
      e.Re.set(i, t2 = new Set);
    for (const e2 of n)
      t2.add(e2);
  }
  for (const i of t.lt)
    e.lt.add(i);
  if (t.Et)
    (e.Et ??= []).push(...t.Et);
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.sn && !projectionWriteActive)
    queueMicrotask(flush);
}
function haltReactivity(e) {
  if (halted)
    return;
  halted = true;
  let t = "[REACTIVITY_HALTED]";
  const i = e !== undefined && globalThis.reportError;
  i || e === undefined ? console.error(t) : console.error(t, e);
  i && i(e);
}
function notifyHalted() {
  if (haltNotified)
    return;
  haltNotified = true;
  console.error("[REACTIVITY_HALTED]");
}
var queueRunToken = 0;

class Queue {
  qe = null;
  ti = [[], []];
  Xt = [];
  ii = 0;
  created = clock;
  addChild(e) {
    this.Xt.push(e);
    e.qe = this;
  }
  removeChild(e) {
    const t = this.Xt.indexOf(e);
    if (t >= 0) {
      this.Xt.splice(t, 1);
      e.qe = null;
    }
  }
  notify(e, t, i, n) {
    if (this.qe)
      return this.qe.notify(e, t, i, n);
    return false;
  }
  run(e) {
    if (this.ti[e - 1].length) {
      const t2 = this.ti[e - 1];
      this.ti[e - 1] = [];
      runQueue(t2, e);
    }
    const t = this.Xt;
    const i = ++queueRunToken;
    for (let n = 0;n < t.length; ) {
      const r = t[n];
      if (r.ii !== i) {
        r.ii = i;
        r.run?.(e);
        if (t[n] !== r) {
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
        i.tn[e - 1].push(t);
      } else {
        this.ti[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.ti[0].push(...this.ti[0]);
    e.ti[1].push(...this.ti[1]);
    this.ti = [[], []];
    for (let t = 0;t < this.Xt.length; t++) {
      let i = this.Xt[t];
      let n = e.Xt[t];
      if (!n) {
        n = {
          ti: [[], []],
          Xt: []
        };
        e.Xt[t] = n;
      }
      i.stashQueues(n);
    }
  }
  restoreQueues(e) {
    this.ti[0].push(...e.ti[0]);
    this.ti[1].push(...e.ti[1]);
    for (let t = 0;t < e.Xt.length; t++) {
      const i = e.Xt[t];
      let n = this.Xt[t];
      if (n)
        n.restoreQueues(i);
    }
  }
}

class GlobalQueue extends Queue {
  sn = false;
  m = createBatch();
  static Fe;
  static We;
  static ct;
  static ni = null;
  static p = null;
  static G = null;
  static M = null;
  static N = null;
  static kt = null;
  static Lt = null;
  static pe = null;
  static Ne = null;
  static ke = null;
  static un = null;
  static wt = null;
  static Wt = null;
  static jt = null;
  static st = null;
  static k = null;
  static ri = null;
  static si = null;
  static Bt = null;
  static fn = null;
  static cn = null;
  static dn = null;
  static In = null;
  static ln = null;
  static Zt = null;
  static qt = null;
  static Mt = null;
  static Kt = null;
  static nt = null;
  static ot = null;
  static Qt = null;
  static ut = null;
  static ye = null;
  static Yt = null;
  static zt = null;
  static Nn = null;
  flush() {
    if (this.sn)
      return;
    if (activeTransition === null && dirtyQueue.EE < dirtyQueue.Ke && this.ti[0].length === 0 && this.ti[1].length === 0 && this.Xt.length === 0 && canUseSimpleSyncFlush(this)) {
      this.sn = true;
      try {
        sweepDormant();
        commitPendingNodes();
      } finally {
        this.sn = false;
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.Ke || this.ti[0].length !== 0 || this.ti[1].length !== 0 || this.m.$t.length !== 0;
      return;
    }
    this.sn = true;
    try {
      if (false)
        ;
      sweepDormant();
      runHeap(dirtyQueue, GlobalQueue.Fe);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, this.m === e2 ? cancelZombieRecompute : GlobalQueue.Fe);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue.In(EFFECT_RENDER);
            GlobalQueue.In(EFFECT_USER);
          }
          this.stashQueues(e2.ei);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.Ke || this.m.$t.length > 0;
          reassignPendingTransition(e2.$t);
          activeTransition = null;
          finalizePureQueue(null, true);
          return;
        }
        const t = activeTransition;
        const i = this.m;
        i !== t && i.$t.push(...t.$t);
        this.restoreQueues(t.ei);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(i.$t);
        finalizePureQueue(t);
        if (i === t) {
          const e2 = createBatch();
          e2.$t = i.$t;
          e2.it = i.it;
          e2.A = i.A;
          e2.En = i.En;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.Ke) {
            runHeap(dirtyQueue, GlobalQueue.Fe);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.Fe);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.Ke || activeTransition !== null;
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
      this.sn = false;
    }
  }
  notify(e, t, i, n) {
    if (t & STATUS_PENDING) {
      if (i & STATUS_PENDING) {
        const t2 = n ?? e.o?._;
        if (t2?.l)
          return true;
        if (t2) {
          if (!activeTransition && !e.ge && currentBatch.$t.length)
            this.initTransition();
          if (activeTransition) {
            const i2 = t2.source;
            let n2 = activeTransition.Re.get(i2);
            if (!n2)
              activeTransition.Re.set(i2, n2 = new Set);
            const r = n2.size;
            n2.add(e);
            if (n2.size !== r) {
              schedule();
              GlobalQueue.si?.(activeTransition);
            }
          }
        }
      }
      return true;
    }
    return false;
  }
  initTransition(e) {
    if (e) {
      e = currentTransition(e);
      if (e.ft === true || e === activeTransition)
        return;
    }
    if (!e && activeTransition && activeTransition._e === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? createBatch();
    } else if (e) {
      const t2 = activeTransition;
      mergeTransitionState(e, t2);
      this.restoreQueues(t2.ei);
      transitions.delete(t2);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition._e = clock;
    const t = this.m;
    if (t !== activeTransition) {
      for (let e2 = 0;e2 < t.$t.length; e2++) {
        const i = t.$t[e2];
        i.ge = activeTransition;
        activeTransition.$t.push(i);
      }
      for (let e2 = 0;e2 < t.it.length; e2++) {
        const i = t.it[e2];
        i.ge = activeTransition;
        activeTransition.it.push(i);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e2 of t.En)
        activeTransition.En.add(e2);
      if (t.lt.size) {
        for (const e2 of t.lt)
          activeTransition.lt.add(e2);
        t.lt.clear();
      }
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.ge)
        e2.ge = activeTransition;
    }
    schedule();
  }
}
function queuePendingNode(e) {
  currentBatch.$t.push(e);
}
var reaskArmed = false;
var notifyEpoch = 0;
function bumpNotifyEpoch() {
  notifyEpoch++;
}
var origin = 0;
function setOrigin(e) {
  const t = origin;
  origin = e;
  return t;
}
function insertSubs(e, t = false) {
  e.At = notifyEpoch;
  const i = e.T;
  const n = (i & CONFIG_HAS_LANE ? e.o?.Oe : undefined) || currentOptimisticLane;
  const r = (i & CONFIG_HAS_SNAPSHOT) !== 0 && e.o?.ze !== undefined;
  const s = reaskArmed;
  for (let i2 = e.u;i2 !== null; i2 = i2.Te) {
    const e2 = i2.Ie;
    if (s)
      e2.ie &= ~REACTIVE_REASK;
    if (e2.ie & REACTIVE_RECOMPUTING_DEPS && i2.yt === e2.tt && i2 !== e2.et)
      e2.ie |= REACTIVE_MISSED_WAKE;
    if (r && e2.T & CONFIG_IN_SNAPSHOT_SCOPE) {
      e2.ie |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && n) {
      e2.ie |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(e2, n);
    } else if (t) {
      e2.ie |= REACTIVE_OPTIMISTIC_DIRTY;
      if (e2.o)
        e2.o.Oe = undefined;
    }
    enqueueSub(e2);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.oe) {
    if (e.Ge !== NOT_PENDING) {
      e.me = e.Ge;
      e.Ge = NOT_PENDING;
    }
    if (e.T & CONFIG_HAS_COMPANIONS)
      GlobalQueue.un(e);
    return;
  }
  if (e.Ge !== NOT_PENDING) {
    e.me = e.Ge;
    e.Ge = NOT_PENDING;
    if (e.Ce && e.Ce !== EFFECT_TRACKED)
      e.He = true;
    if (e.o)
      e.o.Ue = false;
  }
  t.Ae = false;
  t.ie &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  else
    e.T |= CONFIG_INPUTS_PUBLISHED;
  if (t.o != null && (t.o.Xe !== null || t.o.Je !== null))
    GlobalQueue.We(t, false, true);
  if (e.T & CONFIG_HAS_COMPANIONS)
    GlobalQueue.un(e);
}
var storeCommitHook = null;
var heldRevealed = [];
function commitPendingNodes() {
  const e = currentBatch.$t;
  for (let t = 0;t < e.length; t++) {
    const i = e[t];
    commitPendingNode(i);
    i.ge = null;
    if (i.T & CONFIG_HELD_TRUTH) {
      i.T &= ~CONFIG_HELD_TRUTH;
      heldRevealed.push(i);
    }
  }
  e.length = 0;
  storeCommitHook?.();
}
function finalizePureQueue(e = null, t = false) {
  const i = currentBatch;
  const n = !t;
  if (n)
    commitPendingNodes();
  if (!t && globalQueue.Xt.length)
    checkBoundaryChildren(globalQueue);
  const r = e?.Et;
  const s = n && (e ?? i).it.length !== 0;
  if (r && !s) {
    for (const e2 of r)
      if (!(e2.ie & REACTIVE_DISPOSED))
        enqueueSub(e2);
  }
  const o = dirtyQueue.EE >= dirtyQueue.Ke;
  if (o)
    runHeap(dirtyQueue, GlobalQueue.Fe);
  if (n) {
    if (currentBatch !== i) {
      if (e === null || e === i)
        return;
    } else if (o)
      commitPendingNodes();
    const t2 = e ?? i;
    if (t2.it.length)
      GlobalQueue.fn(t2.it);
    if (r && s) {
      for (const e2 of r)
        if (!(e2.ie & REACTIVE_DISPOSED))
          enqueueSub(e2);
      schedule();
    }
    if (t2.lt.size) {
      for (const e2 of t2.lt) {
        if (e2.ie & REACTIVE_DISPOSED)
          continue;
        enqueueSub(e2);
      }
      t2.lt.clear();
      schedule();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.Xt.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.En.size)
      GlobalQueue.ni(t2.En, e);
    if (heldRevealed.length !== 0) {
      while (heldRevealed.length)
        insertSubs(heldRevealed.pop());
      if (dirtyQueue.EE >= dirtyQueue.Ke) {
        runHeap(dirtyQueue, GlobalQueue.Fe);
        commitPendingNodes();
      }
    }
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.dn(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.Xt) {
    t.se?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].ge = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
var currentBatch = globalQueue.m;
function flush(e) {
  if (actionStepDepth > 0) {
    return e ? e() : undefined;
  }
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
  if (globalQueue.sn) {
    return;
  }
  if (halted)
    return;
  while (scheduled || activeTransition) {
    globalQueue.flush();
  }
  origin = 0;
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
  for (let i = e.fe;i; i = i.ae) {
    let e2 = i.Se;
    while (e2) {
      if (e2 === t || e2.ce === t)
        return true;
      e2 = e2.o?.Gt;
    }
  }
  return !!(e.S & STATUS_PENDING && e.o?._ instanceof NotReadyError && e.o?._.source === t);
}
function transitionComplete(e) {
  if (e.ft)
    return true;
  if (e.ue.length) {
    return false;
  }
  let t = true;
  for (const [i, n] of e.Re) {
    let r = false;
    for (const e2 of n) {
      if (reporterBlocksSource(e2, i)) {
        r = true;
        break;
      }
      n.delete(e2);
    }
    if (!r)
      e.Re.delete(i);
    else if (i.S & STATUS_PENDING && i.o?._?.source === i) {
      t = false;
      break;
    }
  }
  if (t && GlobalQueue.cn?.(e))
    t = false;
  t && (e.ft = true);
  return t;
}
function currentTransition(e) {
  while (e.ft && typeof e.ft === "object")
    e = e.ft;
  return e;
}
function waitingTransition(e) {
  for (const t of transitions)
    if (t.Re.has(e))
      return t;
  return null;
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.ie & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  const E = queueFor(e);
  if (E.Ke > e.Be)
    E.Ke = e.Be;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e.qe?.gt ? e.qe.bt?.Be : e.qe?.Be) ?? -1;
  if (t >= e.Be)
    e.Be = t + 1;
  const n = e.Be;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.Tt;
    E2.Nt = e;
    e.Tt = E2;
    I.Tt = e;
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
    if (E.tE)
      markNode(e);
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
  const n = e.Be;
  if (e.Tt === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.Nt;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.Tt.Nt = t2;
    o.Tt = e.Tt;
  }
  e.Tt = e;
  e.Nt = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.Nt) {
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
  for (let E2 = e.u;E2 !== null; E2 = E2.Te) {
    markNode(E2.Ie, REACTIVE_CHECK);
  }
  if (e.T & CONFIG_FW_CHILDREN) {
    for (let E2 = e.o.i;E2 !== null; E2 = E2.Ee) {
      for (let e2 = E2.u;e2 !== null; e2 = e2.Te) {
        markNode(e2.Ie, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.Ke = 0;e.Ke <= e.EE; e.Ke++) {
    let t = e.eE[e.Ke];
    while (t !== undefined) {
      if (t.ie & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.Ke];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.Be;
  for (let E2 = e.fe;E2; E2 = E2.ae) {
    const e2 = E2.Se;
    const n = e2.ce || e2;
    if (n.oe && n.Be >= t)
      t = n.Be + 1;
  }
  if (e.Be !== t) {
    e.Be = t;
    for (let E2 = e.u;E2 !== null; E2 = E2.Te) {
      insertIntoHeapHeight(E2.Ie, queueFor(E2.Ie));
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/owner.js
function markDisposal(e) {
  let t = e.Ye;
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
    t = t.Ze;
  }
}
function disposeChildren(e, t = false, n) {
  const i = e.ie;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t) {
    e.ie = i | REACTIVE_DISPOSED;
    const t2 = e;
    if (t2.o?.Le || t2.o?.Qe)
      GlobalQueue.un(t2);
  }
  if (t && e.oe && e.o !== null)
    e.o.Pe = null;
  let o = n ? e.o?.Xe ?? null : e.Ye;
  while (o) {
    const e2 = o.Ze;
    const t2 = o;
    t2.T &= ~CONFIG_AUTO_DISPOSE;
    deleteFromHeap(t2, queueFor(t2));
    clearDeps(t2);
    disposeChildren(o, true);
    o = e2;
  }
  if (n) {
    if (e.o !== null)
      e.o.Xe = null;
  } else {
    e.Ye = null;
    e.$e = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e.qe !== null && !(e.qe.ie & REACTIVE_DISPOSED)) {
    const t2 = e.St;
    const n2 = e.Ze;
    if (t2 !== null)
      t2.Ze = n2;
    else
      e.qe.Ye = n2;
    if (n2 !== null)
      n2.St = t2;
    e.St = null;
  }
  runDisposal(e, n);
  if (t && e.Ht) {
    const t2 = e.Ht;
    e.Ht = undefined;
    t2();
  }
}
function runDisposal(e, t) {
  let n = t ? e.o?.Je : e.we;
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
      e.o.Je = null;
  } else
    e.we = null;
}
function childId(e, t) {
  let n = e;
  while (n.T & CONFIG_TRANSPARENT && n.qe)
    n = n.qe;
  if (n.id != null)
    return formatId(n.id, t ? n.$e++ : n.$e);
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
  if (!context.we)
    context.we = e;
  else if (Array.isArray(context.we))
    context.we.push(e);
  else
    context.we = [context.we, e];
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
    gt: true,
    bt: t?.gt ? t.bt : t,
    Ye: null,
    Ze: null,
    St: null,
    we: null,
    C: t?.C ?? globalQueue,
    xe: t?.xe || defaultContext,
    $e: 0,
    o: null,
    qe: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.Ye;
    if (e2 === null) {
      t.Ye = i;
    } else {
      i.Ze = e2;
      e2.St = i;
      t.Ye = i;
    }
  }
  return i;
}
function createRoot(e, t) {
  const n = createOwner(t);
  return runWithOwner(n, () => e(() => n.dispose()));
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(e) {
  const n = e.Se;
  const l = e.ae;
  const o = e.Te;
  const s = e.en;
  if (o !== null)
    o.en = s;
  else
    n.dt = s;
  if (s !== null)
    s.Te = o;
  else {
    n.u = o;
    if (o === null) {
      if (n.T & CONFIG_SLOT_NODE)
        slotUnobservedHook(n);
      else
        n.o?.Ct?.();
      const e2 = n;
      e2.oe && e2.T & CONFIG_AUTO_DISPOSE && !(e2.ie & REACTIVE_ZOMBIE) && !(e2.S & STATUS_PENDING) && unobserved(e2);
    }
  }
  return l;
}
function trimStaleDeps(e) {
  const n = e.et;
  let l = n !== null ? n.ae : e.fe;
  if (l !== null) {
    do {
      l = unlinkSubs(l);
    } while (l !== null);
    if (n !== null)
      n.ae = null;
    else
      e.fe = null;
  }
}
function clearDeps(e) {
  let n = e.fe;
  if (!n)
    return;
  do {
    n = unlinkSubs(n);
  } while (n !== null);
  e.fe = null;
  e.et = null;
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
  const o = n.et;
  if (o !== null && o.Se === e) {
    o.je &&= l;
    return;
  }
  let s = null;
  const t = n.ie & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = o !== null ? o.ae : n.fe;
    if (s !== null && s.Se === e) {
      s.yt = n.tt;
      n.et = s;
      s.je = l;
      return;
    }
  }
  const r = e.dt;
  if (r !== null && r.Ie === n && (!t || r.yt === n.tt)) {
    if (t)
      r.je &&= l;
    else
      r.je = l;
    return;
  }
  const u = n.et = e.dt = {
    Se: e,
    Ie: n,
    ae: s,
    en: r,
    Te: null,
    yt: n.tt,
    je: l
  };
  if (o !== null)
    o.ae = u;
  else
    n.fe = u;
  if (r !== null)
    r.Te = u;
  else
    e.u = u;
  bumpNotifyEpoch();
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.o?.le?.has(n))
    return false;
  (ext(e).le ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  const t = e.o?.le;
  if (!t?.delete(n))
    return false;
  if (!t.size)
    e.o.le = undefined;
  return true;
}
function clearPendingSources(e) {
  if (e.o !== null)
    e.o.le = undefined;
}
function retryReaches(e, n) {
  for (let t = e.fe;t; t = t.ae) {
    const e2 = t.Se.ce || t.Se;
    if (e2 === n || e2.o?.le?.has(n))
      return true;
  }
  return false;
}
function parkLoadingWindow(e, n) {
  ext(e).de = true;
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
  for (let t = e.u;t !== null; t = t.Te)
    n(t.Ie, t);
  for (let t = e.o?.i ?? null;t !== null; t = t.Ee) {
    for (let e2 = t.u;e2 !== null; e2 = e2.Te)
      n(e2.Ie, e2);
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
function settlePendingSource(e, n = e) {
  removePendingSource(e, n);
  let t = false;
  let r;
  const o = new Set;
  const i = GlobalQueue.Ne;
  const settle = (s) => {
    if (o.has(s))
      return;
    if (n !== e && retryReaches(s, n))
      return;
    if (!removePendingSource(s, n))
      return;
    o.add(s);
    s._e = clock;
    const l = s.o?.le?.values().next().value;
    const u = s.S & STATUS_ERROR;
    if (l) {
      if (!u)
        setPendingError(s, l);
      i?.(s);
    } else {
      s.S &= ~STATUS_PENDING;
      if (!u)
        setPendingError(s);
      i?.(s);
      if (s.o?.de) {
        enqueueSub(s);
        t = true;
      }
      if (s.o !== null)
        s.o.de = false;
      if (!s.u && s.T & CONFIG_AUTO_DISPOSE)
        (r ??= []).push(s);
    }
    forEachDependent(s, settle);
  };
  forEachDependent(e, settle);
  if (r)
    for (const e2 of r)
      releaseIfSettledUnobserved(e2);
  if (t)
    schedule();
}
function isThenable(e) {
  return e != null && typeof e === "object" && typeof e.then === "function";
}
function releaseFlightTeardown(e) {
  const n = e.o?.De;
  if (n != null) {
    e.o.De = null;
    n();
  }
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
      e.o.Pe = null;
    e.Ae = false;
    return n;
  }
  ext(e).Pe = n;
  const i = origin;
  let s;
  const settleTransition = () => {
    let n2 = resolveTransition(e);
    if (e.o?.Oe)
      n2 = waitingTransition(e) ?? n2;
    if (n2 && e.S & STATUS_UNINITIALIZED && !currentTransition(n2).Re.has(e)) {
      e.ge = null;
      return;
    }
    globalQueue.initTransition(n2);
  };
  const handleError = (t2) => {
    if (e.o?.Pe !== n)
      return;
    let r2 = t2 instanceof NotReadyError;
    if (r2 && e.Ae) {
      if (e.o !== null)
        e.o.Pe = null;
      parkLoadingWindow(e, t2);
      e._e = clock;
      return;
    }
    settleTransition();
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    if (r2)
      settlePendingSource(e);
    e._e = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, o2) => {
    if (e.o?.Pe !== n)
      return;
    if (e.ie & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    setOrigin(i);
    settleTransition();
    const s2 = !!(e.S & STATUS_UNINITIALIZED);
    const l2 = e.o?.Ue;
    trimStaleDeps(e);
    clearStatus(e);
    if (l2)
      e.o.Ue = true;
    const u = resolveLane(e);
    if (u)
      u.he.delete(e);
    if (t) {
      try {
        t(r2);
      } catch (e2) {
        handleError(e2);
        return;
      }
      if (s2)
        clearStatus(e, true);
    } else if (e.o?.be !== undefined) {
      if (e.Ge === NOT_PENDING)
        queuePendingNode(e);
      e.Ge = r2;
      GlobalQueue.pe?.(e, r2);
      if (!hasActiveOverride(e)) {
        insertSubs(e);
      } else
        GlobalQueue.ye(e, r2);
      e._e = clock;
    } else if (u) {
      const n2 = e.Ce;
      const t2 = e.me;
      const o3 = e.ve;
      try {
        if (!n2 && s2 || !o3 || !o3(r2, t2)) {
          e.me = r2;
          e._e = clock;
          GlobalQueue.pe?.(e, r2);
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
    if (e.Ge === NOT_PENDING) {
      e.Ae = false;
      if (l2)
        e.o.Ue = false;
    }
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
    let l2 = false;
    let u = !r2;
    const close = () => {
      if (l2)
        return;
      l2 = true;
      try {
        const e2 = o2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    };
    r2 ? r2(close) : cleanup(close);
    ext(e).De = close;
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
        if (c && u) {
          t3 = r4;
          f2 = true;
          if (r4.done)
            l2 = true;
        } else if (e.o?.Pe !== n) {
          return;
        } else if (!r4.done) {
          i2 = true;
          asyncWrite(r4.value, iterateOrRelease);
        } else {
          l2 = true;
          if (i2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
          settleAutodispose();
        }
      }, (t4) => {
        if (c && u) {
          r3 = t4;
          a = true;
        } else if (e.o?.Pe === n) {
          l2 = true;
          handleError(t4);
          settleAutodispose();
        }
      });
      c = false;
      if (a) {
        l2 = true;
        handleError(r3);
        if (u)
          throw r3;
        return true;
      }
      if (f2 && !t3.done) {
        s = t3.value;
        i2 = true;
        return iterate();
      }
      return f2 && t3.done;
    };
    const f = iterate();
    u = false;
    return i2 || f;
  };
  let l = null;
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
      l = r2;
    return true;
  };
  if (o) {
    let t2 = false, r2 = false, o2, i2 = true;
    const registerDeferredClose = (n2) => {
      if (!e.we)
        e.we = n2;
      else if (Array.isArray(e.we))
        e.we.push(n2);
      else
        e.we = [e.we, n2];
    };
    n.then((r3) => {
      if (i2) {
        s = r3;
        t2 = true;
      } else if (e.o?.Pe === n && !(e.ie & REACTIVE_DISPOSED) && flattenIfIterable(r3, registerDeferredClose))
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
      if (e.Ae)
        return e.me;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else if (!flattenIfIterable(s)) {
      e.Ae = false;
    }
  }
  if (r)
    flattenIfIterable(n);
  if (l !== null) {
    if (!l) {
      if (e.Ae)
        return e.me;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.Ae = false;
  }
  return s;
}
function clearStatus(e, n = false) {
  if (e.o?.le)
    clearPendingSources(e);
  if (e.o?.de) {
    if (e.o !== null)
      e.o.de = false;
  }
  if (e.o !== null)
    e.o.Ue = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e.o?._)
    setPendingError(e);
  if (e.o?.Le || e.o?.Qe)
    GlobalQueue.Ne(e);
  if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.ke !== null)
    GlobalQueue.ke(e);
  const t = statusNotifierOf(e);
  if (t)
    t.call(e);
}
function notifyStatus(e, n, t, r, o) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const i = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const s = i === e;
  const l = n === STATUS_PENDING && e.o?.be !== undefined && !s;
  const u = l && hasActiveOverride(e);
  if (!r) {
    if (n === STATUS_PENDING && i) {
      addPendingSource(e, i);
      if (!(e.S & STATUS_PENDING))
        e.T &= ~CONFIG_INPUTS_PUBLISHED;
      e.S = STATUS_PENDING | e.S & STATUS_UNINITIALIZED;
      setPendingError(e, i, t);
    } else {
      clearPendingSources(e);
      e.S = n | (n !== STATUS_ERROR ? e.S & STATUS_UNINITIALIZED : 0);
      ext(e)._ = t;
    }
    GlobalQueue.Ne?.(e);
    if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.ke !== null)
      GlobalQueue.ke(e);
  }
  if (o && !r) {
    assignOrMergeLane(e, o);
  }
  const f = r || u;
  const a = r || l ? undefined : o;
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
    e2._e = clock;
    if (n === STATUS_PENDING && i && !e2.o?.le?.has(i) || n !== STATUS_PENDING && (e2.o?._ !== t || e2.o?.le)) {
      if (r2.je && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f && !e2.ge)
        queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.Fe = (e) => {
  if (e.Ce === EFFECT_TRACKED) {
    deleteFromHeap(e, queueFor(e));
    e.He = true;
    e.C.enqueue(EFFECT_USER, e.Ve);
  } else
    recompute(e);
};
GlobalQueue.We = disposeChildren;
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
    if (e.Me)
      return true;
    e = e.qe;
  }
  return false;
}
function recompute(e, t = false) {
  bumpNotifyEpoch();
  const n = e.Ce;
  if (!t) {
    if (e.ge && (!n || activeTransition) && activeTransition !== e.ge)
      globalQueue.initTransition(e.ge);
    deleteFromHeap(e, queueFor(e));
    if (e.o !== null) {
      e.o.Pe = null;
      releaseFlightTeardown(e);
    }
    if (e.ge || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.Ye !== null || e.we !== null) {
      markDisposal(e);
      const t2 = ext(e);
      t2.Je = e.we;
      t2.Xe = e.Ye;
      e.we = null;
      e.Ye = null;
      e.$e = 0;
    }
  }
  let i = !!(e.ie & REACTIVE_OPTIMISTIC_DIRTY);
  const l = (e.T & CONFIG_OPTIMISTIC) !== 0 && e.o?.be !== NOT_PENDING && e.o?.be !== undefined;
  const u = !!(e.S & STATUS_UNINITIALIZED);
  const o = e.S & STATUS_ERROR ? e.o?._ : undefined;
  const s = e.S & STATUS_PENDING ? e.o?.le : undefined;
  const a = e.o?.le?.has(e);
  const r = (e.ie & REACTIVE_REASK) !== 0;
  const c = e.Ae;
  const _ = context;
  context = e;
  e.et = null;
  e.tt++;
  e.ie = REACTIVE_RECOMPUTING_DEPS;
  e._e = clock;
  let f = e.Ge === NOT_PENDING ? e.me : e.Ge;
  let E = e.Be;
  let I = false;
  let N = tracking;
  let T = currentOptimisticLane;
  tracking = true;
  const d = latestReadActive;
  latestReadActive = false;
  if (i) {
    const t2 = GlobalQueue.nt(e, true);
    if (t2)
      currentOptimisticLane = t2;
    else if (t2 === false)
      i = false;
  } else if (activeTransition && !t && activeTransition.it.length) {
    const t2 = GlobalQueue.nt(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const S = n && n !== EFFECT_USER;
  const A = stale;
  if (S)
    stale = true;
  if (n && activeTransition !== null && activeTransition.lt.size)
    activeTransition.lt.delete(e);
  try {
    if (e.T & CONFIG_SYNC) {
      f = e.oe(f);
      if (e.o !== null)
        e.o.Pe = null;
      e.Ae = false;
    } else {
      const t2 = e.o?.Pe;
      const n2 = e.oe(f);
      const i2 = typeof n2 === "object" && n2 !== null;
      const l2 = e.o?.Pe !== t2;
      f = l2 || !i2 ? n2 : handleAsync(e, n2);
      if (!l2 && !i2) {
        if (e.o !== null)
          e.o.Pe = null;
        e.Ae = false;
      }
    }
    if (e.S !== 0 || e.o !== null)
      clearStatus(e, t);
    if (e.T & CONFIG_HAS_LANE && e.o?.Oe)
      GlobalQueue.ut(e);
  } catch (t2) {
    const n2 = t2 instanceof NotReadyError;
    if (n2 && e.Ae) {
      parkLoadingWindow(e, t2);
    } else {
      if (n2 && currentOptimisticLane)
        GlobalQueue.ot(e);
      let i2 = false;
      if (n2) {
        ext(e).de = true;
        if (GlobalQueue.st !== null)
          i2 = GlobalQueue.st(e, r);
      }
      notifyStatus(e, n2 ? STATUS_PENDING : STATUS_ERROR, t2, undefined, n2 ? e.o?.Oe : undefined);
      if (n2 && a && !e.o?.Pe)
        settlePendingSource(e);
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = N;
    latestReadActive = d;
    if (S)
      stale = A;
    I = (e.ie & REACTIVE_MISSED_WAKE) !== 0;
    e.ie = REACTIVE_NONE | (t ? e.ie & REACTIVE_SNAPSHOT_STALE : 0);
    context = _;
  }
  if (!e.o?._) {
    trimStaleDeps(e);
    const r2 = l ? unwrapOverride(e.o?.be) : i || e.Ge === NOT_PENDING ? e.me : e.Ge;
    let _2 = false;
    try {
      _2 = !n && u || !e.ve || !e.ve(r2, f);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && _2) {
      e.He = !e.o?._;
      if (!t) {
        e.C.enqueue(n, e.rt ??= GlobalQueue.ct.bind(null, e));
        let t2 = e._t;
        if (t2 !== activeTransition) {
          e._t = activeTransition;
          if (t2 !== null && (t2 = currentTransition(t2)) !== activeTransition && !t2.ft) {
            (t2.Et ??= []).push(e);
            if (activeTransition !== null)
              (activeTransition.Et ??= []).push(e);
          }
        }
      }
    }
    if (e.o?._)
      ;
    else if (_2) {
      const u2 = l ? e.o?.be : undefined;
      if (t || n && (activeTransition !== e.ge || activeTransition === null || e.T & CONFIG_DIRECT_COMMIT) || i) {
        e.me = f;
        if (l && i) {
          ext(e).be = f === undefined ? OVERRIDE_UNDEFINED : f;
          e.Ge = NOT_PENDING;
        }
      } else {
        e.Ge = f;
        if (c)
          e.Ae = true;
        if ((activeTransition || e.ge) && GlobalQueue.pe !== null)
          GlobalQueue.pe(e, f);
      }
      if (e.u !== null && (!l || i || e.o?.be !== u2))
        insertSubs(e, i || l);
      else if (l && !i && e.o.It !== clock)
        GlobalQueue.ye(e, f);
    } else if (l) {
      if (e.Ge === NOT_PENDING)
        queuePendingNode(e);
      e.Ge = f;
      if (c)
        e.Ae = true;
      GlobalQueue.ye(e, f);
    } else if (e.Be != E) {
      for (let t2 = e.u;t2 !== null; t2 = t2.Te) {
        insertIntoHeapHeight(t2.Ie, queueFor(t2.Ie));
      }
    }
    if (!_2 && !e.o?._) {
      if (o !== undefined)
        settleErroredDependents(e, o);
      if (s) {
        for (const t2 of s)
          if (t2 !== e)
            settlePendingSource(e, t2);
      }
    }
    if (a && !(e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)))
      settlePendingSource(e);
  }
  currentOptimisticLane = T;
  const C = e.Ge !== NOT_PENDING || e.o !== null && (e.o.Xe !== null || e.o.Je !== null) || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  C && (!t || e.S & STATUS_PENDING) && (!e.ge || l) && queuePendingNode(e);
  e.ge && n && activeTransition !== e.ge && runInTransition(e.ge, () => recompute(e));
  if (I) {
    enqueueSub(e);
    schedule();
  }
}
function updateIfNecessary(e) {
  if (e.ie & (REACTIVE_RECOMPUTING_DEPS | REACTIVE_DISPOSED))
    return;
  if (e.ie & REACTIVE_CHECK) {
    for (let t = e.fe;t; t = t.ae) {
      const n = t.Se;
      const i = n.ce || n;
      if (i.oe) {
        updateIfNecessary(i);
      }
      if (e.ie & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.ie & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.o?._ && e._e < clock && !e.o?.Pe) {
    recompute(e);
  }
  e.ie = e.ie & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = t !== null && typeof t === "object" && "loadingValue" in t;
  const l = {
    id: inheritId(t, n, context),
    T: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.H ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ve: t?.equals ?? isEqual,
    we: null,
    C: context?.C ?? globalQueue,
    xe: context?.xe ?? defaultContext,
    $e: 0,
    oe: e,
    me: i ? t.loadingValue : undefined,
    Be: 0,
    Nt: undefined,
    Tt: null,
    fe: null,
    et: null,
    tt: 0,
    u: null,
    dt: null,
    qe: context,
    Ze: null,
    St: null,
    Ye: null,
    ie: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: i ? 0 : STATUS_UNINITIALIZED,
    _e: clock,
    Ge: NOT_PENDING,
    ge: null,
    At: -1,
    Ae: i,
    o: null
  };
  if (t?.unobserved)
    ext(l).Ct = t.unobserved;
  setupComputedNode(l, t);
  return l;
}
function ext(e) {
  return e.o ??= {
    be: undefined,
    Ot: undefined,
    It: 0,
    Rt: 0,
    Oe: undefined,
    Le: undefined,
    Qe: undefined,
    Gt: undefined,
    t: 0,
    Pe: null,
    De: null,
    _: undefined,
    de: undefined,
    le: undefined,
    h: undefined,
    Ue: false,
    i: null,
    Ct: undefined,
    ze: undefined,
    Je: null,
    Xe: null,
    Dt: undefined
  };
}
function createEffectNode(e, t, n, i, l) {
  const u = l?.transparent ?? false;
  const o = {
    id: inheritId(l, u, context),
    T: (u ? CONFIG_TRANSPARENT : 0) | (l?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (l?.sync ? CONFIG_SYNC : 0) | (l?.Pt ?? 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ve: false,
    we: null,
    C: context?.C ?? globalQueue,
    xe: context?.xe ?? defaultContext,
    $e: 0,
    oe: e,
    me: undefined,
    Be: 0,
    Nt: undefined,
    Tt: null,
    fe: null,
    et: null,
    tt: 0,
    u: null,
    dt: null,
    qe: context,
    Ze: null,
    St: null,
    Ye: null,
    ie: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    _e: clock,
    Ge: NOT_PENDING,
    ge: null,
    At: -1,
    Ae: false,
    He: false,
    Ft: undefined,
    ht: t,
    vt: n,
    Ht: undefined,
    Ce: i,
    _t: null,
    o: null
  };
  if (l?.unobserved)
    ext(o).Ct = l.unobserved;
  setupComputedNode(o, lazyOptions);
  return o;
}
var effectStatusNotify = null;
function setEffectStatusNotify(e) {
  effectStatusNotify = e;
}
function statusNotifierOf(e) {
  const t = e.o?.h;
  if (t !== undefined)
    return t;
  return e.Ce ? effectStatusNotify ?? undefined : undefined;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.Tt = e;
  const n = context?.gt ? context.bt : context;
  if (context) {
    const t2 = context.Ye;
    if (t2 === null) {
      context.Ye = e;
    } else {
      e.Ze = t2;
      t2.St = e;
      context.Ye = e;
    }
  }
  if (n)
    e.Be = n.Be + 1;
  if (GlobalQueue.kt !== null)
    GlobalQueue.kt(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      ext(e).ze = e.me === undefined ? NO_SNAPSHOT : e.me;
      e.T |= CONFIG_HAS_SNAPSHOT;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    ve: t?.equals ?? isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.H ? CONFIG_NO_SNAPSHOT : 0),
    me: e,
    u: null,
    dt: null,
    _e: clock,
    ce: n,
    Ee: n?.o?.i || null,
    Vt: null,
    Ge: NOT_PENDING,
    ge: null,
    At: -1,
    o: null
  };
  if (t?.unobserved)
    ext(i).Ct = t.unobserved;
  if (n)
    linkFirewallChild(n, i);
  if (snapshotCaptureActive && !(i.T & CONFIG_NO_SNAPSHOT) && !((n?.S ?? 0) & STATUS_PENDING)) {
    ext(i).ze = e === undefined ? NO_SNAPSHOT : e;
    i.T |= CONFIG_HAS_SNAPSHOT;
    snapshotSources.add(i);
  }
  return i;
}
var slotUnobservedHook;
function linkFirewallChild(e, t) {
  const n = t.Ee;
  if (n !== null)
    n.Vt = t;
  ext(e).i = t;
  e.T |= CONFIG_FW_CHILDREN;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (GlobalQueue.Lt === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.Lt !== null)
      return GlobalQueue.Lt(e);
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
function heldFromStale(e, t) {
  const n = e.ge;
  if (n === null || n === activeTransition)
    return false;
  const i = currentTransition(n);
  const l = t._t;
  if (l == null || currentTransition(l) !== i)
    i.lt.add(t);
  return true;
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.wt(e);
  let t = context;
  if (t?.gt)
    t = t.bt;
  const n = e;
  const i = e.ce;
  const l = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Wt(e, t, l, i);
  } else if (typeof n.oe === "function") {
    prepareComputed(e, false);
  }
  if (!n.oe && l === e && e.o?.be === undefined && e.o?.ze === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.Ge === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && heldFromStale(e, t) ? e.me : e.Ge;
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (l.oe) {
      const n2 = queueFor(e);
      if (l.Be >= n2.Ke) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(l);
      } else if (t.T & CONFIG_FRESH_READ)
        updateIfNecessary(l);
      const i2 = l.Be;
      if (i2 >= t.Be && e.qe !== t) {
        t.Be = i2 + 1;
      }
    }
  }
  if (l.S & STATUS_PENDING) {
    if (t && !(stale && !(l.S & STATUS_UNINITIALIZED) && !(l.T & CONFIG_INPUTS_PUBLISHED) && !(l.T & CONFIG_HAS_LANE && GlobalQueue.Mt(l)) && heldFromStale(l, t))) {
      if (currentOptimisticLane === null || GlobalQueue.qt(l)) {
        if (!tracking && e !== t)
          link(e, t);
        throw l.o?._;
      }
    } else if (!t && l.S & STATUS_UNINITIALIZED) {
      throw l.o?._;
    }
  }
  if (l.oe && l.S & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && l._e < clock) {
      recompute(l);
      return read(e);
    } else
      throw l.o?._;
  }
  if (snapshotCaptureActive && t && t.T & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.o?.ze;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const l2 = e.Ge !== NOT_PENDING ? e.Ge : e.me;
      if (l2 !== i2)
        t.ie |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.o?.be !== undefined && e.o?.be !== NOT_PENDING) {
    if (!(t && t.T & CONFIG_AUTHORITATIVE_READ)) {
      if (t && e.T & CONFIG_OVERRIDE_SUPERSEDED)
        return GlobalQueue.Yt(e);
      return unwrapOverride(e.o?.be);
    }
    e.T |= CONFIG_AUTHORITATIVE_OBSERVED;
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.Zt(e, l, t)) {
    return e.me;
  }
  const u = !t || currentOptimisticLane !== null && GlobalQueue.Kt(e, l, t) || e.Ge === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && heldFromStale(e, t) || e.T & CONFIG_HELD_TRUTH && !latestReadActive && !(t.T & CONFIG_AUTHORITATIVE_READ) ? e.me : e.Ge;
  if (pendingCheckActive)
    GlobalQueue.jt(e, u);
  if (!t && l === e && typeof n.oe === "function" && e.T & CONFIG_AUTO_DISPOSE && !(l.S & STATUS_PENDING) && !e.u) {
    dormantNodes.add(e);
    schedule();
  }
  return u;
}
function setSignal(e, t) {
  if (e.ge && activeTransition !== e.ge)
    globalQueue.initTransition(e.ge);
  if (e.T & CONFIG_OPTIMISTIC) {
    if (!projectionWriteActive)
      return GlobalQueue.Bt(e, t);
    const n2 = e.o?.be;
    if (n2 !== undefined && n2 !== NOT_PENDING)
      return GlobalQueue.zt(e, t);
  }
  const n = e.Ge === NOT_PENDING ? e.me : e.Ge;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.ve || !e.ve(n, t);
  if (!i)
    return t;
  const l = e.Ge !== NOT_PENDING;
  if (!l)
    queuePendingNode(e);
  e.Ge = t;
  e.T & CONFIG_HAS_COMPANIONS && GlobalQueue.pe !== null && GlobalQueue.pe(e, t);
  if (e.oe !== undefined)
    e._e = clock;
  if (l && e.At === notifyEpoch && currentOptimisticLane === null && !reaskArmed)
    return t;
  insertSubs(e);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.ie & REACTIVE_MANUAL_WRITE) && e.Ge === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.ie = e.ie & -4 | REACTIVE_MANUAL_WRITE;
  e.Jt = clock;
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/context.js
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
  let r = t.xe[e.id];
  if (r === undefined)
    r = e.defaultValue;
  if (r === undefined) {
    throw new ContextNotFoundError;
  }
  return r;
}
function setContext(e, t, r = getOwner()) {
  if (!r) {
    throw new NoOwnerError;
  }
  r.xe = {
    ...r.xe,
    [e.id]: t === undefined ? e.defaultValue : t
  };
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, e, E, r) {
  const n = !!r?.user;
  const f = createEffectNode(t, e, E, n ? EFFECT_USER : EFFECT_RENDER, r);
  recompute(f, true);
  !r?.defer && (f.Ce === EFFECT_USER || r?.schedule ? f.C.enqueue(f.Ce, runEffect.bind(null, f)) : runEffect(f, LANE_RUN));
}
function notifyEffectStatus(t, e) {
  const E = t !== undefined ? t : this.S;
  const r = e !== undefined ? e : this.o?._;
  if (E & STATUS_ERROR) {
    this.C.notify(this, STATUS_PENDING, 0);
    if (this.Ce === EFFECT_USER) {
      if (this.S & STATUS_ERROR) {
        this.He = true;
        this.C.enqueue(this.Ce, this.rt ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.C.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(r));
      throw r;
    }
  } else if (this.Ce === EFFECT_RENDER) {
    this.C.notify(this, STATUS_PENDING | STATUS_ERROR, E, r);
  }
}
function runEffect(t, e) {
  if (!t.He || t.ie & REACTIVE_DISPOSED)
    return;
  if (t._t !== null && !currentTransition(t._t).ft && (e & LANE_RUN ? !t.o?.Oe : activeTransition !== null)) {
    t.C.enqueue(t.Ce, t.rt);
    return;
  }
  if (t.S & STATUS_ERROR && t.Ce === EFFECT_USER) {
    const e2 = unwrapStatusError(t.o?._);
    t.Ft = t.me;
    t.He = false;
    try {
      t.vt ? t.vt(e2, () => {
        const e3 = t.Ht;
        t.Ht = undefined;
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
  const E = t.Ht;
  t.Ht = undefined;
  try {
    E?.();
    const e2 = t.ht(t.me, t.Ft);
    if (false)
      ;
    t.Ht = e2;
  } catch (e2) {
    ext(t)._ = new StatusError(t, e2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(e2);
      throw e2;
    }
  } finally {
    t.Ft = t.me;
    t.He = false;
  }
}
GlobalQueue.ct = runEffect;
function trackedEffect(t, e) {
  const run = () => {
    if (!E.He || E.ie & REACTIVE_DISPOSED)
      return;
    try {
      E.He = false;
      recompute(E);
    } finally {}
  };
  const E = computed(() => {
    const e2 = E.Ht;
    E.Ht = undefined;
    e2?.();
    const r = staleValues(t);
    E.Ht = r;
  }, {
    ...e,
    lazy: true
  });
  E.Ht = undefined;
  E.T = E.T & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  E.He = true;
  E.Ce = EFFECT_TRACKED;
  E.Ve = run;
  enqueueSub(E);
  schedule();
}
setEffectStatusNotify(notifyEffectStatus);

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/signals.js
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
function onSettled(e) {
  const t = getOwner();
  t && !(t.T & CONFIG_CHILDREN_FORBIDDEN) ? trackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    e();
  });
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/store/store.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/boundaries.js
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
        if (e2) {
          setSignal(this.D, true);
        }
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
        s = flattenArray(n2, t, r) || s;
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.8/node_modules/@solidjs/signals/dist/prod/store/utils.js
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
    const u2 = !!o2 && o2[$SOURCES];
    if (u2) {
      for (let e2 = 0;e2 < u2.length; e2++)
        t.push(u2[e2]);
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
  let u = t.length - 1;
  for (let e2 = u;e2 >= 0; e2--) {
    const r2 = t[e2];
    if (!r2) {
      e2 === u && u--;
      continue;
    }
    const c2 = Object.getOwnPropertyNames(r2);
    for (let t2 = c2.length - 1;t2 >= 0; t2--) {
      const s2 = c2[t2];
      if (s2 === "__proto__" || s2 === "constructor")
        continue;
      if (!n[s2]) {
        o = o || e2 !== u;
        const t3 = Object.getOwnPropertyDescriptor(r2, s2);
        n[s2] = t3.get ? {
          enumerable: true,
          configurable: true,
          get: t3.get.bind(r2)
        } : t3;
      }
    }
  }
  if (!o)
    return t[u];
  const c = {};
  const s = Object.keys(n);
  for (let e2 = s.length - 1;e2 >= 0; e2--) {
    const r2 = s[e2], t2 = n[r2];
    if (t2.get)
      Object.defineProperty(c, r2, t2);
    else
      c[r2] = t2.value;
  }
  c[$SOURCES] = t;
  return c;
}
// ../../node_modules/.bun/solid-js@2.0.0-rc.8/node_modules/solid-js/dist/solid.js
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
function createComponent(Comp, props, name) {
  return untrack(() => Comp(props || {}));
}

// ../../node_modules/.bun/@solidjs+universal@2.0.0-rc.8+df9aec17228584f1/node_modules/@solidjs/universal/dist/universal.js
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
var named = (options, fallback) => options;
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
    effectOptions = named(effectOptions);
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
  function spread(node, props, skipChildren, options) {
    const prevProps = {};
    props || (props = {});
    if (!skipChildren)
      insert(node, () => props.children, undefined, undefined, named(options));
    effect2(() => {
      const r = props.ref;
      (typeof r === "function" || Array.isArray(r)) && ref(() => r, node);
    }, () => {}, named(options));
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
    }, named(options));
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
          const renderOptions = {
            schedule: true,
            onUpdate(value) {
              mounted = collectMounted(element, value);
            }
          };
          if (false)
            ;
          insert(element, () => tree, undefined, undefined, renderOptions);
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
    ref
  };
}

// ../../packages/core/src/renderer.ts
import * as tree2 from "flux:rendertree";

// ../../packages/core/src/window.ts
import { requestFrame, setPointerLock } from "flux:rendertree";
import { renderFrame } from "srt:render";
import { on as on2, once } from "srt:events";
import { exit as nativeExit, background as nativeBackground } from "srt:app";
import { platform } from "flux:process";

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
var EXIT_HOOK_DEADLINE_MS = 2000;
var suspendHandlers = [];
var quitHandlers = [];
var pendingSuspend = null;
function settleHandlers(name, list) {
  let results = [...list].map((fn) => {
    try {
      return Promise.resolve(fn());
    } catch (err) {
      return Promise.reject(err);
    }
  });
  return Promise.allSettled(results).then((settled) => {
    for (let r of settled) {
      if (r.status === "rejected")
        console.error(`Error in ${name} handler:`, r.reason);
    }
  });
}
function dispatchSuspend() {
  let p = settleHandlers("onSuspend", suspendHandlers).finally(() => {
    if (pendingSuspend === p)
      pendingSuspend = null;
  });
  pendingSuspend = p;
  return p;
}
function dispatchQuit() {
  let inflight = pendingSuspend ?? Promise.resolve();
  return inflight.then(() => settleHandlers("onQuit", quitHandlers));
}
on2("suspend", (e) => {
  dispatchSuspend().then(e.done, e.done);
});
on2("quit", (e) => {
  dispatchQuit().then(e.done, e.done);
});
function exit() {
  let deadline = new Promise((resolve2) => setTimeout(resolve2, EXIT_HOOK_DEADLINE_MS));
  Promise.race([dispatchQuit(), deadline]).then(nativeExit, nativeExit);
}
function background() {
  nativeBackground();
}
function backDefault() {
  if (platform === "android")
    background();
  else
    exit();
}
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
        backDefault();
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
  let focused = untrack(focusedNode);
  let cleanup2 = (n) => {
    for (let child of n.children)
      if (child.parent === n)
        cleanup2(child);
    if (n.id === focused)
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
      let previous = node.parent;
      if (previous) {
        let at = previous.children.indexOf(node);
        if (at !== -1)
          previous.children.splice(at, 1);
      }
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
var gamepadsAccessor;
function gamepads() {
  if (!gamepadsAccessor) {
    runWithOwner(null, () => {
      let [pads, setPads] = createSignal([]);
      on4("gamepads", (e) => setPads(e.pads ?? []));
      gamepadsAccessor = pads;
    });
  }
  return gamepadsAccessor();
}
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
// ../../packages/core/src/input-chord.ts
var MODIFIERS = {
  Shift: "shiftKey",
  Ctrl: "ctrlKey",
  Control: "ctrlKey",
  Alt: "altKey",
  Meta: "metaKey"
};
var ORDER = ["shiftKey", "ctrlKey", "altKey", "metaKey"];
function parseModifiers(what, text, parts) {
  let mods = new Set;
  for (let part of parts) {
    let mod = MODIFIERS[part];
    if (!mod)
      throw new Error(`${what}: unknown modifier "${part}" in "${text}" (Shift, Ctrl, Alt or Meta)`);
    mods.add(mod);
  }
  return ORDER.filter((m) => mods.has(m));
}
function mostSpecific(items, mods, event) {
  let hits = items.filter((item) => mods(item).every((m) => event[m]));
  let most = hits.reduce((n, item) => Math.max(n, mods(item).length), 0);
  return hits.filter((item) => mods(item).length === most);
}

// ../../packages/core/src/input-keyboard.ts
function parse(what, text) {
  if (typeof text !== "string" || text.length === 0)
    throw new Error(`keyboard.${what}: expected a key code or key name, got ${String(text)}`);
  let parts = text.split("+");
  let key = parts.pop();
  if (key.length === 0)
    throw new Error(`keyboard.${what}: "${text}" names no key`);
  return {
    text,
    key,
    mods: parseModifiers(`keyboard.${what}`, text, parts)
  };
}
var matches = (event, key) => {
  if (event.code === key || event.key === key)
    return true;
  if (key === "Space" && event.key === " ")
    return true;
  if (event.key.length !== 1)
    return false;
  if (key.length === 1)
    return event.key.toLowerCase() === key.toLowerCase();
  return key.length === 4 && key.startsWith("Key") && event.key.toLowerCase() === key[3].toLowerCase();
};
function held(specs) {
  let down = new Set;
  let [count, setCount] = createSignal(0, {
    ownedWrite: true
  });
  return {
    count,
    key(event, isDown) {
      let onKey = specs.filter((s) => matches(event, s.key));
      for (let s of onKey)
        down.delete(s.text);
      if (isDown)
        for (let s of mostSpecific(onKey, (s2) => s2.mods, event))
          down.add(s.text);
      setCount(down.size);
    },
    blur() {
      down.clear();
      setCount(0);
    },
    has(text) {
      return down.has(text);
    }
  };
}
function key(spec) {
  let state = held([parse("key", spec)]);
  return {
    kind: "button",
    label: `keyboard ${spec}`,
    rate: () => state.count() > 0,
    key: state.key,
    blur: state.blur
  };
}
function axis(neg, pos) {
  let state = held([parse("axis", neg), parse("axis", pos)]);
  return {
    kind: "axis",
    label: `keyboard ${neg}/${pos}`,
    rate: () => {
      state.count();
      return (state.has(pos) ? 1 : 0) - (state.has(neg) ? 1 : 0);
    },
    key: state.key,
    blur: state.blur
  };
}
function vec2(keys) {
  let state = held(["up", "down", "left", "right"].map((side) => parse(`vec2 ${side}`, keys[side])));
  return {
    kind: "vec2",
    label: `keyboard ${keys.up}/${keys.left}/${keys.down}/${keys.right}`,
    rate: () => {
      state.count();
      return [(state.has(keys.right) ? 1 : 0) - (state.has(keys.left) ? 1 : 0), (state.has(keys.down) ? 1 : 0) - (state.has(keys.up) ? 1 : 0)];
    },
    key: state.key,
    blur: state.blur
  };
}
var keyboard = {
  key,
  axis,
  vec2,
  wasd: vec2({
    up: "KeyW",
    down: "KeyS",
    left: "KeyA",
    right: "KeyD"
  }),
  arrows: vec2({
    up: "ArrowUp",
    down: "ArrowDown",
    left: "ArrowLeft",
    right: "ArrowRight"
  })
};
// ../../packages/core/src/input-gamepad-device.ts
var STICK_DEADZONE = 0.15;
var deadzone = (x, y) => Math.hypot(x, y) < STICK_DEADZONE ? [0, 0] : [x, y];
function createGamepadDevice(pads, slot, who) {
  let sumAxis = (read2) => () => {
    let sum = 0;
    for (let pad of pads())
      sum += read2(pad);
    return sum;
  };
  let sumVec2 = (read2) => () => {
    let x = 0;
    let y = 0;
    for (let pad of pads()) {
      let v = read2(pad);
      x += v[0];
      y += v[1];
    }
    return [x, y];
  };
  let anyButton = (name) => () => pads().some((pad) => pad.buttons.includes(name));
  let pressed = (pad, name) => pad.buttons.includes(name) ? 1 : 0;
  let stick = (side) => ({
    kind: "vec2",
    label: `${who} ${side} stick`,
    rate: sumVec2((pad) => deadzone(pad.axes[`${side}X`] ?? 0, pad.axes[`${side}Y`] ?? 0))
  });
  return {
    get slot() {
      return slot();
    },
    leftStick: stick("left"),
    rightStick: stick("right"),
    dpad: {
      kind: "vec2",
      label: `${who} dpad`,
      rate: sumVec2((pad) => [pressed(pad, "dpadRight") - pressed(pad, "dpadLeft"), pressed(pad, "dpadDown") - pressed(pad, "dpadUp")])
    },
    triggers: {
      kind: "axis",
      label: `${who} triggers`,
      rate: sumAxis((pad) => (pad.axes.rightTrigger ?? 0) - (pad.axes.leftTrigger ?? 0))
    },
    shoulders: {
      kind: "axis",
      label: `${who} shoulders`,
      rate: sumAxis((pad) => pressed(pad, "rightShoulder") - pressed(pad, "leftShoulder"))
    },
    axis(name) {
      if (typeof name !== "string" || name.length === 0)
        throw new Error(`gamepad.axis: expected an axis name, got ${String(name)}`);
      return {
        kind: "axis",
        label: `${who} ${name}`,
        rate: sumAxis((pad) => pad.axes[name] ?? 0)
      };
    },
    button(name) {
      if (typeof name !== "string" || name.length === 0)
        throw new Error(`gamepad.button: expected a button name, got ${String(name)}`);
      return {
        kind: "button",
        label: `${who} ${name}`,
        rate: anyButton(name)
      };
    }
  };
}
function createGamepadSlot(read2, slot) {
  if (slot !== undefined && !(Number.isInteger(slot) && slot >= 0))
    throw new Error(`gamepad: slot must be a non-negative integer, got ${String(slot)}`);
  let pads = () => {
    let all = read2();
    if (slot === undefined)
      return all.filter((p) => p !== null);
    let pad = all[slot];
    return pad ? [pad] : [];
  };
  return createGamepadDevice(pads, () => slot, slot === undefined ? "gamepad" : `gamepad ${slot}`);
}
var claimed = new Set;
function createGamepadJoin(read2) {
  let [slot, setSlot] = createSignal(undefined, {
    ownedWrite: true
  });
  let mine;
  let dispose2 = createRoot((dispose3) => {
    createEffect(() => read2(), (pads2) => {
      if (mine !== undefined)
        return;
      for (let i = 0;i < pads2.length; i++) {
        let pad = pads2[i];
        if (!pad || claimed.has(i) || pad.buttons.length === 0)
          continue;
        mine = i;
        claimed.add(i);
        setSlot(i);
        return;
      }
    });
    return dispose3;
  });
  if (getOwner()) {
    onCleanup(() => {
      dispose2();
      if (mine !== undefined)
        claimed.delete(mine);
    });
  }
  let pads = () => {
    let s = slot();
    if (s === undefined)
      return [];
    let pad = read2()[s];
    return pad ? [pad] : [];
  };
  return createGamepadDevice(pads, slot, "gamepad (joined)");
}

// ../../packages/core/src/input-gamepad.ts
function gamepad(slot) {
  return createGamepadSlot(gamepads, slot);
}
gamepad.next = () => createGamepadJoin(gamepads);
// ../../packages/core/src/input-pointer.ts
var WHEEL_OCTAVES = 0.0015 / Math.LN2;
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
