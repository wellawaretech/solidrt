// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/error.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/constants.js
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
var CONFIG_HELD_CHILDREN = 1 << 20;
var CONFIG_INPUTS_PUBLISHED = 1 << 21;
var CONFIG_PROMOTED = 1 << 22;
var CONFIG_ADOPTED_UNFLUSHED = 1 << 24;
var CONFIG_DERIVED_OVERRIDE = 1 << 23;
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/lanes.js
var signalLanes = new WeakMap;
var activeLanes = new Set;
function findLane(n) {
  while (n.cn)
    n = n.cn;
  return n;
}
function mergeLanes(n, e) {
  n = findLane(n);
  e = findLane(e);
  if (n === e)
    return n;
  e.cn = n;
  for (const i of e.ye)
    n.ye.add(i);
  e.ye.clear();
  n.fn[0].push(...e.fn[0]);
  n.fn[1].push(...e.fn[1]);
  e.fn[0].length = 0;
  e.fn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.o?.Ue;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  if (n.o !== null)
    n.o.Ue = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.o?.Ft) {
    const e = ext(n).Ft = currentTransition(n.o?.Ft);
    if (e.Tt !== true)
      return e;
    if (n.o !== null)
      n.o.Ft = null;
  }
  return resolveLane(n)?.Ge ?? n.Ge;
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const t = n.o?.Ue;
  if (t) {
    const r = findLane(t);
    if (activeLanes.has(r)) {
      if (r !== i && (!hasActiveOverride(n) || n.T & CONFIG_DERIVED_OVERRIDE)) {
        if (i.Ln && findLane(i.Ln) === r) {
          ext(n).Ue = e;
          n.T |= CONFIG_HAS_LANE;
        } else if (r.Ln && findLane(r.Ln) === i)
          ;
        else
          mergeLanes(i, r);
      }
      return;
    }
  }
  ext(n).Ue = e;
  n.T |= CONFIG_HAS_LANE;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  et: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  et: 0,
  EE: 0
};
function cancelZombieRecompute(e) {
  if (e.ue & REACTIVE_OPTIMISTIC_DIRTY)
    return GlobalQueue.We(e);
  if (e.ue & REACTIVE_IN_HEAP_HEIGHT)
    e.ue &= -12;
  else {
    deleteFromHeap(e, zombieQueue);
    e.ue &= -4;
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
  return transitions.size === 0 && activeLanes.size === 0 && e.hn.length === 0 && t.rt.length === 0 && t.A.length === 0 && t.dn.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.u !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.ve !== NOT_PENDING)
      continue;
    if (e.o?.Ce !== undefined && e.o?.Ce !== NOT_PENDING)
      continue;
    if (e.o?.t)
      continue;
    transientStoreNodes.delete(e);
    if (e.T & CONFIG_SLOT_NODE)
      slotUnobservedHook(e);
    else
      e.o?.Pt?.();
  }
}
function createBatch() {
  return {
    Pe: clock,
    Ot: [],
    oe: new Map,
    rt: [],
    A: [],
    dn: new Set,
    pe: [],
    Sn: {
      mn: [[], []],
      hn: []
    },
    Tt: false,
    ct: new Set,
    St: null
  };
}
function mergeTransitionState(e, t) {
  t.Tt = e;
  e.pe.push(...t.pe);
  e.he ||= t.he;
  for (const n of activeLanes)
    if (n.Ge === t)
      n.Ge = e;
  if (t.rt.length) {
    e.rt.push(...t.rt);
    t.rt.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const n of t.dn)
    e.dn.add(n);
  for (const [n, i] of t.oe) {
    let t2 = e.oe.get(n);
    if (!t2)
      e.oe.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
  for (const n of t.ct)
    e.ct.add(n);
  if (t.St)
    (e.St ??= []).push(...t.St);
}
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.Kt && !projectionWriteActive)
    queueMicrotask(flush);
}
var wokenTransitions = [];
function wakeParked() {
  for (const e of transitions)
    wokenTransitions.includes(e) || wokenTransitions.push(e);
  schedule();
}
var batchJoins = [];
var ROOT_ERROR_HOOK = Symbol.for("solid-js/root-error-hook");
function haltReactivity(e) {
  if (halted)
    return;
  halted = true;
  let t = "[REACTIVITY_HALTED]";
  const n = e !== undefined && globalThis.reportError;
  n || e === undefined ? console.error(t) : console.error(t, e);
  n && n(e);
}
function notifyHalted() {
  if (haltNotified)
    return;
  haltNotified = true;
  console.error("[REACTIVITY_HALTED]");
}
var queueRunToken = 0;

