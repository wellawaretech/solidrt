// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/error.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/constants.js
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
var CONFIG_LANE_FRAME = 1 << 26;
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/lanes.js
var signalLanes = new WeakMap;
var activeLanes = new Set;
function findLane(n) {
  while (n.ei)
    n = n.ei;
  return n;
}
function mergeLanes(n, e) {
  n = findLane(n);
  e = findLane(e);
  if (n === e)
    return n;
  e.ei = n;
  for (const i of e.Qe)
    n.Qe.add(i);
  e.Qe.clear();
  n.dn[0].push(...e.dn[0]);
  n.dn[1].push(...e.dn[1]);
  e.dn[0].length = 0;
  e.dn[1].length = 0;
  return n;
}
function resolveLane(n) {
  const e = n.o?.me;
  if (!e)
    return;
  const i = findLane(e);
  if (activeLanes.has(i))
    return i;
  if (n.o !== null)
    n.o.me = undefined;
  return;
}
function resolveTransition(n) {
  if (hasActiveOverride(n) && n.o?.Ln) {
    const e = ext(n).Ln = currentTransition(n.o?.Ln);
    if (e.Rn !== true)
      return e;
    if (n.o !== null)
      n.o.Ln = null;
  }
  return resolveLane(n)?.we ?? n.we;
}
function assignOrMergeLane(n, e) {
  const i = findLane(e);
  const t = n.o?.me;
  if (t) {
    const r = findLane(t);
    if (activeLanes.has(r)) {
      if (r !== i && (!hasActiveOverride(n) || n.C & CONFIG_DERIVED_OVERRIDE)) {
        if (i.ii && findLane(i.ii) === r) {
          ext(n).me = e;
          n.C |= CONFIG_HAS_LANE;
        } else if (r.ii && findLane(r.ii) === i)
          ;
        else
          mergeLanes(i, r);
      }
      return;
    }
  }
  ext(n).me = e;
  n.C |= CONFIG_HAS_LANE;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/scheduler.js
var transitions = new Set;
var dirtyQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  ln: 0,
  EE: 0
};
var zombieQueue = {
  eE: new Array(2000).fill(undefined),
  tE: false,
  ln: 0,
  EE: 0
};
function cancelZombieRecompute(e) {
  if (e.oe & REACTIVE_OPTIMISTIC_DIRTY && !laneZombie(e))
    return GlobalQueue.Ke(e);
  if (e.oe & REACTIVE_IN_HEAP_HEIGHT)
    e.oe &= -140;
  else {
    deleteFromHeap(e, zombieQueue);
    e.oe &= -132;
  }
}
function laneZombie(e) {
  let t = e;
  while (t !== null && t.oe & REACTIVE_ZOMBIE)
    t = t._parent;
  return t !== null && (t.C & CONFIG_LANE_FRAME) !== 0;
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
  return transitions.size === 0 && activeLanes.size === 0 && e.ft.length === 0 && t.cn.length === 0 && t.A.length === 0 && t.ti.size === 0 && transientStoreNodes.size === 0 && pendingRearms.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.u !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e._e !== NOT_PENDING)
      continue;
    if (e.o?.Fe !== undefined && e.o?.Fe !== NOT_PENDING)
      continue;
    if (e.o?.t)
      continue;
    transientStoreNodes.delete(e);
    if (e.C & CONFIG_SLOT_NODE)
      slotUnobservedHook(e);
    else
      e.o?.Hn?.();
  }
}
function createBatch() {
  return {
    be: clock,
    Gn: [],
    le: new Map,
    cn: [],
    A: [],
    ti: new Set,
    pe: [],
    dt: {
      Tt: [[], []],
      ft: []
    },
    Rn: false,
    In: new Set,
    Dn: null
  };
}
function mergeTransitionState(e, t) {
  t.Rn = e;
  e.pe.push(...t.pe);
  e.Te ||= t.Te;
  for (const n of activeLanes)
    if (n.we === t)
      n.we = e;
  if (t.cn.length) {
    e.cn.push(...t.cn);
    t.cn.length = 0;
  }
  if (t.A.length) {
    e.A.push(...t.A);
    t.A.length = 0;
  }
  for (const n of t.ti)
    e.ti.add(n);
  for (const [n, i] of t.le) {
    let t2 = e.le.get(n);
    if (!t2)
      e.le.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
  for (const n of t.In)
    e.In.add(n);
  if (t.Dn)
    (e.Dn ??= []).push(...t.Dn);
}
var withheld = false;
function schedule() {
  if (halted) {
    notifyHalted();
    return;
  }
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.Jn) {
    withheld = projectionWriteActive;
    if (!withheld)
      queueMicrotask(flush);
  }
}
var wokenTransitions = [];
function wakeParked() {
  for (const e of transitions)
    wokenTransitions.includes(e) || wokenTransitions.push(e);
  schedule();
}
var pendingRearms = new Set;
function queueRearm(e) {
  pendingRearms.add(e);
  schedule();
}
function drainRearms() {
  const e = Array.from(pendingRearms);
  pendingRearms.clear();
  for (let t = 0;t < e.length; t++)
    e[t].se();
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
  Tt = [[], []];
  ft = [];
  ht = 0;
  created = clock;
  addChild(e) {
    this.ft.push(e);
    e._parent = this;
  }
  removeChild(e) {
    const t = this.ft.indexOf(e);
    if (t >= 0) {
      this.ft.splice(t, 1);
      e._parent = null;
    }
  }
  notify(e, t, n, i) {
    if (this._parent)
      return this._parent.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.Tt[e - 1].length) {
      const t2 = this.Tt[e - 1];
      this.Tt[e - 1] = [];
      runQueue(t2, e);
    }
    const t = this.ft;
    const n = ++queueRunToken;
    for (let i = 0;i < t.length; ) {
      const r = t[i];
      if (r.ht !== n) {
        r.ht = n;
        r.run?.(e);
        if (t[i] !== r) {
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
        n.dn[e - 1].push(t);
      } else {
        this.Tt[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.Tt[0].push(...this.Tt[0]);
    e.Tt[1].push(...this.Tt[1]);
    this.Tt = [[], []];
    for (let t = 0;t < this.ft.length; t++) {
      let n = this.ft[t];
      let i = e.ft[t];
      if (!i) {
        i = {
          Tt: [[], []],
          ft: []
        };
        e.ft[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.Tt[0].push(...e.Tt[0]);
    this.Tt[1].push(...e.Tt[1]);
    for (let t = 0;t < e.ft.length; t++) {
      const n = e.ft[t];
      let i = this.ft[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue extends Queue {
  Jn = false;
  m = createBatch();
  static Ke;
  static en;
  static Cn;
  static _t = null;
  static p = null;
  static G = null;
  static M = null;
  static N = null;
  static Zn = null;
  static jn = null;
  static Ve = null;
  static Ge = null;
  static We = null;
  static ri = null;
  static et = null;
  static nt = null;
  static rt = null;
  static On = null;
  static k = null;
  static Et = null;
  static Nt = null;
  static ot = null;
  static oi = null;
  static ai = null;
  static li = null;
  static si = null;
  static ui = null;
  static lt = null;
  static it = null;
  static tt = null;
  static $n = null;
  static an = null;
  static Sn = null;
  static zn = null;
  static Tn = null;
  static ke = null;
  static Ei = null;
  static ut = null;
  static Me = null;
  static Xn = false;
  static st = null;
  static fi = null;
  flush() {
    if (this.Jn)
      return;
    if (activeTransition === null && dirtyQueue.EE < dirtyQueue.ln && this.Tt[0].length === 0 && this.Tt[1].length === 0 && this.ft.length === 0 && !wokenTransitions.length && !batchJoins.length && canUseSimpleSyncFlush(this)) {
      this.Jn = true;
      try {
        resyncUnflushedCompanions();
        sweepDormant();
        commitPendingNodes();
      } finally {
        this.Jn = false;
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.ln || this.Tt[0].length !== 0 || this.Tt[1].length !== 0 || this.m.Gn.length !== 0;
      return;
    }
    this.Jn = true;
    resyncUnflushedCompanions();
    try {
      while (batchJoins.length)
        this.initTransition(batchJoins.pop());
      if (false)
        ;
      sweepDormant();
      runHeap(dirtyQueue, GlobalQueue.Ke);
      if (activeTransition && GlobalQueue.Ei?.(activeTransition))
        runHeap(dirtyQueue, GlobalQueue.Ke);
      if (pendingRearms.size) {
        drainRearms();
        runHeap(dirtyQueue, GlobalQueue.Ke);
      }
      if (activeTransition) {
        if (this.ft.length) {
          checkBoundaryChildren(this, true);
          if (dirtyQueue.EE >= dirtyQueue.ln)
            runHeap(dirtyQueue, GlobalQueue.Ke);
        }
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          heldTrims.length = 0;
          runHeap(zombieQueue, this.m === e2 ? cancelZombieRecompute : GlobalQueue.Ke);
          if (this.m === e2)
            currentBatch = this.m = createBatch();
          if (activeLanes.size) {
            GlobalQueue.si(EFFECT_RENDER);
            GlobalQueue.si(EFFECT_USER);
          }
          this.stashQueues(e2.dt);
          clock++;
          scheduled = dirtyQueue.EE >= dirtyQueue.ln || this.m.Gn.length > 0;
          reassignPendingTransition(e2.Gn);
          activeTransition = null;
          finalizePureQueue(null, true);
          return;
        }
        const t = activeTransition;
        const n = this.m;
        n !== t && n.Gn.push(...t.Gn);
        this.restoreQueues(t.dt);
        transitions.delete(t);
        activeTransition = null;
        reassignPendingTransition(n.Gn);
        finalizePureQueue(t);
        if (n === t) {
          const e2 = createBatch();
          e2.Gn = n.Gn;
          e2.cn = n.cn;
          e2.A = n.A;
          e2.ti = n.ti;
          currentBatch = this.m = e2;
        }
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue.EE >= dirtyQueue.ln) {
            runHeap(dirtyQueue, GlobalQueue.Ke);
            commitPendingNodes();
          }
        } else {
          if (transitions.size) {
            commitPendingNodes();
            runHeap(zombieQueue, GlobalQueue.Ke);
          }
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue.EE >= dirtyQueue.ln || activeTransition !== null || this.m.Gn.length !== 0;
      activeLanes.size && GlobalQueue.si(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && GlobalQueue.si(EFFECT_USER);
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
      this.Jn = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING) {
      if (n & STATUS_PENDING) {
        const t2 = i ?? e.o?._;
        if (t2?.l)
          return true;
        if (t2) {
          if (!activeTransition && !e.we && currentBatch.Gn.length)
            this.initTransition();
          if (activeTransition) {
            const n2 = t2.source;
            let i2 = activeTransition.le.get(n2);
            if (!i2)
              activeTransition.le.set(n2, i2 = new Set);
            const r = i2.size;
            i2.add(e);
            if (i2.size !== r) {
              schedule();
              GlobalQueue.Nt?.(activeTransition);
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
      if (e.Rn === true || e === activeTransition)
        return;
    }
    if (!e && activeTransition && activeTransition.be === clock)
      return;
    if (!activeTransition) {
      activeTransition = e ?? createBatch();
    } else if (e) {
      const t2 = activeTransition;
      mergeTransitionState(e, t2);
      this.restoreQueues(t2.dt);
      transitions.delete(t2);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.be = clock;
    const t = this.m;
    if (t !== activeTransition) {
      const e2 = this.Jn ? 0 : CONFIG_ADOPTED_UNFLUSHED;
      for (let n = 0;n < t.Gn.length; n++) {
        const i = t.Gn[n];
        if (i.we === null && i._e !== NOT_PENDING && (!i.he || i.oe & REACTIVE_MANUAL_WRITE && !(i.h & STATUS_UNINITIALIZED)) && i.xe && i.xe(i.ce, i._e)) {
          i._e = NOT_PENDING;
          commitPendingNode(i);
          continue;
        }
        i.we = activeTransition;
        i.C |= e2;
        activeTransition.Gn.push(i);
      }
      for (let e3 = 0;e3 < t.cn.length; e3++) {
        const n = t.cn[e3];
        n.we = activeTransition;
        activeTransition.cn.push(n);
      }
      if (t.A.length)
        activeTransition.A.push(...t.A);
      for (const e3 of t.ti)
        activeTransition.ti.add(e3);
      if (t.In.size) {
        for (const e3 of t.In)
          activeTransition.In.add(e3);
        t.In.clear();
      }
      currentBatch = this.m = activeTransition;
    }
    for (const e2 of activeLanes) {
      if (!e2.we)
        e2.we = activeTransition;
    }
    schedule();
  }
}
function queuePendingNode(e) {
  currentBatch.Gn.push(e);
  if (!globalQueue.Jn)
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
  e.vn = notifyEpoch;
  const n = e.C;
  const i = (n & CONFIG_HAS_LANE ? e.o?.me : undefined) || currentOptimisticLane;
  const r = (n & CONFIG_HAS_SNAPSHOT) !== 0 && e.o?.sn !== undefined;
  const s = reaskArmed;
  for (let n2 = e.u;n2 !== null; n2 = n2.Ae) {
    const e2 = n2.ge;
    if (s)
      e2.oe &= ~REACTIVE_REASK;
    if (e2.oe & REACTIVE_RECOMPUTING_DEPS && n2.Be === e2.Ye && n2 !== e2.Nn)
      e2.oe |= REACTIVE_MISSED_WAKE;
    if (r && e2.C & CONFIG_IN_SNAPSHOT_SCOPE) {
      e2.oe |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && i) {
      e2.oe |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(e2, i);
    } else if (t) {
      e2.oe |= REACTIVE_OPTIMISTIC_DIRTY;
      if (e2.o)
        e2.o.me = undefined;
    }
    enqueueSub(e2);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.he) {
    if (e._e !== NOT_PENDING) {
      e.ce = e._e;
      e._e = NOT_PENDING;
    }
    if (e.C & CONFIG_HAS_COMPANIONS)
      GlobalQueue.ri(e);
    return;
  }
  if (e._e !== NOT_PENDING) {
    e.ce = e._e;
    e._e = NOT_PENDING;
    t.h &= ~STATUS_UNINITIALIZED;
    if (e.je && e.je !== EFFECT_TRACKED)
      e.Xe = true;
    if (e.o)
      e.o.Le = false;
  }
  t.ve = false;
  t.oe &= ~REACTIVE_MANUAL_WRITE;
  if (t.o?._ == null)
    trimStaleDeps(t);
  t.C &= -68157441;
  if (!(t.h & STATUS_PENDING))
    t.h &= ~STATUS_UNINITIALIZED;
  else
    e.C |= CONFIG_INPUTS_PUBLISHED;
  if (t.o != null && (t.o._n !== null || t.o.fn !== null))
    GlobalQueue.en(t, false, true);
  if (e.C & CONFIG_HAS_COMPANIONS)
    GlobalQueue.ri(e);
}
var storeCommitHook = null;
var heldRevealed = [];
var heldTrims = [];
function commitPendingNodes() {
  while (heldTrims.length)
    trimStaleDeps(heldTrims.pop());
  const e = currentBatch.Gn;
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    commitPendingNode(n);
    n.we = null;
    if (n.C & CONFIG_HELD_TRUTH) {
      n.C &= ~CONFIG_HELD_TRUTH;
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
  if (!t && globalQueue.ft.length)
    checkBoundaryChildren(globalQueue);
  const r = e?.Dn;
  const s = i && (e ?? n).cn.length !== 0;
  if (r && !s) {
    for (const e2 of r)
      if (!(e2.oe & REACTIVE_DISPOSED))
        enqueueSub(e2);
  }
  const o = dirtyQueue.EE >= dirtyQueue.ln;
  if (o)
    runHeap(dirtyQueue, GlobalQueue.Ke);
  if (i) {
    if (currentBatch !== n) {
      if (e === null || e === n)
        return;
    } else if (o)
      commitPendingNodes();
    const t2 = e ?? n;
    if (t2.cn.length)
      GlobalQueue.oi(t2.cn);
    if (r && s) {
      for (const e2 of r)
        if (!(e2.oe & REACTIVE_DISPOSED))
          enqueueSub(e2);
      schedule();
    }
    if (t2.In.size) {
      for (const e2 of t2.In) {
        if (e2.oe & REACTIVE_DISPOSED)
          continue;
        enqueueSub(e2);
      }
      t2.In.clear();
      schedule();
    }
    if (t2.A.length) {
      GlobalQueue.G(t2.A);
      if (globalQueue.ft.length)
        checkBoundaryChildren(globalQueue);
    }
    if (t2.ti.size)
      GlobalQueue._t(t2.ti, e);
    if (heldRevealed.length !== 0) {
      while (heldRevealed.length)
        insertSubs(heldRevealed.pop());
      if (dirtyQueue.EE >= dirtyQueue.ln) {
        runHeap(dirtyQueue, GlobalQueue.Ke);
        commitPendingNodes();
      }
    }
    sweepTransientStoreNodes();
    if (activeLanes.size)
      GlobalQueue.li(e);
  }
}
function checkBoundaryChildren(e, t) {
  for (const n of e.ft) {
    t ? n.Re?.() : n.Ee?.();
    checkBoundaryChildren(n, t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].we = activeTransition;
    e[t].C &= ~CONFIG_ADOPTED_UNFLUSHED;
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
  if (globalQueue.Jn) {
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
  const i = e.oe;
  if (i & REACTIVE_DISPOSED)
    return false;
  if (i & REACTIVE_ZOMBIE) {
    let t2 = e;
    while (t2 && t2.oe & REACTIVE_ZOMBIE)
      t2 = t2._parent;
    let i2 = t2 && (t2.we || (t2.C & CONFIG_HELD_CHILDREN ? activeTransition : null));
    if (!i2 && t2 && t2.C & CONFIG_LANE_FRAME && t2.o?.me)
      i2 = findLane(t2.o.me).we;
    if (!i2 || (i2 = currentTransition(i2)).Rn === true || i2 === n)
      return false;
  }
  for (let t2 = e.T;t2; t2 = t2._parent)
    if (t2.te & STATUS_PENDING && !t2.q)
      return false;
  if (e.o?.ue?.has(t))
    return true;
  const r = e.Nn;
  for (let n2 = r === null ? null : e.Ie;n2; n2 = n2 === r ? null : n2.Ne) {
    let e2 = n2.Oe;
    while (e2) {
      if (e2 === t || e2.De === t || e2.o?.ue?.has(t))
        return true;
      e2 = e2.o?.kn;
    }
  }
  return !!(e.h & STATUS_PENDING && e.o?._ instanceof NotReadyError && e.o?._.source === t);
}
function sourceObserved(e, t, n) {
  const i = e.le.get(t);
  let r = false;
  for (const e2 of i ?? []) {
    if (reporterBlocksSource(e2, t, n))
      return true;
    if (n && e2.oe & REACTIVE_ZOMBIE)
      r = true;
    else
      i.delete(e2);
  }
  if (!r)
    e.le.delete(t);
  return false;
}
function transitionComplete(e) {
  if (e.Rn)
    return true;
  if (e.pe.length) {
    return false;
  }
  let t = true;
  for (const n of e.le.keys()) {
    if (sourceObserved(e, n, e) && n.o?.ue?.size) {
      t = false;
      break;
    }
  }
  if (t && GlobalQueue.ai?.(e))
    t = false;
  t && (e.Rn = true);
  return t;
}
function currentTransition(e) {
  while (e.Rn && typeof e.Rn === "object")
    e = e.Rn;
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/heap.js
function queueFor(e) {
  return e.oe & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
}
function enqueueSub(e) {
  const E = queueFor(e);
  if (E.ln > e.rn)
    E.ln = e.rn;
  insertIntoHeap(e, E);
}
function actualInsertIntoHeap(e, E) {
  const t = (e._parent?.Wn ? e._parent.qn?.rn : e._parent?.rn) ?? -1;
  if (t >= e.rn)
    e.rn = t + 1;
  const n = e.rn;
  const I = E.eE[n];
  if (I === undefined)
    E.eE[n] = e;
  else {
    const E2 = I.Fn;
    E2.Pn = e;
    e.Fn = E2;
    I.Fn = e;
  }
  if (n > E.EE)
    E.EE = n;
}
function insertIntoHeap(e, E) {
  let t = e.oe;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (t & REACTIVE_CHECK) {
    e.oe = t & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else {
    e.oe = t | REACTIVE_IN_HEAP;
    if (E.tE)
      markNode(e);
  }
  if (!(t & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, E);
}
function insertIntoHeapHeight(e, E) {
  let t = e.oe;
  if (t & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.oe = t | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, E);
}
function deleteFromHeap(e, E) {
  const t = e.oe;
  if (!(t & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.oe = t & -25;
  const n = e.rn;
  if (e.Fn === e)
    E.eE[n] = undefined;
  else {
    const t2 = e.Pn;
    const I = E.eE[n];
    const o = t2 ?? I;
    if (e === I)
      E.eE[n] = t2;
    else
      e.Fn.Pn = t2;
    o.Fn = e.Fn;
  }
  e.Fn = e;
  e.Pn = undefined;
}
function markHeap(e) {
  if (e.tE)
    return;
  e.tE = true;
  for (let E = 0;E <= e.EE; E++) {
    for (let t = e.eE[E];t !== undefined; t = t.Pn) {
      if (t.oe & REACTIVE_IN_HEAP)
        markNode(t);
    }
  }
}
function markNode(e, E = REACTIVE_DIRTY) {
  const t = e.oe;
  if ((t & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= E)
    return;
  e.oe = t & -4 | E;
  for (let E2 = e.u;E2 !== null; E2 = E2.Ae) {
    markNode(E2.ge, REACTIVE_CHECK);
  }
  if (e.C & CONFIG_FW_CHILDREN) {
    for (let E2 = e.o.i;E2 !== null; E2 = E2.Ue) {
      for (let e2 = E2.u;e2 !== null; e2 = e2.Ae) {
        markNode(e2.ge, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, E) {
  e.tE = false;
  for (e.ln = 0;e.ln <= e.EE; e.ln++) {
    let t = e.eE[e.ln];
    while (t !== undefined) {
      if (t.oe & REACTIVE_IN_HEAP)
        E(t);
      else
        adjustHeight(t, e);
      t = e.eE[e.ln];
    }
  }
  e.EE = 0;
}
function adjustHeight(e, E) {
  deleteFromHeap(e, E);
  let t = e.rn;
  for (let E2 = e.Ie;E2; E2 = E2.Ne) {
    const e2 = E2.Oe;
    const n = e2.De || e2;
    if (n.he && n.rn >= t)
      t = n.rn + 1;
  }
  if (e.rn !== t) {
    e.rn = t;
    for (let E2 = e.u;E2 !== null; E2 = E2.Ae) {
      insertIntoHeapHeight(E2.ge, queueFor(E2.ge));
    }
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/owner.js
function markDisposal(e) {
  let n = e.tn;
  while (n) {
    const e2 = n.oe;
    n.oe = e2 | REACTIVE_ZOMBIE;
    if (e2 & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)) {
      deleteFromHeap(n, e2 & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      if (e2 & REACTIVE_IN_HEAP)
        insertIntoHeap(n, zombieQueue);
      else
        insertIntoHeapHeight(n, zombieQueue);
    }
    markDisposal(n);
    n = n.un;
  }
}
function disposeChildren(e, n = false, t) {
  const i = e.oe;
  if (i & REACTIVE_DISPOSED)
    return;
  if (n && !t && e.o !== null && (e.o._n !== null || e.o.fn !== null))
    disposeChildren(e, false, true);
  if (n) {
    e.oe = i | REACTIVE_DISPOSED;
    const n2 = e;
    if (n2.o?.Ze || n2.o?.He)
      GlobalQueue.ri(n2);
    if (n2.C & CONFIG_CHILD_COMPANIONS)
      n2.o.Un.forEach(GlobalQueue.ri);
    const t2 = n2.we;
    if (t2 && n2.h & STATUS_PENDING && !wokenTransitions.includes(t2))
      wokenTransitions.push(t2), schedule();
  }
  if (n && e.he && e.o !== null)
    e.o.Ce = null;
  let l = t ? e.o?._n ?? null : e.tn;
  if (!t)
    e.tn = null;
  while (l) {
    const e2 = l;
    e2.C &= ~CONFIG_AUTO_DISPOSE;
    deleteFromHeap(e2, queueFor(e2));
    clearDeps(e2);
    l.gn = l;
    disposeChildren(l, true);
    l = l.un;
  }
  if (t) {
    if (e.o !== null)
      e.o._n = null;
  } else
    e.En = 0;
  if (n && !t && !(i & REACTIVE_ZOMBIE) && e._parent !== null && !(e._parent.oe & REACTIVE_DISPOSED)) {
    const n2 = e.gn;
    const t2 = e.un;
    if (n2 !== null)
      n2.un = t2;
    else
      e._parent.tn = t2;
    if (t2 !== null)
      t2.gn = n2;
    e.gn = null;
  }
  runDisposal(e, t);
  if (n && e.wn) {
    const n2 = e.wn;
    e.wn = undefined;
    n2();
  }
}
function linkChild(e, n) {
  const t = e.tn;
  n.gn = null;
  n.un = t;
  if (t !== null)
    t.gn = n;
  e.tn = n;
}
function runDisposal(e, n) {
  const t = n ? e.o?.fn : e.qe;
  if (!t)
    return;
  if (n)
    e.o.fn = null;
  else
    e.qe = null;
  if (Array.isArray(t)) {
    for (let e2 = t.length - 1;e2 >= 0; e2--) {
      const n2 = t[e2];
      n2.call(n2);
    }
  } else {
    t.call(t);
  }
}
function childId(e, n) {
  let t = e;
  while (t.C & CONFIG_TRANSPARENT && t._parent)
    t = t._parent;
  if (t.id != null)
    return formatId(t.id, n ? t.En++ : t.En);
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
  if (!context.qe)
    context.qe = e;
  else if (Array.isArray(context.qe))
    context.qe.push(e);
  else
    context.qe = [context.qe, e];
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
    C: t ? CONFIG_TRANSPARENT : 0,
    Wn: true,
    qn: n?.Wn ? n.qn : n,
    tn: null,
    un: null,
    gn: null,
    qe: null,
    T: n?.T ?? globalQueue,
    Je: n?.Je || defaultContext,
    En: 0,
    o: null,
    _parent: n,
    dispose: disposeRootSelf
  };
  if (n)
    linkChild(n, i);
  return i;
}
function createRoot(e, n) {
  const t = createOwner(n);
  return runWithOwner(t, () => e(() => t.dispose()));
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/graph.js
function unlinkSubs(e) {
  const n = e.Oe;
  const l = e.Ne;
  const o = e.Ae;
  const s = e.el;
  if (o !== null)
    o.el = s;
  else
    n.mn = s;
  if (s !== null)
    s.Ae = o;
  else {
    n.u = o;
    if (o === null) {
      if (n.C & CONFIG_SLOT_NODE)
        slotUnobservedHook(n);
      else
        n.o?.Hn?.();
      const e2 = n;
      e2.he && e2.C & CONFIG_AUTO_DISPOSE && !(e2.oe & REACTIVE_ZOMBIE) && !(e2.h & STATUS_PENDING) && unobserved(e2);
    }
  }
  return l;
}
function trimStaleDeps(e) {
  const n = e.Nn;
  let l = n !== null ? n.Ne : e.Ie;
  if (l !== null) {
    do {
      l = unlinkSubs(l);
    } while (l !== null);
    if (n !== null)
      n.Ne = null;
    else
      e.Ie = null;
  }
}
function clearDeps(e) {
  let n = e.Ie;
  if (!n)
    return;
  do {
    n = unlinkSubs(n);
  } while (n !== null);
  e.Ie = null;
  e.Nn = null;
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
    if (!e.u && e.C & CONFIG_AUTO_DISPOSE && !(e.h & STATUS_PENDING) && !(e.oe & (REACTIVE_DISPOSED | REACTIVE_ZOMBIE))) {
      unobserved(e);
    }
  }
  dormantNodes.clear();
}
function link(e, n, l = false) {
  const o = n.Nn;
  if (o !== null && o.Oe === e) {
    o.ze &&= l;
    return;
  }
  let s = null;
  const t = n.oe & REACTIVE_RECOMPUTING_DEPS;
  if (t) {
    s = o !== null ? o.Ne : n.Ie;
    if (s !== null && s.Oe === e) {
      s.Be = n.Ye;
      n.Nn = s;
      s.ze = l;
      return;
    }
  }
  const r = e.mn;
  if (r !== null && r.ge === n && (!t || r.Be === n.Ye)) {
    if (t)
      r.ze &&= l;
    else
      r.ze = l;
    return;
  }
  const u = n.Nn = e.mn = {
    Oe: e,
    ge: n,
    Ne: s,
    el: r,
    Ae: null,
    Be: n.Ye,
    ze: l
  };
  if (o !== null)
    o.Ne = u;
  else
    n.Ie = u;
  if (r !== null)
    r.Ae = u;
  else
    e.u = u;
  bumpNotifyEpoch();
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/async.js
function addPendingSource(e, n) {
  if (e.o?.ue?.has(n))
    return false;
  (ext(e).ue ??= new Set).add(n);
  return true;
}
function removePendingSource(e, n) {
  const t = e.o?.ue;
  if (!t?.delete(n))
    return false;
  if (!t.size)
    e.o.ue = undefined;
  return true;
}
function clearPendingSources(e) {
  if (e.o !== null)
    e.o.ue = undefined;
}
function retryReaches(e, n) {
  for (let t = e.Ie;t; t = t.Ne) {
    const e2 = t.Oe.De || t.Oe;
    if (e2 === n || e2.o?.ue?.has(n))
      return true;
  }
  return false;
}
function parkLoadingWindow(e, n) {
  ext(e).Pe = true;
  if (n.source)
    addPendingSource(e, n.source);
  if (!(e.h & STATUS_ERROR))
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
  for (let t = e.u;t !== null; t = t.Ae)
    n(t.ge, t);
  for (let t = e.o?.i ?? null;t !== null; t = t.Ue) {
    for (let e2 = t.u;e2 !== null; e2 = e2.Ae)
      n(e2.ge, e2);
  }
}
function releaseIfSettledUnobserved(e) {
  e.he && e.C & CONFIG_AUTO_DISPOSE && !e.u && !(e.oe & REACTIVE_ZOMBIE) && !(e.h & STATUS_PENDING) && unobserved(e);
}
function releaseSettledDependents(e) {
  let n;
  const t = new Set;
  const visit = (e2) => {
    if (t.has(e2))
      return;
    t.add(e2);
    if (!e2.u && e2.C & CONFIG_AUTO_DISPOSE)
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
  const o = GlobalQueue.Ge;
  const settle = (s) => {
    if (i.has(s))
      return;
    if (n !== e && retryReaches(s, n))
      return;
    if (!removePendingSource(s, n))
      return;
    i.add(s);
    s.be = clock;
    const u = s.o?.ue?.values().next().value;
    const l = s.h & STATUS_ERROR;
    if (u) {
      if (!l)
        setPendingError(s, u);
      o?.(s);
    } else {
      s.h &= ~STATUS_PENDING;
      if (!l)
        setPendingError(s);
      o?.(s);
      if (s.o?.Pe) {
        enqueueSub(s);
        t = true;
      }
      if (s.o !== null)
        s.o.Pe = false;
      if (!s.u && s.C & CONFIG_AUTO_DISPOSE)
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
  const n = e.o?.ye;
  if (n != null) {
    e.o.ye = null;
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
      e.o.Ce = null;
    e.ve = false;
    return n;
  }
  ext(e).Ce = n;
  e.o.ue = undefined;
  const o = origin;
  let s;
  const settleTransition = () => {
    let n2 = resolveTransition(e);
    if (e.o?.me)
      n2 = waitingTransition(e) ?? n2;
    if (n2 && e.h & STATUS_UNINITIALIZED && !currentTransition(n2).le.has(e)) {
      e.we = null;
      return;
    }
    globalQueue.initTransition(n2);
    enterWaiting(e);
  };
  const handleError = (t2) => {
    if (e.o?.Ce !== n)
      return;
    let r2 = t2 instanceof NotReadyError;
    if (r2 && e.ve) {
      if (e.o !== null)
        e.o.Ce = null;
      parkLoadingWindow(e, t2);
      e.be = clock;
      return;
    }
    settleTransition();
    notifyStatus(e, r2 ? STATUS_PENDING : STATUS_ERROR, t2);
    if (r2)
      settlePendingSource(e);
    e.be = clock;
    if (!r2)
      releaseSettledDependents(e);
  };
  const asyncWrite = (r2, i2) => {
    if (e.o?.Ce !== n)
      return;
    if (e.oe & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    setOrigin(o);
    settleTransition();
    const s2 = !!(e.h & STATUS_UNINITIALIZED);
    const u2 = e.o?.Le;
    landStatus(e);
    if (u2)
      e.o.Le = true;
    const l = resolveLane(e);
    if (l)
      l.Qe.delete(e);
    if (t) {
      try {
        t(r2);
      } catch (e2) {
        handleError(e2);
        return;
      }
      if (s2)
        landStatus(e, true);
    } else if (e.o?.Fe !== undefined && !(l && e.C & CONFIG_DERIVED_OVERRIDE)) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = r2;
      GlobalQueue.Ve?.(e, r2);
      if (!hasActiveOverride(e)) {
        insertSubs(e);
      } else
        GlobalQueue.ke(e, r2);
      e.be = clock;
    } else if (l) {
      const n2 = e.je;
      const t2 = hasActiveOverride(e) ? unwrapOverride(e.o.Fe) : e.ce;
      const i3 = e.xe;
      try {
        if (!n2 && s2 || !i3 || !i3(t2, r2)) {
          if (n2)
            e.ce = r2;
          else
            GlobalQueue.Me(e, r2, l);
          e.be = clock;
          GlobalQueue.Ve?.(e, r2);
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
    if (e._e === NOT_PENDING) {
      e.ve = false;
      if (u2)
        e.o.Le = false;
      trimStaleDeps(e);
    }
    settlePendingSource(e);
    schedule();
    flush();
    i2?.();
  };
  const settleAutodispose = () => {
    if (e.C & CONFIG_AUTO_DISPOSE && !e.u && !(e.h & STATUS_PENDING)) {
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
    ext(e).ye = close;
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
        } else if (e.o?.Ce !== n) {
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
        } else if (e.o?.Ce === n) {
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
      if (!e.qe)
        e.qe = n2;
      else if (Array.isArray(e.qe))
        e.qe.push(n2);
      else
        e.qe = [e.qe, n2];
    };
    n.then((r3) => {
      if (o2) {
        s = r3;
        t2 = true;
      } else if (e.o?.Ce === n && !(e.oe & REACTIVE_DISPOSED) && flattenIfIterable(r3, registerDeferredClose))
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
      if (e.ve)
        return e.ce;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    } else if (!flattenIfIterable(s)) {
      e.ve = false;
    }
  }
  if (r)
    flattenIfIterable(n);
  if (u !== null) {
    if (!u) {
      if (e.ve)
        return e.ce;
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
    e.ve = false;
  }
  return s;
}
function clearStatus(e, n = false) {
  if (e.o?.ue)
    clearPendingSources(e);
  if (e.o?.Pe) {
    if (e.o !== null)
      e.o.Pe = false;
  }
  if (e.o !== null)
    e.o.Le = false;
  e.h = n ? 0 : e.h & STATUS_UNINITIALIZED;
  if (e.o?._)
    setPendingError(e);
  if (e.o?.Ze || e.o?.He)
    GlobalQueue.Ge(e);
  if (e.o?.i && e.C & CONFIG_CHILD_COMPANIONS && GlobalQueue.We !== null)
    GlobalQueue.We(e);
  const t = statusNotifierOf(e);
  if (t)
    t.call(e);
}
function landStatus(e, n = false) {
  const t = e.o?.ue;
  if (t && (t.delete(e), t.size)) {
    e.o.Pe = false;
    if (n)
      e.h = STATUS_PENDING;
    setPendingError(e, t.values().next().value);
  } else
    clearStatus(e, n);
}
function notifyStatus(e, n, t, r, i) {
  if (n === STATUS_ERROR && !(t instanceof StatusError) && !(t instanceof NotReadyError))
    t = new StatusError(e, t);
  const o = n === STATUS_PENDING && t instanceof NotReadyError ? t.source : undefined;
  const s = o === e;
  const u = n === STATUS_PENDING && e.o?.Fe !== undefined && !(e.C & CONFIG_DERIVED_OVERRIDE) && !s;
  const l = u && hasActiveOverride(e);
  if (!r) {
    if (i)
      assignOrMergeLane(e, i);
    if (n === STATUS_PENDING && o) {
      addPendingSource(e, o);
      if (!(e.h & STATUS_PENDING))
        e.C &= ~CONFIG_INPUTS_PUBLISHED;
      e.h = STATUS_PENDING | e.h & STATUS_UNINITIALIZED;
      setPendingError(e, o, t);
    } else {
      clearPendingSources(e);
      e.h = n | (n !== STATUS_ERROR ? e.h & STATUS_UNINITIALIZED : 0);
      ext(e)._ = t;
    }
    GlobalQueue.Ge?.(e);
    if (e.o?.i && e.C & CONFIG_CHILD_COMPANIONS && GlobalQueue.We !== null)
      GlobalQueue.We(e);
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
    e2.be = clock;
    if (n === STATUS_PENDING && r2.Be !== e2.Ye) {
      enqueueSub(e2);
      schedule();
      return;
    }
    if (n === STATUS_PENDING && o && !e2.o?.ue?.has(o) || n !== STATUS_PENDING && (e2.o?._ !== t || e2.o?.ue)) {
      if (r2.ze && n !== STATUS_PENDING && !(t instanceof NotReadyError)) {
        enqueueSub(e2);
        schedule();
        return;
      }
      if (!f)
        e2.we ? o && !e2.je && (e2.h & STATUS_PENDING || e2._e !== NOT_PENDING) && globalQueue.initTransition(e2.we) : queuePendingNode(e2);
      notifyStatus(e2, n, t, f, a);
    }
  });
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/core.js
GlobalQueue.Ke = (e) => {
  if (e.je === EFFECT_TRACKED) {
    deleteFromHeap(e, queueFor(e));
    e.Xe = true;
    e.T.enqueue(EFFECT_USER, e.$e);
  } else
    recompute(e);
};
GlobalQueue.en = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
function notifyOnLane(e, n) {
  const t = currentOptimisticLane;
  currentOptimisticLane = n;
  try {
    insertSubs(e, true);
  } finally {
    currentOptimisticLane = t;
  }
}
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.nn)
      return true;
    e = e._parent;
  }
  return false;
}
function recompute(e, n = false) {
  bumpNotifyEpoch();
  const t = e.je;
  let i = !!(e.oe & REACTIVE_OPTIMISTIC_DIRTY);
  let u = null;
  if (i) {
    u = GlobalQueue.an(e, true);
    if (u === false)
      i = false;
  } else if (e.C & CONFIG_DERIVED_OVERRIDE) {
    u = GlobalQueue.an(e, true);
    if (u)
      i = true;
  } else if (activeTransition && !n && activeTransition.cn.length) {
    u = GlobalQueue.an(e, false);
    if (u)
      i = true;
  }
  if (!n) {
    if (e.we && !t && activeTransition !== e.we)
      globalQueue.initTransition(e.we);
    deleteFromHeap(e, queueFor(e));
    if (e.o !== null) {
      e.o.Ce = null;
      releaseFlightTeardown(e);
    }
    if (t === EFFECT_TRACKED || e.C & (CONFIG_HELD_CHILDREN | CONFIG_LANE_FRAME))
      disposeChildren(e);
    else if (e.tn !== null || e.qe !== null) {
      markDisposal(e);
      const n2 = ext(e);
      n2.fn = e.qe;
      n2._n = e.tn;
      e.qe = null;
      e.tn = null;
      e.En = 0;
      if (u) {
        e.C |= CONFIG_LANE_FRAME;
        findLane(u).dn[0].push(() => {
          e.C &= ~CONFIG_LANE_FRAME;
          disposeChildren(e, false, true);
        });
      }
    }
  }
  const l = (e.C & (CONFIG_OPTIMISTIC | CONFIG_DERIVED_OVERRIDE)) !== 0 && e.o?.Fe !== NOT_PENDING && e.o?.Fe !== undefined;
  const r = !!(e.h & STATUS_UNINITIALIZED);
  const o = e.h & STATUS_ERROR ? e.o?._ : undefined;
  const s = (e.h & STATUS_PENDING) !== 0;
  const a = s ? e.o?.ue : undefined;
  const c = e.o?.ue?.has(e);
  const f = (e.oe & REACTIVE_REASK) !== 0;
  const _ = e.ve;
  const E = stagedEntry;
  stagedEntry = null;
  const d = context;
  context = e;
  e.Nn = null;
  e.Ye++;
  e.oe = REACTIVE_RECOMPUTING_DEPS | e.oe & REACTIVE_ZOMBIE;
  e.be = clock;
  let N = e._e === NOT_PENDING ? e.ce : e._e;
  let I = e.rn;
  let T = false;
  let S = tracking;
  let O = currentOptimisticLane;
  tracking = true;
  const A = latestReadActive;
  latestReadActive = false;
  if (!t)
    currentOptimisticLane = null;
  if (u)
    currentOptimisticLane = u;
  const C = t && t !== EFFECT_USER;
  const p = stale;
  if (C)
    stale = true;
  if (t && activeTransition !== null && activeTransition.In.size)
    activeTransition.In.delete(e);
  try {
    if (e.C & CONFIG_SYNC) {
      N = e.he(N);
      if (e.o !== null)
        e.o.Ce = null;
      e.ve = false;
    } else {
      const n2 = e.o?.Ce;
      const t2 = e.he(N);
      const i2 = typeof t2 === "object" && t2 !== null;
      const u2 = e.o?.Ce !== n2;
      N = u2 || !i2 ? t2 : handleAsync(e, t2);
      if (!u2 && !i2) {
        if (e.o !== null)
          e.o.Ce = null;
        e.ve = false;
      }
    }
    if (e.h !== 0 || e.o !== null)
      clearStatus(e, n && stagedEntry === null);
    if (e.C & CONFIG_HAS_LANE && e.o?.me)
      GlobalQueue.Tn(e);
  } catch (n2) {
    const t2 = n2 instanceof NotReadyError;
    if (t2 && e.ve) {
      parkLoadingWindow(e, n2);
    } else {
      if (t2 && currentOptimisticLane)
        GlobalQueue.Sn(e);
      let i2 = false;
      if (t2) {
        ext(e).Pe = true;
        if (GlobalQueue.On !== null)
          i2 = GlobalQueue.On(e, f);
      }
      notifyStatus(e, t2 ? STATUS_PENDING : STATUS_ERROR, n2, undefined, t2 ? e.o?.me : undefined);
      if (t2 && c && !e.o?.Ce)
        settlePendingSource(e);
      if (t2 && a) {
        for (const n3 of a)
          if (n3 !== e && !e.o?.ue?.has(n3))
            settlePendingSource(e, n3);
      }
      if (i2)
        GlobalQueue.k(e);
    }
  } finally {
    tracking = S;
    latestReadActive = A;
    if (C)
      stale = p;
    T = (e.oe & REACTIVE_MISSED_WAKE) !== 0;
    e.oe = e.oe & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED) | (n ? e.oe & REACTIVE_SNAPSHOT_STALE : 0);
    context = d;
  }
  const R = stagedEntry;
  stagedEntry = E;
  if (e.oe & REACTIVE_DISPOSED) {
    clearDeps(e);
    if (e.o !== null)
      e.o.Ce = null;
    currentOptimisticLane = O;
    return;
  }
  if (!e.o?._) {
    const u2 = l ? unwrapOverride(e.o?.Fe) : i || e._e === NOT_PENDING ? e.ce : e._e;
    let s2 = false;
    try {
      s2 = !t && r || !e.xe || !e.xe(u2, N);
    } catch (n2) {
      notifyStatus(e, STATUS_ERROR, n2);
    }
    if (t && s2) {
      e.Xe = !e.o?._;
      if (!n) {
        e.T.enqueue(t, e.An ??= GlobalQueue.Cn.bind(null, e));
        let n2 = e.pn;
        if (n2 !== activeTransition) {
          e.pn = activeTransition;
          if (n2 !== null && (n2 = currentTransition(n2)) !== activeTransition && !n2.Rn) {
            (n2.Dn ??= []).push(e);
            if (activeTransition !== null)
              (activeTransition.Dn ??= []).push(e);
          }
        }
      }
    }
    if (e.o?._)
      ;
    else if (s2) {
      const u3 = l ? e.o?.Fe : undefined;
      if (n && R === null || t && R === null && (activeTransition !== e.we || activeTransition === null || e.C & CONFIG_DIRECT_COMMIT) || i) {
        if (i && !t && currentOptimisticLane !== null)
          GlobalQueue.Me(e, N, currentOptimisticLane);
        else
          e.ce = N;
        if (i)
          e._e = NOT_PENDING;
      } else {
        e._e = N;
        if (R !== null) {
          if (e.we !== R) {
            e.we = R;
            R.Gn.push(e);
          }
          if (t)
            R.In.add(e);
          if (underFreshLoadingBoundary(e))
            e.T.notify(e, STATUS_PENDING, STATUS_PENDING, new NotReadyError(e));
        }
        if (_)
          e.ve = true;
        if (e.C & CONFIG_HAS_COMPANIONS && GlobalQueue.Ve !== null)
          GlobalQueue.Ve(e, N);
      }
      if (e.u !== null && (!l || i || e.o?.Fe !== u3))
        insertSubs(e, i || l);
      else if (l && !i && e.o.hn !== clock)
        GlobalQueue.ke(e, N);
    } else if (l) {
      if (e._e === NOT_PENDING)
        queuePendingNode(e);
      e._e = N;
      if (_)
        e.ve = true;
      GlobalQueue.ke(e, N);
    } else if (e.rn != I) {
      for (let n2 = e.u;n2 !== null; n2 = n2.Ae) {
        insertIntoHeapHeight(n2.ge, queueFor(n2.ge));
      }
    }
    if (!s2 && !e.o?._) {
      if (o !== undefined)
        settleErroredDependents(e, o);
      if (a) {
        for (const n2 of a)
          if (n2 !== e)
            settlePendingSource(e, n2);
      }
    }
    if (c && !(e.h & (STATUS_PENDING | STATUS_UNINITIALIZED))) {
      settlePendingSource(e);
      wakeParked();
    }
  }
  const D = e.Nn;
  if (t && (s && !(e.h & STATUS_PENDING) || (D === null ? e.Ie !== null : D.Ne !== null)))
    wakeParked();
  if (!e.o?._ && e._e === NOT_PENDING && !(t && e.Xe)) {
    if (n || i || t === EFFECT_TRACKED)
      trimStaleDeps(e);
    else if (e.Nn?.Ne ?? e.Ie)
      heldTrims.push(e);
  }
  currentOptimisticLane = O;
  const G = (e.C & CONFIG_LANE_FRAME) !== 0;
  const h = e._e !== NOT_PENDING || !G && e.o !== null && (e.o._n !== null || e.o.fn !== null) || (e.h & (STATUS_PENDING | STATUS_UNINITIALIZED)) !== 0;
  let P = h && (!n || R !== null || (e.h & STATUS_PENDING) !== 0);
  if (P && (!e.we || l))
    queuePendingNode(e);
  else if (P && (activeTransition === null || i) && !(e.h & (STATUS_PENDING | STATUS_UNINITIALIZED))) {
    P = false;
    if (!G)
      disposeChildren(e, false, true);
  }
  if (P)
    e.C |= CONFIG_HELD_CHILDREN;
  else
    e.C &= ~CONFIG_HELD_CHILDREN;
  if (e.we && t && activeTransition !== e.we && R === null) {
    const n2 = e.pn;
    runInTransition(e.we, () => recompute(e));
    e.pn = n2;
  }
  if (T) {
    enqueueSub(e);
    schedule();
  }
}
function updateIfNecessary(e) {
  if (e.oe & (REACTIVE_RECOMPUTING_DEPS | REACTIVE_DISPOSED))
    return;
  if (e.oe & REACTIVE_CHECK) {
    for (let n = e.Ie;n; n = n.Ne) {
      const t = n.Oe;
      const i = t.De || t;
      if (i.he) {
        updateIfNecessary(i);
      }
      if (e.oe & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.oe & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.o?._ && e.be < clock && !e.o?.Ce) {
    recompute(e);
  }
  e.oe = e.oe & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_ZOMBIE | REACTIVE_DISPOSED | REACTIVE_MANUAL_WRITE);
}
function computed(e, n) {
  const t = n?.transparent ?? false;
  const i = n !== null && typeof n === "object" && "loadingValue" in n;
  const u = {
    id: inheritId(n, t, context),
    C: (t ? CONFIG_TRANSPARENT : 0) | (n?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || n?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (n?.sync ? CONFIG_SYNC : 0) | (n?.H ? CONFIG_NO_SNAPSHOT : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    xe: n?.equals ?? isEqual,
    qe: null,
    T: context?.T ?? globalQueue,
    Je: context?.Je ?? defaultContext,
    En: 0,
    he: e,
    ce: i ? n.loadingValue : undefined,
    rn: 0,
    Pn: undefined,
    Fn: null,
    Ie: null,
    Nn: null,
    Ye: 0,
    u: null,
    mn: null,
    _parent: context,
    un: null,
    gn: null,
    tn: null,
    oe: n?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    h: i ? 0 : STATUS_UNINITIALIZED,
    be: clock,
    _e: NOT_PENDING,
    we: null,
    vn: -1,
    ve: i,
    o: null
  };
  if (n?.unobserved)
    ext(u).Hn = n.unobserved;
  setupComputedNode(u, n);
  return u;
}
function ext(e) {
  return e.o ??= {
    Fe: undefined,
    Ln: undefined,
    hn: 0,
    bn: NOT_PENDING,
    Vn: 0,
    me: undefined,
    Ze: undefined,
    He: undefined,
    kn: undefined,
    t: 0,
    Ce: null,
    ye: null,
    _: undefined,
    Pe: undefined,
    ue: undefined,
    S: undefined,
    Le: false,
    i: null,
    Hn: undefined,
    sn: undefined,
    fn: null,
    _n: null,
    Un: undefined
  };
}
function createEffectNode(e, n, t, i, u) {
  const l = u?.transparent ?? false;
  const r = {
    id: inheritId(u, l, context),
    C: (l ? CONFIG_TRANSPARENT : 0) | (u?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (u?.sync ? CONFIG_SYNC : 0) | (u?.yn ?? 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    xe: false,
    qe: null,
    T: context?.T ?? globalQueue,
    Je: context?.Je ?? defaultContext,
    En: 0,
    he: e,
    ce: undefined,
    rn: 0,
    Pn: undefined,
    Fn: null,
    Ie: null,
    Nn: null,
    Ye: 0,
    u: null,
    mn: null,
    _parent: context,
    un: null,
    gn: null,
    tn: null,
    oe: REACTIVE_LAZY,
    h: STATUS_UNINITIALIZED,
    be: clock,
    _e: NOT_PENDING,
    we: null,
    vn: -1,
    ve: false,
    Xe: false,
    xn: undefined,
    Qn: n,
    Mn: t,
    wn: undefined,
    je: i,
    pn: null,
    o: null
  };
  if (u?.unobserved)
    ext(r).Hn = u.unobserved;
  setupComputedNode(r, lazyOptions);
  return r;
}
var effectStatusNotify = null;
function setEffectStatusNotify(e) {
  effectStatusNotify = e;
}
function statusNotifierOf(e) {
  const n = e.o?.S;
  if (n !== undefined)
    return n;
  return e.je ? effectStatusNotify ?? undefined : undefined;
}
var lazyOptions = {
  lazy: true
};
function setupComputedNode(e, n) {
  e.Fn = e;
  const t = context?.Wn ? context.qn : context;
  if (context)
    linkChild(context, e);
  if (t)
    e.rn = t.rn + 1;
  if (GlobalQueue.Zn !== null)
    GlobalQueue.Zn(e);
  !n?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !n?.lazy) {
    if (!(e.h & STATUS_PENDING) && !(e.C & CONFIG_NO_SNAPSHOT)) {
      ext(e).sn = e.ce === undefined ? NO_SNAPSHOT : e.ce;
      e.C |= CONFIG_HAS_SNAPSHOT;
      snapshotSources.add(e);
    }
  }
}
function signal(e, n, t = null) {
  const i = {
    xe: n?.equals ?? isEqual,
    C: (n?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (n?.H ? CONFIG_NO_SNAPSHOT : 0),
    ce: e,
    u: null,
    mn: null,
    be: clock,
    De: t,
    Ue: t?.o?.i || null,
    Yn: null,
    _e: NOT_PENDING,
    we: null,
    vn: -1,
    o: null
  };
  if (n?.unobserved)
    ext(i).Hn = n.unobserved;
  if (t)
    linkFirewallChild(t, i);
  if (snapshotCaptureActive && !(i.C & CONFIG_NO_SNAPSHOT) && !((t?.h ?? 0) & STATUS_PENDING)) {
    ext(i).sn = e === undefined ? NO_SNAPSHOT : e;
    i.C |= CONFIG_HAS_SNAPSHOT;
    snapshotSources.add(i);
  }
  return i;
}
var slotUnobservedHook;
function linkFirewallChild(e, n) {
  const t = n.Ue;
  if (t !== null)
    t.Yn = n;
  ext(e).i = n;
  e.C |= CONFIG_FW_CHILDREN;
}
function isEqual(e, n) {
  return e === n;
}
function untrack(e, n) {
  if (GlobalQueue.jn === null && !tracking && true)
    return e();
  const t = tracking;
  tracking = false;
  try {
    if (GlobalQueue.jn !== null)
      return GlobalQueue.jn(e);
    return e();
  } finally {
    tracking = t;
  }
}
var spectating = false;
function spectate(e) {
  const n = spectating;
  spectating = true;
  try {
    return untrack(e);
  } finally {
    spectating = n;
  }
}
function prepareComputed(e, n) {
  if (e.oe & REACTIVE_LAZY) {
    e.oe &= ~REACTIVE_LAZY;
    recompute(e, true);
  } else if (e.oe & REACTIVE_DISPOSED) {
    if (e.C & CONFIG_AUTO_DISPOSE) {
      const n2 = e._parent;
      if (n2 !== null) {
        if (n2.oe & REACTIVE_DISPOSED) {
          e.C &= ~CONFIG_AUTO_DISPOSE;
          return;
        }
        if (!(e.oe & REACTIVE_ZOMBIE))
          linkChild(n2, e);
      }
      recompute(e, true);
    }
  } else if (n) {
    updateIfNecessary(e);
  }
}
var READ_SLOW = Symbol("read-slow");
function recordStaleReplay(e, n) {
  const t = n.pn;
  if (t == null || currentTransition(t) !== e)
    e.In.add(n);
}
function ownsHold(e) {
  return activeTransition !== null && currentTransition(e) === currentTransition(activeTransition);
}
function heldFromStale(e, n) {
  const t = e.we;
  if (t === null || ownsHold(t))
    return false;
  const i = currentTransition(t);
  recordStaleReplay(i, n);
  const u = i.le.get(e);
  if (u)
    u.add(n);
  else if (e.h & STATUS_PENDING)
    runInTransition(i, () => n.T.notify(n, STATUS_PENDING, STATUS_PENDING, e.o._));
  return true;
}
var stagedEntry = null;
function underFreshLoadingBoundary(e) {
  for (let n = e.T;n !== null; n = n._parent)
    if (n.te & STATUS_PENDING)
      return !n.q;
  return false;
}
function enterStagedRead(e, n = e.we) {
  if (!n || n === activeTransition || pendingCheckActive)
    return;
  if (e?.o?.kn || context?.o?.kn)
    return;
  if (spectating)
    return;
  const t = context;
  const i = activeTransition === null && !globalQueue.Jn;
  if (i && GlobalQueue.Xn)
    return;
  if (t.oe & REACTIVE_RECOMPUTING_DEPS && !(t.C & CONFIG_OPTIMISTIC) && (stagedEntry === null || stagedEntry === n) && (i || !GlobalQueue.Xn && underFreshLoadingBoundary(t))) {
    stagedEntry = n;
    return;
  }
  globalQueue.initTransition(n);
}
function readerSeesCommitted(e, n, t, i) {
  return !!(!n || currentOptimisticLane !== null && GlobalQueue.$n(e, t, n) || e._e === NOT_PENDING || n.C & CONFIG_CHILDREN_FORBIDDEN || stale && !i && heldFromStale(e, n) || e.C & CONFIG_HELD_TRUTH && currentOptimisticLane !== null && !latestReadActive && !(n.C & CONFIG_AUTHORITATIVE_READ));
}
var unflushedStaged = false;
function markUnflushedStaged() {
  unflushedStaged = true;
}
function unflushedValue(e, n = e.ce) {
  if (globalQueue.Jn || e._e === NOT_PENDING || e.C & CONFIG_PROMOTED || e.o?.kn)
    return NOT_PENDING;
  if (e.we === null || e.C & CONFIG_ADOPTED_UNFLUSHED)
    return n;
  return e.o === null ? NOT_PENDING : e.o.bn;
}
var unflushedRewrites = [];
var promotedWrites = [];
function unflushedOverride(e) {
  return !globalQueue.Jn && e.o?.hn === clock && !e.o?.kn;
}
function hasActiveOverride(e) {
  const n = e.o;
  return n !== null && n.Fe !== undefined && n.Fe !== NOT_PENDING;
}
function visibleOverride(e) {
  return hasActiveOverride(e) && !unflushedOverride(e);
}
function markLateLinker(e) {
  e.oe |= REACTIVE_MISSED_WAKE;
  return true;
}
var unflushedCompanions = [];
function resyncUnflushedCompanions() {
  unflushedStaged = false;
  if (unflushedRewrites.length !== 0) {
    for (const e of unflushedRewrites)
      e.o.bn = NOT_PENDING;
    unflushedRewrites.length = 0;
  }
  if (promotedWrites.length !== 0) {
    for (const e of promotedWrites)
      e.C &= ~CONFIG_PROMOTED;
    promotedWrites.length = 0;
  }
  if (unflushedCompanions.length !== 0) {
    for (const e of unflushedCompanions)
      GlobalQueue.Ve(e, e._e !== NOT_PENDING ? e._e : e.ce);
    unflushedCompanions.length = 0;
  }
}
function read(e) {
  if (latestReadActive)
    return GlobalQueue.et(e);
  let n = context;
  if (n?.Wn)
    n = n.qn;
  const t = e;
  const i = e.De;
  const u = i || e;
  if (pendingCheckActive) {
    GlobalQueue.nt(e, n, u, i);
  } else if (typeof t.he === "function") {
    prepareComputed(e, false);
  }
  if (!t.he && u === e && e.o?.Fe === undefined && e.o?.sn === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && (!unflushedStaged || e._e === NOT_PENDING) && true) {
    if (n && tracking)
      link(e, n);
    return !n || e._e === NOT_PENDING || n.C & CONFIG_CHILDREN_FORBIDDEN || stale && heldFromStale(e, n) ? e.ce : (enterStagedRead(e), e._e);
  }
  if (n && tracking) {
    link(e, n, pendingCheckActive);
    if (u.he) {
      const t2 = queueFor(e);
      if (u.rn >= t2.ln) {
        markNode(n);
        markHeap(t2);
        updateIfNecessary(u);
      } else if (n.C & CONFIG_FRESH_READ)
        updateIfNecessary(u);
      const i2 = u.rn;
      if (i2 >= n.rn && e._parent !== n) {
        n.rn = i2 + 1;
      }
    }
  }
  if (u.h & STATUS_PENDING) {
    if (n && !(stale && !(u.h & STATUS_UNINITIALIZED) && !(u.C & CONFIG_INPUTS_PUBLISHED) && !(u.C & CONFIG_HAS_LANE && GlobalQueue.tt(u)) && heldFromStale(u, n))) {
      if (currentOptimisticLane === null || GlobalQueue.it(u)) {
        if (!tracking && !spectating && e !== n)
          link(e, n);
        throw u.o?._;
      }
    } else if (!n && u.h & STATUS_UNINITIALIZED && !(hasActiveOverride(e) && e.C & CONFIG_DERIVED_OVERRIDE)) {
      throw u.o?._;
    }
  }
  if (u.he && u.h & STATUS_ERROR) {
    if (tracking && !pendingCheckActive && u.be < clock) {
      recompute(u);
      return read(e);
    } else
      throw u.o?._;
  }
  if (snapshotCaptureActive && n && n.C & CONFIG_IN_SNAPSHOT_SCOPE) {
    const t2 = e.o?.sn;
    if (t2 !== undefined) {
      const i2 = t2 === NO_SNAPSHOT ? undefined : t2;
      const u2 = e._e !== NOT_PENDING ? e._e : e.ce;
      if (u2 !== i2)
        n.oe |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  const l = serve(e, n, u, e.ce);
  if (!n && u === e && typeof t.he === "function" && e.C & CONFIG_AUTO_DISPOSE && !(u.h & STATUS_PENDING) && !e.u && !visibleOverride(e)) {
    dormantNodes.add(e);
    schedule();
  }
  return l;
}
function serve(e, n, t, i) {
  if (hasActiveOverride(e)) {
    if (!(n && n.C & CONFIG_AUTHORITATIVE_READ) && !unflushedOverride(e)) {
      if (n && e.C & (CONFIG_HAS_LANE | CONFIG_OVERRIDE_SUPERSEDED))
        return GlobalQueue.ut(e, n);
      return unwrapOverride(e.o?.Fe);
    }
    e.C |= CONFIG_AUTHORITATIVE_OBSERVED;
  }
  if (currentOptimisticLane !== null && activeTransition !== null && n !== null && GlobalQueue.lt(e, t, n)) {
    return i;
  }
  const u = e._e !== NOT_PENDING && (e.h & STATUS_UNINITIALIZED) !== 0;
  if (u && (!n || spectating))
    throw new NotReadyError(null);
  const l = n && unflushedStaged ? unflushedValue(e, i) : NOT_PENDING;
  if (l !== NOT_PENDING) {
    markLateLinker(n);
    if (pendingCheckActive)
      GlobalQueue.rt(e, l);
    return l;
  }
  const r = readerSeesCommitted(e, n, t, u) ? i : (enterStagedRead(e), e._e);
  if (pendingCheckActive)
    GlobalQueue.rt(e, r);
  return r;
}
function stashHeldRewrite(e) {
  if (globalQueue.Jn)
    return;
  const n = ext(e);
  if (n.bn === NOT_PENDING) {
    n.bn = e._e;
    unflushedRewrites.push(e);
    unflushedStaged = true;
  }
}
function notePromotedWrite(e) {
  if (globalQueue.Jn || e.C & CONFIG_PROMOTED)
    return;
  e.C |= CONFIG_PROMOTED;
  promotedWrites.push(e);
}
function captureWriteSnapshot(e, n) {
  if (e.C & CONFIG_NO_SNAPSHOT || e.he !== undefined || e.De || e.o?.sn !== undefined)
    return;
  ext(e).sn = n === undefined ? NO_SNAPSHOT : n;
  e.C |= CONFIG_HAS_SNAPSHOT;
  snapshotSources.add(e);
}
function setSignal(e, n) {
  if (e.we && activeTransition !== e.we) {
    if (globalQueue.Jn)
      globalQueue.initTransition(e.we);
    else {
      batchJoins.push(e.we);
      schedule();
    }
  }
  if (e.C & CONFIG_OPTIMISTIC) {
    if (!projectionWriteActive)
      return GlobalQueue.ot(e, n);
    const t2 = e.o?.Fe;
    if (t2 !== undefined && t2 !== NOT_PENDING)
      return GlobalQueue.st(e, n);
  }
  const t = e._e === NOT_PENDING ? e.ce : e._e;
  if (typeof n === "function")
    n = n(t);
  const i = !!(e.h & STATUS_UNINITIALIZED) || !e.xe || !e.xe(t, n);
  if (!i)
    return n;
  if (snapshotCaptureActive)
    captureWriteSnapshot(e, t);
  const u = e._e !== NOT_PENDING;
  if (!u)
    queuePendingNode(e);
  else if (e.we !== null)
    stashHeldRewrite(e);
  e._e = n;
  if (context !== null)
    notePromotedWrite(e);
  if (e.C & CONFIG_HAS_COMPANIONS && GlobalQueue.Ve !== null) {
    GlobalQueue.Ve(e, n);
    if (!globalQueue.Jn)
      unflushedCompanions.push(e);
  }
  if (e.he !== undefined)
    e.be = clock;
  if (u && e.vn === notifyEpoch && currentOptimisticLane === null && !reaskArmed)
    return n;
  insertSubs(e);
  schedule();
  return n;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, queueFor(e));
  if (!(e.oe & REACTIVE_MANUAL_WRITE) && e._e === NOT_PENDING) {
    queuePendingNode(e);
    schedule();
  }
  e.oe = e.oe & -4 | REACTIVE_MANUAL_WRITE;
  e.ct = clock;
}
function heldDerivation(e) {
  return e.we !== null && activeTransition !== e.we && !((e.De || e).oe & REACTIVE_MANUAL_WRITE);
}
function rederiveHeld(e) {
  e.oe |= REACTIVE_DIRTY;
  enqueueSub(e);
}
function setMemo(e, n) {
  const t = heldDerivation(e);
  if (t && typeof n === "function")
    n = n(e.ce);
  const i = setSignal(e, n);
  t ? rederiveHeld(e) : suppressComputedRecompute(e);
  return i;
}
function runWithOwner(e, n) {
  const t = context;
  const i = tracking;
  context = e;
  tracking = false;
  try {
    return n();
  } finally {
    context = t;
    tracking = i;
  }
}
function staleValues(e, n = true) {
  const t = stale;
  stale = n;
  try {
    return e();
  } finally {
    stale = t;
  }
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/context.js
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
  let r = t.Je[e.id];
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
  r.Je = {
    ...r.Je,
    [e.id]: t === undefined ? e.defaultValue : t
  };
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/effect.js
function effect(t, e, E, r) {
  const n = !!r?.user;
  const i = createEffectNode(t, e, E, n ? EFFECT_USER : EFFECT_RENDER, r);
  recompute(i, true);
  !r?.defer && i._e === NOT_PENDING && (i.je === EFFECT_USER || r?.schedule ? i.T.enqueue(i.je, runEffect.bind(null, i)) : runEffect(i, LANE_RUN));
}
function notifyEffectStatus(t, e) {
  const E = t !== undefined ? t : this.h;
  const r = e !== undefined ? e : this.o?._;
  if (E & STATUS_ERROR) {
    this.T.notify(this, STATUS_PENDING, 0);
    if (this.je === EFFECT_USER) {
      if (this.h & STATUS_ERROR) {
        this.Xe = true;
        this.T.enqueue(this.je, this.An ??= runEffect.bind(null, this));
      }
      return;
    }
    if (!this.T.notify(this, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(unwrapStatusError(r));
      throw r;
    }
  } else if (this.je === EFFECT_RENDER) {
    this.T.notify(this, STATUS_PENDING | STATUS_ERROR, E, r);
  }
}
function runEffect(t, e) {
  if (!t.Xe || t.oe & REACTIVE_DISPOSED)
    return;
  if (t.pn !== null && !currentTransition(t.pn).Rn && (e & LANE_RUN ? !t.o?.me : activeTransition !== null)) {
    t.T.enqueue(t.je, t.An);
    return;
  }
  if (t.h & STATUS_ERROR && t.je === EFFECT_USER) {
    const e2 = unwrapStatusError(t.o?._);
    t.xn = t.ce;
    t.Xe = false;
    try {
      t.Mn ? t.Mn(e2, () => {
        const e3 = t.wn;
        t.wn = undefined;
        e3?.();
      }) : console.error(e2);
    } catch (e3) {
      if (!t.T.notify(t, STATUS_ERROR, STATUS_ERROR)) {
        haltReactivity(e3);
        throw e3;
      }
    }
    return;
  }
  const E = t.o?._ == null;
  const r = t.wn;
  t.wn = undefined;
  try {
    r?.();
    const e2 = t.Qn(t.ce, t.xn);
    if (false)
      ;
    t.wn = e2;
  } catch (e2) {
    ext(t)._ = new StatusError(t, e2);
    t.h |= STATUS_ERROR;
    if (!t.T.notify(t, STATUS_ERROR, STATUS_ERROR)) {
      haltReactivity(e2);
      throw e2;
    }
  } finally {
    t.xn = t.ce;
    t.Xe = false;
    if (E)
      trimStaleDeps(t);
  }
}
GlobalQueue.Cn = runEffect;
function trackedEffect(t, e) {
  const run = () => {
    if (!E.Xe || E.oe & REACTIVE_DISPOSED)
      return;
    try {
      E.Xe = false;
      recompute(E);
    } finally {}
  };
  const E = computed(() => {
    const e2 = E.wn;
    E.wn = undefined;
    e2?.();
    const r = staleValues(t);
    E.wn = r;
  }, {
    ...e,
    lazy: true
  });
  E.wn = undefined;
  E.C = E.C & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  E.Xe = true;
  E.je = EFFECT_TRACKED;
  E.$e = run;
  enqueueSub(E);
  schedule();
}
setEffectStatusNotify(notifyEffectStatus);

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/core/error-hooks.js
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/signals.js
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
    n2.C &= ~CONFIG_AUTO_DISPOSE;
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
  t && !(t.C & CONFIG_CHILDREN_FORBIDDEN) ? trackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, function fire() {
    if (dirtyQueue.EE >= dirtyQueue.ln)
      return globalQueue.enqueue(EFFECT_USER, fire);
    e();
  });
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $RECORD = Symbol(0);
var $AFFECTS = Symbol(0);
var rawValues = new WeakSet;
var OBJECT_PROTO = Object.prototype;
var wrappableProtos = new WeakMap;
function ownEnumerableKeys(e) {
  return Reflect.ownKeys(e).filter((t) => Object.prototype.propertyIsEnumerable.call(e, t));
}
var affectsScopes = new Map;

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/boundaries.js
function boundaryComputed(e, t) {
  const r = computed(e, {
    lazy: true
  });
  ext(r).S = (e2, t2) => {
    const n = e2 !== undefined ? e2 : r.h;
    const i = t2 !== undefined ? t2 : r.o?._;
    r.h &= ~r.R;
    const s = r.T.notify(r, STATUS_PENDING | STATUS_ERROR, n, i);
    const o = n & ~r.R & (STATUS_PENDING | STATUS_ERROR);
    if (o) {
      r.h &= ~o;
      if (r.o?._ === i && !(r.h & (STATUS_PENDING | STATUS_ERROR))) {
        if (r.o !== null)
          r.o._ = undefined;
      }
    }
    if (!s && n & STATUS_ERROR) {
      haltReactivity(unwrapStatusError(i));
      throw i;
    }
  };
  r.R = t;
  r.C &= ~CONFIG_AUTO_DISPOSE;
  recompute(r, true);
  return r;
}
function onNode(e, t, r) {
  let n = false;
  const i = runWithOwner(e, () => computed(() => {
    try {
      r();
    } catch (e2) {
      if (!(e2 instanceof NotReadyError))
        throw e2;
    }
    if (n) {
      if (currentOptimisticLane !== null)
        t.O = currentOptimisticLane;
      queueRearm(t);
    } else
      n = true;
  }, {
    lazy: true
  }));
  ext(i).S = (e2, t2) => {
    const r2 = e2 !== undefined ? e2 : i.h;
    if (r2 & STATUS_PENDING) {
      i.h &= ~STATUS_PENDING;
      if (i.o?._ instanceof NotReadyError)
        i.o._ = undefined;
      enqueueSub(i);
      schedule();
    }
    if (r2 & STATUS_ERROR) {
      const e3 = t2 !== undefined ? t2 : i.o?._;
      i.h &= ~STATUS_ERROR;
      if (i.o?._ === e3 && i.o !== null)
        i.o._ = undefined;
      if (!i.T.notify(i, STATUS_ERROR, r2, e3)) {
        haltReactivity(unwrapStatusError(e3));
        throw e3;
      }
    }
  };
  i.C &= ~CONFIG_AUTO_DISPOSE;
  recompute(i, true);
  return i;
}
function createBoundChildren(e, t, r, n) {
  const i = e.T;
  i.addChild(e.T = r);
  cleanup(() => i.removeChild(e.T));
  return runWithOwner(e, () => {
    const e2 = computed(t);
    return boundaryComputed(() => flatten(read(e2)), n);
  });
}
var RevealControllerContext = /* @__PURE__ */ createContext(null);
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.I() : e.U.size === 0 && !e.v;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.D() : isSlotReady(e);
}
function setSlotState(e, t, r, n) {
  setSignal(e.P, r);
  setSignal(e.L, n);
  if (isRevealController(e)) {
    if (!r && e.j === t)
      e.j = undefined;
    return e.B(r, n);
  }
  if (!r && e.W === t && e.q)
    e.W = undefined;
}

class RevealController {
  V;
  F;
  Z = [];
  j;
  P = signal(false, {
    ownedWrite: true,
    H: true
  });
  L = signal(false, {
    ownedWrite: true,
    H: true
  });
  J = true;
  K = true;
  X = false;
  constructor(e, t) {
    this.V = e;
    this.F = t;
  }
  Y(e) {
    for (let t = 0;t < this.Z.length; t++) {
      const r = this.Z[t];
      if ((isRevealController(r) ? r.j : r.W) !== this)
        continue;
      if (e(r) === false)
        return false;
    }
    return true;
  }
  I() {
    return this.Y(isSlotReady);
  }
  D() {
    const e = untrack(this.V);
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
  $(e) {
    if (this.Z.includes(e))
      return;
    this.Z.push(e);
    const t = untrack(this.V);
    setSignal(e.P, true), setSignal(e.L, t === "sequential" ? !!untrack(this.F) : false);
    untrack(() => this.B());
  }
  ee(e) {
    const t = this.Z.indexOf(e);
    if (t >= 0)
      this.Z.splice(t, 1);
    untrack(() => this.B());
  }
  B(e, t) {
    if (this.X)
      return;
    this.X = true;
    const r = this.J;
    const n = this.K;
    try {
      const r2 = e ?? read(this.P), n2 = untrack(this.V), i = n2 === "sequential" && !!untrack(this.F), s = t ?? i;
      if (r2) {
        this.Y((e2) => setSlotState(e2, this, true, s));
      } else if (n2 === "natural") {
        this.Y((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.L, false);
            setSignal(e2.P, false);
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
            return setSlotState(t2, this, true, i);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.L, false);
            setSignal(t2.P, false);
            t2.B(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.J = this.I();
      this.K = this.D();
      this.X = false;
    }
    if (this.j && (r !== this.J || n !== this.K))
      this.j.B();
  }
}

class CollectionQueue extends Queue {
  te;
  U = new Set;
  re;
  ne;
  v = true;
  P = signal(false, {
    ownedWrite: true,
    H: true
  });
  _;
  L = signal(false, {
    ownedWrite: true,
    H: true
  });
  W;
  q = false;
  ie;
  O = null;
  constructor(e) {
    super();
    this.te = e;
  }
  run(e) {
    if (!e || read(this.P) && (!_revealUsed || read(this.L)))
      return;
    return super.run(e);
  }
  se() {
    const e = this.O;
    this.O = null;
    if (this.re === undefined || this.re.oe & REACTIVE_DISPOSED)
      return;
    if (!this.q)
      return;
    const t = new Set;
    for (const e2 of transitions)
      for (const [r, n] of e2.le) {
        for (const e3 of n)
          if (this.ae(e3) && reporterBlocksSource(e3, r)) {
            t.add(r);
            e3.o?.ue?.forEach((e4) => t.add(e4));
          }
      }
    if (!t.size)
      return;
    this.q = false;
    this.U = t;
    this.v = true;
    this.fe(e);
    wakeParked();
  }
  fe(e) {
    if (e === null)
      setSignal(this.P, true);
    else {
      this.P.ce = true;
      notifyOnLane(this.P, e);
    }
  }
  Se() {
    for (const e of this.U) {
      if (e.he !== undefined)
        recompute(e);
    }
    schedule();
  }
  notify(e, t, r, n) {
    if (!(t & this.te))
      return super.notify(e, t, r, n);
    if (this.te & STATUS_PENDING && this.q)
      return super.notify(e, t, r, n);
    if (r & this.te) {
      this.v = true;
      const t2 = n?.source || e.o?._?.source;
      if (t2) {
        const r2 = this.U.size === 0;
        this.U.add(t2);
        if (this.te & STATUS_PENDING)
          e.o?.ue?.forEach((e2) => this.U.add(e2));
        if (r2) {
          setSignal(this.P, true);
        }
        if (this.te & STATUS_ERROR) {
          const e2 = unwrapStatusError(t2.o?._);
          setSignal(this._, e2);
          reportClientError(e2, this.ie, t2);
        }
      }
    }
    t &= ~this.te;
    return t ? super.notify(e, t, r, n) : true;
  }
  ae(e) {
    if (e.oe & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
      return false;
    for (let t = e.T;t; t = t._parent) {
      if (t === this)
        return true;
      if (t.te & STATUS_PENDING && !t.q)
        return false;
    }
    return false;
  }
  de(e) {
    return !!(e.oe & REACTIVE_DISPOSED || !e.o?.t && !(e.h & this.te) && !(this.te & STATUS_ERROR && e.h & STATUS_PENDING) && !(this.te & STATUS_PENDING && e.h & STATUS_UNINITIALIZED && e._e !== NOT_PENDING));
  }
  Re() {
    if (this.q || !(this.ne.h & STATUS_PENDING))
      return;
    this.Ee();
  }
  Ee() {
    for (const e of this.U)
      if (this.de(e))
        this.U.delete(e);
    if (!this.U.size) {
      if (this.te & STATUS_PENDING && this.v && !this.q && this.re) {
        this.v = !!(this.re.h & this.te);
      } else {
        this.v = false;
      }
      if (!this.v) {
        setSignal(this.P, false);
      }
    }
    if (_revealUsed)
      this.W?.B();
  }
}
function createCollectionBoundary(e, t, r, n) {
  const i = createOwner();
  if (_revealUsed)
    setContext(RevealControllerContext, null, i);
  const s = new CollectionQueue(e);
  s.ie = i;
  if (e === STATUS_ERROR)
    s._ = signal(undefined, {
      ownedWrite: true,
      H: true
    });
  n && onNode(i, s, n);
  const o = s.re = createBoundChildren(i, t, s, e);
  spectate(() => {
    let t2 = false;
    try {
      read(o);
    } catch (e2) {
      if (e2 instanceof NotReadyError)
        t2 = true;
      else
        throw e2;
    }
    s.v = t2 || !!(o.h & e) || o.o?._ instanceof NotReadyError;
  });
  const l = _revealUsed && e === STATUS_PENDING ? getContext(RevealControllerContext) : null;
  if (l) {
    s.W = l;
    l.$(s);
    cleanup(() => l.ee(s));
  }
  return accessor(s.ne = computed(() => {
    if (!read(s.P)) {
      const e2 = read(o);
      if (!untrack(() => read(s.P)))
        return s.q = true, e2;
    }
    if (_revealUsed && read(s.L))
      return;
    return r(s);
  }, {
    H: true
  }));
}
function createErrorBoundary(e, t) {
  return createCollectionBoundary(STATUS_ERROR, e, (e2) => t(accessor(e2._), () => e2.Se()));
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
  let i = false;
  for (let s = 0;s < e.length; s++) {
    try {
      let n2 = e[s];
      if (typeof n2 === "function" && !n2.length) {
        if (r?.doNotUnwrap) {
          t.push(n2);
          i = true;
          continue;
        }
        do {
          n2 = n2();
        } while (typeof n2 === "function" && !n2.length);
      }
      if (Array.isArray(n2)) {
        i = flattenArray(n2, t, r) || i;
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
  return i;
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/utils.js
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
function recordOf(e) {
  return e[$RECORD];
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
  const n = recordOf(e);
  if (n instanceof MergeView)
    return mergeHasStaticKeys(n);
  return n !== undefined && entryHasStaticKeys(n, SOURCE_OMIT);
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
    if (recordOf(e) !== undefined)
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
  return e != null && $PROXY in e ? e[$RECORD] : undefined;
}
function resolvedTable(e) {
  if (e == null || !($PROXY in e))
    return;
  const n = recordOf(e);
  if (n instanceof OmitView)
    return omitTable(n);
  return n === undefined ? undefined : mergeTable(n);
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
    if (t2 === SOURCE_PLAIN) {
      const t3 = r[e2];
      if (n in t3)
        return t3[n];
      continue;
    }
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
    if (typeof n === "symbol") {
      if (n === $PROXY)
        return t;
      if (n === $RECORD)
        return e;
      if (n === $TARGET)
        return;
    }
    const r = e.table;
    let i;
    if (typeof r !== "object") {
      if (r + 1 < READS_FOR_TABLE) {
        e.table = r + 1;
        const { sources: t2, kinds: i2 } = e;
        for (let r2 = t2.length - 1;r2 >= 0; r2--) {
          if (i2[r2] !== SOURCE_PLAIN)
            return mergeGet(e, n);
          const f2 = t2[r2][n];
          if (f2 !== undefined || n in t2[r2])
            return f2;
        }
        return;
      }
      i = mergeTable(e);
      if (i === undefined)
        return mergeGet(e, n);
    } else if (r === null)
      return mergeGet(e, n);
    else
      i = r;
    const f = i.get(n);
    return f === undefined ? undefined : f[n];
  },
  has(e, n) {
    if (n === $PROXY)
      return true;
    if (n === $TARGET || n === $RECORD)
      return false;
    const t = mergeReadTable(e);
    if (t !== undefined)
      return t.has(n);
    return mergeHas(e, n);
  },
  set: trueFn,
  deleteProperty: trueFn,
  getOwnPropertyDescriptor(e, n) {
    if (n === $PROXY || n === $TARGET || n === $RECORD)
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
  const n = new Array(e.length);
  const t = new Array(e.length);
  let r = 0;
  let i = undefined;
  let f = 0;
  for (let u2 = 0;u2 < e.length; u2++) {
    const o2 = e[u2];
    if (!o2)
      continue;
    f++;
    i = o2;
    if (typeof o2 === "function") {
      n[r] = createMemo(o2);
      t[r++] = SOURCE_MEMO;
      continue;
    }
    if ($PROXY in o2) {
      const e2 = recordOf(o2);
      if (e2 instanceof MergeView) {
        for (let i2 = 0;i2 < e2.sources.length; i2++) {
          n[r] = e2.sources[i2];
          t[r++] = e2.kinds[i2];
        }
      } else if (e2 !== undefined) {
        n[r] = e2;
        t[r++] = SOURCE_OMIT;
      } else {
        n[r] = o2;
        t[r++] = SOURCE_PROXY;
      }
      continue;
    }
    n[r] = o2;
    t[r++] = SOURCE_PLAIN;
  }
  if (r !== n.length) {
    n.length = r;
    t.length = r;
  }
  if (SUPPORTS_PROXY) {
    if (f === 1 && typeof i !== "function")
      return i;
    return new Proxy(new MergeView(n, t), mergeTraps);
  }
  const u = Object.create(null);
  let o = false;
  let c = n.length - 1;
  for (let e2 = c;e2 >= 0; e2--) {
    const t2 = n[e2];
    if (!t2) {
      e2 === c && c--;
      continue;
    }
    const r2 = Object.getOwnPropertyNames(t2);
    for (let n2 = r2.length - 1;n2 >= 0; n2--) {
      const i2 = r2[n2];
      if (i2 === "__proto__" || i2 === "constructor")
        continue;
      if (!u[i2]) {
        o = o || e2 !== c;
        const n3 = Object.getOwnPropertyDescriptor(t2, i2);
        u[i2] = n3.get ? {
          enumerable: true,
          configurable: true,
          get: n3.get.bind(t2)
        } : n3;
      }
    }
  }
  if (!o)
    return n[c];
  const s = {};
  const d = Object.keys(u);
  for (let e2 = d.length - 1;e2 >= 0; e2--) {
    const n2 = d[e2], t2 = u[n2];
    if (t2.get)
      Object.defineProperty(s, n2, t2);
    else
      s[n2] = t2.value;
  }
  return s;
}
// ../../node_modules/.bun/solid-js@2.0.0-rc.13/node_modules/solid-js/dist/solid.js
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
var LIVE_RESUME_FROM = Symbol.for("solid.LiveResumeFrom");
var LIVE_LOCAL = Symbol.for("solid.LiveLocal");
var openScopes = new Set;
var liveGates = new Map;
var nodeGate = new WeakMap;
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
// ../../node_modules/.bun/@solidjs+universal@2.0.0-rc.13+2ff9c3435a48139a/node_modules/@solidjs/universal/dist/universal.js
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
import { renderFrame } from "sol:render";
import { on as on2, once } from "sol:events";
import { exit as nativeExit, background as nativeBackground, registerProtocolHandler as nativeRegisterProtocolHandler } from "sol:app";
import { platform } from "flux:process";

// ../../packages/core/src/core.ts
import * as tree from "flux:rendertree";
import { on } from "sol:events";
var handlers = new Map;
var MOVE_BIT = 1;
var POINTER_INTEREST = {
  onPointerMove: MOVE_BIT,
  onPointerDown: 2,
  onPointerUp: 4,
  onPointerEnter: 8,
  onPointerLeave: 16,
  onWheel: 32,
  onPointerCancel: 64
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
var latestTick = 0;
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
  let unsubCancel = null;
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
    if (!bootstrap)
      latestTick = t;
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
    unsubCancel = on2("pointerCancel", (raw) => {
      bubble(raw, "onPointerCancel");
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
    if (unsubCancel)
      unsubCancel();
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
import { on as on3 } from "sol:events";
// ../../packages/core/src/gamepad.ts
import { on as on4 } from "sol:events";
var gamepadsAccessor;
var buttonTimes = [];
var lastPads = [];
function recordButtonTimes(pads, at) {
  for (let slot = 0;slot < Math.max(pads.length, lastPads.length); slot++) {
    let before = lastPads[slot]?.buttons ?? [];
    let after = pads[slot]?.buttons ?? [];
    let times = buttonTimes[slot] ??= new Map;
    for (let name of after)
      if (!before.includes(name))
        times.set(name, at);
    for (let name of before)
      if (!after.includes(name))
        times.set(name, at);
  }
  lastPads = pads;
}
function gamepadButtonChangedAt(slot, name) {
  return buttonTimes[slot]?.get(name) ?? null;
}
function gamepads() {
  if (!gamepadsAccessor) {
    runWithOwner(null, () => {
      let [pads, setPads] = createSignal([]);
      on4("gamepads", (e) => {
        let next = e.pads ?? [];
        recordButtonTimes(next, e.timeStamp);
        setPads(next);
      });
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
import { decodeImage as decodeImage2, encodeImage, encodeTexture, transcodeTexture } from "flux:image";
var imageCache = new Map;
// ../../packages/core/src/cursor.ts
import { decodeImage as decodeImage3 } from "flux:image";
import { createCursor as registerCursor, dropCursor } from "flux:rendertree";
// ../../packages/core/src/textures.ts
var KINDS = ["color", "normal", "data"];
var DEFAULT_COMPRESS = true;
var SMALL_CODEC_QUALITY = 0.75;
var ACCURATE_CODEC_QUALITY = 0.9;
function frozen(settings, files) {
  for (let kind of KINDS)
    Object.freeze(settings[kind]);
  for (let rule of files) {
    Object.freeze(rule.match);
    Object.freeze(rule);
  }
  return Object.freeze({
    ...settings,
    files: Object.freeze(files)
  });
}
var DEFAULT_TEXTURE_SETTINGS = frozen({
  color: {
    compress: DEFAULT_COMPRESS,
    codec: "etc1s",
    quality: SMALL_CODEC_QUALITY
  },
  normal: {
    compress: DEFAULT_COMPRESS,
    codec: "uastc",
    quality: ACCURATE_CODEC_QUALITY
  },
  data: {
    compress: DEFAULT_COMPRESS,
    codec: "uastc",
    quality: ACCURATE_CODEC_QUALITY
  }
}, []);
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
import { on as on5 } from "sol:events";
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
  let changedAt = null;
  let [count, setCount] = createSignal(0, {
    ownedWrite: true
  });
  return {
    count,
    key(event, isDown) {
      let onKey = specs.filter((s) => matches(event, s.key));
      let before = [...down].join();
      for (let s of onKey)
        down.delete(s.text);
      if (isDown)
        for (let s of mostSpecific(onKey, (s2) => s2.mods, event))
          down.add(s.text);
      if ([...down].join() !== before)
        changedAt = event.timeStamp;
      setCount(down.size);
    },
    blur() {
      if (down.size > 0)
        changedAt = null;
      down.clear();
      setCount(0);
    },
    changedAt: () => changedAt,
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
    changedAt: state.changedAt,
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
function createGamepadDevice(pads, slot, buttonChangedAt, who) {
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
          rate: anyButton(name),
          changedAt: () => buttonChangedAt(name)
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
    let all = read2.pads();
    if (slot === undefined)
      return all.filter((p) => p !== null);
    let pad = all[slot];
    return pad ? [pad] : [];
  };
  let changedAt = (name) => {
    if (slot !== undefined)
      return read2.buttonChangedAt(slot, name);
    let latest2 = null;
    let count = untrack(read2.pads).length;
    for (let i = 0;i < count; i++) {
      let at = read2.buttonChangedAt(i, name);
      if (at !== null && (latest2 === null || at > latest2))
        latest2 = at;
    }
    return latest2;
  };
  return createGamepadDevice(pads, () => slot, changedAt, slot === undefined ? "gamepad" : `gamepad ${slot}`);
}
var claimed = new Set;
function createGamepadJoin(read2) {
  let [slot, setSlot] = createSignal(undefined, {
    ownedWrite: true
  });
  let mine;
  let dispose2 = createRoot((dispose3) => {
    createEffect(() => read2.pads(), (pads2) => {
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
    let pad = read2.pads()[s];
    return pad ? [pad] : [];
  };
  let changedAt = (name) => {
    let s = untrack(slot);
    return s === undefined ? null : read2.buttonChangedAt(s, name);
  };
  return createGamepadDevice(pads, slot, changedAt, "gamepad (joined)");
}

// ../../packages/core/src/input-gamepad.ts
var reader = {
  pads: gamepads,
  buttonChangedAt: gamepadButtonChangedAt
};
function gamepad(slot) {
  return createGamepadSlot(reader, slot);
}
gamepad.next = () => createGamepadJoin(reader);
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