class Queue {
  _parent = null;
  mn = [[], []];
  hn = [];
  An = 0;
  created = clock;
  addChild(e) {
    this.hn.push(e);
    e._parent = this;
  }
  removeChild(e) {
    const t = this.hn.indexOf(e);
    if (t >= 0) {
      this.hn.splice(t, 1);
      e._parent = null;
    }
  }
  notify(e, t, n, i) {
    if (this._parent)
      return this._parent.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.mn[e - 1].length) {
      const t2 = this.mn[e - 1];
      this.mn[e - 1] = [];
      runQueue(t2, e);
    }
    const t = this.hn;
    const n = ++queueRunToken;
    for (let i = 0;i < t.length; ) {
      const s = t[i];
      if (s.An !== n) {
        s.An = n;
        s.run?.(e);
        if (t[i] !== s) {
          i = 0;
          continue;
        }
      }
      i++;
    }
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const n = findLane(currentOptimisticLane);
        n.fn[e - 1].push(t);
      } else {
        this.mn[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.mn[0].push(...this.mn[0]);
    e.mn[1].push(...this.mn[1]);
    this.mn = [[], []];
    for (let t = 0;t < this.hn.length; t++) {
      let n = this.hn[t];
      let i = e.hn[t];
      if (!i) {
        i = {
          mn: [[], []],
          hn: []
        };
        e.hn[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.mn[0].push(...e.mn[0]);
    this.mn[1].push(...e.mn[1]);
    for (let t = 0;t < e.hn.length; t++) {
      const n = e.hn[t];
      let i = this.hn[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue extends Queue {
  Kt = false;
  m = createBatch();
  static We;
  static Be;
  static Et;
  static Cn = null;
  static p = null;
  static G = null;
  static M = null;
  static N = null;
  static wt = null;
  static Yt = null;
  static me = null;
  static Oe = null;
  static Me = null;
  static En = null;
  static zt = null;
  static Jt = null;
  static nn = null;
  static Nt = null;
  static k = null;
  static pn = null;
  static vn = null;
  static ln = null;
  static On = null;
  static Nn = null;
  static Rn = null;
  static _n = null;
  static In = null;
  static tn = null;
  static $t = null;
  static Xt = null;
  static Bt = null;
  static st = null;
  static _t = null;
  static Zt = null;
  static ft = null;
  static we = null;
  static Tn = null;
  static en = null;
  static Ve = null;
  static jt = false;
  static un = null;
  static Dn = null;
  flush() {
    if (this.Kt)
      return;
    if (activeTransition === null && dirtyQueue.EE < dirtyQueue.et && this.mn[0].length === 0 && this.mn[1].length === 0 && this.hn.length === 0 && !wokenTransitions.length && !batchJoins.length && canUseSimpleSyncFlush(this)) {
      this.Kt = true;
      try {
        resyncUnflushedCompanions();
        sweepDormant();
        commitPendingNodes();
      } finally {
        this.Kt = false;
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.et || this.mn[0].length !== 0 || this.mn[1].length !== 0 || this.m.Ot.length !== 0;
      return;
    }
    this.Kt = true;
    resyncUnflushedCompanions();
    try {
      while (batchJoins.length)
        this.initTransition(batchJoins.pop());
      if (false)
        ;
      sweepDormant();
      runHeap(dirtyQueue, GlobalQueue.We);
      if (activeTransition) {
        if (GlobalQueue.Tn?.(activeTransition))
          runHeap(dirtyQueue, GlobalQueue.We);
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          heldTrims.length = 0;
          runHeap(zombieQueue, this.m === e2 ? cancelZombieRecompute : GlobalQueue.We);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue._n(EFFECT_RENDER);
            GlobalQueue._n(EFFECT_USER);
          }
          this.stashQueues(e2.Sn);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.et || this.m.Ot.length > 0;
          reassignPendingTransition(e2.Ot);
          activeTransition = null;
          finalizePureQueue(null, true);
          return;
        }
        const t = activeTransition;
        const n = this.m;
        n !== t && n.Ot.push(...t.Ot);
        this.restoreQueues(t.Sn);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(n.Ot);
        finalizePureQueue(t);
        if (n === t) {
          const e2 = createBatch();
          e2.Ot = n.Ot;
          e2.rt = n.rt;
          e2.A = n.A;
          e2.dn = n.dn;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.et) {
            runHeap(dirtyQueue, GlobalQueue.We);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.We);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.et || activeTransition !== null;
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
      while (!scheduled && !activeTransition && wokenTransitions.length)
        this.initTransition(wokenTransitions.pop());
      this.Kt = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING) {
      if (n & STATUS_PENDING) {
        const t2 = i ?? e.o?._;
        if (t2?.l)
          return true;
        if (t2) {
          if (!activeTransition && !e.Ge && currentBatch.Ot.length)
            this.initTransition();
          if (activeTransition) {
            const n2 = t2.source;
            let i2 = activeTransition.oe.get(n2);
            if (!i2)
              activeTransition.oe.set(n2, i2 = new Set);
            const s = i2.size;
            i2.add(e);
            if (i2.size !== s) {
              schedule();
              GlobalQueue.vn?.(activeTransition);
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
      if (e.Tt === true || e === activeTransition)
        return;
    }
    if (!e && activeTransition && activeTransition.Pe === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? createBatch();
    } else if (e) {
      const t2 = activeTransition;
      mergeTransitionState(e, t2);
      this.restoreQueues(t2.Sn);
      transitions.delete(t2);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.Pe = clock;
    const t = this.m;
    if (t !== activeTransition) {
      const e2 = this.Kt ? 0 : CONFIG_ADOPTED_UNFLUSHED;
      for (let n = 0;n < t.Ot.length; n++) {
        const i = t.Ot[n];
        if (i.Ge === null && i.ve !== NOT_PENDING && (!i.ce || i.ue & REACTIVE_MANUAL_WRITE && !(i.S & STATUS_UNINITIALIZED)) && i.Fe && i.Fe(i.Qe, i.ve)) {
          i.ve = NOT_PENDING;
          commitPendingNode(i);
          continue;
        }
        i.Ge = activeTransition;
        i.T |= e2;
        activeTransition.Ot.push(i);
      }
      for (let e3 = 0;e3 < t.rt.length; e3++) {
        const n = t.rt[e3];
        n.Ge = activeTransition;
        activeTransition.rt.push(n);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e3 of t.dn)
        activeTransition.dn.add(e3);
      if (t.ct.size) {
        for (const e3 of t.ct)
          activeTransition.ct.add(e3);
        t.ct.clear();
      }
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.Ge)
        e2.Ge = activeTransition;
    }
    schedule();
  }
}
function queuePendingNode(e) {
  currentBatch.Ot.push(e);
  if (!globalQueue.Kt)
    markUnflushedStaged();
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
  e.ht = notifyEpoch;
  const n = e.T;
  const i = (n & CONFIG_HAS_LANE ? e.o?.Ue : undefined) || currentOptimisticLane;
  const s = (n & CONFIG_HAS_SNAPSHOT) !== 0 && e.o?.nt !== undefined;
  const r = reaskArmed;
  for (let n2 = e.u;n2 !== null; n2 = n2.Ne) {
    const e2 = n2._e;
    if (r)
      e2.ue &= ~REACTIVE_REASK;
    if (e2.ue & REACTIVE_RECOMPUTING_DEPS && n2.qe === e2.Ze && n2 !== e2.ot)
      e2.ue |= REACTIVE_MISSED_WAKE;
    if (s && e2.T & CONFIG_IN_SNAPSHOT_SCOPE) {
      e2.ue |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && i) {
      e2.ue |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(e2, i);
    } else if (t) {
      e2.ue |= REACTIVE_OPTIMISTIC_DIRTY;
      if (e2.o)
        e2.o.Ue = undefined;
    }
    enqueueSub(e2);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.ce) {
    if (e.ve !== NOT_PENDING) {
      e.Qe = e.ve;
      e.ve = NOT_PENDING;
    }
    if (e.T & CONFIG_HAS_COMPANIONS)
      GlobalQueue.En(e);
    return;
  }
  if (e.ve !== NOT_PENDING) {
    e.Qe = e.ve;
    e.ve = NOT_PENDING;
    t.S &= ~STATUS_UNINITIALIZED;
    if (e.Le && e.Le !== EFFECT_TRACKED)
      e.Ye = true;
    if (e.o)
      e.o.be = false;
  }
  t.ge = false;
  t.ue &= ~REACTIVE_MANUAL_WRITE;
  if (t.o?._ == null)
    trimStaleDeps(t);
  t.T &= ~CONFIG_HELD_CHILDREN;
  if (!(t.S & STATUS_PENDING))
    t.S &= ~STATUS_UNINITIALIZED;
  else
    e.T |= CONFIG_INPUTS_PUBLISHED;
  if (t.o != null && (t.o.lt !== null || t.o.it !== null))
    GlobalQueue.Be(t, false, true);
  if (e.T & CONFIG_HAS_COMPANIONS)
    GlobalQueue.En(e);
}
var storeCommitHook = null;
var heldRevealed = [];
var heldTrims = [];
function commitPendingNodes() {
  while (heldTrims.length)
    trimStaleDeps(heldTrims.pop());
  const e = currentBatch.Ot;
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    commitPendingNode(n);
    n.Ge = null;
    if (n.T & CONFIG_HELD_TRUTH) {
      n.T &= ~CONFIG_HELD_TRUTH;
      heldRevealed.push(n);
    }
  }
  e.length = 0;
  storeCommitHook?.();
}
function finalizePureQueue(e = null, t = false) {
  const n = currentBatch;
  const i = !t;
  if (i)
    commitPendingNodes();
  if (!t && globalQueue.hn.length)
    checkBoundaryChildren(globalQueue);
  const s = e?.St;
  const r = i && (e ?? n).rt.length !== 0;
  if (s && !r) {
    for (const e2 of s)
      if (!(e2.ue & REACTIVE_DISPOSED))
        enqueueSub(e2);
  }
  const o = dirtyQueue.EE >= dirtyQueue.et;
  if (o)
    runHeap(dirtyQueue, GlobalQueue.We);
  if (i) {
    if (currentBatch !== n) {
      if (e === null || e === n)
        return;
    } else if (o)
      commitPendingNodes();
    const t2 = e ?? n;
    if (t2.rt.length)
      GlobalQueue.On(t2.rt);
    if (s && r) {
      for (const e2 of s)
        if (!(e2.ue & REACTIVE_DISPOSED))
          enqueueSub(e2);
      schedule();
    }
    if (t2.ct.size) {
      for (const e2 of t2.ct) {
        if (e2.ue & REACTIVE_DISPOSED)
          continue;
        enqueueSub(e2);
      }
      t2.ct.clear();
      schedule();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.hn.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.dn.size)
      GlobalQueue.Cn(t2.dn, e);
    if (heldRevealed.length !== 0) {
      while (heldRevealed.length)
        insertSubs(heldRevealed.pop());
      if (dirtyQueue.EE >= dirtyQueue.et) {
        runHeap(dirtyQueue, GlobalQueue.We);
        commitPendingNodes();
      }
    }
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.Rn(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.hn) {
    t.fe?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].Ge = activeTransition;
    e[t].T &= ~CONFIG_ADOPTED_UNFLUSHED;
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
  if (globalQueue.Kt) {
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
  for (let n = 0;n < e.length; n++)
    e[n](t);
}
function reporterBlocksSource(e, t, n) {
  const i = e.ue;
  if (i & REACTIVE_DISPOSED)
    return false;
  if (i & REACTIVE_ZOMBIE) {
    let t2 = e;
    while (t2 && t2.ue & REACTIVE_ZOMBIE)
      t2 = t2._parent;
    let i2 = t2 && (t2.Ge || (t2.T & CONFIG_HELD_CHILDREN ? activeTransition : null));
    if (!i2 || (i2 = currentTransition(i2)).Tt === true || i2 === n)
      return false;
  }
  for (let t2 = e.C;t2; t2 = t2._parent)
    if (t2.ee & STATUS_PENDING && !t2.L)
      return false;
  if (e.o?.ae?.has(t))
    return true;
  const s = e.ot;
  for (let n2 = s === null ? null : e.Se;n2; n2 = n2 === s ? null : n2.de) {
    let e2 = n2.Ee;
    while (e2) {
      if (e2 === t || e2.Te === t || e2.o?.ae?.has(t))
        return true;
      e2 = e2.o?.Ht;
    }
  }
  return !!(e.S & STATUS_PENDING && e.o?._ instanceof NotReadyError && e.o?._.source === t);
}
function sourceObserved(e, t, n) {
  const i = e.oe.get(t);
  let s = false;
  for (const e2 of i ?? []) {
    if (reporterBlocksSource(e2, t, n))
      return true;
    if (n && e2.ue & REACTIVE_ZOMBIE)
      s = true;
    else
      i.delete(e2);
  }
  if (!s)
    e.oe.delete(t);
  return false;
}
function transitionComplete(e) {
  if (e.Tt)
    return true;
  if (e.pe.length) {
    return false;
  }
  let t = true;
  for (const n of e.oe.keys()) {
    if (sourceObserved(e, n, e) && n.o?.ae?.size) {
      t = false;
      break;
    }
  }
  if (t && GlobalQueue.Nn?.(e))
    t = false;
  t && (e.Tt = true);
  return t;
}
function currentTransition(e) {
  while (e.Tt && typeof e.Tt === "object")
    e = e.Tt;
  return e;
}
function waitingTransition(e) {
  for (const t of transitions)
    if (sourceObserved(t, e))
      return t;
  return null;
}
function enterWaiting(e) {
  for (const t of transitions)
    if (sourceObserved(t, e))
      globalQueue.initTransition(t);
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.ue & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  const E = queueFor(e);
  if (E.et > e.tt)
    E.et = e.tt;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e._parent?.xt ? e._parent.Qt?.tt : e._parent?.tt) ?? -1;
  if (t >= e.tt)
    e.tt = t + 1;
  const n = e.tt;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.Rt;
    E2.At = e;
    e.Rt = E2;
    I.Rt = e;
  }
  if (n > E.EE)
    E.EE = n;
}
function insertIntoHeap(e, E) {
  let t = e.ue;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (t & REACTIVE_CHECK) {
    e.ue = t & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else {
    e.ue = t | REACTIVE_IN_HEAP;
    if (E.tE)
      markNode(e);
  }
  if (!(t & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, E);
}
function insertIntoHeapHeight(e, E) {
  let t = e.ue;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.ue = t | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, E);
}
function deleteFromHeap(e, E) {
  const t = e.ue;
  if (!(t & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.ue = t & -25;
  const n = e.tt;
  if (e.Rt === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.At;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.Rt.At = t2;
    o.Rt = e.Rt;
  }
  e.Rt = e;
  e.At = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.At) {
      if (t.ue & REACTIVE_IN_HEAP)
        markNode(t);
    }
  }
}
function markNode(e, E = REACTIVE_DIRTY) {
  const t = e.ue;
  if ((t & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= E)
    return;
  e.ue = t & -4 | E;
  for (let E2 = e.u;E2 !== null; E2 = E2.Ne) {
    markNode(E2._e, REACTIVE_CHECK);
  }
  if (e.T & CONFIG_FW_CHILDREN) {
    for (let E2 = e.o.i;E2 !== null; E2 = E2.De) {
      for (let e2 = E2.u;e2 !== null; e2 = e2.Ne) {
        markNode(e2._e, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.et = 0;e.et <= e.EE; e.et++) {
    let t = e.eE[e.et];
    while (t !== undefined) {
      if (t.ue & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.et];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.tt;
  for (let E2 = e.Se;E2; E2 = E2.de) {
    const e2 = E2.Ee;
    const n = e2.Te || e2;
    if (n.ce && n.tt >= t)
      t = n.tt + 1;
  }
  if (e.tt !== t) {
    e.tt = t;
    for (let E2 = e.u;E2 !== null; E2 = E2.Ne) {
      insertIntoHeapHeight(E2._e, queueFor(E2._e));
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/owner.js
function markDisposal(e) {
  let t = e.Xe;
  while (t) {
    const e2 = t.ue;
    t.ue = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(t, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(t, zombieQueue);
      else
        insertIntoHeapHeight(t, zombieQueue);
    }
    markDisposal(t);
    t = t.$e;
  }
}
function disposeChildren(e, t = false, n) {
  const i = e.ue;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t) {
    e.ue = i | REACTIVE_DISPOSED;
    const t2 = e;
    if (t2.o?.je || t2.o?.xe)
      GlobalQueue.En(t2);
    if (t2.T & CONFIG_CHILD_COMPANIONS)
      t2.o.bt.forEach(GlobalQueue.En);
    const n2 = t2.Ge;
    if (n2 && t2.S & STATUS_PENDING && !wokenTransitions.includes(n2))
      wokenTransitions.push(n2), schedule();
  }
  if (t && e.ce && e.o !== null)
    e.o.Re = null;
  let o = n ? e.o?.lt ?? null : e.Xe;
  while (o) {
    const e2 = o.$e;
    const t2 = o;
    t2.T &= ~CONFIG_AUTO_DISPOSE;
    deleteFromHeap(t2, queueFor(t2));
    clearDeps(t2);
    disposeChildren(o, true);
    o = e2;
  }
  if (n) {
    if (e.o !== null)
      e.o.lt = null;
  } else {
    e.Xe = null;
    e.ut = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e._parent !== null && !(e._parent.ue & REACTIVE_DISPOSED)) {
    const t2 = e.Dt;
    const n2 = e.$e;
    if (t2 !== null)
      t2.$e = n2;
    else
      e._parent.Xe = n2;
    if (n2 !== null)
      n2.Dt = t2;
    e.Dt = null;
  }
  runDisposal(e, n);
  if (t && e.yt) {
    const t2 = e.yt;
    e.yt = undefined;
    t2();
  }
}
function runDisposal(e, t) {
  let n = t ? e.o?.it : e.ke;
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
      e.o.it = null;
  } else
    e.ke = null;
}
function childId(e, t) {
  let n = e;
  while (n.T & CONFIG_TRANSPARENT && n._parent)
    n = n._parent;
  if (n.id != null)
    return formatId(n.id, t ? n.ut++ : n.ut);
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
  if (!context.ke)
    context.ke = e;
  else if (Array.isArray(context.ke))
    context.ke.push(e);
  else
    context.ke = [context.ke, e];
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
    xt: true,
    Qt: t?.xt ? t.Qt : t,
    Xe: null,
    $e: null,
    Dt: null,
    ke: null,
    C: t?.C ?? globalQueue,
    ze: t?.ze || defaultContext,
    ut: 0,
    o: null,
    _parent: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.Xe;
    if (e2 === null) {
      t.Xe = i;
    } else {
      i.$e = e2;
      e2.Dt = i;
      t.Xe = i;
    }
  }
  return i;
}
function createRoot(e, t) {
  const n = createOwner(t);
  return runWithOwner(n, () => e(() => n.dispose()));
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(e) {
  const n = e.Ee;
  const l = e.de;
  const o = e.Ne;
  const s = e.rn;
  if (o !== null)
    o.rn = s;
  else
    n.Gt = s;
  if (s !== null)
    s.Ne = o;
  else {
    n.u = o;
    if (o === null) {
      if (n.T & CONFIG_SLOT_NODE)
        slotUnobservedHook(n);
      else
        n.o?.Pt?.();
      const e2 = n;
      e2.ce && e2.T & CONFIG_AUTO_DISPOSE && !(e2.ue & REACTIVE_ZOMBIE) && !(e2.S & STATUS_PENDING) && unobserved(e2);
    }
  }
  return l;
}
function trimStaleDeps(e) {
  const n = e.ot;
  let l = n !== null ? n.de : e.Se;
  if (l !== null) {
    do {
      l = unlinkSubs(l);
    } while (l !== null);
    if (n !== null)
      n.de = null;
    else
      e.Se = null;
  }
}
function clearDeps(e) {
  let n = e.Se;
  if (!n)
    return;
  do {
    n = unlinkSubs(n);
  } while (n !== null);
  e.Se = null;
  e.ot = null;
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
    if (!e.u && e.T & CONFIG_AUTO_DISPOSE && !(e.S & STATUS_PENDING) && !(e.ue & (REACTIVE_DISPOSED | REACTIVE_ZOMBIE))) {
      unobserved(e);
    }
  }
  dormantNodes.clear();
}
function link(e, n, l = false) {
  const o = n.ot;
  if (o !== null && o.Ee === e) {
    o.He &&= l;
    return;
  }
  let s = null;
  const t = n.ue & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = o !== null ? o.de : n.Se;
    if (s !== null && s.Ee === e) {
      s.qe = n.Ze;
      n.ot = s;
      s.He = l;
      return;
    }
  }
  const r = e.Gt;
  if (r !== null && r._e === n && (!t || r.qe === n.Ze)) {
    if (t)
      r.He &&= l;
    else
      r.He = l;
    return;
  }
  const u = n.ot = e.Gt = {
    Ee: e,
    _e: n,
    de: s,
    rn: r,
    Ne: null,
    qe: n.Ze,
    He: l
  };
  if (o !== null)
    o.de = u;
  else
    n.Se = u;
  if (r !== null)
    r.Ne = u;
  else
    e.u = u;
  bumpNotifyEpoch();
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.o?.ae?.has(n))
    return false;
  (ext(e).ae ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  const t = e.o?.ae;
  if (!t?.delete(n))
    return false;
  if (!t.size)
    e.o.ae = undefined;
  return true;
}
function clearPendingSources(e) {
  if (e.o !== null)
    e.o.ae = undefined;
}
function retryReaches(e, n) {
  for (let t = e.Se;t; t = t.de) {
    const e2 = t.Ee.Te || t.Ee;
    if (e2 === n || e2.o?.ae?.has(n))
      return true;
  }
  return false;
}
function parkLoadingWindow(e, n) {
  ext(e).Ie = true;
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
  for (let t = e.u;t !== null; t = t.Ne)
    n(t._e, t);
  for (let t = e.o?.i ?? null;t !== null; t = t.De) {
    for (let e2 = t.u;e2 !== null; e2 = e2.Ne)
      n(e2._e, e2);
  }
}
function releaseIfSettledUnobserved(e) {
  e.ce && e.T & CONFIG_AUTO_DISPOSE && !e.u && !(e.ue & REACTIVE_ZOMBIE) && !(e.S & STATUS_PENDING) && unobserved(e);
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
  const i = new Set;
  const o = GlobalQueue.Oe;
  const settle = (s) => {
    if (i.has(s))
      return;
    if (n !== e && retryReaches(s, n))
      return;
    if (!removePendingSource(s, n))
      return;
    i.add(s);
    s.Pe = clock;
    const u = s.o?.ae?.values().next().value;
    const l = s.S & STATUS_ERROR;
    if (u) {
      if (!l)
        setPendingError(s, u);
      o?.(s);
    } else {
      s.S &= ~STATUS_PENDING;
      if (!l)
        setPendingError(s);
      o?.(s);
      if (s.o?.Ie) {
        enqueueSub(s);
        t = true;
      }
      if (s.o !== null)
        s.o.Ie = false;
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
  const n = e.o?.Ae;
  if (n != null) {
    e.o.Ae = null;
    n();
  }
}
function handleAsync(e, n, t) {
  let r = false;
  let i = false;
  if (typeof n === "object" && n !== null) {
    untrack(() => {
      r = n[Symbol.asyncIterator];
      i = !r && isThenable(n);
    });
  }
  if (!i && !r) {
    if (e.o !== null)
      e.o.Re = null;
    e.ge = false;
    return n;
  }
  ext(e).Re = n;
  e.o.ae = undefined;
  const o = origin;
  let s;
  const settleTransition = () => {
    let n2 = resolveTransition(e);
    if (e.o?.Ue)
      n2 = waitingTransition(e) ?? n2;
    if (n2 && e.S & STATUS_UNINITIALIZED && !currentTransition(n2).oe.has(e)) {
      e.Ge = null;
      return;
    }
    globalQueue.initTransition(n2);
    enterWaiting(e);
  };
  const handleError = (t2) => {
    if (e.o?.Re !== n)
      return;
    let r2 = t2 instanceof NotReadyError;
    if (r2 && e.ge) {
      if (e.o !== null)
        e.o.Re = null;
      parkLoadingWindow(e, t2);
      e.Pe = clock;
      return;
    }
    settleTransition();
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    if (r2)
      settlePendingSource(e);
    e.Pe = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, i2) => {
    if (e.o?.Re !== n)
      return;
    if (e.ue & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    setOrigin(o);
    settleTransition();
    const s2 = !!(e.S & STATUS_UNINITIALIZED);
    const u2 = e.o?.be;
    landStatus(e);
    if (u2)
      e.o.be = true;
    const l = resolveLane(e);
    if (l)
      l.ye.delete(e);
    if (t) {
      try {
        t(r2);
      } catch (e2) {
        handleError(e2);
        return;
      }
      if (s2)
        landStatus(e, true);
    } else if (e.o?.Ce !== undefined && !(l && e.T & CONFIG_DERIVED_OVERRIDE)) {
      if (e.ve === NOT_PENDING)
        queuePendingNode(e);
      e.ve = r2;
      GlobalQueue.me?.(e, r2);
      if (!hasActiveOverride(e)) {
        insertSubs(e);
      } else
        GlobalQueue.we(e, r2);
      e.Pe = clock;
    } else if (l) {
      const n2 = e.Le;
      const t2 = hasActiveOverride(e) ? unwrapOverride(e.o.Ce) : e.Qe;
      const i3 = e.Fe;
      try {
        if (!n2 && s2 || !i3 || !i3(r2, t2)) {
          if (n2)
            e.Qe = r2;
          else
            GlobalQueue.Ve(e, r2, l);
          e.Pe = clock;
          GlobalQueue.me?.(e, r2);
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
    if (e.ve === NOT_PENDING) {
      e.ge = false;
      if (u2)
        e.o.be = false;
      trimStaleDeps(e);
    }
    settlePendingSource(e);
    schedule();
    flush();
    i2?.();
  };
  const settleAutodispose = () => {
    if (e.T & CONFIG_AUTO_DISPOSE && !e.u && !(e.S & STATUS_PENDING)) {
      unobserved(e);
      return true;
    }
    return false;
  };
  const consumeIterator = (t2, r2) => {
    const i2 = t2[Symbol.asyncIterator]();
    let o2 = false;
    let u2 = false;
    let l = !r2;
    const close = () => {
      if (u2)
        return;
      u2 = true;
      try {
        const e2 = i2.return?.();
        if (isThenable(e2))
          e2.then(undefined, () => {});
      } catch {}
    };
    r2 ? r2(close) : cleanup(close);
    ext(e).Ae = close;
    const iterateOrRelease = () => {
      if (!settleAutodispose())
        iterate();
    };
    const iterate = () => {
      let t3, r3, f2 = false, a = false, c = true;
      const S = i2.next();
      const d = isThenable(S) ? S : {
        then: (e2) => void e2(S)
      };
      d.then((r4) => {
        if (c && l) {
          t3 = r4;
          f2 = true;
          if (r4.done)
            u2 = true;
        } else if (e.o?.Re !== n) {
          return;
        } else if (!r4.done) {
          o2 = true;
          asyncWrite(r4.value, iterateOrRelease);
        } else {
          u2 = true;
          if (o2) {
            schedule();
            flush();
          } else {
            asyncWrite(undefined);
          }
          settleAutodispose();
        }
      }, (t4) => {
        if (c && l) {
          r3 = t4;
          a = true;
        } else if (e.o?.Re === n) {
          u2 = true;
          handleError(t4);
          settleAutodispose();
        }
      });
      c = false;
      if (a) {
        u2 = true;
        handleError(r3);
        if (l)
          throw r3;
        return true;
      }
      if (f2 && !t3.done) {
        s = t3.value;
        o2 = true;
        return iterate();
      }
      return f2 && t3.done;
    };
    const f = iterate();
    l = false;
    return o2 || f;
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
  if (i) {
    let t2 = false, r2 = false, i2, o2 = true;
    const registerDeferredClose = (n2) => {
      if (!e.ke)
        e.ke = n2;
      else if (Array.isArray(e.ke))
        e.ke.push(n2);
      else
        e.ke = [e.ke, n2];
    };
    n.then((r3) => {
      if (o2) {
        s = r3;
        t2 = true;
      } else if (e.o?.Re === n && !(e.ue & REACTIVE_DISPOSED) && flattenIfIterable(r3, registerDeferredClose))
        ;
      else {
        asyncWrite(r3);
        settleAutodispose();
      }
    }, (e2) => {
      if (o2) {
        i2 = e2;
        r2 = true;
      } else {
        handleError(e2);
        settleAutodispose();
      }
    });
    o2 = false;
    if (r2) {
      handleError(i2);
      throw i2;
    } else if (!t2) {
      if (e.ge)
        return e.Qe;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else if (!flattenIfIterable(s)) {
      e.ge = false;
    }
  }
  if (r)
    flattenIfIterable(n);
  if (u !== null) {
    if (!u) {
      if (e.ge)
        return e.Qe;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.ge = false;
  }
  return s;
}
function clearStatus(e, n = false) {
  if (e.o?.ae)
    clearPendingSources(e);
  if (e.o?.Ie) {
    if (e.o !== null)
      e.o.Ie = false;
  }
  if (e.o !== null)
    e.o.be = false;
  e.S = n ? 0 : e.S & STATUS_UNINITIALIZED;
  if (e.o?._)
    setPendingError(e);
  if (e.o?.je || e.o?.xe)
    GlobalQueue.Oe(e);
  if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.Me !== null)
    GlobalQueue.Me(e);
  const t = statusNotifierOf(e);
  if (t)
    t.call(e);
}
function landStatus(e, n = false) {
  const t = e.o?.ae;
  if (t && (t.delete(e), t.size)) {
    e.o.Ie = false;
    if (n)
      e.S = STATUS_PENDING;
    setPendingError(e, t.values().next().value);
  } else
    clearStatus(e, n);
}
function notifyStatus(e, n, t, r, i) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const o = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const s = o === e;
  const u = n === STATUS_PENDING && e.o?.Ce !== undefined && !(e.T & CONFIG_DERIVED_OVERRIDE) && !s;
  const l = u && hasActiveOverride(e);
  if (!r) {
    if (i)
      assignOrMergeLane(e, i);
    if (n === STATUS_PENDING && o) {
      addPendingSource(e, o);
      if (!(e.S & STATUS_PENDING))
        e.T &= ~CONFIG_INPUTS_PUBLISHED;
      e.S = STATUS_PENDING | e.S & STATUS_UNINITIALIZED;
      setPendingError(e, o, t);
    } else {
      clearPendingSources(e);
      e.S = n | (n !== STATUS_ERROR ? e.S & STATUS_UNINITIALIZED : 0);
      ext(e)._ = t;
    }
    GlobalQueue.Oe?.(e);
    if (e.o?.i && e.T & CONFIG_CHILD_COMPANIONS && GlobalQueue.Me !== null)
      GlobalQueue.Me(e);
  }
  const f = r || l;
  const a = r || u ? undefined : i;
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
    e2.Pe = clock;
    if (n === STATUS_PENDING && r2.qe !== e2.Ze) {
      enqueueSub(e2);
      schedule();
      return;
    }
    if (n === STATUS_PENDING && o && !e2.o?.ae?.has(o) || n !== STATUS_PENDING && (e2.o?._ !== t || e2.o?.ae)) {
      if (r2.He && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f)
        e2.Ge ? o && !e2.Le && (e2.S & STATUS_PENDING || e2.ve !== NOT_PENDING) && globalQueue.initTransition(e2.Ge) : queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.We = (e) => {
  if (e.Le === EFFECT_TRACKED) {
    deleteFromHeap(e, queueFor(e));
    e.Ye = true;
    e.C.enqueue(EFFECT_USER, e.Ke);
  } else
    recompute(e);
};
GlobalQueue.Be = disposeChildren;
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
    if (e.Je)
      return true;
    e = e._parent;
  }
  return false;
}
function recompute(e, t = false) {
  bumpNotifyEpoch();
  const n = e.Le;
  if (!t) {
    if (e.Ge && !n && activeTransition !== e.Ge)
      globalQueue.initTransition(e.Ge);
    deleteFromHeap(e, queueFor(e));
    if (e.o !== null) {
      e.o.Re = null;
      releaseFlightTeardown(e);
    }
    if (n === EFFECT_TRACKED || e.T & CONFIG_HELD_CHILDREN)
      disposeChildren(e);
    else if (e.Xe !== null || e.ke !== null) {
      markDisposal(e);
      const t2 = ext(e);
      t2.it = e.ke;
      t2.lt = e.Xe;
      e.ke = null;
      e.Xe = null;
      e.ut = 0;
    }
  }
  let i = !!(e.ue & REACTIVE_OPTIMISTIC_DIRTY);
  const l = (e.T & (CONFIG_OPTIMISTIC | CONFIG_DERIVED_OVERRIDE)) !== 0 && e.o?.Ce !== NOT_PENDING && e.o?.Ce !== undefined;
  const u = !!(e.S & STATUS_UNINITIALIZED);
  const o = e.S & STATUS_ERROR ? e.o?._ : undefined;
  const s = (e.S & STATUS_PENDING) !== 0;
  const r = s ? e.o?.ae : undefined;
  const a = e.o?.ae?.has(e);
  const c = (e.ue & REACTIVE_REASK) !== 0;
  const f = e.ge;
  const _ = stagedEntry;
  stagedEntry = null;
  const N = context;
  context = e;
  e.ot = null;
  e.Ze++;
  e.ue = REACTIVE_RECOMPUTING_DEPS;
  e.Pe = clock;
  let d = e.ve === NOT_PENDING ? e.Qe : e.ve;
  let E = e.tt;
  let I = false;
  let T = tracking;
  let S = currentOptimisticLane;
  tracking = true;
  const O = latestReadActive;
  latestReadActive = false;
  if (!n)
    currentOptimisticLane = null;
  if (i) {
    const t2 = GlobalQueue.st(e, true);
    if (t2)
      currentOptimisticLane = t2;
    else if (t2 === false)
      i = false;
  } else if (e.T & CONFIG_DERIVED_OVERRIDE) {
    const t2 = GlobalQueue.st(e, true);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  } else if (activeTransition && !t && activeTransition.rt.length) {
    const t2 = GlobalQueue.st(e, false);
    if (t2) {
      i = true;
      currentOptimisticLane = t2;
    }
  }
  const C = n && n !== EFFECT_USER;
  const A = stale;
  if (C)
    stale = true;
  if (n && activeTransition !== null && activeTransition.ct.size)
    activeTransition.ct.delete(e);
  try {
    if (e.T & CONFIG_SYNC) {
      d = e.ce(d);
      if (e.o !== null)
        e.o.Re = null;
      e.ge = false;
    } else {
      const t2 = e.o?.Re;
      const n2 = e.ce(d);
      const i2 = typeof n2 === "object" && n2 !== null;
      const l2 = e.o?.Re !== t2;
      d = l2 || !i2 ? n2 : handleAsync(e, n2);
      if (!l2 && !i2) {
        if (e.o !== null)
          e.o.Re = null;
        e.ge = false;
      }
    }
    if (e.S !== 0 || e.o !== null)
      clearStatus(e, t && stagedEntry === null);
    if (e.T & CONFIG_HAS_LANE && e.o?.Ue)
      GlobalQueue.ft(e);
  } catch (t2) {
    const n2 = t2 instanceof NotReadyError;
    if (n2 && e.ge) {
      parkLoadingWindow(e, t2);
    } else {
      if (n2 && currentOptimisticLane)
        GlobalQueue._t(e);
      let i2 = false;
      if (n2) {
        ext(e).Ie = true;
        if (GlobalQueue.Nt !== null)
          i2 = GlobalQueue.Nt(e, c);
      }
      notifyStatus(e, n2 ? STATUS_PENDING : STATUS_ERROR, t2, undefined, n2 ? e.o?.Ue : undefined);
      if (n2 && a && !e.o?.Re)
        settlePendingSource(e);
      if (n2 && r) {
        for (const t3 of r)
          if (t3 !== e && !e.o?.ae?.has(t3))
            settlePendingSource(e, t3);
      }
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = T;
    latestReadActive = O;
    if (C)
      stale = A;
    I = (e.ue & REACTIVE_MISSED_WAKE) !== 0;
    e.ue = REACTIVE_NONE | (t ? e.ue & REACTIVE_SNAPSHOT_STALE : 0);
    context = N;
  }
  const p = stagedEntry;
  stagedEntry = _;
  if (!e.o?._) {
    const s2 = l ? unwrapOverride(e.o?.Ce) : i || e.ve === NOT_PENDING ? e.Qe : e.ve;
    let c2 = false;
    try {
      c2 = !n && u || !e.Fe || !e.Fe(s2, d);
    } catch (t2) {
      notifyStatus(e, STATUS_ERROR, t2);
    }
    if (n && c2) {
      e.Ye = !e.o?._;
      if (!t) {
        e.C.enqueue(n, e.dt ??= GlobalQueue.Et.bind(null, e));
        let t2 = e.It;
        if (t2 !== activeTransition) {
          e.It = activeTransition;
          if (t2 !== null && (t2 = currentTransition(t2)) !== activeTransition && !t2.Tt) {
            (t2.St ??= []).push(e);
            if (activeTransition !== null)
              (activeTransition.St ??= []).push(e);
          }
        }
      }
    }
    if (e.o?._)
      ;
    else if (c2) {
      const u2 = l ? e.o?.Ce : undefined;
      if (t && p === null || n && p === null && (activeTransition !== e.Ge || activeTransition === null || e.T & CONFIG_DIRECT_COMMIT) || i) {
        if (i && !n && currentOptimisticLane !== null)
          GlobalQueue.Ve(e, d, currentOptimisticLane);
        else
          e.Qe = d;
        if (i)
          e.ve = NOT_PENDING;
      } else {
        e.ve = d;
        if (p !== null) {
          e.Ge = p;
          p.Ot.push(e);
          if (n)
            p.ct.add(e);
        }
        if (f)
          e.ge = true;
        if (e.T & CONFIG_HAS_COMPANIONS && GlobalQueue.me !== null)
          GlobalQueue.me(e, d);
      }
      if (e.u !== null && (!l || i || e.o?.Ce !== u2))
        insertSubs(e, i || l);
      else if (l && !i && e.o.Ct !== clock)
        GlobalQueue.we(e, d);
    } else if (l) {
      if (e.ve === NOT_PENDING)
        queuePendingNode(e);
      e.ve = d;
      if (f)
        e.ge = true;
      GlobalQueue.we(e, d);
    } else if (e.tt != E) {
      for (let t2 = e.u;t2 !== null; t2 = t2.Ne) {
        insertIntoHeapHeight(t2._e, queueFor(t2._e));
      }
    }
    if (!c2 && !e.o?._) {
      if (o !== undefined)
        settleErroredDependents(e, o);
      if (r) {
        for (const t2 of r)
          if (t2 !== e)
            settlePendingSource(e, t2);
      }
    }
    if (a && !(e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)))
      settlePendingSource(e);
  }
  const R = e.ot;
  if (n && (s && !(e.S & STATUS_PENDING) || (R === null ? e.Se !== null : R.de !== null)))
    wakeParked();
  if (!e.o?._ && e.ve === NOT_PENDING && !(n && e.Ye)) {
    if (t || i || n === EFFECT_TRACKED)
      trimStaleDeps(e);
    else if (e.ot?.de ?? e.Se)
      heldTrims.push(e);
  }
  currentOptimisticLane = S;
  const G = e.ve !== NOT_PENDING || e.o !== null && (e.o.lt !== null || e.o.it !== null) || (e.S & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  let D = G && (!t || p !== null || (e.S & STATUS_PENDING) !== 0);
  if (D && (!e.Ge || l))
    queuePendingNode(e);
  else if (D && activeTransition === null && !(e.S & (STATUS_PENDING | STATUS_UNINITIALIZED))) {
    D = false;
    disposeChildren(e, false, true);
  }
  if (D)
    e.T |= CONFIG_HELD_CHILDREN;
  else
    e.T &= ~CONFIG_HELD_CHILDREN;
  if (e.Ge && n && activeTransition !== e.Ge && p === null) {
    const t2 = e.It;
    runInTransition(e.Ge, () => recompute(e));
    e.It = t2;
  }
  if (I) {
    enqueueSub(e);
    schedule();
  }
}
function updateIfNecessary(e) {
  if (e.ue & (REACTIVE_RECOMPUTING_DEPS | REACTIVE_DISPOSED))
    return;
  if (e.ue & REACTIVE_CHECK) {
    for (let t = e.Se;t; t = t.de) {
      const n = t.Ee;
      const i = n.Te || n;
      if (i.ce) {
        updateIfNecessary(i);
      }
      if (e.ue & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.ue & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.o?._ && e.Pe < clock && !e.o?.Re) {
    recompute(e);
  }
  e.ue = e.ue & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = t !== null && typeof t === "object" && "loadingValue" in t;
  const l = {
    id: inheritId(t, n, context),
    T: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (t?.Z ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Fe: t?.equals ?? isEqual,
    ke: null,
    C: context?.C ?? globalQueue,
    ze: context?.ze ?? defaultContext,
    ut: 0,
    ce: e,
    Qe: i ? t.loadingValue : undefined,
    tt: 0,
    At: undefined,
    Rt: null,
    Se: null,
    ot: null,
    Ze: 0,
    u: null,
    Gt: null,
    _parent: context,
    $e: null,
    Dt: null,
    Xe: null,
    ue: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    S: i ? 0 : STATUS_UNINITIALIZED,
    Pe: clock,
    ve: NOT_PENDING,
    Ge: null,
    ht: -1,
    ge: i,
    o: null
  };
  if (t?.unobserved)
    ext(l).Pt = t.unobserved;
  setupComputedNode(l, t);
  return l;
}
function ext(e) {
  return e.o ??= {
    Ce: undefined,
    Ft: undefined,
    Ct: 0,
    gt: NOT_PENDING,
    vt: 0,
    Ue: undefined,
    je: undefined,
    xe: undefined,
    Ht: undefined,
    t: 0,
    Re: null,
    Ae: null,
    _: undefined,
    Ie: undefined,
    ae: undefined,
    h: undefined,
    be: false,
    i: null,
    Pt: undefined,
    nt: undefined,
    it: null,
    lt: null,
    bt: undefined
  };
}
function createEffectNode(e, t, n, i, l) {
  const u = l?.transparent ?? false;
  const o = {
    id: inheritId(l, u, context),
    T: (u ? CONFIG_TRANSPARENT : 0) | (l?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (l?.sync ? CONFIG_SYNC : 0) | (l?.kt ?? 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    Fe: false,
    ke: null,
    C: context?.C ?? globalQueue,
    ze: context?.ze ?? defaultContext,
    ut: 0,
    ce: e,
    Qe: undefined,
    tt: 0,
    At: undefined,
    Rt: null,
    Se: null,
    ot: null,
    Ze: 0,
    u: null,
    Gt: null,
    _parent: context,
    $e: null,
    Dt: null,
    Xe: null,
    ue: REACTIVE_LAZY,
    S: STATUS_UNINITIALIZED,
    Pe: clock,
    ve: NOT_PENDING,
    Ge: null,
    ht: -1,
    ge: false,
    Ye: false,
    Ut: undefined,
    Lt: t,
    Vt: n,
    yt: undefined,
    Le: i,
    It: null,
    o: null
  };
  if (l?.unobserved)
    ext(o).Pt = l.unobserved;
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
  return e.Le ? effectStatusNotify ?? undefined : undefined;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, t) {
  e.Rt = e;
  const n = context?.xt ? context.Qt : context;
  if (context) {
    const t2 = context.Xe;
    if (t2 === null) {
      context.Xe = e;
    } else {
      e.$e = t2;
      t2.Dt = e;
      context.Xe = e;
    }
  }
  if (n)
    e.tt = n.tt + 1;
  if (GlobalQueue.wt !== null)
    GlobalQueue.wt(e);
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.S & STATUS_PENDING) && !(e.T & CONFIG_NO_SNAPSHOT)) {
      ext(e).nt = e.Qe === undefined ? NO_SNAPSHOT : e.Qe;
      e.T |= CONFIG_HAS_SNAPSHOT;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    Fe: t?.equals ?? isEqual,
    T: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.Z ? CONFIG_NO_SNAPSHOT : 0),
    Qe: e,
    u: null,
    Gt: null,
    Pe: clock,
    Te: n,
    De: n?.o?.i || null,
    Mt: null,
    ve: NOT_PENDING,
    Ge: null,
    ht: -1,
    o: null
  };
  if (t?.unobserved)
    ext(i).Pt = t.unobserved;
  if (n)
    linkFirewallChild(n, i);
  if (snapshotCaptureActive && !(i.T & CONFIG_NO_SNAPSHOT) && !((n?.S ?? 0) & STATUS_PENDING)) {
    ext(i).nt = e === undefined ? NO_SNAPSHOT : e;
    i.T |= CONFIG_HAS_SNAPSHOT;
    snapshotSources.add(i);
  }
  return i;
}
var slotUnobservedHook;
function linkFirewallChild(e, t) {
  const n = t.De;
  if (n !== null)
    n.Mt = t;
  ext(e).i = t;
  e.T |= CONFIG_FW_CHILDREN;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (GlobalQueue.Yt === null && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (GlobalQueue.Yt !== null)
      return GlobalQueue.Yt(e);
    return e();
  } finally {
    tracking = n;
  }
}
var spectating = false;
function spectate(e) {
  const t = spectating;
  spectating = true;
  try {
    return untrack(e);
  } finally {
    spectating = t;
  }
}
function prepareComputed(e, t) {
  if (e.ue & REACTIVE_LAZY) {
    e.ue &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.ue & REACTIVE_DISPOSED) {
    if (e.T & CONFIG_AUTO_DISPOSE)
      recompute(e, true);
  } else if (t) {
    updateIfNecessary(e);
  }
}
var READ_SLOW = Symbol("read-slow");
function recordStaleReplay(e, t) {
  const n = t.It;
  if (n == null || currentTransition(n) !== e)
    e.ct.add(t);
}
function ownsHold(e) {
  return activeTransition !== null && currentTransition(e) === currentTransition(activeTransition);
}
function heldFromStale(e, t) {
  const n = e.Ge;
  if (n === null || ownsHold(n))
    return false;
  const i = currentTransition(n);
  recordStaleReplay(i, t);
  const l = i.oe.get(e);
  if (l)
    l.add(t);
  else if (e.S & STATUS_PENDING)
    runInTransition(i, () => t.C.notify(t, STATUS_PENDING, STATUS_PENDING, e.o._));
  return true;
}
var stagedEntry = null;
function enterStagedRead(e, t = e.Ge) {
  if (!t || t === activeTransition || pendingCheckActive)
    return;
  if (e?.o?.Ht || context?.o?.Ht)
    return;
  const n = context;
  if (activeTransition === null && !globalQueue.Kt) {
    if (GlobalQueue.jt)
      return;
    if (n.ue & REACTIVE_RECOMPUTING_DEPS && !(n.T & CONFIG_OPTIMISTIC) && (stagedEntry === null || stagedEntry === t)) {
      stagedEntry = t;
      return;
    }
  }
  globalQueue.initTransition(t);
}
function readerSeesCommitted(e, t, n, i) {
  return !!(!t || currentOptimisticLane !== null && GlobalQueue.Bt(e, n, t) || e.ve === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && !i && heldFromStale(e, t) || e.T & CONFIG_HELD_TRUTH && !latestReadActive && !(t.T & CONFIG_AUTHORITATIVE_READ));
}
var unflushedStaged = false;
function markUnflushedStaged() {
  unflushedStaged = true;
}
function unflushedValue(e, t = e.Qe) {
  if (globalQueue.Kt || e.ve === NOT_PENDING || e.T & CONFIG_PROMOTED || e.o?.Ht)
    return NOT_PENDING;
  if (e.Ge === null || e.T & CONFIG_ADOPTED_UNFLUSHED)
    return t;
  return e.o === null ? NOT_PENDING : e.o.gt;
}
var unflushedRewrites = [];
var promotedWrites = [];
function unflushedOverride(e) {
  return !globalQueue.Kt && e.o?.Ct === clock && !e.o?.Ht;
}
function hasActiveOverride(e) {
  const t = e.o;
  return t !== null && t.Ce !== undefined && t.Ce !== NOT_PENDING;
}
function visibleOverride(e) {
  return hasActiveOverride(e) && !unflushedOverride(e);
}
function markLateLinker(e) {
  e.ue |= REACTIVE_MISSED_WAKE;
  return true;
}
var unflushedCompanions = [];
function resyncUnflushedCompanions() {
  unflushedStaged = false;
  if (unflushedRewrites.length !== 0) {
    for (const e of unflushedRewrites)
      e.o.gt = NOT_PENDING;
    unflushedRewrites.length = 0;
  }
  if (promotedWrites.length !== 0) {
    for (const e of promotedWrites)
      e.T &= ~CONFIG_PROMOTED;
    promotedWrites.length = 0;
  }
  if (unflushedCompanions.length !== 0) {
    for (const e of unflushedCompanions)
      GlobalQueue.me(e, e.ve !== NOT_PENDING ? e.ve : e.Qe);
    unflushedCompanions.length = 0;
  }
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.zt(e);
  let t = context;
  if (t?.xt)
    t = t.Qt;
  const n = e;
  const i = e.Te;
  const l = i || e;
  if (pendingCheckActive) {
    GlobalQueue.Jt(e, t, l, i);
  } else if (typeof n.ce === "function") {
    prepareComputed(e, false);
  }
  if (!n.ce && l === e && e.o?.Ce === undefined && e.o?.nt === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && (!unflushedStaged || e.ve === NOT_PENDING) && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.ve === NOT_PENDING || t.T & CONFIG_CHILDREN_FORBIDDEN || stale && heldFromStale(e, t) ? e.Qe : (enterStagedRead(e), e.ve);
  }
  if (t && tracking) {
    link(e, t, pendingCheckActive);
    if (l.ce) {
      const n2 = queueFor(e);
      if (l.tt >= n2.et) {
        markNode(t);
        markHeap(n2);
        updateIfNecessary(l);
      } else if (t.T & CONFIG_FRESH_READ)
        updateIfNecessary(l);
      const i2 = l.tt;
      if (i2 >= t.tt && e._parent !== t) {
        t.tt = i2 + 1;
      }
    }
  }
  if (l.S & STATUS_PENDING) {
    if (t && !(stale && !(l.S & STATUS_UNINITIALIZED) && !(l.T & CONFIG_INPUTS_PUBLISHED) && !(l.T & CONFIG_HAS_LANE && GlobalQueue.Xt(l)) && heldFromStale(l, t))) {
      if (currentOptimisticLane === null || GlobalQueue.$t(l)) {
        if (!tracking && !spectating && e !== t)
          link(e, t);
        throw l.o?._;
      }
    } else if (!t && l.S & STATUS_UNINITIALIZED) {
      throw l.o?._;
    }
  }
  if (l.ce && l.S & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && l.Pe < clock) {
      recompute(l);
      return read(e);
    } else
      throw l.o?._;
  }
  if (snapshotCaptureActive && t && t.T & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.o?.nt;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const l2 = e.ve !== NOT_PENDING ? e.ve : e.Qe;
      if (l2 !== i2)
        t.ue |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  const u = serve(e, t, l, e.Qe);
  if (!t && l === e && typeof n.ce === "function" && e.T & CONFIG_AUTO_DISPOSE && !(l.S & STATUS_PENDING) && !e.u && !visibleOverride(e)) {
    dormantNodes.add(e);
    schedule();
  }
  return u;
}
function serve(e, t, n, i) {
  if (hasActiveOverride(e)) {
    if (!(t && t.T & CONFIG_AUTHORITATIVE_READ) && !unflushedOverride(e)) {
      if (t && e.T & (CONFIG_HAS_LANE | CONFIG_OVERRIDE_SUPERSEDED))
        return GlobalQueue.en(e, t);
      return unwrapOverride(e.o?.Ce);
    }
    e.T |= CONFIG_AUTHORITATIVE_OBSERVED;
  }
  if (currentOptimisticLane !== null && activeTransition !== null && t !== null && GlobalQueue.tn(e, n, t)) {
    return i;
  }
  const l = e.ve !== NOT_PENDING && (e.S & STATUS_UNINITIALIZED) !== 0;
  if (l && !t)
    throw new NotReadyError(null);
  const u = t && unflushedStaged ? unflushedValue(e, i) : NOT_PENDING;
  if (u !== NOT_PENDING) {
    markLateLinker(t);
    if (pendingCheckActive)
      GlobalQueue.nn(e, u);
    return u;
  }
  const o = readerSeesCommitted(e, t, n, l) ? i : (enterStagedRead(e), e.ve);
  if (pendingCheckActive)
    GlobalQueue.nn(e, o);
  return o;
}
function stashHeldRewrite(e) {
  if (globalQueue.Kt)
    return;
  const t = ext(e);
  if (t.gt === NOT_PENDING) {
    t.gt = e.ve;
    unflushedRewrites.push(e);
    unflushedStaged = true;
  }
}
function notePromotedWrite(e) {
  if (globalQueue.Kt || e.T & CONFIG_PROMOTED)
    return;
  e.T |= CONFIG_PROMOTED;
  promotedWrites.push(e);
}
function setSignal(e, t) {
  if (e.Ge && activeTransition !== e.Ge) {
    if (globalQueue.Kt)
      globalQueue.initTransition(e.Ge);
    else {
      batchJoins.push(e.Ge);
      schedule();
    }
  }
  if (e.T & CONFIG_OPTIMISTIC) {
    if (!projectionWriteActive)
      return GlobalQueue.ln(e, t);
    const n2 = e.o?.Ce;
    if (n2 !== undefined && n2 !== NOT_PENDING)
      return GlobalQueue.un(e, t);
  }
  const n = e.ve === NOT_PENDING ? e.Qe : e.ve;
  if (typeof t === "function")
    t = t(n);
  const i = !!(e.S & STATUS_UNINITIALIZED) || !e.Fe || !e.Fe(n, t);
  if (!i)
    return t;
  const l = e.ve !== NOT_PENDING;
  if (!l)
    queuePendingNode(e);
  else if (e.Ge !== null)
    stashHeldRewrite(e);
  e.ve = t;
  if (context !== null)
    notePromotedWrite(e);
  if (e.T & CONFIG_HAS_COMPANIONS && GlobalQueue.me !== null) {
    GlobalQueue.me(e, t);
    if (!globalQueue.Kt)
      unflushedCompanions.push(e);
  }
  if (e.ce !== undefined)
    e.Pe = clock;
  if (l && e.ht === notifyEpoch && currentOptimisticLane === null && !reaskArmed)
    return t;
  insertSubs(e);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.ue & REACTIVE_MANUAL_WRITE) && e.ve === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.ue = e.ue & -4 | REACTIVE_MANUAL_WRITE;
  e.sn = clock;
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/context.js
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
  let r = t.ze[e.id];
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
  r.ze = {
    ...r.ze,
    [e.id]: t === undefined ? e.defaultValue : t
  };
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, e, E, r) {
  const n = !!r?.user;
  const i = createEffectNode(t, e, E, n ? EFFECT_USER : EFFECT_RENDER, r);
  recompute(i, true);
  !r?.defer && i.ve === NOT_PENDING && (i.Le === EFFECT_USER || r?.schedule ? i.C.enqueue(i.Le, runEffect.bind(null, i)) : runEffect(i, LANE_RUN));
}
function notifyEffectStatus(t, e) {
  const E = t !== undefined ? t : this.S;
  const r = e !== undefined ? e : this.o?._;
  if (E & STATUS_ERROR) {
    this.C.notify(this, STATUS_PENDING, 0);
    if (this.Le === EFFECT_USER) {
      if (this.S & STATUS_ERROR) {
        this.Ye = true;
        this.C.enqueue(this.Le, this.dt ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.C.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(r));
      throw r;
    }
  } else if (this.Le === EFFECT_RENDER) {
    this.C.notify(this, STATUS_PENDING | STATUS_ERROR, E, r);
  }
}
function runEffect(t, e) {
  if (!t.Ye || t.ue & REACTIVE_DISPOSED)
    return;
  if (t.It !== null && !currentTransition(t.It).Tt && (e & LANE_RUN ? !t.o?.Ue : activeTransition !== null)) {
    t.C.enqueue(t.Le, t.dt);
    return;
  }
  if (t.S & STATUS_ERROR && t.Le === EFFECT_USER) {
    const e2 = unwrapStatusError(t.o?._);
    t.Ut = t.Qe;
    t.Ye = false;
    try {
      t.Vt ? t.Vt(e2, () => {
        const e3 = t.yt;
        t.yt = undefined;
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
  const E = t.o?._ == null;
  const r = t.yt;
  t.yt = undefined;
  try {
    r?.();
    const e2 = t.Lt(t.Qe, t.Ut);
    if (false)
      ;
    t.yt = e2;
  } catch (e2) {
    ext(t)._ = new StatusError(t, e2);
    t.S |= STATUS_ERROR;
    if (!t.C.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(e2);
      throw e2;
    }
  } finally {
    t.Ut = t.Qe;
    t.Ye = false;
    if (E)
      trimStaleDeps(t);
  }
}
GlobalQueue.Et = runEffect;
function trackedEffect(t, e) {
  const run = () => {
    if (!E.Ye || E.ue & REACTIVE_DISPOSED)
      return;
    try {
      E.Ye = false;
      recompute(E);
    } finally {}
  };
  const E = computed(() => {
    const e2 = E.yt;
    E.yt = undefined;
    e2?.();
    const r = staleValues(t);
    E.yt = r;
  }, {
    ...e,
    lazy: true
  });
  E.yt = undefined;
  E.T = E.T & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  E.Ye = true;
  E.Le = EFFECT_TRACKED;
  E.Ke = run;
  enqueueSub(E);
  schedule();
}
setEffectStatusNotify(notifyEffectStatus);

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/core/error-hooks.js
var ambientHook;
var reported = new WeakSet;
function hookFor(e) {
  for (let n = e;n; n = n._parent) {
    const e2 = n[ROOT_ERROR_HOOK];
    if (e2 !== undefined)
      return e2;
  }
  return ambientHook;
}
function labels(e) {
  const n = [];
  for (let o = e;o; o = o._parent) {
    const e2 = o._name;
    if (typeof e2 === "string" && e2.length)
      n.push(e2);
  }
  return n.length ? n.reverse() : undefined;
}
function reportClientError(e, n, o) {
  const r = e !== null && (typeof e === "object" || typeof e === "function");
  if (r) {
    if (reported.has(e))
      return;
    reported.add(e);
  }
  const t = hookFor(n);
  if (t === undefined)
    return;
  const i = {};
  const f = labels(n);
  const c = labels(o) ?? f;
  if (c !== undefined)
    i.ownerPath = c;
  if (f !== undefined)
    i.boundaryPath = f;
  try {
    t(e, i);
  } catch (e2) {
    console.error(e2);
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/signals.js
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
  t && !(t.T & CONFIG_CHILDREN_FORBIDDEN) ? trackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, function fire() {
    if (dirtyQueue.EE >= dirtyQueue.et)
      return globalQueue.enqueue(EFFECT_USER, fire);
    e();
  });
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/store/store.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/boundaries.js
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
  return isRevealController(e) ? e.O() : e.I.size === 0 && !e.v;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.U() : isSlotReady(e);
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
  V;
  q = [];
  j;
  D = signal(false, {
    ownedWrite: true,
    Z: true
  });
  P = signal(false, {
    ownedWrite: true,
    Z: true
  });
  H = true;
  J = true;
  K = false;
  constructor(e, t) {
    this.F = e;
    this.V = t;
  }
  X(e) {
    for (let t = 0;t < this.q.length; t++) {
      const r = this.q[t];
      if ((isRevealController(r) ? r.j : r.W) !== this)
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
    const e = untrack(this.F);
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
    const t = untrack(this.F);
    setSignal(e.D, true), setSignal(e.P, t === "sequential" ? !!untrack(this.V) : false);
    untrack(() => this.B());
  }
  $(e) {
    const t = this.q.indexOf(e);
    if (t >= 0)
      this.q.splice(t, 1);
    untrack(() => this.B());
  }
  B(e, t) {
    if (this.K)
      return;
    this.K = true;
    const r = this.H;
    const n = this.J;
    try {
      const r2 = e ?? read(this.D), n2 = untrack(this.F), s = n2 === "sequential" && !!untrack(this.V), i = t ?? s;
      if (r2) {
        this.X((e2) => setSlotState(e2, this, true, i));
      } else if (n2 === "natural") {
        this.X((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.P, false);
            setSignal(e2.D, false);
            e2.B(false, false);
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
            setSignal(t2.P, false);
            setSignal(t2.D, false);
            t2.B(false, false);
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
    if (this.j && (r !== this.H || n !== this.J))
      this.j.B();
  }
}

class CollectionQueue extends Queue {
  ee;
  I = new Set;
  te;
  v = true;
  D = signal(false, {
    ownedWrite: true,
    Z: true
  });
  _;
  P = signal(false, {
    ownedWrite: true,
    Z: true
  });
  W;
  L = false;
  re;
  ne = ON_INIT;
  se;
  constructor(e) {
    super();
    this.ee = e;
  }
  run(e) {
    if (!e || read(this.D) && (!_revealUsed || read(this.P)))
      return;
    return super.run(e);
  }
  ie() {
    return spectate(() => {
      try {
        return this.re();
      } catch {
        return ON_INIT;
      }
    });
  }
  notify(e, t, r, n) {
    if (!(t & this.ee))
      return super.notify(e, t, r, n);
    if (this.L && this.re) {
      const e2 = this.ie();
      if (e2 !== this.ne) {
        this.ne = e2;
        this.L = false;
        this.I.clear();
        for (const e3 of transitions)
          for (const [t2, r2] of e3.oe)
            for (const e4 of r2)
              if (this.le(e4)) {
                this.I.add(t2);
                e4.o?.ae?.forEach((e5) => this.I.add(e5));
              }
        if (this.I.size) {
          setSignal(this.D, true);
        }
        wakeParked();
      }
    }
    if (this.ee & STATUS_PENDING && this.L)
      return super.notify(e, t, r, n);
    if (r & this.ee) {
      this.v = true;
      const t2 = n?.source || e.o?._?.source;
      if (t2) {
        const r2 = this.I.size === 0;
        this.I.add(t2);
        if (this.ee & STATUS_PENDING)
          e.o?.ae?.forEach((e2) => this.I.add(e2));
        if (r2) {
          setSignal(this.D, true);
        }
        if (this.ee & STATUS_ERROR) {
          const e2 = unwrapStatusError(t2.o?._);
          setSignal(this._, e2);
          reportClientError(e2, this.se, t2);
        }
      }
    }
    t &= ~this.ee;
    return t ? super.notify(e, t, r, n) : true;
  }
  le(e) {
    if (e.ue & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
      return false;
    for (let t = e.C;t; t = t._parent) {
      if (t === this)
        return true;
      if (t.ee & STATUS_PENDING && !t.L)
        return false;
    }
    return false;
  }
  fe() {
    for (const e of this.I) {
      if (e.ue & REACTIVE_DISPOSED || !e.o?.t && !(e.S & this.ee) && !(this.ee & STATUS_ERROR && e.S & STATUS_PENDING))
        this.I.delete(e);
    }
    if (!this.I.size) {
      if (this.ee & STATUS_PENDING && this.v && !this.L && this.te) {
        this.v = !!(this.te.S & this.ee);
      } else {
        this.v = false;
      }
      if (!this.v) {
        setSignal(this.D, false);
        if (this.re) {
          const e = this.ie();
          if (e !== ON_INIT)
            this.ne = e;
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
  i.se = s;
  if (e === STATUS_ERROR)
    i._ = signal(undefined, {
      ownedWrite: true,
      Z: true
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
    i.v = t2 || !!(o.S & e) || o.o?._ instanceof NotReadyError;
  });
  const l = _revealUsed && e === STATUS_PENDING ? getContext(RevealControllerContext) : null;
  if (l) {
    i.W = l;
    l.Y(i);
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
    Z: true
  }));
}
function createErrorBoundary(e, t) {
  return createCollectionBoundary(STATUS_ERROR, e, (e2) => t(accessor(e2._), () => {
    for (const t2 of e2.I) {
      if (t2.ce !== undefined)
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.9/node_modules/@solidjs/signals/dist/prod/store/utils.js
function trueFn() {
  return true;
}
var SOURCE_PLAIN = 0;
var SOURCE_OMIT = 1;
var SOURCE_PROXY = 2;
var SOURCE_MEMO = 3;
var SOURCE_MERGE = 4;
var EMPTY = Object.freeze({});
function leafOf(e, n) {
  return n === SOURCE_MEMO ? (e = e()) == null ? EMPTY : e : e;
}
var $SOURCES = Symbol(0);
var $OMIT = Symbol(0);
var $VIEW = Symbol(0);

class OmitView {
  source;
  kind;
  hidden;
  table = 0;
  keys = undefined;
  descs = undefined;
  constructor(e, n, t) {
    this.source = e;
    this.kind = n;
    this.hidden = t;
  }
}
function isHidden(e, n) {
  const t = e.hidden;
  return typeof t === "function" ? t(n) : t.includes(n);
}
function viewSource(e) {
  return leafOf(e.source, e.kind);
}
function isView(e) {
  return e[$TARGET] === undefined && (e[$OMIT] !== undefined || e[$VIEW] !== undefined);
}
function leafKeys(e, n) {
  if (n === SOURCE_PLAIN)
    return Object.keys(e);
  if (n === SOURCE_PROXY || e[$PROXY] === e)
    return Reflect.ownKeys(e);
  return Object.keys(e);
}
function sourceKeys(e, n) {
  if (n === SOURCE_OMIT) {
    if (e.kind === SOURCE_MERGE)
      return mergeKeysOf(e.source, false, e);
    const n2 = leafKeys(viewSource(e), e.kind);
    const t = [];
    for (let r = 0;r < n2.length; r++)
      if (!isHidden(e, n2[r]))
        t.push(n2[r]);
    return t;
  }
  return leafKeys(leafOf(e, n), n);
}
function sourceHas(e, n, t) {
  if (n === SOURCE_OMIT) {
    if (isHidden(e, t))
      return false;
    return e.kind === SOURCE_MERGE ? mergeHas(e.source, t) : (t in viewSource(e));
  }
  return t in leafOf(e, n);
}
function sourceGet(e, n, t) {
  if (n === SOURCE_OMIT) {
    if (isHidden(e, t))
      return;
    return e.kind === SOURCE_MERGE ? mergeGet(e.source, t) : viewSource(e)[t];
  }
  return leafOf(e, n)[t];
}
function entryHasStaticKeys(e, n) {
  if (n === SOURCE_PLAIN)
    return true;
  if (n !== SOURCE_OMIT)
    return false;
  if (e.kind === SOURCE_PLAIN)
    return true;
  return e.kind === SOURCE_MERGE && mergeHasStaticKeys(e.source);
}
function mergeHasStaticKeys(e) {
  const { sources: n, kinds: t } = e;
  for (let e2 = 0;e2 < n.length; e2++)
    if (!entryHasStaticKeys(n[e2], t[e2]))
      return false;
  return true;
}
function hasStaticKeys(e) {
  if (!($PROXY in e))
    return true;
  if (e[$TARGET] !== undefined)
    return false;
  const n = e[$VIEW];
  if (n !== undefined)
    return mergeHasStaticKeys(n);
  const t = e[$OMIT];
  return t !== undefined && entryHasStaticKeys(t, SOURCE_OMIT);
}
function accessorDescriptor(e, n = true) {
  return {
    configurable: true,
    enumerable: n,
    get: e,
    set: trueFn
  };
}
function sourceDescriptor(e, n, t, r = false) {
  if (n === SOURCE_OMIT) {
    if (isHidden(e, t))
      return;
    return sourceDescriptor(e.source, e.kind, t, r);
  }
  if (n === SOURCE_MERGE)
    return mergeDescriptor(e, t);
  if (n === SOURCE_MEMO) {
    return r || t in leafOf(e, n) ? accessorDescriptor(() => leafOf(e, n)[t]) : undefined;
  }
  if (n === SOURCE_PROXY) {
    if (isView(e))
      return Reflect.getOwnPropertyDescriptor(e, t);
    return r || t in e ? accessorDescriptor(() => e[t]) : undefined;
  }
  const i = Reflect.getOwnPropertyDescriptor(e, t);
  if (i === undefined)
    return;
  if (i.get !== undefined || i.set !== undefined)
    return accessorDescriptor(() => e[t], i.enumerable);
  if (i.configurable)
    return i;
  return {
    configurable: true,
    enumerable: i.enumerable,
    writable: true,
    value: i.value
  };
}
class MergeView {
  sources;
  kinds;
  table = 0;
  keys = undefined;
  descs = undefined;
  constructor(e, n) {
    this.sources = e;
    this.kinds = n;
  }
}
function viewOf(e) {
  if (e == null || !($PROXY in e) || e[$TARGET] !== undefined)
    return;
  const n = e[$VIEW];
  return n !== undefined ? n : e[$OMIT];
}
function resolvedTable(e) {
  if (e == null || !($PROXY in e) || e[$TARGET] !== undefined)
    return;
  const n = e[$OMIT];
  if (n !== undefined)
    return omitTable(n);
  const t = e[$VIEW];
  return t === undefined ? undefined : mergeTable(t);
}
function mergeTable(e) {
  let n = e.table;
  if (typeof n !== "object") {
    const { sources: t, kinds: r } = e;
    for (let n2 = 0;n2 < t.length; n2++) {
      if (!entryHasStaticKeys(t[n2], r[n2])) {
        e.table = null;
        return;
      }
    }
    n = new Map;
    collectTable(n, e, undefined);
    e.table = n;
  }
  return n === null ? undefined : n;
}
function collectTable(e, n, t) {
  const { sources: r, kinds: i } = n;
  for (let n2 = 0;n2 < r.length; n2++) {
    const f = r[n2];
    if (i[n2] === SOURCE_OMIT) {
      if (f.kind === SOURCE_MERGE) {
        if (t === undefined)
          t = [f];
        else
          t.push(f);
        collectTable(e, f.source, t);
        t.pop();
        continue;
      }
      const n3 = f.source;
      const r2 = Reflect.ownKeys(n3);
      for (let i2 = 0;i2 < r2.length; i2++) {
        const u = r2[i2];
        if (!isHidden(f, u) && !hiddenByAny(t, u))
          tableSet(e, u, n3);
      }
    } else {
      const n3 = Reflect.ownKeys(f);
      for (let r2 = 0;r2 < n3.length; r2++) {
        const i2 = n3[r2];
        if (!hiddenByAny(t, i2))
          tableSet(e, i2, f);
      }
    }
  }
}
function hiddenByAny(e, n) {
  if (e !== undefined) {
    for (let t = e.length - 1;t >= 0; t--)
      if (isHidden(e[t], n))
        return true;
  }
  return false;
}
function tableSet(e, n, t) {
  if (e.has(n))
    e.delete(n);
  e.set(n, t);
}
function omitTable(e) {
  let n = e.table;
  if (typeof n !== "object") {
    const t = e.source;
    if (e.kind === SOURCE_MERGE) {
      if (!mergeHasStaticKeys(t)) {
        e.table = null;
        return;
      }
      n = new Map;
      collectTable(n, t, [e]);
    } else if (e.kind === SOURCE_PLAIN) {
      n = new Map;
      const r = Reflect.ownKeys(t);
      for (let i = 0;i < r.length; i++) {
        const f = r[i];
        if (!isHidden(e, f))
          n.set(f, t);
      }
    } else {
      e.table = null;
      return;
    }
    e.table = n;
  }
  return n === null ? undefined : n;
}
var propertyIsEnumerable = Object.prototype.propertyIsEnumerable;
function tableOwnKeys(e, n) {
  let t = e.keys;
  if (t === undefined) {
    t = e.keys = [];
    for (const [e2, r] of n)
      if (propertyIsEnumerable.call(r, e2))
        t.push(e2);
  }
  return t;
}
function tableDescriptor(e, n, t) {
  const r = n.get(t);
  if (r === undefined)
    return;
  let i = e.descs;
  if (i === undefined)
    i = e.descs = new Map;
  let f = i.get(t);
  if (f === undefined) {
    f = sourceDescriptor(r, SOURCE_PLAIN, t);
    if (f === undefined)
      return;
    i.set(t, f);
    return f;
  }
  if (f.get !== undefined)
    return f;
  return {
    configurable: true,
    enumerable: f.enumerable,
    writable: f.writable,
    value: r[t]
  };
}
var READS_FOR_TABLE = 16;
function mergeReadTable(e) {
  const n = e.table;
  if (typeof n === "object")
    return n === null ? undefined : n;
  if (n + 1 < READS_FOR_TABLE) {
    e.table = n + 1;
    return;
  }
  return mergeTable(e);
}
function tableOf(e) {
  const n = e.table;
  return typeof n === "object" && n !== null ? n : undefined;
}
var MISSING = Symbol();
function mergeLookup(e, n) {
  const t = tableOf(e);
  if (t !== undefined) {
    const e2 = t.get(n);
    return e2 === undefined ? MISSING : e2[n];
  }
  const { sources: r, kinds: i } = e;
  for (let e2 = r.length - 1;e2 >= 0; e2--) {
    const t2 = i[e2];
    if (t2 === SOURCE_OMIT) {
      const t3 = r[e2];
      if (isHidden(t3, n))
        continue;
      if (t3.kind === SOURCE_MERGE) {
        const e3 = mergeLookup(t3.source, n);
        if (e3 !== MISSING)
          return e3;
        continue;
      }
      const i2 = viewSource(t3);
      if (n in i2)
        return i2[n];
    } else {
      const i2 = leafOf(r[e2], t2);
      if (n in i2)
        return i2[n];
    }
  }
  return MISSING;
}
function mergeGet(e, n) {
  const t = mergeLookup(e, n);
  return t === MISSING ? undefined : t;
}
function mergeHas(e, n) {
  const t = tableOf(e);
  if (t !== undefined)
    return t.has(n);
  const { sources: r, kinds: i } = e;
  for (let e2 = r.length - 1;e2 >= 0; e2--)
    if (sourceHas(r[e2], i[e2], n))
      return true;
  return false;
}
function mergeDescriptor(e, n) {
  const t = tableOf(e);
  if (t !== undefined)
    return tableDescriptor(e, t, n);
  const { sources: r, kinds: i } = e;
  for (let t2 = r.length - 1;t2 >= 0; t2--) {
    if (!sourceHas(r[t2], i[t2], n))
      continue;
    return sourceDescriptor(r[t2], i[t2], n, true) ?? accessorDescriptor(() => mergeGet(e, n));
  }
  return;
}
function mergeKeysOf(e, n, t) {
  const r = [];
  collectKeys(e, t === undefined ? undefined : [t], n, r, null);
  return r;
}
function collectKeys(e, n, t, r, i) {
  const { sources: f, kinds: u } = e;
  for (let e2 = 0;e2 < f.length; e2++) {
    let o = f[e2], c = u[e2];
    let s;
    if (c === SOURCE_OMIT) {
      if (o.kind === SOURCE_MERGE) {
        if (n === undefined)
          n = [o];
        else
          n.push(o);
        collectKeys(o.source, n, t, r, i);
        n.pop();
        continue;
      }
      s = o;
      c = o.kind;
      o = o.source;
    }
    o = leafOf(o, c);
    const d = t ? ownEnumerableKeys(o) : leafKeys(o, c);
    for (let e3 = 0;e3 < d.length; e3++) {
      const t2 = d[e3];
      if (s !== undefined && isHidden(s, t2))
        continue;
      if (hiddenByAny(n, t2))
        continue;
      addKey(r, i, t2, o);
    }
  }
}
function addKey(e, n, t, r) {
  const i = e.indexOf(t);
  if (i !== -1) {
    e.splice(i, 1);
    if (n !== null)
      n.splice(i, 1);
  }
  e.push(t);
  if (n !== null)
    n.push(r);
}
function mergeEnumerableKeys(e, n) {
  const t = mergeTable(e);
  if (t === undefined)
    return mergeKeysOf(e, true, n);
  const r = tableOwnKeys(e, t);
  if (n === undefined)
    return r;
  const i = [];
  for (let e2 = 0;e2 < r.length; e2++)
    if (!isHidden(n, r[e2]))
      i.push(r[e2]);
  return i;
}
var mergeTraps = {
  get(e, n, t) {
    if (n === $PROXY)
      return t;
    if (n === $TARGET || n === $OMIT)
      return;
    if (n === $SOURCES)
      return e.sources;
    if (n === $VIEW)
      return e;
    const r = mergeReadTable(e);
    if (r !== undefined) {
      const e2 = r.get(n);
      return e2 === undefined ? undefined : e2[n];
    }
    return mergeGet(e, n);
  },
  has(e, n) {
    if (n === $PROXY)
      return true;
    if (n === $TARGET || n === $OMIT || n === $SOURCES || n === $VIEW)
      return false;
    const t = mergeReadTable(e);
    if (t !== undefined)
      return t.has(n);
    return mergeHas(e, n);
  },
  set: trueFn,
  deleteProperty: trueFn,
  getOwnPropertyDescriptor(e, n) {
    if (n === $PROXY || n === $TARGET || n === $OMIT || n === $SOURCES || n === $VIEW)
      return;
    const t = mergeReadTable(e);
    if (t !== undefined)
      return tableDescriptor(e, t, n);
    return mergeDescriptor(e, n);
  },
  ownKeys(e) {
    return mergeEnumerableKeys(e);
  }
};
function merge(...e) {
  if (e.length === 1 && typeof e[0] !== "function")
    return e[0];
  const n = [];
  const t = [];
  let r = undefined;
  let i = 0;
  for (let f2 = 0;f2 < e.length; f2++) {
    const u2 = e[f2];
    if (!u2)
      continue;
    i++;
    r = u2;
    if (typeof u2 === "function") {
      n.push(createMemo(u2));
      t.push(SOURCE_MEMO);
      continue;
    }
    if ($PROXY in u2) {
      if (u2[$TARGET] === undefined) {
        const e2 = u2[$VIEW];
        if (e2 !== undefined) {
          for (let r3 = 0;r3 < e2.sources.length; r3++) {
            n.push(e2.sources[r3]);
            t.push(e2.kinds[r3]);
          }
          continue;
        }
        const r2 = u2[$OMIT];
        if (r2 !== undefined) {
          n.push(r2);
          t.push(SOURCE_OMIT);
          continue;
        }
      }
      n.push(u2);
      t.push(SOURCE_PROXY);
      continue;
    }
    n.push(u2);
    t.push(SOURCE_PLAIN);
  }
  if (SUPPORTS_PROXY) {
    if (i === 1 && typeof r !== "function")
      return r;
    return new Proxy(new MergeView(n, t), mergeTraps);
  }
  const f = Object.create(null);
  let u = false;
  let o = n.length - 1;
  for (let e2 = o;e2 >= 0; e2--) {
    const t2 = n[e2];
    if (!t2) {
      e2 === o && o--;
      continue;
    }
    const r2 = Object.getOwnPropertyNames(t2);
    for (let n2 = r2.length - 1;n2 >= 0; n2--) {
      const i2 = r2[n2];
      if (i2 === "__proto__" || i2 === "constructor")
        continue;
      if (!f[i2]) {
        u = u || e2 !== o;
        const n3 = Object.getOwnPropertyDescriptor(t2, i2);
        f[i2] = n3.get ? {
          enumerable: true,
          configurable: true,
          get: n3.get.bind(t2)
        } : n3;
      }
    }
  }
  if (!u)
    return n[o];
  const c = {};
  const s = Object.keys(f);
  for (let e2 = s.length - 1;e2 >= 0; e2--) {
    const n2 = s[e2], t2 = f[n2];
    if (t2.get)
      Object.defineProperty(c, n2, t2);
    else
      c[n2] = t2.value;
  }
  return c;
}
// ../../node_modules/.bun/solid-js@2.0.0-rc.9/node_modules/solid-js/dist/solid.js
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createRoot;
var _createMemo;
var _createRenderEffect;
var latchedOnce = new WeakSet;
var LIVE_SOURCE = Symbol.for("solid.LiveSource");
var createMemo2 = (...args) => {
  return (_createMemo || createMemo)(...args);
};
var createRoot2 = (...args) => (_createRoot || createRoot)(...args);
var createRenderEffect2 = (...args) => (_createRenderEffect || createRenderEffect)(...args);
var _fragments = new Map;
var _truncated = new Set;
var _revealSubs = new Set;
var _truncationRejectors = new Map;
function createComponent(Comp, props, name) {
  return untrack(() => Comp(props || {}));
}
// ../../node_modules/.bun/@solidjs+universal@2.0.0-rc.9+24e9e1e07e3a1217/node_modules/@solidjs/universal/dist/universal.js
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
    const apply = (newProps) => {
      for (const prop in prevProps) {
        if (prop in newProps)
          continue;
        if (prop !== "ref")
          setProperty(node, prop, undefined, prevProps[prop]);
        delete prevProps[prop];
      }
      for (const prop in newProps) {
        const value = newProps[prop];
        if (value === prevProps[prop])
          continue;
        if (prop === "ref") {
          (typeof value === "function" || Array.isArray(value)) && ref(() => value, node);
        } else
          setProperty(node, prop, value, prevProps[prop]);
        prevProps[prop] = value;
      }
    };
    const childrenOptions = () => named(options);
    if (Array.isArray(props)) {
      if (!skipChildren)
        insert(node, () => {
          for (let i = props.length - 1;i >= 0; i--) {
            const s = resolveSource(props[i]);
            if (s != null && entryHas(s, "children"))
              return entryGet(s, "children");
          }
        }, undefined, undefined, childrenOptions());
      effect2(() => collectSources({}, props, undefined), apply, named(options));
      return prevProps;
    }
    if (!skipChildren) {
      if (typeof props !== "function" && props != null && hasStaticKeys(props)) {
        const desc = Object.getOwnPropertyDescriptor(props, "children");
        if (desc !== undefined) {
          if (desc.get === undefined)
            insert(node, desc.value, undefined, undefined, childrenOptions());
          else
            insert(node, () => props.children, undefined, undefined, childrenOptions());
        }
      } else
        insert(node, () => {
          const s = resolveSource(props);
          return s != null ? entryGet(s, "children") : undefined;
        }, undefined, undefined, childrenOptions());
    }
    effect2(() => {
      const s = resolveSource(props);
      const newProps = {};
      const table = resolvedTable(s);
      if (table !== undefined) {
        for (const [prop, leaf] of table) {
          if (typeof prop !== "string" || prop === "children")
            continue;
          newProps[prop] = leaf[prop];
        }
        return newProps;
      }
      if (s != null) {
        const view = viewOf(s);
        if (view instanceof OmitView)
          collectProps(newProps, view, SOURCE_OMIT);
        else if (view !== undefined)
          collectSources(newProps, view.sources, view.kinds);
        else
          collectProps(newProps, s, $PROXY in s ? SOURCE_PROXY : SOURCE_PLAIN);
      }
      return newProps;
    }, apply, named(options));
    return prevProps;
  }
  function resolveSource(s) {
    return typeof s === "function" ? s() : s;
  }
  function entryHas(s, key) {
    const view = viewOf(s);
    return view instanceof OmitView ? sourceHas(view, SOURCE_OMIT, key) : (key in s);
  }
  function entryGet(s, key) {
    const view = viewOf(s);
    return view instanceof OmitView ? sourceGet(view, SOURCE_OMIT, key) : s[key];
  }
  function collectSources(out, sources, kinds) {
    const resolved = [];
    const resolvedKinds = [];
    for (let i = 0;i < sources.length; i++)
      pushEntry(resolved, resolvedKinds, sources[i], kinds !== undefined ? kinds[i] : SOURCE_MEMO);
    for (let i = 0;i < resolved.length; i++)
      collectProps(out, resolved[i], resolvedKinds[i], resolved, resolvedKinds, i + 1);
    return out;
  }
  function pushEntry(resolved, kinds, s, kind) {
    if (kind !== SOURCE_MEMO) {
      resolved.push(s);
      kinds.push(kind);
      return;
    }
    s = resolveSource(s);
    if (s == null)
      return;
    const view = viewOf(s);
    if (view instanceof OmitView) {
      resolved.push(view);
      kinds.push(SOURCE_OMIT);
    } else if (view !== undefined) {
      const { sources: f, kinds: k } = view;
      for (let j = 0;j < f.length; j++)
        pushEntry(resolved, kinds, f[j], k[j]);
    } else {
      resolved.push(s);
      kinds.push($PROXY in s ? SOURCE_PROXY : SOURCE_PLAIN);
    }
  }
  function collectProps(out, s, kind, later, laterKinds, from) {
    const keys = sourceKeys(s, kind);
    outer:
      for (let i = 0;i < keys.length; i++) {
        const prop = keys[i];
        if (typeof prop !== "string" || prop === "children")
          continue;
        if (later !== undefined) {
          for (let j = from;j < later.length; j++)
            if (sourceHas(later[j], laterKinds[j], prop))
              continue outer;
        }
        out[prop] = sourceGet(s, kind, prop);
      }
    return out;
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
        createRoot2((dispose2) => {
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
var warnedMagnitude = -1;
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
  let magnitude = Math.floor(Math.log10(total));
  if (fresh.length === 0 && magnitude <= warnedMagnitude)
    return;
  for (let [type] of fresh)
    warnedLeakTypes.add(type);
  warnedMagnitude = magnitude;
  let list = [...counts].map(([type, n]) => `<${type}> x${n}`).join(", ");
  console.warn(`Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` + `The usual cause is reading an element-valued prop more than once (every read ` + `builds a new subtree); read it once where it mounts, or resolve it with ` + `children(). If these nodes are intentionally kept for later mounting, ignore ` + `this. The next warning comes when a new element type joins the list or the ` + `total passes ${10 ** (magnitude + 1)}.`);
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
    let root = createErrorBoundary(() => {
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
// ../../packages/core/src/cursor.ts
import { decodeImage as decodeImage3 } from "flux:image";
import { createCursor as registerCursor, dropCursor } from "flux:rendertree";
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
var pending = new Map;
// ../../packages/core/src/transform.ts
import { on as on5 } from "srt:events";
// ../../packages/core/src/swipe.ts
var SWIPE_ANGLE_TOLERANCE = 30;
var OFF_AXIS_RATIO = Math.tan(SWIPE_ANGLE_TOLERANCE * Math.PI / 180);
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
var DEVICE = "keyboard";
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
    id: `${DEVICE}:key:${spec}`,
    device: DEVICE,
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
    id: `${DEVICE}:axis:${neg}/${pos}`,
    device: DEVICE,
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
    id: `${DEVICE}:vec2:${keys.up}/${keys.down}/${keys.left}/${keys.right}`,
    device: DEVICE,
    rate: () => {
      state.count();
      return [(state.has(keys.right) ? 1 : 0) - (state.has(keys.left) ? 1 : 0), (state.has(keys.down) ? 1 : 0) - (state.has(keys.up) ? 1 : 0)];
    },
    key: state.key,
    blur: state.blur
  };
}
var MODIFIER_NAMES = new Set(["Shift", "Control", "Alt", "Meta"]);
function resolve2(spec) {
  let colon = spec.indexOf(":");
  let kind = colon < 0 ? spec : spec.slice(0, colon);
  let rest = colon < 0 ? "" : spec.slice(colon + 1);
  let parts = rest.split("/");
  switch (kind) {
    case "key":
      return key(rest);
    case "axis":
      if (parts.length !== 2)
        throw new Error(`keyboard.resolve: "${spec}" needs two keys, neg/pos`);
      return axis(parts[0], parts[1]);
    case "vec2":
      if (parts.length !== 4)
        throw new Error(`keyboard.resolve: "${spec}" needs four keys, up/down/left/right`);
      return vec2({
        up: parts[0],
        down: parts[1],
        left: parts[2],
        right: parts[3]
      });
    default:
      throw new Error(`keyboard.resolve: unknown source "${spec}" (key:<spec>, axis:<neg>/<pos>, vec2:<up>/<down>/<left>/<right>)`);
  }
}
var keyboard = {
  name: DEVICE,
  key,
  axis,
  vec2,
  resolve: resolve2,
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
var LISTEN_THRESHOLD = 0.5;
var DEVICE2 = "gamepad";
var deadzone = (x, y) => Math.hypot(x, y) < STICK_DEADZONE ? [0, 0] : [x, y];
var STICKS = [["leftStick", "leftX", "leftY"], ["rightStick", "rightX", "rightY"]];
var DPAD_BUTTONS = ["dpadUp", "dpadDown", "dpadLeft", "dpadRight"];
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
    id: `${DEVICE2}:${side}Stick`,
    device: DEVICE2,
    rate: sumVec2((pad) => deadzone(pad.axes[`${side}X`] ?? 0, pad.axes[`${side}Y`] ?? 0))
  });
  let axes = new Map;
  let buttons = new Map;
  let device = {
    name: DEVICE2,
    get slot() {
      return slot();
    },
    leftStick: stick("left"),
    rightStick: stick("right"),
    dpad: {
      kind: "vec2",
      label: `${who} dpad`,
      id: `${DEVICE2}:dpad`,
      device: DEVICE2,
      rate: sumVec2((pad) => [pressed(pad, "dpadRight") - pressed(pad, "dpadLeft"), pressed(pad, "dpadDown") - pressed(pad, "dpadUp")])
    },
    triggers: {
      kind: "axis",
      label: `${who} triggers`,
      id: `${DEVICE2}:triggers`,
      device: DEVICE2,
      rate: sumAxis((pad) => (pad.axes.rightTrigger ?? 0) - (pad.axes.leftTrigger ?? 0))
    },
    shoulders: {
      kind: "axis",
      label: `${who} shoulders`,
      id: `${DEVICE2}:shoulders`,
      device: DEVICE2,
      rate: sumAxis((pad) => pressed(pad, "rightShoulder") - pressed(pad, "leftShoulder"))
    },
    axis(name) {
      if (typeof name !== "string" || name.length === 0)
        throw new Error(`gamepad.axis: expected an axis name, got ${String(name)}`);
      let source = axes.get(name);
      if (!source) {
        source = {
          kind: "axis",
          label: `${who} ${name}`,
          id: `${DEVICE2}:axis:${name}`,
          device: DEVICE2,
          rate: sumAxis((pad) => pad.axes[name] ?? 0)
        };
        axes.set(name, source);
      }
      return source;
    },
    button(name) {
      if (typeof name !== "string" || name.length === 0)
        throw new Error(`gamepad.button: expected a button name, got ${String(name)}`);
      let source = buttons.get(name);
      if (!source) {
        source = {
          kind: "button",
          label: `${who} ${name}`,
          id: `${DEVICE2}:button:${name}`,
          device: DEVICE2,
          rate: anyButton(name)
        };
        buttons.set(name, source);
      }
      return source;
    },
    resolve(spec) {
      let colon = spec.indexOf(":");
      let head = colon < 0 ? spec : spec.slice(0, colon);
      let rest = colon < 0 ? "" : spec.slice(colon + 1);
      switch (head) {
        case "leftStick":
        case "rightStick":
        case "dpad":
        case "triggers":
        case "shoulders":
          if (rest)
            break;
          return device[head];
        case "axis":
          return device.axis(rest);
        case "button":
          return device.button(rest);
      }
      throw new Error(`gamepad.resolve: unknown source "${spec}" (leftStick, rightStick, dpad, triggers, shoulders, axis:<name>, button:<name>)`);
    },
    listen(kind, found) {
      if (kind !== "button" && kind !== "axis" && kind !== "vec2")
        throw new Error(`gamepad.listen: expected a kind, got ${String(kind)}`);
      if (typeof found !== "function")
        throw new Error("gamepad.listen: expects a function");
      let held2 = new Set;
      let arm = (pads2) => {
        let now = new Set;
        for (let pad of pads2) {
          for (let b of pad.buttons)
            now.add(b);
          for (let [name, v] of Object.entries(pad.axes))
            if (Math.abs(v) >= LISTEN_THRESHOLD)
              now.add(name);
          for (let [name, x, y] of STICKS)
            if (Math.hypot(pad.axes[x] ?? 0, pad.axes[y] ?? 0) >= LISTEN_THRESHOLD)
              now.add(name);
        }
        for (let name of held2)
          if (!now.has(name))
            held2.delete(name);
        return now;
      };
      for (let name of arm(untrack(pads)))
        held2.add(name);
      let fresh = (now, name) => now.has(name) && !held2.has(name);
      return createRoot((dispose2) => {
        createEffect(() => pads(), (ps) => {
          let now = arm(ps);
          let pick = () => {
            if (kind === "button") {
              for (let pad of ps)
                for (let b of pad.buttons)
                  if (fresh(now, b))
                    return device.button(b);
              return;
            }
            if (kind === "axis") {
              for (let pad of ps)
                for (let name of Object.keys(pad.axes))
                  if (fresh(now, name))
                    return device.axis(name);
              return;
            }
            for (let [name] of STICKS)
              if (fresh(now, name))
                return device[name];
            for (let pad of ps)
              for (let b of pad.buttons)
                if (DPAD_BUTTONS.includes(b) && fresh(now, b))
                  return device.dpad;
          };
          let source = pick();
          if (source)
            found(source);
        }, {
          defer: true
        });
        return dispose2;
      });
    }
  };
  return device;
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
