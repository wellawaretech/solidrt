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
function deferSlotRelease(e) {
  transientStoreNodes.add(e);
}
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
function setProjectionWriteActive(e) {
  projectionWriteActive = e;
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
function scheduleWithheld() {
  if (withheld) {
    withheld = false;
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
function setStoreCommitHook(e) {
  storeCommitHook = e;
}
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
var PENDING_OWNER = {};
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
  if (!context.qe)
    context.qe = e;
  else if (Array.isArray(context.qe))
    context.qe.push(e);
  else
    context.qe = [context.qe, e];
  return e;
}
function isDisposed(e) {
  return !!(e.oe & (REACTIVE_DISPOSED | REACTIVE_ZOMBIE));
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
function setLatestReadActive(e) {
  latestReadActive = e;
}
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
function setSlotUnobserved(e) {
  slotUnobservedHook = e;
}
function linkFirewallChild(e, n) {
  const t = n.Ue;
  if (t !== null)
    t.Yn = n;
  ext(e).i = n;
  e.C |= CONFIG_FW_CHILDREN;
}
function unlinkFirewallChild(e) {
  const n = e;
  const t = n.De;
  if (!t)
    return;
  const i = n.Yn;
  const u = n.Ue;
  if (i !== null)
    i.Ue = u;
  else if (t.o.i === n)
    t.o.i = u;
  if (u !== null)
    u.Yn = i;
  n.Yn = null;
  t.o.Un?.delete(n);
}
function slotSignal(e, n, t, i, u, l = null) {
  const r = {
    xe: n,
    C: CONFIG_OWNED_WRITE | CONFIG_SLOT_NODE,
    ce: e,
    u: null,
    mn: null,
    be: clock,
    De: l,
    Ue: l?.o?.i || null,
    Yn: null,
    _e: NOT_PENDING,
    we: null,
    vn: -1,
    o: null,
    Bn: t,
    Kn: i,
    acc: u,
    px: undefined,
    pxv: undefined
  };
  if (l)
    linkFirewallChild(l, r);
  if (snapshotCaptureActive && !((l?.h ?? 0) & STATUS_PENDING)) {
    ext(r).sn = e === undefined ? NO_SNAPSHOT : e;
    r.C |= CONFIG_HAS_SNAPSHOT;
    snapshotSources.add(r);
  }
  return r;
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
function readNodeFast(e) {
  if (latestReadActive || pendingCheckActive || e.he || e.De || e.o?.Fe !== undefined || e.o?.sn !== undefined || activeTransition !== null || currentOptimisticLane !== null || snapshotCaptureActive || unflushedStaged && e._e !== NOT_PENDING || false)
    return READ_SLOW;
  let n = context;
  if (n?.Wn)
    n = n.qn;
  if (n && tracking)
    link(e, n);
  return !n || e._e === NOT_PENDING || n.C & CONFIG_CHILDREN_FORBIDDEN || stale && heldFromStale(e, n) ? e.ce : (enterStagedRead(e), e._e);
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
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/next/target.js
var $OWNER = Symbol(0);
var storeNextLookup = new WeakMap;
function isOwned(e) {
  return e[$OWNER] !== undefined;
}
function lookupTarget(e, o) {
  const t = e[$OWNER];
  return t !== undefined && t.fam === o ? t : (o?.map ?? storeNextLookup).get(e);
}
function devAssertNeverUserMutation(e) {
  return;
}
var optHooks = null;
function markDescendants(e) {
  let o = e;
  while (o && !o.d) {
    o.d = true;
    o = o.u;
  }
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/store.js
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $RECORD = Symbol(0);
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
    GlobalQueue.Et(o);
  if (affectsScopes.size) {
    let r = e[STORE_VALUE];
    for (const [e2, s] of affectsScopes) {
      if (e2 !== o && e2.o?.t && (s.key === undefined || s.key === t)) {
        let t2 = r;
        for (;; ) {
          if (s.scope.has(t2)) {
            GlobalQueue.Et(e2);
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

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/next/store.js
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
  this.kc = undefined;
  this.nc = undefined;
  this.ab = undefined;
  this.fam = undefined;
  this.s = undefined;
  this.ovl = undefined;
  this.del = undefined;
  this.wk = undefined;
  this.hv = undefined;
  this.ht = undefined;
}
TargetShape.prototype = Object.prototype;
function createTarget(e, t, n, r = t?.fam ?? null) {
  const i = Array.isArray(e) ? [] : new TargetShape;
  i.v = e;
  i.ch = e[$TARGET] !== undefined;
  i.pb = null;
  i.n = null;
  i.h = null;
  i.k = null;
  i.dk = null;
  i.wk = null;
  i.u = t;
  i.pk = n;
  i.px = null;
  i.d = false;
  i.a = false;
  i.sc = 0;
  i.kc = 0;
  i.nc = 0;
  i.ab = null;
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
  const i = lookupTarget(e, r);
  if (i !== undefined)
    return i.px;
  const l = e[$TARGET];
  if (l !== undefined && l.px === e) {
    if (r === null || l.fam === r)
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
var slotNodeEquals = function(e, t) {
  return isEqual(e, t) || sameLogicalSlot(this.Bn, e, t);
};
setSlotUnobserved((e) => {
  if (e.o?.t)
    return;
  if (hasActiveOverride(e) || e._e !== NOT_PENDING)
    return deferSlotRelease(e);
  const t = e.Bn;
  const n = e.Kn;
  if (t.n && t.n[n] === e) {
    delete t.n[n];
    t.nc--;
    unlinkFirewallChild(e);
  }
});
function getNode(e, t, n, r = -1) {
  const i = e.n ??= Object.create(null);
  let l = i[t];
  if (l === undefined) {
    const o = heldFoldTransition(e);
    let s = heldAdoptionTransition(e);
    if (s !== null && !heldKey(e, t))
      s = null;
    if (s !== null)
      n = e.hv[t];
    else if ((s = o) !== null)
      n = e.v[t];
    const f = l = slotSignal(n, slotNodeEquals, e, t, r === -1 ? isOwnAccessor(e.pb ?? e.v, t) : r === 1, e.fam?.node ?? undefined);
    if (e.fam?.opt) {
      ext(f).Fe = NOT_PENDING;
      f.C |= CONFIG_OPTIMISTIC;
    }
    if (t !== $AFFECTS && affectsScopesLive())
      inheritAffectsMarks(f, e.v, t);
    if (s !== null)
      stageHeldKey(f, o !== null ? e.del !== null && e.del.has(t) ? undefined : e.pb[t] : e.v[t], s);
    i[t] = l;
    e.nc++;
    markDescendants(e);
  }
  return l;
}
function heldAdoptionTransition(e) {
  if (e.ht === null || e.ht === PLAIN_HOLD || heldMaskView(e) === null)
    return null;
  const t = currentTransition(e.ht);
  return t.Rn === false ? t : null;
}
function liveFoldTransition(e) {
  if (e.pb === null)
    return null;
  const t = foldBatches.get(e);
  if (t === undefined)
    return null;
  const n = currentTransition(t);
  return n.Rn === false ? n : null;
}
function heldFoldTransition(e) {
  if (e.ch || e.fam?.opt === true || inDraft(e))
    return null;
  return liveFoldTransition(e);
}
function holdVisible(e, t) {
  if (e === null || ownsHold(e))
    return true;
  if (stale) {
    recordStaleReplay(e, t);
    return false;
  }
  enterStagedRead(null, e);
  return true;
}
var heldKeys = new WeakMap;
function heldKey(e, t) {
  let n = heldKeys.get(e);
  if (Array.isArray(n))
    heldKeys.set(e, n = adoptionChangedKeys(e, n[0]));
  return n === WK_ALL || n.has(t);
}
function adoptionChangedKeys(e, t) {
  const n = e.hv;
  if (t[$TARGET] !== undefined || e.fam?.opt === true || n[$TARGET] !== undefined || Object.getPrototypeOf(n) !== Object.getPrototypeOf(t) || !plainProto(t))
    return WK_ALL;
  const r = new Set;
  for (const i of Reflect.ownKeys(n))
    if (!hasOwn.call(t, i) || isOwnAccessor(n, i) || isOwnAccessor(t, i) || propertyIsEnumerable.call(n, i) !== propertyIsEnumerable.call(t, i) || !(isEqual(n[i], t[i]) || sameLogicalSlot(e, n[i], t[i])))
      r.add(i);
  for (const e2 of Reflect.ownKeys(t))
    if (!hasOwn.call(n, e2))
      r.add(e2);
  return r;
}
function stageHeldKey(e, t, n) {
  if (slotNodeEquals.call(e, e.ce, t))
    return;
  e._e = t;
  e.we = n;
  n.Gn.push(e);
}
function sameLogicalSlot(e, t, n) {
  if (t === null || typeof t !== "object" || n === null || typeof n !== "object")
    return false;
  const r = lookupTarget(t, e.fam);
  return r !== undefined && r === lookupTarget(n, e.fam);
}
function getHasNode(e, t, n) {
  const r = e.h ??= Object.create(null);
  let i = r[t];
  if (i === undefined) {
    const l = i = signal(n, {
      equals: isEqual,
      unobserved() {
        if (l.o?.t)
          return;
        if (hasActiveOverride(l) || l._e !== NOT_PENDING)
          return deferSlotRelease(l);
        if (e.h && e.h[t] === l) {
          delete e.h[t];
          unlinkFirewallChild(l);
        }
      }
    }, e.fam?.node ?? undefined);
    l.C |= CONFIG_OWNED_WRITE;
    if (e.fam?.opt) {
      ext(l).Fe = NOT_PENDING;
      l.C |= CONFIG_OPTIMISTIC;
    }
    if (affectsScopesLive())
      inheritAffectsMarks(l, e.v, t);
    r[t] = i;
    markDescendants(e);
  }
  return i;
}
function observerHoldsKeySet(e, t) {
  const n = e.k;
  if (n === null)
    return false;
  const r = n.mn;
  return r !== null && r.ge === t && (!(t.oe & REACTIVE_RECOMPUTING_DEPS) || r.Be === t.Ye);
}
function getKeySetNode(e) {
  let t = e.k;
  if (t === null) {
    const n = t = signal(0, {
      equals: false,
      unobserved() {
        if (e.k === n) {
          e.k = null;
          unlinkFirewallChild(n);
        }
      }
    }, e.fam?.node ?? undefined);
    n.C |= CONFIG_OWNED_WRITE;
    if (e.fam?.opt) {
      ext(n).Fe = NOT_PENDING;
      n.C |= CONFIG_OPTIMISTIC;
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
  t.sc || scanAccessorsOnce(t);
  if (t.sc === 2) {
    const n2 = {
      ...e
    };
    n2[$OWNER] = t;
    return n2;
  }
  const n = Object.getOwnPropertyDescriptors(e);
  for (const r2 of Reflect.ownKeys(n)) {
    const i = n[r2];
    if (r2 === "length" && Array.isArray(e))
      continue;
    i.configurable = true;
    if (!i.get && !i.set)
      i.writable = true;
    else
      t.a = true;
  }
  const r = Array.isArray(e) ? Object.defineProperties([], n) : Object.create(Object.getPrototypeOf(e), n);
  r[$OWNER] = t;
  return r;
}
function wideClone(e, t) {
  const n = Object.create(null);
  for (const t2 of Reflect.ownKeys(e))
    n[t2] = e[t2];
  Object.setPrototypeOf(n, Object.prototype);
  n[$OWNER] = t;
  return n;
}
function copyOwn(e, t, n) {
  const r = Object.getOwnPropertyDescriptor(t, n);
  if (r.get || r.set || !r.enumerable || !r.writable || !r.configurable)
    Object.defineProperty(e, n, r);
  else
    e[n] = r.value;
}
function scanAccessorsOnce(e) {
  const t = e.v;
  const n = Reflect.ownKeys(t);
  let r = Object.getPrototypeOf(t) === Object.prototype;
  for (const i of n) {
    if (lookupGetter.call(t, i) !== undefined || lookupSetter.call(t, i) !== undefined) {
      e.a = true;
      r = false;
      break;
    }
    if (r && !propertyIsEnumerable.call(t, i))
      r = false;
  }
  e.sc = r ? 2 : 1;
  e.kc = n.length;
  return !e.a;
}
var OVERLAY_MIN_KEYS = 32;
var OVERLAY_REBUILD_MAX_KEYS = 1024;
var OVERLAY_REBUILD_MIN_ADDS = 16;
function overlayRebuilds(e, t) {
  if (e.kc > OVERLAY_REBUILD_MAX_KEYS)
    return false;
  if (e.del !== null && e.del.size !== 0)
    return true;
  const n = e.wk;
  if (n !== null && n !== WK_ALL && n.size <= OVERLAY_REBUILD_MIN_ADDS)
    return false;
  const r = e.v;
  let i = 0;
  for (const e2 of Reflect.ownKeys(t))
    if (!hasOwn.call(r, e2) && ++i > OVERLAY_REBUILD_MIN_ADDS)
      return true;
  return false;
}
function materializePB(e) {
  const t = e.pb;
  if (!e.ovl)
    return t;
  const n = e.sc === 2 ? wideClone(e.v, e) : cloneRaw(e.v, e);
  if (e.del !== null) {
    for (const t2 of e.del)
      delete n[t2];
    e.kc -= e.del.size;
    e.del = null;
  }
  for (const r of Reflect.ownKeys(t))
    e.sc === 2 ? n[r] = t[r] : copyOwn(n, t, r);
  e.pb = n;
  e.ovl = false;
  return n;
}
function ensurePB(e) {
  let t = e.pb;
  if (t !== null && !tentativePBs.has(t) && e.fam?.opt === true && !projectionWriteActive && !getWriteOverride() && foldBatches.has(e)) {
    stagedTruthPB.set(e, t);
    t = e.pb = null;
  }
  if (activeTransition !== null)
    foldBatches.set(e, activeTransition);
  if (t === null) {
    const n = e.v;
    if (!e.fam?.opt && !e.ch && !Array.isArray(n) && (e.sc !== 0 ? !e.a : scanAccessorsOnce(e)) && e.kc > OVERLAY_MIN_KEYS && isOwned(n)) {
      t = e.pb = Object.create(n);
      e.ovl = true;
    } else
      t = e.pb = cloneRaw(n, e);
    if (e.fam?.opt && !projectionWriteActive && !getWriteOverride()) {
      tentativePBs.add(t);
      const n2 = e.n;
      if (n2 !== null) {
        for (const e2 of Reflect.ownKeys(n2)) {
          const r2 = n2[e2];
          if (hasActiveOverride(r2))
            t[e2] = unwrapOverride(r2.o?.Fe);
        }
      }
      const r = e.h;
      if (r !== null) {
        for (const e2 of Reflect.ownKeys(r)) {
          const n3 = r[e2];
          if (hasActiveOverride(n3) && !unwrapOverride(n3.o?.Fe))
            delete t[e2];
        }
      }
    }
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
  if (t !== PLAIN_HOLD && currentTransition(t)?.Rn === true)
    return e.ht = e.hv = null;
  return e.hv;
}
function adoptPB(e, t, n = false) {
  if (!n) {
    queueFold(e);
    if (e.pb !== null) {
      if (e.ovl)
        materializePB(e);
      e.ab = e.pb;
    } else
      e.ab ??= foldOlds.get(e);
  }
  if (getWriteOverride()) {
    e.ht = e.hv = null;
  } else if (activeTransition !== null || !n && latestPullActive) {
    if (heldMaskView(e) === null)
      e.hv = e.v;
    e.ht = activeTransition ?? PLAIN_HOLD;
    if (!n && activeTransition !== null)
      heldAdoptions.add(e);
    heldKeys.set(e, [t]);
  }
  e.pb = null;
  e.ovl = false;
  e.del = null;
  e.sc = 0;
  e.a = false;
  e.wk = null;
  e.v = t;
  e.ch = t[$TARGET] !== undefined;
  const r = t[$OWNER];
  if (r !== undefined && r.fam === e.fam)
    t[$OWNER] = e;
  else
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
var stagedTruthPB = new WeakMap;
var tentativePBs = new WeakSet;
function draftSeesOverrides(e) {
  return e.pb === null || !tentativePBs.has(e.pb);
}
function privatizeCommitted(e) {
  if (isOwned(e.v))
    return;
  const t = e.v;
  const n = cloneRaw(t, e);
  e.v = n;
  e.ch = false;
  if (e.u) {
    privatizeCommitted(e.u);
    devAssertNeverUserMutation(e.u.v);
    const n2 = e.u.v;
    const r = parentSlotKey(e, t);
    if (n2[r] === t)
      n2[r] = e.v;
  }
}
function parentSlotKey(e, t) {
  const n = e.pk;
  const r = e.u.v;
  if (r[n] === t || !Array.isArray(r))
    return n;
  const i = r.indexOf(t);
  if (i === -1)
    return n;
  e.pk = i;
  return i;
}
function flattenOverlay(e, t) {
  privatizeCommitted(e);
  const n = e.v;
  for (const r of Reflect.ownKeys(t))
    e.sc === 2 ? n[r] = t[r] : copyOwn(n, t, r);
  if (e.del !== null) {
    for (const t2 of e.del)
      delete n[t2];
    e.kc -= e.del.size;
    e.del = null;
  }
  e.pb = null;
  e.ovl = false;
  e.wk = null;
}
function drainFolds() {
  if (foldOlds.size === 0)
    return;
  const e = [...foldOlds];
  foldOlds.clear();
  for (const [t, n] of e) {
    if (t.ht === PLAIN_HOLD)
      t.ht = t.hv = null;
    if (t.pb !== null) {
      const e3 = foldBatches.get(t);
      if (e3 !== undefined) {
        if (currentTransition(e3).Rn === false) {
          foldOlds.set(t, n);
          continue;
        }
        foldBatches.delete(t);
      }
      let r = false;
      const i = t.pb;
      const l = t.n;
      if (l !== null) {
        const e4 = t.wk;
        const n2 = e4 === null || e4 === WK_ALL || t.a === true || !plainProto(t.ovl ? t.v : i) ? Reflect.ownKeys(l) : e4;
        for (const e5 of n2) {
          const t2 = l[e5];
          if (t2 !== undefined && t2._e !== NOT_PENDING) {
            r = true;
            break;
          }
        }
      }
      if (r) {
        foldOlds.set(t, n);
        continue;
      }
      if (t.ovl && (t.v !== n || !overlayRebuilds(t, i))) {
        flattenOverlay(t, i);
      } else if (t.v !== n) {
        privatizeCommitted(t);
        const e4 = t.v;
        const r2 = t.wk;
        if (r2 !== null && r2 !== WK_ALL) {
          for (const t2 of r2) {
            if (hasOwn.call(i, t2))
              copyOwn(e4, i, t2);
            else
              delete e4[t2];
          }
        } else {
          for (const t2 of Reflect.ownKeys(i)) {
            const r3 = Object.getOwnPropertyDescriptor(i, t2);
            if (r3.get || r3.set || !r3.enumerable || !r3.writable || !r3.configurable)
              Object.defineProperty(e4, t2, r3);
            else if (r3.value !== n[t2] || !hasOwn.call(n, t2))
              e4[t2] = r3.value;
          }
          for (const t2 of Reflect.ownKeys(n)) {
            if (!hasOwn.call(i, t2))
              delete e4[t2];
          }
        }
        t.pb = null;
        t.wk = null;
      } else {
        t.v = t.ovl ? materializePB(t) : i;
        t.ch = false;
        t.pb = null;
        t.wk = null;
      }
    }
    const e2 = t.ab;
    t.ab = null;
    if (t.v !== n) {
      if (t.u) {
        const e3 = parentSlotKey(t, n);
        if (t.u.v[e3] === n) {
          privatizeCommitted(t.u);
          devAssertNeverUserMutation(t.u.v);
          t.u.v[e3] = t.v;
        }
      }
    }
    if (e2 !== null && e2 !== t.v)
      notifyFold(t, e2, t.v);
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
  const i = e.wk;
  const l = i === WK_ALL || e.a === true || !plainProto(e.ovl ? e.v : t) ? null : i;
  if (r !== null) {
    const i2 = l ?? Reflect.ownKeys(r);
    for (const l2 of i2) {
      const i3 = r[l2];
      if (i3 === undefined)
        continue;
      if (i3.acc === true || hasOwn.call(t, l2) && lookupGetter.call(t, l2) !== undefined) {
        i3.acc = isOwnAccessor(t, l2);
        const e2 = Object.getOwnPropertyDescriptor(n, l2);
        const r2 = Object.getOwnPropertyDescriptor(t, l2);
        if (e2 && (e2.get || e2.set) || r2 && (r2.get || r2.set)) {
          if (e2?.get !== r2?.get || e2?.set !== r2?.set || e2?.value !== r2?.value)
            setSignal(i3, () => FORCE);
          continue;
        }
        if (!isEqual(e2?.value, r2?.value))
          setSignal(i3, () => r2?.value);
        continue;
      }
      const o2 = e.del !== null && e.del.has(l2) ? undefined : t[l2];
      if (derivedSetter !== null && i3.De === derivedSetter && heldDerivation(i3))
        heldDerivationHit = true;
      setSignal(i3, () => o2);
    }
  }
  const o = e.h;
  if (o !== null) {
    const n2 = l ?? Reflect.ownKeys(o);
    for (const r2 of n2) {
      const n3 = o[r2];
      if (n3 !== undefined)
        setSignal(n3, r2 in t && !(e.del !== null && e.del.has(r2)));
    }
  }
  if (e.dk !== null) {
    if (e.del !== null && e.del.size !== 0)
      bumpDeep(e);
    else
      for (const r2 of l ?? Reflect.ownKeys(t)) {
        if (r2 === $OWNER)
          continue;
        const i2 = t[r2];
        const l2 = n[r2];
        if (i2 !== null && typeof i2 === "object" ? !targetsEqual(l2, i2) : !isEqual(l2, i2)) {
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
  if (e.fam !== null && e.pb !== null && getWriteOverride() && activeTransition === null) {
    if (e.ht !== null)
      e.ht = e.hv = null;
    if (e.ovl) {
      if (!overlayRebuilds(e, t))
        return flattenOverlay(e, t);
      t = materializePB(e);
    }
    const n2 = e.v;
    e.pb = null;
    e.v = t;
    e.ch = false;
    if (e.u) {
      const r2 = parentSlotKey(e, n2);
      if (e.u.v[r2] === n2) {
        privatizeCommitted(e.u);
        devAssertNeverUserMutation(e.u.v);
        e.u.v[r2] = t;
      }
    }
  }
}
var FORCE = Symbol();
function targetsEqual(e, t) {
  if (e === null || typeof e !== "object" || t === null || typeof t !== "object")
    return false;
  const n = lookupTarget(e, null);
  return n !== undefined && n === lookupTarget(t, null);
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
  if (Reflect.ownKeys(e).length - +isOwned(e) !== n.length - +isOwned(t))
    return true;
  for (const t2 of n)
    if (t2 !== $OWNER && !(t2 in e))
      return true;
  return false;
}
function notifyKeyDiff(e, t, n, r, i = true) {
  if (e.acc === true || i && hasOwn.call(r, t) && lookupGetter.call(r, t) !== undefined) {
    e.acc = isOwnAccessor(r, t);
    const i2 = Object.getOwnPropertyDescriptor(n, t);
    const l = Object.getOwnPropertyDescriptor(r, t);
    if (i2 && (i2.get || i2.set) || l && (l.get || l.set)) {
      if (i2?.get !== l?.get || i2?.set !== l?.set || i2?.value !== l?.value)
        setSignal(e, () => FORCE);
      return;
    }
    const o = i2?.value;
    const s = l?.value;
    if (!isEqual(o, s) && !targetsEqual(o, s))
      setSignal(e, typeof s === "function" ? () => s : s);
  } else {
    const i2 = n[t];
    const l = r[t];
    if (!isEqual(i2, l) && !targetsEqual(i2, l))
      setSignal(e, typeof l === "function" ? () => l : l);
  }
}
function hasAccessorFlag(e) {
  return e.acc === true;
}
function notifyKeyValue(e, t, n, r, i, l) {
  if (e.acc === true) {
    notifyKeyDiff(e, t, i, l, false);
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
var heldAdoptions = new Set;
function stageHeldAdoptions() {
  const e = [...heldAdoptions];
  heldAdoptions.clear();
  for (const t of e) {
    const e2 = t.ab;
    if (e2 === null || e2 === t.v)
      continue;
    notifyFold(t, e2, t.v);
    t.ab = t.v;
  }
}
var UNSAFE_KEYS = new Set(["__proto__", "prototype", "constructor"]);
function readerContext() {
  const e = getOwner();
  return e === null ? null : e.Wn ? e.qn ?? null : e;
}
function foldHeld(e) {
  const t = e.n;
  if (t === null)
    return false;
  for (const e2 of Reflect.ownKeys(t)) {
    const n = t[e2];
    if (n._e !== NOT_PENDING && n.we != null && n.we.Rn !== true)
      return true;
  }
  return false;
}
function readSource(e, t) {
  const n = e.ht;
  if (n !== null && !latestReadActive && !inDraft(e) && !getWriteOverride()) {
    const r = heldMaskView(e);
    if (r !== null && (t === undefined || heldKey(e, t))) {
      const e2 = readerContext();
      if (e2 === null || e2.C & CONFIG_CHILDREN_FORBIDDEN || !holdVisible(n === PLAIN_HOLD ? null : currentTransition(n), e2))
        return r;
    }
  }
  return pendingBackingVisible(e, false, t) ? e.pb : e.v;
}
function pendingBackingVisible(e, t, n) {
  if (e.pb === null)
    return false;
  if (inDraft(e) || getWriteOverride())
    return true;
  if (heldTruthMasked(e))
    return false;
  const r = readerContext();
  if (r === null || r.C & CONFIG_CHILDREN_FORBIDDEN) {
    const n2 = liveFoldTransition(e);
    if (t)
      return n2 === null || ownsHold(n2);
    return e.fam !== null && r === null && !foldHeld(e) && n2 === null;
  }
  const i = liveFoldTransition(e);
  if (i !== null && n !== undefined && !e.ch && e.fam?.opt !== true) {
    const t2 = e.wk;
    if (t2 != null && t2 !== WK_ALL && !t2.has(n))
      return false;
  }
  return holdVisible(i, r);
}
function heldTruthMasked(e) {
  if (e.fam?.opt !== true || currentOptimisticLane === null || latestReadActive || authoritativeServe())
    return false;
  const t = foldBatches.get(e);
  return t !== undefined && optHooks.retainsOptimism(t);
}
var hasOwn = Object.prototype.hasOwnProperty;
var lookupGetter = Object.prototype.__lookupGetter__;
var lookupSetter = Object.prototype.__lookupSetter__;
var propertyIsEnumerable = Object.prototype.propertyIsEnumerable;
function isOwnAccessor(e, t) {
  return hasOwn.call(e, t) && (lookupGetter.call(e, t) !== undefined || lookupSetter.call(e, t) !== undefined);
}
function authoritativeRead() {
  const e = context;
  return e !== null && (e.C & CONFIG_AUTHORITATIVE_READ) !== 0;
}
function authoritativeServe() {
  return projectionWriteActive || getWriteOverride() || authoritativeRead();
}
function nodeValue(e, t) {
  let n;
  if (authoritativeServe())
    n = e._e !== NOT_PENDING ? e._e : t;
  else if (latestReadActive)
    n = visibleOverride(e) ? unwrapOverride(e.o?.Fe) : e._e !== NOT_PENDING ? e._e : t;
  else
    n = serve(e, readerContext(), e.De || e, t);
  return n === FORCE ? t : n;
}
function readerOverride(e, t) {
  return e.C & CONFIG_OVERRIDE_SUPERSEDED ? nodeValue(e, t) : unwrapOverride(e.o?.Fe);
}
function resolveChainedRaw(e, t, n) {
  const r = e.v[$TARGET];
  if (r.ch) {
    const e2 = resolveChainedRaw(r, t, n);
    return e2 === n ? n : wrapNext(e2, r, t);
  }
  const i = lookupTarget(n, r.fam);
  if (i !== undefined)
    return i.px;
  if ((r.v[t] === n || r.pb?.[t] === n) && isWrappable(n))
    return wrapNext(n, r, t);
  return n;
}
function serveDataKey(e, t, n, r, i, l = -1) {
  const o = e.ch && r === e.v;
  let s = n;
  if (t === "length" && e.fam?.opt === true && !o && Array.isArray(r)) {
    if (!inDraft(e)) {
      const i2 = e.n?.length;
      if (i2 !== undefined) {
        if (getObserver() !== null) {
          const e2 = read(i2);
          if (hasActiveOverride(i2) && !authoritativeServe())
            return e2 === FORCE ? r.length : e2;
        } else if (hasActiveOverride(i2) && !authoritativeServe()) {
          return nodeValue(i2, r.length);
        }
      } else if (getObserver() !== null) {
        read(getNode(e, t, n));
      }
    }
    return (authoritativeServe() || inDraft(e) && !draftSeesOverrides(e) ? r : optHooks.optimisticView(e, r, inDraft(e))).length;
  }
  if (inDraft(e)) {
    if (e.fam?.opt && draftSeesOverrides(e) && !authoritativeServe()) {
      const n2 = e.n?.[t];
      if (n2 !== undefined && hasActiveOverride(n2))
        s = unwrapOverride(n2.o?.Fe);
    }
  } else {
    if (getObserver() !== null) {
      if (i === undefined)
        i = getNode(e, t, n, l);
      let r2 = readNodeFast(i);
      if (r2 === READ_SLOW)
        r2 = read(i);
      if (!o || hasActiveOverride(i))
        s = r2 === FORCE ? n : r2;
    } else if (i !== undefined && (!o || hasActiveOverride(i))) {
      s = nodeValue(i, n);
    }
  }
  if (e.s)
    return serveShallow(e, t, s);
  if (e.ch && !o && s !== null && typeof s === "object" && s[$TARGET] === undefined)
    s = resolveChainedRaw(e, t, s);
  if (i !== undefined) {
    if (i.pxv === s && s !== undefined)
      return draftServe(e, i.px);
    if (!isWrappable(s))
      return s;
    const n2 = wrapNext(s, e, t);
    i.px = n2;
    i.pxv = s;
    return draftServe(e, n2);
  }
  if (!isWrappable(s))
    return s;
  return draftServe(e, wrapNext(s, e, t));
}
function firewallGate(e) {
  if (projectionWriteActive || getWriteOverride())
    return;
  const t = e.fam?.node;
  if (t != null && t.h & (STATUS_UNINITIALIZED | STATUS_ERROR))
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
      if (t === $OWNER)
        return;
      if (t === $RECORD)
        return;
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
    const r = readSource(e, t);
    if (e.del !== null && r === e.pb && e.del.has(t)) {
      if (!inDraft(e) && getObserver() !== null)
        read(getNode(e, t, undefined));
      return;
    }
    const i = e.n?.[t];
    if (e.ch === false && writeScopes === null) {
      const n2 = i;
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
    let l = -1;
    {
      let o2;
      if (i !== undefined)
        o2 = i.acc === true;
      else if (!inDraft(e) && getObserver() !== null) {
        o2 = isOwnAccessor(r, t);
        if (r === (e.pb ?? e.v))
          l = o2 ? 1 : 0;
      } else
        o2 = false;
      if (o2) {
        if (!inDraft(e) && getObserver() !== null)
          read(i ?? getNode(e, t, undefined, l));
        const o3 = Reflect.get(r, t, n);
        if (e.s)
          return serveShallow(e, t, o3);
        return isWrappable(o3) ? draftServe(e, wrapNext(o3, e, t)) : o3;
      }
    }
    const o = e.ovl && r === e.pb;
    if ((t === "constructor" || t === "__proto__" || t === "prototype") && !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)))
      return;
    let s = r[t];
    if (s === undefined ? !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)) : false) {
      s = Reflect.get(r, t, n);
      if (typeof s === "function")
        return s;
      if (s === undefined && !inDraft(e)) {
        if (getObserver() !== null)
          read(getNode(e, t, undefined, l));
        const n2 = e.n?.[t];
        if (n2) {
          const r2 = nodeValue(n2, undefined);
          if (e.s)
            return serveShallow(e, t, r2);
          return isWrappable(r2) ? draftServe(e, wrapNext(r2, e, t)) : r2;
        }
      } else if (s === undefined && inDraft(e) && e.fam?.opt && draftSeesOverrides(e) && !authoritativeServe()) {
        const n2 = e.n?.[t];
        if (n2 !== undefined && hasActiveOverride(n2))
          s = unwrapOverride(n2.o?.Fe);
      }
      if (e.s)
        return serveShallow(e, t, s);
      return isWrappable(s) ? draftServe(e, wrapNext(s, e, t)) : s;
    }
    if (typeof s === "function" && !hasOwn.call(r, t) && !(o && hasOwn.call(e.v, t)))
      return s;
    return serveDataKey(e, t, s, r, i, l);
  },
  has(e, t) {
    if (t === $TARGET || t === $PROXY || t === $TRACK)
      return true;
    if (t === $OWNER || t === $RECORD)
      return false;
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    if (e.fam !== null && getObserver() === null && !inDraft(e))
      firewallGate(e);
    const n = readSource(e, t);
    let r = t in n;
    if (r && e.del !== null && n === e.pb && e.del.has(t))
      r = false;
    if (!inDraft(e)) {
      if (getObserver() !== null) {
        const n2 = getHasNode(e, t, r);
        const i = read(n2);
        if (hasActiveOverride(n2))
          r = !!i;
      } else if (!authoritativeServe()) {
        const n2 = e.h?.[t];
        if (n2 !== undefined && hasActiveOverride(n2))
          r = !!nodeValue(n2, r);
      }
    } else if (e.fam?.opt && draftSeesOverrides(e) && !authoritativeServe()) {
      const n2 = e.h?.[t];
      if (n2 !== undefined && hasActiveOverride(n2))
        r = !!unwrapOverride(n2.o?.Fe);
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
    return visibleKeys(e, readSource(e));
  },
  getOwnPropertyDescriptor(e, t) {
    if (t === $OWNER || t === $RECORD)
      return;
    if (pendingCheckActive)
      witnessAffectsMark(e, t);
    const n = getObserver();
    if (e.fam !== null && n === null && !inDraft(e))
      firewallGate(e);
    const r = readSource(e, t);
    const i = visibleDescriptor(e, r, t);
    if (!inDraft(e) && n !== null && !observerHoldsKeySet(e, n)) {
      let n2 = t in r;
      if (n2 && e.del !== null && r === e.pb && e.del.has(t))
        n2 = false;
      read(getHasNode(e, t, n2));
    }
    if (i === undefined)
      return;
    if (!(t === "length" && Array.isArray(e)))
      i.configurable = true;
    return i;
  },
  set(e, t, n) {
    const r = inDraft(e);
    const i = !r && getWriteOverride();
    if (!r && !i)
      return true;
    if (t === "__proto__")
      return true;
    const l = e.s ? n : unwrapValue(n);
    const o = ensurePB(e);
    pendingNotify.add(e);
    if (Array.isArray(o)) {
      if (t === "length")
        e.wk = WK_ALL;
      else if (e.wk !== WK_ALL) {
        const n2 = e.wk ??= new Set;
        n2.add(t);
        n2.add("length");
      }
    } else {
      if (e.wk !== WK_ALL)
        (e.wk ??= new Set).add(t);
      if (!(t in o))
        e.kc++;
    }
    if (UNSAFE_KEYS.has(t)) {
      Object.defineProperty(o, t, {
        value: l,
        writable: true,
        enumerable: true,
        configurable: true
      });
      if (e.del !== null)
        e.del.delete(t);
      return true;
    }
    if (e.ovl && e.sc !== 2 && !hasOwn.call(o, t)) {
      Object.defineProperty(o, t, {
        value: l,
        writable: true,
        enumerable: true,
        configurable: true
      });
    } else
      o[t] = l;
    if (e.del !== null)
      e.del.delete(t);
    if (e.s && l !== null && typeof l === "object")
      markRawOne(l);
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
    if (n.get || n.set)
      e.a = true;
    if ("value" in n)
      n = {
        ...n,
        value: unwrapValue(n.value)
      };
    const l = ensurePB(e);
    if (e.a || !(n.enumerable && n.writable && n.configurable))
      e.sc = 1;
    pendingNotify.add(e);
    if (e.wk !== WK_ALL)
      (e.wk ??= new Set).add(t);
    Object.defineProperty(l, t, n);
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
    if (e.wk !== WK_ALL)
      (e.wk ??= new Set).add(t);
    delete i[t];
    if (e.ovl && hasOwn.call(e.v, t))
      (e.del ??= new Set).add(t);
    if (r)
      notifyWrites(e);
    return true;
  }
};
var derivedSetter = null;
var heldDerivationHit = false;
function derivedStoreWrite(e, t, n) {
  derivedSetter = e;
  heldDerivationHit = false;
  try {
    storeSetterNext(t, n);
  } finally {
    derivedSetter = null;
    heldDerivationHit ? rederiveHeld(e) : suppressComputedRecompute(e);
  }
}
function storeSetterNext(e, t, n = true) {
  const r = e[$TARGET];
  const i = writeScopes;
  writeScopes = new Set;
  writeScopes.add(scopeKey(r));
  writing++;
  let l;
  try {
    l = t(e);
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
  if (l !== undefined && l !== e && isWrappable(l)) {
    if (r.fam?.opt && !projectionWriteActive && !getWriteOverride()) {
      optHooks.notifyOptimisticWrites(r, unwrapValue(l));
    } else {
      adoptPB(r, unwrapValue(l));
    }
  }
  if (writing === 0 && heldAdoptions.size)
    stageHeldAdoptions();
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
function visibleKeys(e, t) {
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
  for (let e2 = n.length - 1;e2 >= 0 && typeof n[e2] === "symbol"; e2--) {
    if (n[e2] === $OWNER) {
      n.splice(e2, 1);
      break;
    }
  }
  if (!authoritativeServe() && e.fam?.opt && e.h !== null && (!inDraft(e) || draftSeesOverrides(e))) {
    let t2 = null;
    const r = inDraft(e);
    for (const i of Reflect.ownKeys(e.h)) {
      const l = e.h[i];
      if (!(r ? hasActiveOverride(l) : visibleOverride(l)))
        continue;
      t2 ??= new Set(n);
      const o = r ? unwrapOverride(l.o?.Fe) : readerOverride(l, t2.has(i));
      if (o)
        t2.add(i);
      else
        t2.delete(i);
    }
    if (t2 !== null)
      return [...t2];
  }
  return n;
}
function visibleDescriptor(e, t, n) {
  let r = Object.getOwnPropertyDescriptor(t, n);
  if (e.ovl && t === e.pb) {
    if (e.del !== null && e.del.has(n))
      return;
    if (r === undefined)
      r = Object.getOwnPropertyDescriptor(e.v, n);
  }
  const i = inDraft(e);
  if (!authoritativeServe() && e.fam?.opt && (!i || draftSeesOverrides(e))) {
    const t2 = e.h?.[n];
    if (t2 !== undefined && (i ? hasActiveOverride(t2) : visibleOverride(t2))) {
      const l = i ? unwrapOverride(t2.o?.Fe) : readerOverride(t2, r !== undefined);
      if (!l)
        return;
      if (r === undefined) {
        const t3 = e.n?.[n];
        return {
          value: t3 === undefined ? undefined : i ? hasActiveOverride(t3) ? unwrapOverride(t3.o?.Fe) : undefined : nodeValue(t3, undefined),
          writable: true,
          enumerable: true,
          configurable: true
        };
      }
    }
  }
  return r;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/map.js
function mapArray(t, s, e) {
  const i = typeof e?.keyed === "function" ? e.keyed : undefined;
  const r = s.length > 1;
  const n = s;
  const o = {
    ie: createOwner(),
    wt: 0,
    Ot: t,
    yt: [],
    At: n,
    Mt: [],
    St: [],
    Kn: i,
    jt: i || e?.keyed === false ? [] : undefined,
    kt: r && e?.keyed !== false ? [] : undefined,
    gt: e?.keyed === false,
    bt: e?.fallback
  };
  const h = computed(updateKeyedMap.bind(o), undefined);
  o.ie.qn = h;
  h.C &= ~CONFIG_AUTO_DISPOSE;
  return accessor(h);
}
var pureOptions = {
  ownedWrite: true
};
function trySmallMove(t, s, e, i) {
  const r = t.yt;
  const n = t.wt - 1;
  const o = [];
  const h = [];
  const f = [];
  let c = 256;
  let a = i;
  let u = i;
  let l = false;
  while (a <= n && u <= e - 1) {
    const t2 = r[a];
    const i2 = s[u];
    if (t2 === i2) {
      if (!l) {
        f.push(a, u, 0);
        l = true;
      }
      f[f.length - 1]++;
      a++;
      u++;
      continue;
    }
    l = false;
    let p = -1;
    let d = Math.min(32 - o.length, n - a, c);
    for (let t3 = 1;t3 <= d; t3++) {
      if (r[a + t3] === i2) {
        p = t3;
        break;
      }
    }
    c -= p === -1 ? d : p;
    let m = -1;
    d = Math.min(32 - h.length, e - 1 - u, c);
    for (let e2 = 1;e2 <= d; e2++) {
      if (s[u + e2] === t2) {
        m = e2;
        break;
      }
    }
    c -= m === -1 ? d : m;
    if (p !== -1 && (m === -1 || p <= m)) {
      while (p-- > 0)
        o.push(a++);
      continue;
    }
    if (m !== -1) {
      while (m-- > 0)
        h.push(u++);
      continue;
    }
    if (c <= 0 || o.length === 32 || h.length === 32)
      return false;
    o.push(a++);
    h.push(u++);
  }
  for (;a <= n; a++) {
    if (o.length === 32)
      return false;
    o.push(a);
  }
  for (;u <= e - 1; u++) {
    if (h.length === 32)
      return false;
    h.push(u);
  }
  return commitSmallMove(t, s, e, o, h, f);
}
function commitSmallMove(t, s, e, i, r, n) {
  const o = t.yt;
  let h;
  let f;
  let c;
  if (r.length !== 0) {
    c = new Array(i.length);
    for (f = 0;f < r.length; f++) {
      let t2 = -1;
      for (h = 0;h < i.length; h++) {
        if (!c[h] && o[i[h]] === s[r[f]]) {
          t2 = h;
          break;
        }
      }
      if (t2 === -1)
        return false;
      c[t2] = true;
      r[f] = r[f] << 6 | t2;
    }
  }
  if (i.length !== 0 || r.length !== 0) {
    const t2 = new Set;
    for (h = 0;h < i.length; h++)
      t2.add(o[i[h]]);
    for (f = 0;f < r.length; f++)
      t2.add(s[r[f] >> 6]);
    for (let s2 = 0;s2 < n.length; s2 += 3) {
      const e2 = n[s2];
      for (let i2 = 0, r2 = n[s2 + 2];i2 < r2; i2++)
        if (t2.has(o[e2 + i2]))
          return false;
    }
  }
  const a = t.Mt;
  const u = t.St;
  const l = a.slice(0, e);
  const p = u.slice(0, e);
  for (let t2 = 0;t2 < n.length; t2 += 3) {
    const s2 = n[t2];
    const e2 = n[t2 + 1];
    if (s2 !== e2) {
      for (let i2 = 0;i2 < n[t2 + 2]; i2++) {
        l[e2 + i2] = a[s2 + i2];
        p[e2 + i2] = u[s2 + i2];
      }
    }
  }
  for (f = 0;f < r.length; f++) {
    const t2 = r[f] >> 6;
    const s2 = i[r[f] & 63];
    l[t2] = a[s2];
    p[t2] = u[s2];
  }
  t.Mt = l;
  t.St = p;
  t.wt = e;
  t.yt = s.slice(0);
  for (h = 0;h < i.length; h++) {
    if (c === undefined || !c[h])
      u[i[h]].dispose();
  }
  return true;
}
function updateKeyedMap() {
  const t = this.Ot() || [], s = t.length;
  t[$TRACK];
  runWithOwner(this.ie, () => {
    let e, i, r, n, o = this.jt ? this.gt ? () => {
      r[i] = signal(t[i], pureOptions);
      return this.At(accessor(r[i]), i);
    } : () => {
      r[i] = signal(t[i], pureOptions);
      n && (n[i] = signal(i, pureOptions));
      return this.At(accessor(r[i]), n ? accessor(n[i]) : undefined);
    } : this.kt ? () => {
      const s2 = t[i];
      n[i] = signal(i, pureOptions);
      return this.At(s2, accessor(n[i]));
    } : () => {
      const s2 = t[i];
      return this.At(s2);
    };
    if (s === 0) {
      if (this.wt !== 0) {
        this.ie.dispose(false);
        this.St = [];
        this.yt = [];
        this.Mt = [];
        this.wt = 0;
        this.jt && (this.jt = []);
        this.kt && (this.kt = []);
      }
      if (this.bt && !this.Mt[0]) {
        this.St[0]?.dispose();
        this.Mt[0] = runWithOwner(this.St[0] = createOwner(), this.bt);
      }
    } else if (this.wt === 0) {
      const h = new Array(s);
      const f = new Array(s);
      r = this.jt && new Array(s);
      n = this.kt && new Array(s);
      try {
        for (i = 0;i < s; i++)
          h[i] = runWithOwner(f[i] = createOwner(), o);
      } catch (t2) {
        for (e = 0;e <= i; e++)
          f[e]?.dispose();
        throw t2;
      }
      if (this.St[0])
        this.St[0].dispose();
      this.Mt = h;
      this.St = f;
      r && (this.jt = r);
      n && (this.kt = n);
      this.yt = t.slice(0);
      this.wt = s;
    } else {
      let h, f, c, a, u, l, p, d, m, w, O;
      const _ = (this.Wt = currentOptimisticLane !== null || this.Wt && activeLanes.size !== 0) ? GlobalQueue.st : setSignal;
      for (h = 0, f = Math.min(this.wt, s);h < f && (this.yt[h] === t[h] || this.jt && compare(this.Kn, this.yt[h], t[h])); h++) {
        if (this.jt)
          _(this.jt[h], t[h]);
      }
      for (f = this.wt - 1, c = s - 1;f >= h && c >= h && (this.yt[f] === t[c] || this.jt && compare(this.Kn, this.yt[f], t[c])); f--, c--)
        ;
      if (h === s && this.wt === s) {
        this.yt = t.slice(0);
        return;
      }
      if (s <= this.wt && f - h > 64 && this.jt === undefined && this.kt === undefined) {
        const e2 = h + (c - h >> 1);
        const i2 = t[e2];
        const r2 = Math.min(f, e2 + 32);
        let n2 = Math.max(h, e2 - 32);
        while (n2 <= r2 && this.yt[n2] !== i2)
          n2++;
        if (n2 <= r2 && trySmallMove(this, t, s, h))
          return;
      }
      const y = s - this.wt;
      const A = new Array(s);
      const M = new Array(s);
      r = this.jt ? new Array(s) : undefined;
      n = this.kt ? new Array(s) : undefined;
      l = new Map;
      p = new Array(c + 1);
      for (i = c;i >= h; i--) {
        a = t[i];
        u = this.Kn ? this.Kn(a) : a;
        e = l.get(u);
        p[i] = e === undefined ? -1 : e;
        l.set(u, i);
      }
      for (e = h;e <= f; e++) {
        a = this.yt[e];
        u = this.Kn ? this.Kn(a) : a;
        i = l.get(u);
        if (i !== undefined && i !== -1) {
          A[i] = this.Mt[e];
          M[i] = this.St[e];
          r && (r[i] = this.jt[e]);
          n && (n[i] = this.kt[e]);
          i = p[i];
          l.set(u, i);
        } else {
          (d ??= []).push(this.St[e]);
          if (false)
            ;
        }
      }
      try {
        for (i = h;i <= c; i++) {
          if (M[i] !== undefined)
            continue;
          (m ??= []).push(M[i] = createOwner());
          if (false)
            ;
          A[i] = runWithOwner(M[i], o);
        }
      } catch (t2) {
        if (m)
          for (e = 0;e < m.length; e++)
            m[e].dispose();
        throw t2;
      }
      for (e = 0;e < h; e++) {
        A[e] = this.Mt[e];
        M[e] = this.St[e];
        r && (r[e] = this.jt[e]);
        n && (n[e] = this.kt[e]);
      }
      for (i = h;i <= c; i++) {
        if (r)
          _(r[i], t[i]);
        if (n)
          _(n[i], i);
      }
      for (i = c + 1;i < s; i++) {
        A[i] = this.Mt[i - y];
        M[i] = this.St[i - y];
        if (r) {
          r[i] = this.jt[i - y];
          _(r[i], t[i]);
        }
        if (n) {
          n[i] = this.kt[i - y];
          if (y !== 0)
            _(n[i], i);
        }
      }
      this.Mt = A;
      this.St = M;
      r && (this.jt = r);
      n && (this.kt = n);
      this.wt = s;
      this.yt = t.slice(0);
      if (d)
        for (e = 0;e < d.length; e++)
          d[e].dispose();
      if (false)
        ;
    }
  });
  return this.Mt;
}
function compare(t, s, e) {
  return t ? t(s) === t(e) : true;
}
// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/next/reconcile.js
function reconcileNextState(e, t, n, o = false) {
  if (t == null)
    throw new Error("");
  const f = t?.[$TARGET];
  if (f === undefined || f.px !== t)
    throw new Error("");
  if (f.ovl)
    materializePB(f);
  let i = n === null ? null : typeof n === "string" ? (e2) => e2?.[n] : n;
  if (o && e !== t && e?.[$TARGET] !== undefined) {
    const t2 = f.pb ?? f.v;
    if (t2 === e)
      return;
    adoptPB(f, e);
    return;
  }
  const l = unwrapValue(e);
  if (i) {
    const e2 = f.pb ?? f.v;
    const t2 = i(e2);
    if (t2 !== undefined && !sameKey(i(l), t2)) {
      if (!o)
        throw new Error("");
      const e3 = f.pb ?? f.v;
      if (isOwned(e3))
        delete e3[$OWNER];
      else
        (f.fam?.map ?? storeNextLookup).delete(e3);
      adoptPB(f, l);
      return;
    }
  }
  if (f.fam?.opt === true && !projectionWriteActive && !getWriteOverride()) {
    optHooks.applyTentative(f, l, i);
    return;
  }
  applyAdopt(f, l, i, o);
}
function applyAdopt(e, t, n, o = false) {
  const f = e.pb ?? e.v;
  if (t === f && !isOwned(f))
    return;
  const i = e.fam;
  const l = i?.opt === true ? optHooks.optimisticView(e, f) : f;
  const r = Array.isArray(t);
  const s = i === null;
  const u = e.s === true;
  const a = f;
  adoptPB(e, t, s);
  if (u)
    markRawIngest(t);
  if (Array.isArray(l) !== r) {
    if (s)
      notifyFold(e, a, t);
    return;
  }
  if (r) {
    const f2 = l;
    const r2 = t;
    const c = s ? e.n : null;
    let d = 0;
    if (n && !u) {
      const l2 = f2.length;
      const s2 = r2.length;
      let u2 = false;
      let p = 0;
      for (const y2 = Math.min(l2, s2);p < y2; p++) {
        const l3 = r2[p];
        const s3 = f2[p];
        if (s3 !== l3 && !(s3 !== null && typeof s3 === "object" && l3 !== null && typeof l3 === "object" && sameKey(n(s3), n(l3))))
          break;
        if ((s3 !== l3 || l3 !== null && typeof l3 === "object" && isOwned(l3)) && l3 !== null && typeof l3 === "object")
          descend(unwrapValue(s3), l3, n, i, o);
        if (e.dk !== null && !u2 && !(l3 !== null && typeof l3 === "object" ? targetsEqual(s3, l3) : isEqual(s3, l3))) {
          bumpDeep(e);
          u2 = true;
        }
        if (c !== null) {
          const e2 = c[p];
          if (e2 !== undefined) {
            d++;
            notifyKeyValue(e2, p, a[p], l3, a, t);
          }
        }
      }
      if (e.dk !== null && !u2 && p < r2.length)
        bumpDeep(e);
      const y = p;
      let w = null;
      for (;p < r2.length; p++) {
        const e2 = r2[p];
        if (e2 !== null && typeof e2 === "object") {
          const t2 = n(e2);
          let l3;
          if (t2 !== undefined) {
            if (w === null) {
              w = new Map;
              for (let e4 = y;e4 < f2.length; e4++) {
                const t3 = unwrapValue(f2[e4]);
                if (t3 !== null && typeof t3 === "object") {
                  const o2 = n(t3);
                  if (o2 === undefined)
                    continue;
                  const f3 = w.get(o2);
                  if (f3 === undefined)
                    w.set(o2, e4);
                  else if (Array.isArray(f3))
                    f3.push(e4);
                  else
                    w.set(o2, [f3, e4]);
                }
              }
            }
            const e3 = w.get(t2);
            if (e3 === undefined)
              l3 = undefined;
            else if (Array.isArray(e3)) {
              l3 = unwrapValue(f2[e3.shift()]);
              if (e3.length === 1)
                w.set(t2, e3[0]);
            } else {
              l3 = unwrapValue(f2[e3]);
              w.delete(t2);
            }
          } else {
            l3 = unwrapValue(f2[p]);
          }
          descend(l3, e2, n, i, o);
        }
        if (c !== null) {
          const e3 = c[p];
          if (e3 !== undefined) {
            d++;
            notifyKeyDiff(e3, p, a, t, false);
          }
        }
      }
    } else {
      const l2 = Math.min(f2.length, r2.length);
      const s2 = r2.length;
      let p = false;
      for (let y = 0;y < s2; y++) {
        const s3 = r2[y];
        if (!u && y < l2 && s3 !== null && typeof s3 === "object")
          descend(unwrapValue(f2[y]), s3, n, i, o);
        if (e.dk !== null && !p && !(s3 !== null && typeof s3 === "object" ? targetsEqual(f2[y], s3) : isEqual(f2[y], s3))) {
          bumpDeep(e);
          p = true;
        }
        if (c !== null) {
          const e2 = c[y];
          if (e2 !== undefined) {
            d++;
            notifyKeyDiff(e2, y, a, t, false);
          }
        }
      }
    }
    if (s) {
      if (c !== null && d < e.nc) {
        for (const e2 of Reflect.ownKeys(c)) {
          const n2 = typeof e2 === "string" ? +e2 : NaN;
          if (!(n2 >= 0 && n2 < r2.length))
            notifyKeyDiff(c[e2], e2, a, t, false);
        }
      }
      notifyFoldTail(e, a, t);
    }
    return;
  } else {
    const f2 = s ? e.n : null;
    let r2 = 0;
    let c = false;
    for (const s2 in t) {
      const d2 = t[s2];
      const p = a[s2];
      const y = d2 !== null && typeof d2 === "object";
      if (p === d2 && (!y || !isOwned(d2)) && (f2 === null || f2[s2] === undefined || !hasAccessorFlag(f2[s2]))) {
        if (f2 !== null && f2[s2] !== undefined)
          r2++;
        continue;
      }
      if (y && !u)
        descend(unwrapValue(l[s2]), d2, n, i, o);
      if (e.dk !== null && !c && !(y ? targetsEqual(p, d2) : isEqual(p, d2))) {
        bumpDeep(e);
        c = true;
      }
      if (f2 !== null) {
        const e2 = f2[s2];
        if (e2 !== undefined) {
          r2++;
          notifyKeyValue(e2, s2, p, d2, a, t);
        }
      }
    }
    const d = Object.getOwnPropertySymbols(t);
    for (let e2 = 0;e2 < d.length; e2++) {
      const s2 = d[e2];
      if (s2 === $OWNER)
        continue;
      const c2 = t[s2];
      if (!u && c2 !== null && typeof c2 === "object")
        descend(unwrapValue(l[s2]), c2, n, i, o);
      if (f2 !== null) {
        const e3 = f2[s2];
        if (e3 !== undefined) {
          r2++;
          notifyKeyValue(e3, s2, a[s2], c2, a, t);
        }
      }
    }
    if (s) {
      if (f2 !== null && r2 < e.nc) {
        for (const e2 of Reflect.ownKeys(f2)) {
          if (!hasOwnP.call(t, e2))
            notifyKeyDiff(f2[e2], e2, a, t, false);
        }
      }
      notifyFoldTail(e, a, t);
    }
    return;
  }
}
var hasOwnP = Object.prototype.hasOwnProperty;
function sameKey(e, t) {
  return e === t || e !== e && t !== t;
}
function descend(e, t, n, o, f = false) {
  if (e === null || typeof e !== "object" || t === null || typeof t !== "object")
    return;
  const i = lookupTarget(e, o);
  if (i === undefined)
    return;
  if (!isWrappable(t))
    return;
  if (rawValuesUsed && isRawValue(t))
    return;
  t = unwrapValue(t);
  if (Array.isArray(e) !== Array.isArray(t))
    return;
  if (n) {
    const o2 = n(e);
    const f2 = n(t);
    if (o2 !== undefined && f2 !== undefined && !sameKey(o2, f2))
      return;
  }
  if (!f && n !== null && !i.d)
    return;
  applyAdopt(i, t, n, f);
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/next/projection.js
function wrapDraft(e, t, r, o, i) {
  const mutate = (e2) => {
    if (!t())
      return true;
    const o2 = projectionWriteActive;
    setWriteOverride(true);
    setProjectionWriteActive(true);
    try {
      r ? r(e2) : e2();
    } finally {
      setWriteOverride(false);
      setProjectionWriteActive(o2);
    }
    if (!o2 && i)
      i();
    return true;
  };
  const n = {
    get(n2, c) {
      let s;
      const l = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        s = e[c];
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(l);
      }
      return !o && typeof s === "object" && s !== null && c !== $TARGET ? wrapDraft(s, t, r, false, i) : s;
    },
    has(t2, r2) {
      let o2;
      const i2 = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        o2 = r2 in e;
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(i2);
      }
      return o2;
    },
    set: (t2, r2, o2) => mutate(() => {
      e[r2] = o2;
    }),
    deleteProperty: (t2, r2) => mutate(() => {
      delete e[r2];
    }),
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
      let o2;
      const i2 = projectionWriteActive;
      setWriteOverride(true);
      setProjectionWriteActive(true);
      try {
        o2 = Reflect.getOwnPropertyDescriptor(e, r2);
      } finally {
        setWriteOverride(false);
        setProjectionWriteActive(i2);
      }
      if (o2)
        o2.configurable = true;
      return o2;
    },
    defineProperty: (t2, r2, o2) => mutate(() => {
      Reflect.defineProperty(e, r2, o2);
    })
  };
  return new Proxy(Array.isArray(e) ? [] : {}, n);
}
function createProjectionNextInternal(e, t, r) {
  const o = {
    map: new WeakMap,
    node: null,
    shallow: !!r?.shallow
  };
  const i = wrapNext(t, null, null, o);
  if (o.shallow) {
    i[$TARGET].s = true;
    markRawIngest(t);
  }
  let n;
  if (r?.seedLoadingValue)
    n = {
      loadingValue: undefined
    };
  const c = computed(() => {
    if (!o.node)
      o.node = getOwner();
    runProjectionComputedNext(i, e, r?.key === undefined ? "id" : r.key);
  }, n);
  c.C &= ~CONFIG_AUTO_DISPOSE;
  o.node = c;
  return {
    store: i,
    node: c
  };
}
function createStoreDerivedNext(e, t, r) {
  const { store: o, node: i } = createProjectionNextInternal(e, t, r);
  return [o, (e2) => derivedStoreWrite(i, o, e2)];
}
function cloneState(e, t) {
  return t ? Array.isArray(e) ? e.slice() : {
    ...e
  } : JSON.parse(JSON.stringify(e));
}
function runProjectionComputedNext(e, t, r, o, i) {
  const n = getOwner();
  const c = e[$TARGET];
  const s = c.fam;
  const l = s.run = (s.run || 0) + 1;
  let u;
  const a = n.ve ? cloneState(c[STORE_VALUE], c.s) : null;
  const f = wrapDraft(e, () => s.run === l && !isDisposed(n), i, c.s, () => {
    if (!(n.h & STATUS_PENDING) && !n.ve)
      scheduleWithheld();
  });
  storeSetterNext(f, (i2) => {
    u = t(a ?? i2);
    const commit = (t2) => {
      if (a && (t2 === undefined || t2 === a))
        t2 = cloneState(a, c.s);
      if (t2 === i2 || t2 === undefined)
        return;
      const write = () => storeSetterNext(e, (e2) => reconcileNextState(t2, e2, r, true), false);
      o ? o(write, t2) : write();
    };
    const s2 = handleAsync(n, u, commit);
    if (!n.ve)
      commit(s2);
  }, false);
  return n;
}

// ../../node_modules/.bun/@solidjs+signals@2.0.0-rc.13/node_modules/@solidjs/signals/dist/prod/store/index.js
function createStore(e, t, r) {
  if (typeof e === "function")
    return createStoreDerivedNext(e, t, r);
  return createStoreNext(e, !!t?.shallow);
}
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
var propertyIsEnumerable2 = Object.prototype.propertyIsEnumerable;
function tableOwnKeys(e, n) {
  let t = e.keys;
  if (t === undefined) {
    t = e.keys = [];
    for (const [e2, r] of n)
      if (propertyIsEnumerable2.call(r, e2))
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
var sharedConfig = {
  hydrating: false,
  registry: undefined,
  done: false
};
var _createRoot;
var _createMemo;
var _createSignal;
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
var createSignal2 = (...args) => {
  return (_createSignal || createSignal)(...args);
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
  const create = () => runWithOwner(owner, () => mapArray(() => props.each, props.children, options));
  if (sharedConfig.hydrating)
    mapped = create();
  const list = () => (mapped ?? (mapped = create()))();
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
var globalMoveUnsub = null;
var interestRoot = null;
function onPointerMove(fn) {
  globalMoveSubs.add(fn);
  if (globalMoveSubs.size === 1) {
    globalMoveUnsub = on("pointerMove", (raw) => {
      let e = {
        timeStamp: raw.timeStamp,
        predicted: raw.predicted,
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
  if (getOwner())
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
function getLayoutBox2(node) {
  return tree.getLayoutBox(node.id);
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
var nextFrameId = 1;
var animationFrames = new Map;
var refreshRate = 60;
var latestTick = 0;
function onFrame(fn, options) {
  let demand = options?.demand !== false;
  let frameId = null;
  let cancelled = false;
  let extendedFn = (tick, frame, rate) => {
    if (cancelled)
      return;
    frameId = nextFrameId++;
    animationFrames.set(frameId, extendedFn);
    if (demand)
      requestFrame();
    try {
      fn(tick, frame, rate);
    } catch (err) {
      console.error("Error in onFrame callback:", err);
    }
  };
  frameId = nextFrameId++;
  animationFrames.set(frameId, extendedFn);
  if (demand)
    requestFrame();
  let cleanup2 = () => {
    cancelled = true;
    animationFrames.delete(frameId);
  };
  if (getOwner())
    onCleanup(cleanup2);
  return cleanup2;
}
var sizeAccessor;
var safeAreaAccessor;
var displayScaleAccessor;
function ensureResizeState() {
  if (sizeAccessor)
    return;
  runWithOwner(null, () => {
    let [size, setSize] = createSignal({
      width: 0,
      height: 0
    });
    let [safe, setSafe] = createSignal({
      top: 0,
      left: 0,
      right: 0,
      bottom: 0
    });
    let [scale, setScale] = createSignal(1);
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
  });
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
    runWithOwner(null, () => {
      let [focused, setFocused] = createSignal(true);
      on2("windowFocus", () => setFocused(true));
      on2("windowBlur", () => setFocused(false));
      focusedAccessor = focused;
    });
  }
  return focusedAccessor();
}
var keyboardHeightAccessor;
function keyboardHeight() {
  if (!keyboardHeightAccessor) {
    runWithOwner(null, () => {
      let [height, setHeight] = createSignal(0);
      on2("keyboardVisibility", ({
        height: h
      }) => setHeight(h ?? 0));
      keyboardHeightAccessor = height;
    });
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
  runPublish();
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
  if (getOwner())
    onCleanup(unsubscribe);
  return unsubscribe;
}
var lateHandlers = [];
var publishHandlers = [];
function runHandlers(handlers2) {
  for (let fn of [...handlers2]) {
    try {
      fn();
    } catch (err) {
      console.error("Error in onBeforeRender callback:", err);
    }
  }
}
function runBeforeRender(bootstrap) {
  if (!bootstrap)
    runHandlers(lateHandlers);
  runHandlers(publishHandlers);
}
function runPublish() {
  runHandlers(publishHandlers);
}
function onLink(fn) {
  let unsubscribe = on2("link", (e) => fn(e.link));
  if (getOwner())
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
  if (getOwner())
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
    runBeforeRender(bootstrap);
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
function mixColors2(a, b, t) {
  return tree3.mixColors(a, b, t);
}
var ALPHA_MAX = 255;
function withAlpha(color, alpha) {
  let rgb = parseColor2(color) >>> 8;
  let a = Math.round(Math.min(1, Math.max(0, alpha)) * ALPHA_MAX);
  return "#" + rgb.toString(16).padStart(6, "0") + a.toString(16).padStart(2, "0");
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
function createRadialGradient(cx, cy, r, stops, opts) {
  return {
    __gradient: "radial",
    cx,
    cy,
    r,
    circle: opts?.shape === "circle",
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
import { on as on3 } from "sol:events";
var devicesAccessor;
function ensureDevicesState() {
  if (devicesAccessor)
    return;
  runWithOwner(null, () => {
    let [devices, setDevices] = createSignal(undefined);
    on3("inputDevices", (d) => {
      setDevices({
        keyboard: !!d.keyboard,
        mouse: !!d.mouse,
        touch: !!d.touch,
        screenKeyboard: !!d.screenKeyboard
      });
    });
    devicesAccessor = devices;
  });
}
var systemThemeAccessor;
function ensureSystemThemeState() {
  if (systemThemeAccessor)
    return;
  runWithOwner(null, () => {
    let [theme, setTheme] = createSignal("unknown");
    on3("systemTheme", (e) => setTheme(e.theme ?? "unknown"));
    systemThemeAccessor = theme;
  });
}
var launchValue;
function ensureLaunchState() {
  if (launchValue)
    return;
  runWithOwner(null, () => {
    on3("launch", (e) => {
      launchValue = e.state === "restored" ? "restored" : "fresh";
    });
  });
  launchValue ??= "fresh";
}
var launchLinkValue;
function ensureLaunchLinkState() {
  if (launchLinkValue !== undefined)
    return;
  runWithOwner(null, () => {
    on3("launchLink", (e) => {
      launchLinkValue = typeof e.link === "string" ? e.link : null;
    });
  });
  launchLinkValue ??= null;
}
var visibilityAccessor;
function ensureVisibilityState() {
  if (visibilityAccessor)
    return;
  runWithOwner(null, () => {
    let [visibility, setVisibility] = createSignal("visible");
    on3("visibility", (e) => setVisibility(e.state === "hidden" ? "hidden" : "visible"));
    visibilityAccessor = visibility;
  });
}
var orientationAccessor;
function ensureOrientationState() {
  if (orientationAccessor)
    return;
  runWithOwner(null, () => {
    let [orientation, setOrientation] = createSignal("unknown");
    on3("displayOrientation", (e) => {
      setOrientation(e.orientation ?? "unknown");
    });
    orientationAccessor = orientation;
  });
}
var textScaleAccessor;
function ensureTextScaleState() {
  if (textScaleAccessor)
    return;
  runWithOwner(null, () => {
    let [scale, setScale] = createSignal(1);
    on3("textScale", (e) => {
      setScale(typeof e.scale === "number" && e.scale > 0 ? e.scale : 1);
    });
    textScaleAccessor = scale;
  });
}
var mouseSeenAccessor;
var touchSeenAccessor;
function ensurePointerState() {
  if (mouseSeenAccessor)
    return;
  runWithOwner(null, () => {
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
    unsubMove = onPointerMove(note);
    unsubs.push(unsubMove, on3("pointerDown", note));
    mouseSeenAccessor = mouse;
    touchSeenAccessor = touch;
  });
}
var keyboardSeenAccessor;
function ensureKeyboardState() {
  if (keyboardSeenAccessor)
    return;
  runWithOwner(null, () => {
    let [keyboard, setKeyboard] = createSignal(false);
    let unsub = on3("keydown", () => {
      setKeyboard(true);
      unsub();
    });
    keyboardSeenAccessor = keyboard;
  });
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
  get launch() {
    ensureLaunchState();
    return launchValue;
  },
  get launchLink() {
    ensureLaunchLinkState();
    return launchLinkValue;
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

// ../../packages/core/src/shaders.ts
var glsl = String.raw;
var SCREEN_SIZE_GLSL = glsl`
  float screenSizeScale(vec2 size, float pxPerUnit, vec2 screenPx) {
    float smallest = min(abs(size.x), abs(size.y)) * pxPerUnit;
    if (smallest <= 0.0) return 1.0;
    float target = smallest;
    if (screenPx.x > 0.0) target = max(target, screenPx.x);
    if (screenPx.y > 0.0) target = min(target, screenPx.y);
    return target / smallest;
  }
`;

// ../../packages/core/src/gpu.ts
import { captureSnapshot, readTexture } from "flux:gpu";
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
  let origin2 = new Error().stack ?? "";
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
${origin2}`);
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
var pending = new Map;
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
  },
  pend(pointerId, owner) {
    let p = pending.get(pointerId);
    if (!p) {
      p = {
        owners: new Set,
        fires: []
      };
      pending.set(pointerId, p);
    }
    p.owners.add(owner);
  },
  decide(pointerId, owner, won) {
    let p = pending.get(pointerId);
    if (!p || !p.owners.has(owner))
      return;
    if (won) {
      pending.delete(pointerId);
      return;
    }
    p.owners.delete(owner);
    if (p.owners.size > 0)
      return;
    pending.delete(pointerId);
    for (let fire of p.fires)
      fire();
  },
  defer(pointerId, fire) {
    let p = pending.get(pointerId);
    if (!p)
      return false;
    p.fires.push(fire);
    return true;
  }
};
// ../../packages/core/src/velocity.ts
var VELOCITY_WINDOW_MS = 100;
var VELOCITY_MAX = 8000;
var VELOCITY_REST_MS = 50;
var VELOCITY_STOP_GAP_MS = 40;
var VELOCITY_MIN_TRAVEL = 8;
var VELOCITY_MIN_STEP_MS = 1;
var VELOCITY_SAMPLES = 20;
var FLING_MIN_VELOCITY = 50;
var ZERO = {
  vx: 0,
  vy: 0
};
var flingVelocity = (v) => Math.hypot(v.vx, v.vy) < FLING_MIN_VELOCITY ? ZERO : v;
function createVelocityTracker() {
  let xs = new Float64Array(VELOCITY_SAMPLES);
  let ys = new Float64Array(VELOCITY_SAMPLES);
  let ts = new Float64Array(VELOCITY_SAMPLES);
  let head = 0;
  let count = 0;
  return {
    push(x, y, at) {
      xs[head] = x;
      ys[head] = y;
      ts[head] = at;
      head = (head + 1) % VELOCITY_SAMPLES;
      if (count < VELOCITY_SAMPLES)
        count++;
    },
    shift(dx, dy) {
      for (let i = 0;i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES;
        xs[k] = xs[k] + dx;
        ys[k] = ys[k] + dy;
      }
    },
    reset() {
      head = 0;
      count = 0;
    },
    velocity(at) {
      if (count < 2)
        return ZERO;
      let newest = (head - 1 + VELOCITY_SAMPLES) % VELOCITY_SAMPLES;
      let movedAt = ts[newest];
      for (let i = 1;i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES;
        if (xs[k] !== xs[newest] || ys[k] !== ys[newest])
          break;
        movedAt = ts[k];
      }
      if (at - movedAt > VELOCITY_REST_MS)
        return ZERO;
      let s0 = 0;
      let s1 = 0;
      let s2 = 0;
      let s3 = 0;
      let s4 = 0;
      let x0 = 0;
      let x1 = 0;
      let x2 = 0;
      let y0 = 0;
      let y1 = 0;
      let y2 = 0;
      let times = 0;
      let prev = Infinity;
      let later = ts[newest];
      let farX = 0;
      let farY = 0;
      for (let i = 0;i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES;
        if (at - ts[k] > VELOCITY_WINDOW_MS)
          break;
        if (later - ts[k] > VELOCITY_STOP_GAP_MS)
          break;
        later = ts[k];
        let t = ts[k] - ts[newest];
        if (prev - t >= VELOCITY_MIN_STEP_MS) {
          times++;
          prev = t;
        }
        let x = xs[k] - xs[newest];
        let y = ys[k] - ys[newest];
        farX = x;
        farY = y;
        let tt = t * t;
        s0 += 1;
        s1 += t;
        s2 += tt;
        s3 += tt * t;
        s4 += tt * tt;
        x0 += x;
        x1 += t * x;
        x2 += tt * x;
        y0 += y;
        y1 += t * y;
        y2 += tt * y;
      }
      if (times < 2)
        return ZERO;
      if (Math.hypot(farX, farY) < VELOCITY_MIN_TRAVEL)
        return ZERO;
      let det = s0 * (s2 * s4 - s3 * s3) - s1 * (s1 * s4 - s2 * s3) + s2 * (s1 * s3 - s2 * s2);
      let slope = (m0, m1, m2) => {
        let line = (s0 * m1 - s1 * m0) / (s0 * s2 - s1 * s1);
        if (times < 3)
          return line;
        let curve = (s0 * (m1 * s4 - s3 * m2) - m0 * (s1 * s4 - s3 * s2) + s2 * (s1 * m2 - m1 * s2)) / det;
        return curve * line < 0 ? 0 : curve;
      };
      let vx = slope(x0, x1, x2) * 1000;
      let vy = slope(y0, y1, y2) * 1000;
      let speed = Math.hypot(vx, vy);
      if (speed > VELOCITY_MAX) {
        let f = VELOCITY_MAX / speed;
        vx *= f;
        vy *= f;
      }
      return {
        vx,
        vy
      };
    }
  };
}

// ../../packages/core/src/pan.ts
var PAN_SLOP = 8;
function createPan(options) {
  let origin2 = null;
  let active = null;
  let armed = null;
  let tracker = createVelocityTracker();
  let past = (e) => {
    if (!origin2)
      return false;
    let dx = Math.abs(e.clientX - origin2.x);
    let dy = Math.abs(e.clientY - origin2.y);
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
    origin2 = null;
  };
  let cancel = () => {
    let started = active != null;
    reset();
    if (started)
      options.onPanCancel?.();
  };
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
        origin2 = {
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
          origin2 = {
            x: e.parentX,
            y: e.parentY
          };
          tracker.reset();
          if (!e.predicted)
            tracker.push(e.parentX, e.parentY, e.timeStamp);
          options.onPanStart?.();
        } else {
          reset();
        }
        return;
      }
      if (active === e.pointerId && origin2) {
        if (!e.predicted)
          tracker.push(e.parentX, e.parentY, e.timeStamp);
        options.onPanMove?.(e.parentX - origin2.x, e.parentY - origin2.y);
        origin2 = {
          x: e.parentX,
          y: e.parentY
        };
      }
    },
    onPointerUp: (e) => {
      if (active === e.pointerId) {
        let velocity = flingVelocity(tracker.velocity(e.timeStamp));
        reset();
        options.onPanEnd?.(velocity);
      } else if (armed === e.pointerId) {
        reset();
      }
    },
    onPointerCancel: (e) => {
      if (active === e.pointerId)
        cancel();
      else if (armed === e.pointerId)
        reset();
    }
  };
  return {
    handlers: handlers2,
    cancel
  };
}
// ../../packages/core/src/transform.ts
import { on as on5 } from "sol:events";
// ../../packages/core/src/swipe.ts
var SWIPE_ANGLE_TOLERANCE = 30;
var OFF_AXIS_RATIO = Math.tan(SWIPE_ANGLE_TOLERANCE * Math.PI / 180);
// ../../packages/core/src/input-axes.ts
var clampNum = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
function combineRates(kind, values) {
  if (kind === "axis") {
    let sum = 0;
    for (let v of values)
      sum += v;
    return clampNum(sum, -1, 1);
  }
  let x = 0;
  let y = 0;
  for (let v of values) {
    x += v[0];
    y += v[1];
  }
  let len = Math.hypot(x, y);
  return len > 1 ? [x / len, y / len] : [x, y];
}

// ../../packages/core/src/input-processors.ts
var HOLD_MS = 400;
var TAP_MS = 200;
var DOUBLE_TAP_GAP_MS = 300;
function checkSource(source) {
  let s = source;
  if (!s || typeof s !== "object" && typeof s !== "function" || s.kind !== "button" && s.kind !== "axis" && s.kind !== "vec2" || typeof s.label !== "string" || typeof s.id !== "string") {
    throw new Error(`createInputMap: not an input source: ${String(source)}`);
  }
  return s;
}
var checkAxisSource = (what, source) => {
  checkSource(source);
  if (source.kind === "button")
    throw new Error(`${what}: "${source.label}" is a button`);
};
var checkButtonSource = (what, source) => {
  checkSource(source);
  if (source.kind !== "button")
    throw new Error(`${what}: "${source.label}" is a ${source.kind}, not a button`);
};
var checkTimedSource = (what, source) => {
  checkButtonSource(what, source);
  if (typeof source.changedAt !== "function") {
    throw new Error(`${what}: "${source.label}" does not say when it changes (no changedAt), so its presses cannot be timed`);
  }
};
var checkMs = (what, ms) => {
  if (!Number.isFinite(ms) || ms < 0)
    throw new Error(`${what}: ms must be a non-negative number, got ${String(ms)}`);
};
function invert(source) {
  checkAxisSource("invert", source);
  let neg = (v) => typeof v === "number" ? -v : [-v[0], -v[1]];
  return {
    kind: source.kind,
    label: `${source.label} (inverted)`,
    id: `invert(${source.id})`,
    device: source.device,
    rate: source.rate ? () => neg(source.rate()) : undefined,
    deltas: source.deltas ? (sink) => source.deltas({
      begin: sink.begin,
      end: (velocity) => sink.end(velocity === undefined ? undefined : neg(velocity)),
      delta: (value, focal) => sink.delta(neg(value), focal)
    }) : undefined,
    key: source.key,
    blur: source.blur
  };
}
function scale(source, factor) {
  checkAxisSource("scale", source);
  if (!Number.isFinite(factor))
    throw new Error(`scale: factor must be a finite number, got ${factor}`);
  let mul = (v) => typeof v === "number" ? v * factor : [v[0] * factor, v[1] * factor];
  return {
    kind: source.kind,
    label: `${source.label} (x${factor})`,
    id: `scale(${factor},${source.id})`,
    device: source.device,
    rate: source.rate ? () => mul(source.rate()) : undefined,
    deltas: source.deltas ? (sink) => source.deltas({
      begin: sink.begin,
      end: (velocity) => sink.end(velocity === undefined ? undefined : mul(velocity)),
      delta: (value, focal) => sink.delta(mul(value), focal)
    }) : undefined,
    key: source.key,
    blur: source.blur
  };
}
function derived(source, label, id2, edge) {
  let [pressed, setPressed] = createSignal(false, {
    ownedWrite: true
  });
  let timer = null;
  let hooks = {
    set: setPressed,
    later(ms, run) {
      hooks.clearLater();
      timer = setTimeout(() => {
        timer = null;
        run();
      }, ms);
    },
    clearLater() {
      if (timer !== null)
        clearTimeout(timer);
      timer = null;
    }
  };
  let dispose2 = createRoot((dispose3) => {
    createEffect(() => source.rate(), (down, prev) => {
      if (down !== prev)
        edge(down, source.changedAt?.() ?? null, hooks);
    }, {
      defer: true
    });
    return dispose3;
  });
  if (getOwner()) {
    onCleanup(() => {
      hooks.clearLater();
      dispose2();
    });
  }
  return {
    kind: "button",
    label,
    id: id2,
    device: source.device,
    rate: pressed,
    key: source.key,
    blur: source.blur
  };
}
var pulse = (hooks) => {
  hooks.set(true);
  hooks.later(0, () => hooks.set(false));
};
function hold(source, ms = HOLD_MS) {
  checkButtonSource("hold", source);
  checkMs("hold", ms);
  return derived(source, `${source.label} (hold ${ms} ms)`, `hold(${ms},${source.id})`, (down, _at, hooks) => {
    if (down)
      hooks.later(ms, () => hooks.set(true));
    else {
      hooks.clearLater();
      hooks.set(false);
    }
  });
}
function tap(source, ms = TAP_MS) {
  checkTimedSource("tap", source);
  checkMs("tap", ms);
  let downAt = null;
  return derived(source, `${source.label} (tap)`, `tap(${ms},${source.id})`, (down, at, hooks) => {
    if (down)
      downAt = at;
    else {
      if (at !== null && downAt !== null && at - downAt <= ms)
        pulse(hooks);
      downAt = null;
    }
  });
}
function doubleTap(source, gapMs = DOUBLE_TAP_GAP_MS, tapMs = TAP_MS) {
  checkTimedSource("doubleTap", source);
  checkMs("doubleTap", gapMs);
  checkMs("doubleTap", tapMs);
  let downAt = null;
  let lastTap = -Infinity;
  return derived(source, `${source.label} (double tap)`, `doubleTap(${gapMs},${tapMs},${source.id})`, (down, at, hooks) => {
    if (down) {
      downAt = at;
      return;
    }
    let pressedAt = downAt;
    downAt = null;
    if (at === null || pressedAt === null) {
      lastTap = -Infinity;
      return;
    }
    if (at - pressedAt > tapMs)
      return;
    if (at - lastTap <= gapMs) {
      lastTap = -Infinity;
      pulse(hooks);
    } else
      lastTap = at;
  });
}
function chord(...sources) {
  if (sources.length < 2)
    throw new Error("chord: needs at least two button sources");
  for (let s of sources)
    checkButtonSource("chord", s);
  return {
    kind: "button",
    label: sources.map((s) => s.label).join(" + "),
    id: `chord(${sources.map((s) => s.id).join(",")})`,
    device: sources[0].device,
    rate: () => sources.every((s) => s.rate?.() === true),
    changedAt: sources.every((s) => s.changedAt) ? () => {
      let latest2 = null;
      for (let s of sources) {
        let at = s.changedAt();
        if (at === null)
          return null;
        if (latest2 === null || at > latest2)
          latest2 = at;
      }
      return latest2;
    } : undefined,
    key: (event, down) => {
      for (let s of sources)
        s.key?.(event, down);
    },
    blur: () => {
      for (let s of sources)
        s.blur?.();
    }
  };
}

// ../../packages/core/src/input-id.ts
var splitArgs = (text) => {
  let out = [];
  let depth = 0;
  let start = 0;
  for (let i = 0;i < text.length; i++) {
    let c = text[i];
    if (c === "(")
      depth++;
    else if (c === ")")
      depth--;
    else if (c === "," && depth === 0) {
      out.push(text.slice(start, i));
      start = i + 1;
    }
  }
  out.push(text.slice(start));
  return out;
};
var number = (id2, text) => {
  let n = Number(text);
  if (text.trim() === "" || !Number.isFinite(n))
    throw new Error(`resolveSource: "${id2}" has a non-numeric argument "${text}"`);
  return n;
};
function resolveSource(id2, devices) {
  if (typeof id2 !== "string" || id2.length === 0)
    throw new Error(`resolveSource: expected an id, got ${String(id2)}`);
  let processed = /^([A-Za-z]+)\((.*)\)$/.exec(id2);
  if (processed) {
    let [, name2, inner] = processed;
    let args = splitArgs(inner);
    let axis = (i) => resolveSource(args[i], devices);
    let button = (i) => resolveSource(args[i], devices);
    let arity = (n) => {
      if (args.length !== n)
        throw new Error(`resolveSource: ${name2}() takes ${n} argument(s), "${id2}" gives ${args.length}`);
    };
    switch (name2) {
      case "invert":
        arity(1);
        return invert(axis(0));
      case "scale":
        arity(2);
        return scale(axis(1), number(id2, args[0]));
      case "hold":
        arity(2);
        return hold(button(1), number(id2, args[0]));
      case "tap":
        arity(2);
        return tap(button(1), number(id2, args[0]));
      case "doubleTap":
        arity(3);
        return doubleTap(button(2), number(id2, args[0]), number(id2, args[1]));
      case "chord":
        return chord(...args.map((_, i) => button(i)));
      default:
        throw new Error(`resolveSource: unknown processor "${name2}" in "${id2}"`);
    }
  }
  let colon = id2.indexOf(":");
  if (colon <= 0)
    throw new Error(`resolveSource: "${id2}" names no device (expected "<device>:<spec>")`);
  let name = id2.slice(0, colon);
  let device = devices[name];
  if (!device)
    throw new Error(`resolveSource: no "${name}" device given for "${id2}" (given: ${Object.keys(devices).join(", ") || "none"})`);
  return device.resolve(id2.slice(colon + 1));
}

// ../../packages/core/src/input-chord.ts
var MODIFIERS = {
  Shift: "shiftKey",
  Ctrl: "ctrlKey",
  Control: "ctrlKey",
  Alt: "altKey",
  Meta: "metaKey"
};
var ORDER = ["shiftKey", "ctrlKey", "altKey", "metaKey"];
var NAMES = {
  shiftKey: "Shift",
  ctrlKey: "Ctrl",
  altKey: "Alt",
  metaKey: "Meta"
};
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
var chordName = (mods) => mods.map((m) => NAMES[m]).join("+");
var eventModifiers = (event) => ORDER.filter((m) => event[m]);
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
function keySpec(event) {
  if (MODIFIER_NAMES.has(event.key))
    return null;
  let key2 = event.code || event.key;
  if (!key2)
    return null;
  let mods = chordName(eventModifiers(event));
  return mods ? `${mods}+${key2}` : key2;
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

// ../../packages/core/src/input-map.ts
var compatible = (source, action2) => source === action2 || source === "button" && action2 === "axis";
var neutral = (kind) => kind === "button" ? false : kind === "axis" ? 0 : [0, 0];
var isNeutral = (v) => typeof v === "boolean" ? !v : typeof v === "number" ? v === 0 : v[0] === 0 && v[1] === 0;
function checkValue(what, kind, v) {
  if (kind === "button") {
    if (typeof v !== "boolean")
      throw new Error(`${what}: expected a boolean, got ${String(v)}`);
  } else if (kind === "axis") {
    if (typeof v !== "number" || !Number.isFinite(v))
      throw new Error(`${what}: expected a finite number, got ${String(v)}`);
  } else if (!Array.isArray(v) || v.length !== 2 || !Number.isFinite(v[0]) || !Number.isFinite(v[1])) {
    throw new Error(`${what}: expected [x, y] finite numbers, got ${JSON.stringify(v)}`);
  }
}
var AXIS_PARTS = ["neg", "pos"];
var VEC2_PARTS = ["up", "down", "left", "right"];
function createInputMap(actions) {
  if (!actions || typeof actions !== "object")
    throw new Error("createInputMap: expected an object of action kinds");
  let states = new Map;
  let [device, setDevice] = createSignal(undefined, {
    ownedWrite: true
  });
  for (let [name, kind] of Object.entries(actions)) {
    if (kind !== "button" && kind !== "axis" && kind !== "vec2")
      throw new Error(`createInputMap: action "${name}" has kind "${String(kind)}", expected "button", "axis" or "vec2"`);
    let [version, setVersion] = createSignal(0, {
      ownedWrite: true
    });
    let state2 = {
      kind,
      sources: [],
      script: null,
      enabled: true,
      depth: 0,
      version,
      bump: () => setVersion(untrack(version) + 1),
      value: () => {
        version();
        if (!state2.enabled)
          return neutral(kind);
        let s = state2.script;
        if (kind === "button") {
          if (s === true)
            return true;
          return state2.sources.some((src) => src.rate?.() === true);
        }
        let values = [];
        if (s !== null)
          values.push(s);
        for (let src of state2.sources) {
          if (!src.rate)
            continue;
          if (src.kind === "button" && src.deltas)
            continue;
          let v = src.rate();
          if (src.kind === "button")
            values.push(v ? 1 : 0);
          else
            values.push(v);
        }
        return combineRates(kind, values);
      },
      gesture: new Set,
      live: new Map,
      watch: new Map
    };
    states.set(name, state2);
  }
  let state = (name) => {
    let s = states.get(name);
    if (!s)
      throw new Error(`createInputMap: no action "${name}" (declared: ${[...states.keys()].join(", ")})`);
    return s;
  };
  let axisState = (name, what) => {
    let s = state(name);
    if (s.kind === "button")
      throw new Error(`createInputMap: ${what} needs an axis or vec2 action, "${name}" is a button`);
    return s;
  };
  let buttonState = (name, what) => {
    let s = state(name);
    if (s.kind !== "button")
      throw new Error(`createInputMap: ${what} needs a button action, "${name}" is a ${s.kind}`);
    return s;
  };
  let touched = (source) => {
    if (source?.device && untrack(device) !== source.device)
      setDevice(source.device);
  };
  let closeGesture = (s, velocity) => {
    s.depth--;
    s.gesture.forEach((g) => g.end?.(velocity));
  };
  let sink = (s, source) => ({
    begin: () => {
      if (!s.enabled)
        return;
      s.depth++;
      s.gesture.forEach((g) => g.begin?.());
    },
    delta: (value, focal) => {
      touched(source);
      if (!s.enabled)
        return;
      s.gesture.forEach((g) => g.delta?.(value, focal));
    },
    end: (velocity) => {
      if (s.depth > 0)
        closeGesture(s, velocity);
    }
  });
  let watchDevice = (source) => createRoot((dispose2) => {
    createEffect(() => source.rate(), (v) => {
      if (!isNeutral(v))
        touched(source);
    }, {
      defer: true
    });
    return dispose2;
  });
  let switchActions = (names, on6, what) => {
    if (names.length === 0)
      throw new Error(`createInputMap: ${what}() needs at least one action`);
    for (let name of names) {
      let s = state(name);
      if (s.enabled === on6)
        continue;
      s.enabled = on6;
      while (!on6 && s.depth > 0)
        closeGesture(s);
      s.bump();
    }
  };
  let checkBind = (name, source) => {
    let s = state(name);
    checkSource(source);
    if (!compatible(source.kind, s.kind))
      throw new Error(`createInputMap: cannot bind ${source.kind} source "${source.label}" to ${s.kind} action "${name}"`);
    return s;
  };
  let bindOne = (name, source) => {
    let s = checkBind(name, source);
    if (s.sources.includes(source))
      return;
    s.sources.push(source);
    s.bump();
    if (source.deltas && s.kind !== "button")
      s.live.set(source, source.deltas(sink(s, source)));
    if (source.rate && source.device)
      s.watch.set(source, watchDevice(source));
  };
  let unbindOne = (name, source) => {
    let s = state(name);
    let i = s.sources.indexOf(source);
    if (i < 0)
      return;
    s.sources.splice(i, 1);
    s.bump();
    let stop = s.live.get(source);
    if (stop) {
      s.live.delete(source);
      stop();
    }
    let unwatch = s.watch.get(source);
    if (unwatch) {
      s.watch.delete(source);
      unwatch();
    }
  };
  let edge = (name, want, callback) => {
    let s = buttonState(name, want ? "onPress" : "onRelease");
    if (typeof callback !== "function")
      throw new Error(`createInputMap: ${want ? "onPress" : "onRelease"}("${name}") expects a function`);
    let last = untrack(() => s.value());
    return createRoot((dispose2) => {
      createEffect(() => s.value(), (pressed) => {
        if (pressed === want && last !== want)
          untrack(callback);
        last = pressed;
      }, {
        defer: true
      });
      return dispose2;
    });
  };
  let capture = null;
  let map = {
    actions,
    bind(actionOrList, ...sources) {
      let list = Array.isArray(actionOrList) ? actionOrList : sources.map((source) => ({
        action: actionOrList,
        source
      }));
      if (list.length === 0)
        throw new Error("createInputMap: bind() needs at least one source");
      for (let b of list)
        bindOne(b.action, b.source);
      return () => {
        for (let b of list)
          unbindOne(b.action, b.source);
      };
    },
    unbind: unbindOne,
    bindings(action2) {
      let out = [];
      for (let [name, s] of states) {
        if (action2 !== undefined && name !== action2)
          continue;
        for (let source of s.sources)
          out.push({
            action: name,
            source
          });
      }
      if (action2 !== undefined)
        state(action2);
      return out;
    },
    value(action2) {
      return state(action2).value();
    },
    pressed(action2) {
      return buttonState(action2, "pressed").value();
    },
    onPress: (action2, callback) => edge(action2, true, callback),
    onRelease: (action2, callback) => edge(action2, false, callback),
    onGesture(action2, listener) {
      let s = axisState(action2, "onGesture");
      if (!listener || typeof listener !== "object")
        throw new Error(`createInputMap: onGesture("${action2}") expects a listener object`);
      s.gesture.add(listener);
      return () => {
        s.gesture.delete(listener);
      };
    },
    set(action2, value) {
      let s = state(action2);
      checkValue(`set("${action2}")`, s.kind, value);
      s.script = value;
      s.bump();
    },
    press(action2) {
      let s = buttonState(action2, "press");
      s.script = true;
      s.bump();
    },
    release(action2) {
      let s = buttonState(action2, "release");
      s.script = false;
      s.bump();
    },
    nudge(action2, delta, focal) {
      let s = axisState(action2, "nudge");
      checkValue(`nudge("${action2}")`, s.kind, delta);
      if (focal !== undefined)
        checkValue(`nudge("${action2}") focal`, "vec2", focal);
      sink(s).delta(delta, focal);
    },
    begin(action2) {
      sink(axisState(action2, "begin")).begin();
    },
    end(action2, velocity) {
      let s = axisState(action2, "end");
      if (velocity !== undefined)
        checkValue(`end("${action2}") velocity`, s.kind, velocity);
      sink(s).end(velocity);
    },
    enable: (...actions2) => switchActions(actions2, true, "enable"),
    disable: (...actions2) => switchActions(actions2, false, "disable"),
    enabled(action2) {
      let s = state(action2);
      s.version();
      return s.enabled;
    },
    device,
    save() {
      let out = {};
      for (let [name, s] of states)
        out[name] = s.sources.map((source) => source.id);
      return out;
    },
    load(saved, devices) {
      if (!saved || typeof saved !== "object")
        throw new Error("createInputMap: load() expects an object of id lists per action");
      if (!devices || typeof devices !== "object")
        throw new Error("createInputMap: load() expects the devices to resolve through");
      let plan = [];
      for (let [name, ids] of Object.entries(saved)) {
        if (!Array.isArray(ids))
          throw new Error(`createInputMap: load() action "${name}": expected an array of ids`);
        let sources = ids.map((id2) => resolveSource(id2, devices));
        for (let source of sources)
          checkBind(name, source);
        plan.push([name, sources]);
      }
      for (let [name, sources] of plan) {
        for (let source of [...state(name).sources])
          unbindOne(name, source);
        for (let source of sources)
          bindOne(name, source);
      }
    },
    rebind(action2, devices, options = {}) {
      let s = state(action2);
      if (!devices || typeof devices !== "object")
        throw new Error("createInputMap: rebind() expects the devices to listen on");
      if (capture)
        throw new Error("createInputMap: a rebind is already pending on this map");
      let {
        signal: signal2,
        replace = true,
        part
      } = options;
      let parts = s.kind === "axis" ? AXIS_PARTS : s.kind === "vec2" ? VEC2_PARTS : [];
      if (part !== undefined) {
        if (s.kind === "button")
          throw new Error(`createInputMap: rebind("${action2}") part: "${action2}" is a button, a key rebinds it whole`);
        if (!parts.includes(part))
          throw new Error(`createInputMap: rebind("${action2}") part "${part}": a ${s.kind} action has ${parts.join("/")}`);
      }
      if (!devices.keyboard && !devices.gamepad?.listen && !devices.pointer?.listen)
        throw new Error(`createInputMap: rebind("${action2}") has no device to listen on`);
      if (signal2?.aborted)
        return Promise.reject(signal2.reason ?? new Error("rebind aborted"));
      return new Promise((resolve3, reject) => {
        let stops = [];
        let settled = false;
        let prevAbort = signal2?.onabort ?? null;
        let cleanup2 = () => {
          settled = true;
          capture = null;
          if (signal2)
            signal2.onabort = prevAbort;
          for (let stop of stops)
            stop();
        };
        let abort = () => {
          if (settled)
            return;
          cleanup2();
          reject(signal2.reason ?? new Error("rebind aborted"));
        };
        let found = (source, replacing) => {
          if (settled)
            return;
          settled = true;
          queueMicrotask(() => {
            try {
              cleanup2();
              if (replacing) {
                for (let bound of [...s.sources])
                  if (bound.device === source.device)
                    unbindOne(action2, bound);
              }
              bindOne(action2, source);
              resolve3(source);
            } catch (err) {
              reject(err);
            }
          });
        };
        let keyboard2 = devices.keyboard;
        if (keyboard2) {
          capture = (event) => {
            let spec = keySpec(event);
            if (spec === null)
              return;
            if (s.kind === "button") {
              found(keyboard2.resolve(`key:${spec}`), replace);
              return;
            }
            if (part === undefined)
              return;
            let prefix = `${s.kind}:`;
            let composite = s.sources.find((b) => b.device === "keyboard" && b.id.startsWith(`keyboard:${prefix}`));
            if (!composite) {
              settled = true;
              cleanup2();
              reject(new Error(`createInputMap: rebind("${action2}") part "${part}": no keyboard ${s.kind} composite is bound to "${action2}"`));
              return;
            }
            let specs = composite.id.slice(`keyboard:${prefix}`.length).split("/");
            specs[parts.indexOf(part)] = spec;
            let next = keyboard2.resolve(`${prefix}${specs.join("/")}`);
            unbindOne(action2, composite);
            found(next, false);
          };
        }
        for (let name of ["gamepad", "pointer"]) {
          let d = devices[name];
          if (d?.listen) {
            stops.push(d.listen(s.kind, (source) => {
              if (compatible(source.kind, s.kind))
                found(source, replace);
            }));
          }
        }
        if (signal2) {
          signal2.onabort = (event) => {
            prevAbort?.(event);
            abort();
          };
        }
      });
    },
    handlers: {
      onKeyDown: (event) => forwardKey(event, true),
      onKeyUp: (event) => forwardKey(event, false),
      onBlur: () => {
        let seen = new Set;
        for (let s of states.values()) {
          for (let source of s.sources) {
            if (seen.has(source))
              continue;
            seen.add(source);
            source.blur?.();
          }
        }
      }
    },
    drive(axes, names) {
      let stops = [];
      for (let [axis2, kind] of Object.entries(axes.kinds)) {
        let mapped = names && axis2 in names ? names[axis2] : axis2;
        if (mapped === null)
          continue;
        if (mapped === undefined || !states.has(mapped)) {
          throw new Error(`createInputMap: drive() has no action for axis "${axis2}" (declared: ${[...states.keys()].join(", ")}); declare it, map it with names, or skip it with null`);
        }
        let s = state(mapped);
        if (s.kind !== kind)
          throw new Error(`createInputMap: drive() maps ${kind} axis "${axis2}" to ${s.kind} action "${mapped}"`);
        stops.push(axes.add(axis2, () => s.value()));
        stops.push(map.onGesture(mapped, {
          begin: () => axes.begin(axis2),
          delta: (value, focal) => axes.nudge(axis2, value, focal),
          end: (velocity) => axes.end(axis2, velocity)
        }));
      }
      return () => {
        for (let stop of stops)
          stop();
      };
    }
  };
  let forwardKey = (event, down) => {
    if (capture && down && !event.repeat) {
      capture(event);
      return;
    }
    let seen = new Set;
    for (let s of states.values()) {
      for (let source of s.sources) {
        if (seen.has(source) || !source.key)
          continue;
        seen.add(source);
        source.key(event, down);
      }
    }
  };
  return map;
}
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
// ../../packages/router/src/route.ts
var REST_PARAM = "rest";
function parseSegments(path) {
  let out = [];
  for (let piece of path.split("/")) {
    if (piece === "")
      continue;
    if (piece === "$") {
      out.push({
        kind: "rest"
      });
    } else if (piece.startsWith("$")) {
      let optional = piece.endsWith("?");
      let name = piece.slice(1, optional ? -1 : undefined);
      if (name === "")
        throw new Error(`Route path "${path}": a param needs a name`);
      out.push({
        kind: "param",
        name,
        optional
      });
    } else {
      out.push({
        kind: "literal",
        value: piece
      });
    }
  }
  for (let i = 0;i < out.length - 1; i++) {
    let s = out[i];
    if (s.kind === "rest" || s.kind === "param" && s.optional) {
      throw new Error(`Route path "${path}": "${s.kind === "rest" ? "$" : "$" + s.name + "?"}" must be the last segment`);
    }
  }
  return out;
}
function createRootRoute(options = {}) {
  let root = {
    path: "/",
    parent: null,
    children: [],
    component: options.component ?? null,
    parse: null,
    segments: [],
    tabs: false,
    _params: () => {}
  };
  place(root, options.children ?? []);
  return root;
}
function createRoute(options) {
  let route = {
    path: options.path,
    parent: null,
    children: [],
    component: options.component ?? null,
    parse: options.params?.parse ?? null,
    segments: parseSegments(options.path),
    tabs: options.tabs ?? false,
    _params: () => {}
  };
  place(route, options.children ?? []);
  return route;
}
function place(parent, children2) {
  for (let child of children2) {
    if (child.parent) {
      throw new Error(`Route "${formatPattern(child)}" is already placed; a route has one place in the tree`);
    }
    child.parent = parent;
    parent.children.push(child);
    checkTabs(child);
  }
}
function checkTabs(route) {
  if (route.parent?.tabs) {
    if (route.segments.some((s) => s.kind !== "literal")) {
      throw new Error(`Tab "${formatPattern(route)}": a tab's path is literal`);
    }
    let own = formatPattern(route);
    for (let sibling of route.parent.children) {
      if (sibling !== route && formatPattern(sibling) === own) {
        throw new Error(`Tab "${own}" is declared twice`);
      }
    }
  }
  if (route.tabs) {
    for (let anc = route.parent;anc; anc = anc.parent) {
      if (anc.tabs)
        throw new Error(`Route "${formatPattern(route)}": tabs inside a tab are not supported`);
      if (anc.segments.some((s) => s.kind !== "literal")) {
        throw new Error(`Route "${formatPattern(route)}": a tabs route takes no params above it`);
      }
    }
  }
  for (let child of route.children)
    checkTabs(child);
}
function tabsRoutes(root) {
  let out = [];
  let walk = (route) => {
    if (route.tabs)
      out.push(route);
    for (let child of route.children)
      walk(child);
  };
  walk(root);
  return out;
}
function splitPath(path) {
  return path.split("/").filter((s) => s !== "").map((s) => {
    try {
      return decodeURIComponent(s);
    } catch {
      return s;
    }
  });
}
function consume(route, segments) {
  let raw = {};
  let i = 0;
  for (let s of route.segments) {
    if (s.kind === "rest") {
      raw[REST_PARAM] = segments.slice(i).join("/");
      return {
        raw,
        rest: []
      };
    }
    let piece = segments[i];
    if (piece === undefined) {
      if (s.kind === "param" && s.optional)
        return {
          raw,
          rest: []
        };
      return null;
    }
    if (s.kind === "literal") {
      if (piece !== s.value)
        return null;
    } else {
      raw[s.name] = piece;
    }
    i++;
  }
  return {
    raw,
    rest: segments.slice(i)
  };
}
function matchFrom(route, segments, inherited) {
  let own = consume(route, segments);
  if (!own)
    return null;
  let params;
  try {
    params = {
      ...inherited,
      ...route.parse ? route.parse(own.raw) : own.raw
    };
  } catch {
    return null;
  }
  let here = {
    route,
    params
  };
  if (own.rest.length === 0) {
    for (let child of route.children) {
      if (child.segments.length === 0) {
        let below = matchFrom(child, [], params);
        if (below)
          return [here, ...below];
      }
    }
    return [here];
  }
  for (let child of route.children) {
    let below = matchFrom(child, own.rest, params);
    if (below)
      return [here, ...below];
  }
  return null;
}
function matchPath(root, path) {
  return matchFrom(root, splitPath(path), {});
}
function formatPath(route, params = {}) {
  let chain = [];
  for (let r = route;r; r = r.parent)
    chain.unshift(r);
  let out = [];
  for (let r of chain) {
    for (let s of r.segments) {
      if (s.kind === "literal") {
        out.push(s.value);
      } else if (s.kind === "rest") {
        let rest = params[REST_PARAM];
        if (rest !== undefined && rest !== null && rest !== "")
          out.push(String(rest));
      } else {
        let value = params[s.name];
        if (value === undefined || value === null) {
          if (s.optional)
            continue;
          throw new Error(`Route "${formatPattern(route)}": missing param "${s.name}"`);
        }
        out.push(encodeURIComponent(String(value)));
      }
    }
  }
  return "/" + out.join("/");
}
function formatPattern(route) {
  let parts = [];
  for (let r = route;r; r = r.parent) {
    let own = r.path.replace(/^\/+|\/+$/g, "");
    if (own)
      parts.unshift(own);
  }
  return "/" + parts.join("/");
}
function linkToPath(link2) {
  let m = /^([A-Za-z][A-Za-z0-9+.-]+):(.*)$/s.exec(link2);
  let rest = link2;
  if (m) {
    let scheme = m[1].toLowerCase();
    rest = m[2];
    if (scheme === "http" || scheme === "https") {
      rest = rest.replace(/^\/\/[^/]*/, "");
    } else {
      rest = rest.replace(/^\/\//, "");
    }
  }
  rest = rest.replace(/[?#].*$/s, "");
  return rest.startsWith("/") ? rest : "/" + rest;
}
// ../../packages/router/src/router.tsx
import { reportLocation } from "sol:dev";

// ../../packages/router/src/stack.ts
function findTabs(root) {
  let found = tabsRoutes(root);
  if (found.length === 0)
    return null;
  if (found.length > 1) {
    throw new Error(`Router: one tabs route per tree ("${formatPattern(found[0])}" and "${formatPattern(found[1])}")`);
  }
  let route = found[0];
  return {
    route,
    path: formatPath(route),
    roots: route.children.map((tab) => formatPath(tab))
  };
}
function resolve3(tree4, tabs, path) {
  let matches2 = matchPath(tree4, path);
  if (!matches2)
    return null;
  if (!tabs)
    return {
      path,
      matches: matches2,
      tab: null
    };
  let at = matches2.findIndex((m) => m.route === tabs.route);
  if (at < 0)
    return {
      path,
      matches: matches2,
      tab: null
    };
  let tab = matches2[at + 1];
  if (!tab)
    return resolve3(tree4, tabs, tabs.roots[0]);
  return {
    path,
    matches: matches2,
    tab: formatPath(tab.route)
  };
}
function allRoots(tabs, active) {
  let stacks = {};
  for (let root of tabs.roots)
    stacks[root] = [root];
  return {
    active,
    stacks
  };
}
function showTabs(stack, marker) {
  let at = stack.lastIndexOf(marker);
  if (at === stack.length - 1)
    return stack;
  return at >= 0 ? stack.slice(0, at + 1) : [...stack, marker];
}
function withTab(state, tabs, tab, stack) {
  let current = state.tabs ?? allRoots(tabs, tab);
  let same = current.stacks[tab] === stack;
  return {
    stack: showTabs(state.stack, tabs.path),
    tabs: {
      active: tab,
      stacks: same ? current.stacks : {
        ...current.stacks,
        [tab]: stack
      }
    }
  };
}
function toRoot(state, tabs, target) {
  if (target.path !== target.tab)
    return null;
  let current = state.tabs?.stacks[target.tab] ?? [target.tab];
  let showing = state.tabs?.active === target.tab && state.stack[state.stack.length - 1] === tabs.path;
  return showing && current.length > 1 ? [target.tab] : current;
}
function push(state, tabs, target) {
  if (!tabs || target.tab === null)
    return {
      ...state,
      stack: [...state.stack, target.path]
    };
  let current = state.tabs?.stacks[target.tab] ?? [target.tab];
  return withTab(state, tabs, target.tab, toRoot(state, tabs, target) ?? [...current, target.path]);
}
function replace(state, tabs, target) {
  if (!tabs || target.tab === null)
    return {
      ...state,
      stack: [...state.stack.slice(0, -1), target.path]
    };
  let current = state.tabs?.stacks[target.tab] ?? [target.tab];
  let next = toRoot(state, tabs, target) ?? (current.length > 1 ? [...current.slice(0, -1), target.path] : [target.tab, target.path]);
  return withTab(state, tabs, target.tab, next);
}
function reset(tabs, target) {
  if (!tabs)
    return {
      stack: [target.path],
      tabs: null
    };
  if (target.tab === null)
    return {
      stack: [target.path],
      tabs: allRoots(tabs, tabs.roots[0])
    };
  let all = allRoots(tabs, target.tab);
  all.stacks[target.tab] = target.path === target.tab ? [target.tab] : [target.tab, target.path];
  return {
    stack: [tabs.path],
    tabs: all
  };
}
function back(state, tabs) {
  if (tabs && state.tabs && state.stack[state.stack.length - 1] === tabs.path) {
    let active = state.tabs.active;
    let current = state.tabs.stacks[active] ?? [active];
    if (current.length > 1)
      return withTab(state, tabs, active, current.slice(0, -1));
    if (active !== tabs.roots[0])
      return {
        ...state,
        tabs: {
          ...state.tabs,
          active: tabs.roots[0]
        }
      };
  }
  if (state.stack.length > 1)
    return {
      ...state,
      stack: state.stack.slice(0, -1)
    };
  return null;
}
function currentPath(state, tabs) {
  let top = state.stack[state.stack.length - 1];
  if (tabs && state.tabs && top === tabs.path) {
    let stack = state.tabs.stacks[state.tabs.active];
    return stack?.[stack.length - 1];
  }
  return top;
}
function changed(prev, next) {
  let out = [];
  if (prev.stack !== next.stack)
    out.push(null);
  if (prev.tabs && next.tabs && prev.tabs.stacks !== next.tabs.stacks) {
    for (let tab of Object.keys(next.tabs.stacks)) {
      if (prev.tabs.stacks[tab] !== next.tabs.stacks[tab])
        out.push(tab);
    }
  }
  return out;
}
function initialState(tree4, tabs, initial, normalize) {
  let target = (path) => {
    let t = resolve3(tree4, tabs, normalize(path));
    if (!t)
      console.warn(`Router: no route matches "${path}"; dropped from the initial stack`);
    return t;
  };
  if (initial !== undefined && !Array.isArray(initial) && typeof initial !== "string") {
    if (fits(tree4, tabs, initial, normalize))
      return {
        stack: initial.stack,
        tabs: tabs ? initial.tabs : null
      };
    initial = undefined;
  }
  let paths = initial === undefined ? ["/"] : typeof initial === "string" ? [initial] : initial;
  let state = null;
  for (let path of paths) {
    let t = target(path);
    if (!t)
      continue;
    state = state ? push(state, tabs, t) : reset(tabs, t);
  }
  return state ?? {
    stack: ["/"],
    tabs: tabs ? allRoots(tabs, tabs.roots[0]) : null
  };
}
function fits(tree4, tabs, saved, normalize) {
  let misfit = (what) => {
    console.warn(`Router: the saved stack does not fit the route tree (${what}); not resumed`);
    return false;
  };
  let lands = (path, tab) => resolve3(tree4, tabs, normalize(path))?.tab === tab;
  for (let path of saved.stack) {
    if (!(tabs && path === tabs.path) && !lands(path, null))
      return misfit(`"${path}"`);
  }
  if (!tabs)
    return saved.stack.length > 0 || misfit("empty");
  if (!tabs.roots.includes(saved.tabs.active))
    return misfit(`active tab "${saved.tabs.active}"`);
  for (let root of tabs.roots) {
    let entries = saved.tabs.stacks[root];
    if (!entries || entries[0] !== root)
      return misfit(`tab "${root}"`);
    for (let path of entries)
      if (!lands(path, root))
        return misfit(`"${path}" in tab "${root}"`);
  }
  return saved.stack.length > 0 || misfit("empty");
}

// ../../packages/router/src/router.tsx
function createRouter(options) {
  let tree4 = options.tree;
  let tabs = findTabs(tree4);
  let launch = env.launchLink;
  let [state, setState] = createSignal(initialState(tree4, tabs, launch != null ? launch : options.initial, linkToPath));
  let located = (path) => {
    if (path === undefined)
      return null;
    let matches2 = matchPath(tree4, path);
    return matches2 ? {
      path,
      matches: matches2
    } : null;
  };
  let location = createMemo(() => located(currentPath(state(), tabs)));
  let tabLocations = new Map;
  for (let root of tabs?.roots ?? []) {
    tabLocations.set(root, createMemo(() => {
      let stack = state().tabs?.stacks[root];
      return located(stack?.[stack.length - 1]);
    }));
  }
  let blockers = new Map;
  let href = (target) => {
    if (typeof target === "string")
      return linkToPath(target);
    if ("route" in target)
      return formatPath(target.route, target.params);
    return formatPath(target);
  };
  let allowed = async (scopes) => {
    let pending2 = [];
    for (let scope of scopes) {
      for (let block of blockers.get(scope) ?? []) {
        let verdict = block();
        if (verdict === false)
          return false;
        if (verdict !== true)
          pending2.push(verdict);
      }
    }
    if (pending2.length === 0)
      return true;
    return (await Promise.all(pending2)).every(Boolean);
  };
  let router = {
    entries: () => {
      let s = state();
      return s.tabs ? {
        stack: s.stack,
        tabs: s.tabs
      } : s.stack;
    },
    location,
    href,
    state,
    tabs,
    activeTab: () => state().tabs?.active ?? null,
    tabLocation: (tab) => tabLocations.get(tab) ?? (() => null),
    blockers,
    tree: tree4,
    async navigate(target, options2 = {}) {
      let path = href(target);
      let to = resolve3(tree4, tabs, path);
      if (!to) {
        console.warn(`Router: no route matches "${path}"`);
        return false;
      }
      let apply = (s) => options2.reset ? reset(tabs, to) : options2.replace ? replace(s, tabs, to) : push(s, tabs, to);
      let before = untrack(state);
      if (!await allowed(changed(before, apply(before))))
        return false;
      setState((s) => apply(s));
      return true;
    },
    async back() {
      let before = untrack(state);
      let next = back(before, tabs);
      if (!next)
        return false;
      if (!await allowed(changed(before, next)))
        return false;
      setState((s) => back(s, tabs) ?? s);
      return true;
    }
  };
  return router;
}
var RouterContext = createContext2();
var DepthContext = createContext2(0);
var ScopeContext = createContext2();
function Router(props) {
  let router = untrack(() => props.router ?? createRouter({
    tree: createRootRoute({
      children: routesOf(props.children)
    }),
    initial: props.initial
  }));
  onBack((e) => {
    if (back(untrack(router.state), router.tabs) === null)
      return;
    e.preventDefault();
    router.back();
  });
  onLink((link2) => {
    router.navigate(linkToPath(link2));
  });
  createEffect(() => router.location()?.path ?? null, (path) => reportLocation(path));
  onCleanup(() => reportLocation(null));
  return createComponent2(RouterContext, {
    value: router,
    get children() {
      return createComponent2(ScopeContext, {
        get value() {
          return {
            location: router.location,
            tab: null
          };
        },
        get children() {
          return createComponent2(RouteView, {
            depth: 0
          });
        }
      });
    }
  });
}
function routesOf(slot) {
  let list = children(() => slot).toArray();
  for (let item of list) {
    if (typeof item !== "object" || item === null || !("segments" in item)) {
      throw new Error("<Router> and <Route> take only <Route> elements as children");
    }
  }
  return list;
}
function RouteView(props) {
  let router = useContext(RouterContext);
  let scope = useContext(ScopeContext);
  let route = createMemo(() => scope.location()?.matches[props.depth]?.route ?? null);
  let tabsRoute = router.tabs?.route ?? null;
  let tabsSeen = createMemo(() => tabsRoute !== null && route() === tabsRoute);
  let seen = false;
  let tabsMounted = createMemo(() => seen ||= tabsSeen());
  let render2 = (r) => r.component ? createComponent2(r.component, {}) : createComponent2(Outlet, {});
  return createComponent2(DepthContext, {
    get value() {
      return props.depth;
    },
    get children() {
      return [createComponent2(Show, {
        get when() {
          return tabsMounted();
        },
        get children() {
          var _el$ = createElement("view", {
            flex: 1,
            flexDirection: "column"
          });
          insert(_el$, () => render2(tabsRoute));
          effect3(() => tabsSeen() ? "flex" : "none", (_v$, _$p) => {
            setProp(_el$, "display", _v$, _$p);
          });
          return _el$;
        }
      }), createComponent2(Show, {
        get when() {
          return memo2(() => !!tabsSeen())() ? null : route();
        },
        keyed: true,
        children: render2
      })];
    }
  });
}
function Outlet() {
  let router = useContext(RouterContext);
  let depth = useContext(DepthContext);
  let scope = useContext(ScopeContext);
  let here = untrack(() => scope.location()?.matches[depth]?.route ?? null);
  if (here !== null && here === router.tabs?.route)
    return createComponent2(TabsOutlet, {
      depth
    });
  return createComponent2(RouteView, {
    depth: depth + 1
  });
}
function TabsOutlet(props) {
  let router = useContext(RouterContext);
  let seen = [];
  let visited = createMemo(() => {
    let active = router.activeTab();
    if (active !== null && !seen.includes(active))
      seen = [...seen, active];
    return seen;
  });
  return createComponent2(For, {
    get each() {
      return visited();
    },
    children: (tab) => (() => {
      var _el$2 = createElement("view", {
        flex: 1,
        flexDirection: "column"
      });
      insert(_el$2, createComponent2(ScopeContext, {
        get value() {
          return {
            location: router.tabLocation(tab),
            tab
          };
        },
        get children() {
          return createComponent2(RouteView, {
            get depth() {
              return props.depth + 1;
            }
          });
        }
      }));
      effect3(() => router.activeTab() === tab ? "flex" : "none", (_v$, _$p) => {
        setProp(_el$2, "display", _v$, _$p);
      });
      return _el$2;
    })()
  });
}
function useRouter() {
  return useContext(RouterContext);
}
function useLocation() {
  return useContext(ScopeContext).location;
}
function useParams(route) {
  let scope = useContext(ScopeContext);
  return createMemo(() => {
    let matches2 = scope.location()?.matches ?? [];
    let match = (route && matches2.find((m) => m.route === route)) ?? matches2[matches2.length - 1];
    return match?.params ?? {};
  });
}
// ../../packages/components/src/theme.ts
var THEMED_COMPONENTS = ["button", "card", "badge", "switch", "checkbox", "radio", "slider", "item", "select", "segmentedControl", "textInput", "richTextEditor", "tooltip", "divider", "progressBar", "spinner", "contextMenu", "navShell"];
var MATERIAL_ROLES = ["surface", "control", "accent", "overlay"];
var ELEVATION_LEVELS = ["flat", "raised", "floating", "overlay"];
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
var MOTION = {
  fast: 100,
  base: 150,
  slow: 250
};
var SIZE = {
  navRail: 72,
  navSidebar: 220,
  splitViewList: 320,
  menuMinWidth: 120,
  slider: 200
};
var LIGHT = {
  direction: [0.3, 1, 1],
  color: "#ffffff",
  ambient: 0.5
};
var SHADOW = {
  color: "#000000",
  strength: 1
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
function pick(name, value, scheme) {
  if (!Array.isArray(value))
    return value;
  if (!scheme)
    throw new Error(`Theme value "${name}" is a [light, dark] pair; pass a scheme to defineTheme`);
  return value[scheme === "light" ? 0 : 1];
}
function defineTheme(def, scheme) {
  let color = {};
  for (let key2 in def.color) {
    let k = key2;
    let value = def.color[k];
    if (value == null)
      continue;
    color[k] = pick(key2, value, scheme);
  }
  if (def.color.ring == null)
    color.ring = color.text;
  if (def.color.selection == null)
    color.selection = color.overlayPressed;
  if (!("thumb" in color))
    color.thumb = undefined;
  let components = {};
  for (let name of THEMED_COMPONENTS)
    components[name] = def.components?.[name];
  let material = {};
  for (let role2 of MATERIAL_ROLES)
    material[role2] = {
      ...def.material?.[role2]
    };
  let elevation = {};
  for (let level of ELEVATION_LEVELS)
    elevation[level] = def.elevation?.[level] ?? 0;
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
    motion: {
      ...MOTION,
      ...def.motion
    },
    size: {
      ...SIZE,
      ...def.size
    },
    icons: {
      chevronDown: def.icons?.chevronDown,
      check: def.icons?.check
    },
    components,
    light: {
      direction: def.light?.direction ?? LIGHT.direction,
      color: def.light?.color != null ? pick("light.color", def.light.color, scheme) : LIGHT.color,
      ambient: def.light?.ambient != null ? pick("light.ambient", def.light.ambient, scheme) : LIGHT.ambient
    },
    shadow: {
      color: def.shadow?.color != null ? pick("shadow.color", def.shadow.color, scheme) : SHADOW.color,
      strength: def.shadow?.strength != null ? pick("shadow.strength", def.shadow.strength, scheme) : SHADOW.strength
    },
    material,
    elevation,
    finish: {
      program: def.finish?.program,
      params: def.finish?.params,
      textures: def.finish?.textures,
      previous: def.finish?.previous,
      vertexCount: def.finish?.vertexCount
    }
  };
}
var DEFAULT = {
  color: {
    background: ["#ffffff", "#0b0f17"],
    surface: ["#f6f8fa", "#161b22"],
    surfaceAlt: ["#eaeef2", "#21262d"],
    text: ["#1f2328", "#b1bac4"],
    textMuted: ["#606366", "#939aa4"],
    border: ["rgba(0,0,0,0.15)", "rgba(255,255,255,0.14)"],
    primary: "#547ebf",
    onPrimary: "#ffffff",
    secondary: "#2b5696",
    onSecondary: "#ffffff",
    danger: ["#cf222e", "#f85149"],
    scrim: ["rgba(0,0,0,0.4)", "rgba(0,0,0,0.6)"],
    overlayHover: ["rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)"],
    overlayPressed: ["rgba(0,0,0,0.14)", "rgba(255,255,255,0.14)"],
    selection: ["rgba(84,126,191,0.30)", "rgba(84,126,191,0.40)"]
  },
  text: {
    roles: {
      caption: {
        size: 12
      }
    }
  }
};
var LIT = {
  ...DEFAULT,
  color: {
    ...DEFAULT.color,
    thumb: "#f4f6fa"
  },
  light: {
    ambient: 0.35
  },
  shadow: {
    strength: [1, 1.7]
  },
  material: {
    surface: {
      sheen: 0.5,
      bevel: 0.6
    },
    control: {
      sheen: 0.6,
      bevel: 0.7
    },
    accent: {
      sheen: 1,
      bevel: 0.9
    },
    overlay: {
      sheen: 0.4,
      bevel: 0.6
    }
  },
  elevation: {
    flat: 0,
    raised: 2,
    floating: 6,
    overlay: 14
  }
};
var darkTheme = defineTheme(DEFAULT, "dark");
var lightTheme = defineTheme(DEFAULT, "light");
var litDarkTheme = defineTheme(LIT, "dark");
var litLightTheme = defineTheme(LIT, "light");
var [themeStore, setThemeStore] = createStore({
  ...darkTheme
});
var theme = themeStore;
function setTheme(partial) {
  setThemeStore((s) => {
    for (let key2 in partial) {
      let k = key2;
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

// ../../packages/components/src/motion.tsx
var PRESS_SCALE = 0.97;
var TRAVEL_BOUNCE = 0.15;
function fadeMotion() {
  if (policy.motion === "none")
    return;
  return {
    duration: theme.motion.base,
    curve: "ease-out"
  };
}
function feedbackFade() {
  if (policy.motion === "none")
    return;
  return {
    duration: theme.motion.fast,
    curve: "ease-out"
  };
}
function travelMotion(bounce = TRAVEL_BOUNCE) {
  if (policy.motion !== "normal")
    return;
  return {
    duration: theme.motion.slow,
    bounce
  };
}
function colorFade() {
  let fade = fadeMotion();
  return fade === undefined ? undefined : {
    color: fade
  };
}
function scaleFeedback() {
  if (policy.motion !== "normal")
    return;
  return {
    scale: {
      duration: theme.motion.fast
    }
  };
}
function pressScale(pressed) {
  return pressed && policy.motion === "normal" ? PRESS_SCALE : 1;
}
function elevationMotion() {
  if (policy.motion !== "normal")
    return;
  return {
    shadow: {
      duration: theme.motion.slow,
      bounce: TRAVEL_BOUNCE
    }
  };
}
function popupFade() {
  if (policy.motion === "none")
    return;
  return {
    opacity: {
      duration: theme.motion.base,
      curve: "ease-out",
      from: 0,
      exit: 0
    }
  };
}
function PressFeedback(props) {
  let tint = () => props.pressed ? theme.color.overlayPressed : theme.color.overlayHover;
  let fade = () => {
    let f = feedbackFade();
    return f === undefined ? undefined : {
      color: f
    };
  };
  var _el$ = createElement("d-rect");
  effect3(() => ({
    e: fade(),
    t: props.pressed || props.hovered ? tint() : withAlpha(tint(), 0),
    a: props.radius
  }), ({
    e,
    t,
    a
  }, _p$) => {
    e !== _p$?.e && setProp(_el$, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$, "color", t, _p$?.t);
    a !== _p$?.a && setProp(_el$, "radius", a, _p$?.a);
  });
  return _el$;
}

// ../../packages/components/src/window.tsx
function themeFinish() {
  let f = theme.finish;
  if (f.program == null)
    return null;
  return {
    program: f.program,
    params: {
      ...f.params,
      uScale: displayScale()
    },
    textures: f.textures,
    previous: f.previous,
    vertexCount: f.vertexCount
  };
}
function Window(props) {
  var _el$ = createElement("window");
  spread(_el$, [() => props.layout, {
    get title() {
      return props.title;
    },
    get fullscreen() {
      return props.fullscreen;
    },
    get shader() {
      return memo2(() => props.shader === undefined)() ? themeFinish() : props.shader;
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
    get onPointerCancel() {
      return props.onPointerCancel;
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
  }], true);
  insert(_el$, (() => {
    var _c$ = memo2(() => props.style?.backgroundColor != null);
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: colorFade(),
        t: props.style.backgroundColor
      }), ({
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "color", t, _p$?.t);
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
function splitTransition(t, parts) {
  if (t == null || typeof t === "string")
    return {
      root: t,
      background: t,
      border: t
    };
  let root = {};
  let background2 = {};
  let border = {};
  for (let [key2, value] of Object.entries(t)) {
    if (key2 === "all" || key2 === "stagger") {
      root[key2] = value;
      background2[key2] = value;
      border[key2] = value;
    } else if (key2 in STYLE_TO_BACKGROUND || key2 in STYLE_TO_BORDER) {
      if (key2 in STYLE_TO_BACKGROUND)
        background2[STYLE_TO_BACKGROUND[key2]] = value;
      if (key2 in STYLE_TO_BORDER)
        border[STYLE_TO_BORDER[key2]] = value;
    } else if (!parts?.includes(key2)) {
      root[key2] = value;
    }
  }
  let pick2 = (o) => Object.keys(o).length ? o : undefined;
  return {
    root: pick2(root),
    background: pick2(background2),
    border: pick2(border)
  };
}
function withTransitionDefaults(t, defaults) {
  if (t === null || typeof t === "string")
    return t;
  if (t?.all !== undefined)
    return t;
  let filled;
  for (let key2 in defaults) {
    let spec = defaults[key2];
    if (spec === undefined || t && key2 in t)
      continue;
    (filled ??= {})[key2] = spec;
  }
  if (!filled)
    return t;
  return {
    ...filled,
    ...t
  };
}
function partTransition(t, part, coreProp, fallback) {
  let spec;
  if (t === null)
    spec = undefined;
  else if (typeof t === "string")
    spec = t;
  else if (t)
    spec = t[part] ?? t.all ?? fallback;
  else
    spec = fallback;
  return spec == null ? undefined : {
    [coreProp]: spec
  };
}
function partTransitionEnd(part, coreProp, handler) {
  if (!handler)
    return;
  return (e) => {
    if (e.property === coreProp)
      handler({
        property: part
      });
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
var FONT_KEYS = ["fontFamily", "fontSize", "lineHeight", "fontStyle", "fontWeight"];
var TEXT_KEYS = [...FONT_KEYS, "textAlign", "maxLines"];
function splitTextLayout(layout, paragraph = false) {
  let text = {};
  let box = {};
  if (!layout)
    return {
      text,
      box
    };
  let keys = paragraph ? TEXT_KEYS : FONT_KEYS;
  for (let key2 in layout) {
    let value = layout[key2];
    if (keys.includes(key2))
      text[key2] = value;
    else
      box[key2] = value;
  }
  return {
    text,
    box
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
  spread(_el$, [{
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
    get onPointerCancel() {
      return props.onPointerCancel;
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
  }], true);
  insert(_el$, (() => {
    var _c$ = memo2(() => !!hasBackground());
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: withTransitionDefaults(split().background, colorFade()),
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
        e: withTransitionDefaults(split().border, colorFade()),
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
function Text(props) {
  let role = () => theme.text[props.variant ?? "body"];
  let size = () => (props.layout?.fontSize ?? role().size) * policy.textScale;
  let color = () => props.style?.color ?? theme.color[props.color ?? (props.muted ? "textMuted" : "text")];
  let box = createMemo(() => splitTextLayout(props.layout, true).box);
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
  spread(_el$, [{
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
    get onPointerCancel() {
      return props.onPointerCancel;
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
  }], true);
  insert(_el$2, () => props.children);
  effect3(() => ({
    e: withTransitionDefaults(split().text, colorFade()),
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
  let replace2 = (start, end, text) => {
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
      replace2(start, end, text);
    },
    deleteBackward: () => {
      let [start, end] = range();
      if (start !== end)
        replace2(start, end, "");
      else if (start > 0)
        replace2(step(value(), start, "left"), start, "");
    },
    deleteForward: () => {
      let v = value();
      let [start, end] = range();
      if (start !== end)
        replace2(start, end, "");
      else if (end < v.length)
        replace2(end, step(v, end, "right"), "");
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
    setValue: (next) => replace2(0, value().length, next),
    clear: () => replace2(0, value().length, "")
  };
}
function createTextEditorLayout(viewport, input) {
  let [viewportWidth, setViewportWidth] = createSignal(0);
  let [viewportHeight, setViewportHeight] = createSignal(0);
  let font = createMemo(() => input().font, {
    equals: sameOptions
  });
  let space = createMemo(() => measureText2(" ", font()));
  let prepared = createMemo(() => {
    let {
      text,
      runs
    } = input();
    return prepareText2(text, {
      ...font(),
      runs,
      carets: true
    });
  });
  let breaking = createMemo(() => {
    let {
      text,
      wrap,
      caretWidth = 0
    } = input();
    return {
      text,
      wrap,
      caretWidth
    };
  }, {
    equals: sameBreakInput
  });
  let placed = createMemo((prev) => {
    let {
      text,
      wrap,
      caretWidth
    } = breaking();
    let width = wrap ? Math.max(0, viewportWidth() - caretWidth) : Infinity;
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
        to: line.to,
        hardBreak: line.hardBreak
      });
      y += line.height;
      hardBreak = line.hardBreak;
      line = layoutNextLine(units, line.cursor, width);
    }
    if (out.length === 0 || hardBreak) {
      let n = units.units.length;
      out.push({
        start: text.length,
        end: text.length,
        y,
        height: space().height,
        width: 0,
        from: n,
        to: n,
        hardBreak: false
      });
    }
    if (prev && prev.units === units.units) {
      for (let i = 0;i < out.length && i < prev.lines.length; i++) {
        if (sameLine(prev.lines[i], out[i]))
          out[i] = prev.lines[i];
      }
    }
    let holds = units.units === prepared().units ? holdRange(units.units, out) : [Infinity, Infinity];
    return {
      units: units.units,
      lines: out,
      holds
    };
  }, {
    equals: samePlacement
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
  let xAt = (index, offset) => {
    let x = 0;
    for (let stop of lineStops(index)) {
      if (stop.offset > offset)
        break;
      x = stop.x;
    }
    return x;
  };
  let caret = createMemo(() => {
    let offset = input().caret;
    let index = caretLine();
    let line = lines()[index];
    return {
      x: xAt(index, offset),
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
  let selectionRects = (anchor, focus) => {
    let start = Math.min(anchor, focus);
    let end = Math.max(anchor, focus);
    if (start >= end)
      return [];
    let ls = lines();
    let first = lineOf(start);
    let last = lineOf(end);
    let breakWidth = space().width;
    let out = [];
    for (let i = first;i <= last; i++) {
      let line = ls[i];
      let x = i === first ? xAt(i, start) : 0;
      let width = i === last ? xAt(i, end) - x : Math.max(line.width - x, 0) + breakWidth;
      if (width <= 0)
        continue;
      out.push({
        x,
        y: line.y,
        width,
        height: line.height
      });
    }
    return out;
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
  let scrollX = createMemo((prev) => {
    let {
      caretWidth,
      wrap
    } = breaking();
    if (wrap)
      return 0;
    let contentWidth = lines().reduce((w, l) => Math.max(w, l.width), 0);
    let c = caret();
    return follow(prev ?? 0, c.x, caretWidth, viewportWidth(), contentWidth + caretWidth);
  });
  let scrollY = createMemo((prev) => {
    let ls = lines();
    let last = ls[ls.length - 1];
    let c = caret();
    return follow(prev ?? 0, c.y, c.height, viewportHeight(), last.y + last.height);
  });
  let keepsBreaks = (width) => {
    let {
      wrap,
      caretWidth
    } = breaking();
    if (!wrap)
      return false;
    let [min, max] = placed().holds;
    let w = Math.max(0, width - caretWidth);
    return w >= min && w < max;
  };
  let keepsScroll = (height) => {
    let ls = lines();
    let last = ls[ls.length - 1];
    let c = caret();
    let current = scrollY();
    return follow(current, c.y, c.height, height, last.y + last.height) === current;
  };
  onLayout(() => {
    let node = viewport();
    if (!node)
      return;
    let box = getLayoutBox2(node);
    let width = box?.width ?? 0;
    let height = box?.height ?? 0;
    if (!keepsBreaks(width))
      setViewportWidth(width);
    if (!keepsScroll(height))
      setViewportHeight(height);
  });
  return {
    lines,
    caret,
    caretLine,
    offsetAtX,
    selectionRects,
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
function holdRange(units, lines) {
  let min = 0;
  let max = Infinity;
  for (let i = 0;i < lines.length; i++) {
    let line = lines[i];
    if (line.width > min)
      min = line.width;
    let next = lines[i + 1];
    if (!next || line.hardBreak || next.from >= units.length)
      continue;
    let pen = 0;
    for (let u = line.from;u < line.to; u++)
      pen += units[u].advance;
    let join = pen + unitInk(units, next.from);
    if (join < max)
      max = join;
  }
  return [min, max];
}
function sameBreakInput(a, b) {
  return a.text === b.text && a.wrap === b.wrap && a.caretWidth === b.caretWidth;
}
function sameOptions(a, b) {
  let ka = Object.keys(a);
  let kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => a[k] === b[k]);
}
function sameLine(a, b) {
  return a.start === b.start && a.end === b.end && a.y === b.y && a.height === b.height && a.width === b.width;
}
function samePlacement(a, b) {
  if (a.units !== b.units || a.lines.length !== b.lines.length)
    return false;
  return a.lines.every((l, i) => sameLine(l, b.lines[i]));
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
var NAV_REPEAT_DELAY = 400;
var NAV_REPEAT_INTERVAL = 100;
var NAV_THRESHOLD = 0.5;
var uiActions = {
  navigate: "vec2",
  cycle: "axis",
  select: "button"
};
function uiBindings(devices) {
  let out = [];
  let kb = devices.keyboard;
  if (kb) {
    out.push({
      action: "navigate",
      source: kb.arrows
    }, {
      action: "cycle",
      source: kb.axis("Shift+Tab", "Tab")
    }, {
      action: "select",
      source: kb.key("Enter")
    }, {
      action: "select",
      source: kb.key("Space")
    }, {
      action: "select",
      source: kb.key("Select")
    });
  }
  let pad = devices.gamepad;
  if (pad) {
    out.push({
      action: "navigate",
      source: pad.dpad
    }, {
      action: "navigate",
      source: pad.leftStick
    }, {
      action: "select",
      source: pad.button("south")
    });
  }
  return out;
}
function repeating(read2, step) {
  let current = null;
  let timer = null;
  let stop = () => {
    if (timer == null)
      return;
    clearTimeout(timer);
    timer = null;
  };
  let later = (value, delay) => {
    timer = setTimeout(() => {
      step(value);
      later(value, NAV_REPEAT_INTERVAL);
    }, delay);
  };
  createEffect(read2, (value) => {
    if (value === current)
      return;
    current = value;
    stop();
    if (value === null)
      return;
    untrack(() => step(value));
    later(value, NAV_REPEAT_DELAY);
  });
  onCleanup(stop);
}
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
var defaultMap = () => {
  let map = createInputMap(uiActions);
  map.bind(uiBindings({
    keyboard,
    gamepad: gamepad()
  }));
  return map;
};
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
    untrack(() => {
      let focused = focusedNode();
      if (focused != null && getNodePath(focused).includes(scopeNode.id))
        return;
      let placed = reachable();
      if (placed.length > 0)
        focusFirst(placed);
      else if (focused != null)
        setFocus(null);
    });
  });
  let input = options?.input ?? defaultMap();
  for (let [name, kind] of Object.entries(uiActions)) {
    if (input.actions[name] !== kind)
      throw new Error(`createFocusNav: the input map needs a ${kind} action "${name}" (declare it with uiActions)`);
  }
  let direction = () => {
    let [x, y] = input.value("navigate");
    if (Math.max(Math.abs(x), Math.abs(y)) < NAV_THRESHOLD)
      return null;
    if (Math.abs(x) > Math.abs(y))
      return x > 0 ? "right" : "left";
    return y > 0 ? "down" : "up";
  };
  let cycleStep = () => {
    let v = input.value("cycle");
    return v >= NAV_THRESHOLD ? 1 : v <= -NAV_THRESHOLD ? -1 : null;
  };
  repeating(direction, move);
  repeating(cycleStep, tab);
  onCleanup(input.onPress("select", activate));
  return {
    input,
    handlers: input.handlers,
    move,
    tab,
    activate
  };
}

// ../../packages/components/src/light.ts
var SHADOW_OFFSET = 1;
var SHADOW_BLUR = 2.5;
var SHADOW_ALPHA = 0.42;
var CONTACT_OFFSET = 0.5;
var CONTACT_BLUR = 1.5;
var CONTACT_BLUR_PER_UNIT = 0.35;
var CONTACT_ALPHA = 0.3;
var AMBIENT_FILL = 0.6;
var MAX_SLANT = 2.5;
var SHEEN_LIGHT = 0.2;
var SHEEN_SHADE = 0.24;
var LIGHT_TINT = 0.3;
var BEVEL_LIGHT = 0.55;
var BEVEL_SHADE = 0.45;
var MIN_Z = 0.05;
var ANGLE_STEPS = 128;
var SLANT_STEPS = 64;
var OBLIQUITY_STEPS = 64;
var SHADOW_STEP = 0.25;
var clamp2 = (v, lo, hi) => v < lo ? lo : v > hi ? hi : v;
var CACHE_LIMIT = 4096;
var QUANTUM = 512;
var cache = new Map;
function cached(key2, compute) {
  let hit = cache.get(key2);
  if (hit !== undefined)
    return hit;
  if (cache.size >= CACHE_LIMIT)
    cache.clear();
  let value = compute();
  cache.set(key2, value);
  return value;
}
var quantize = (t) => Math.round(t * QUANTUM) / QUANTUM;
function alpha(color, a) {
  let q = quantize(a);
  return cached(`a|${color}|${q}`, () => withAlpha(color, q));
}
function mix(base, other, t) {
  let q = quantize(t);
  return cached(`m|${base}|${other}|${q}`, () => {
    let a = (parseColor2(base) >>> 0 & 255) / 255;
    let mixed = mixColors2(base, other, q);
    return a < 1 ? withAlpha(mixed, a) : mixed;
  });
}
function multiply(base, light) {
  let a = parseColor2(base) >>> 0;
  let b = parseColor2(light) >>> 0;
  let hex = "#";
  for (let shift of [24, 16, 8]) {
    let product = toLinear(a >>> shift & 255) * toLinear(b >>> shift & 255);
    hex += toSrgb(product).toString(16).padStart(2, "0");
  }
  return hex;
}
function toLinear(channel) {
  let c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}
function toSrgb(linear) {
  let c = linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
  return Math.round(clamp2(c, 0, 1) * 255);
}
function lightAxis(direction) {
  let [x, y, z] = direction;
  let length = Math.hypot(x, y, z) || 1;
  let planar = Math.hypot(x, y);
  let zz = Math.max(z, MIN_Z);
  let index = planar > 0.0001 ? Math.round(Math.atan2(y, x) / (Math.PI * 2) * ANGLE_STEPS) : ANGLE_STEPS / 4;
  let slantX = Math.round(clamp2(x / zz, -MAX_SLANT, MAX_SLANT) * SLANT_STEPS) / SLANT_STEPS;
  let slantY = Math.round(clamp2(y / zz, -MAX_SLANT, MAX_SLANT) * SLANT_STEPS) / SLANT_STEPS;
  let obliquity = Math.round((z <= 0 ? 1 : planar / length) * OBLIQUITY_STEPS) / OBLIQUITY_STEPS;
  return cached(`L|${index}|${slantX}|${slantY}|${obliquity}`, () => {
    let angle = index / ANGLE_STEPS * Math.PI * 2;
    return {
      ax: Math.cos(angle),
      ay: Math.sin(angle),
      slantX,
      slantY,
      obliquity
    };
  });
}
var axisMemo;
function themeAxis() {
  axisMemo ??= runWithOwner(null, () => createRoot(() => createMemo(() => lightAxis(theme.light.direction))));
  return axisMemo();
}
function alongLight(axis2, lit, shaded, invert2, mid) {
  let s = (Math.abs(axis2.ax) + Math.abs(axis2.ay)) / 2;
  let [from, to] = invert2 ? [shaded, lit] : [lit, shaded];
  let template = cached(`g|${from}|${mid}|${to}`, () => createLinearGradient(0, 0, 0, 1, mid ? [{
    offset: 0,
    color: from
  }, {
    offset: 0.5,
    color: mid
  }, {
    offset: 1,
    color: to
  }] : [{
    offset: 0,
    color: from
  }, {
    offset: 1,
    color: to
  }]));
  return cached(`G|${from}|${mid}|${to}|${axis2.ax}|${axis2.ay}`, () => ({
    ...template,
    x0: 0.5 - axis2.ax * s,
    y0: 0.5 - axis2.ay * s,
    x1: 0.5 + axis2.ax * s,
    y1: 0.5 + axis2.ay * s
  }));
}
var snap = (v) => Math.round(v / SHADOW_STEP) * SHADOW_STEP;
function shadow(x, y, blur, color) {
  let sx = snap(x);
  let sy = snap(y);
  let sb = snap(blur);
  return cached(`s|${sx}|${sy}|${sb}|${color}`, () => ({
    x: sx,
    y: sy,
    blur: sb,
    color
  }));
}
function litFace(base, light) {
  if (typeof base !== "string")
    return base;
  let t = LIGHT_TINT * (1 - light.ambient);
  if (t <= 0)
    return base;
  return mix(base, cached(`x|${base}|${light.color}`, () => multiply(base, light.color)), t);
}
function sheenFill(base, light, axis2, sheen, invert2) {
  if (!sheen || sheen <= 0 || typeof base !== "string" || base === "transparent")
    return base;
  let face = litFace(base, light);
  let k = sheen * axis2.obliquity;
  if (k <= 0)
    return face;
  let lit = mix(face, light.color, SHEEN_LIGHT * k);
  let shaded = mix(face, "#000000", SHEEN_SHADE * k * (1 - light.ambient * AMBIENT_FILL));
  return alongLight(axis2, lit, shaded, invert2);
}
function bevelStroke(light, axis2, bevel, invert2) {
  let k = (bevel ?? 0) * axis2.obliquity;
  if (k <= 0)
    return;
  let lit = alpha(light.color, BEVEL_LIGHT * k);
  let shaded = alpha("#000000", BEVEL_SHADE * k * (1 - light.ambient * AMBIENT_FILL));
  return alongLight(axis2, lit, shaded, invert2, alpha(light.color, 0));
}
function keyShadow(light, axis2, height, tone) {
  if (height <= 0 || tone.strength <= 0)
    return;
  return shadow(height * SHADOW_OFFSET * axis2.slantX, height * SHADOW_OFFSET * axis2.slantY, height * SHADOW_BLUR, alpha(tone.color, SHADOW_ALPHA * tone.strength * (1 - light.ambient * AMBIENT_FILL)));
}
function contactShadow(light, axis2, height, tone) {
  if (height <= 0 || tone.strength <= 0)
    return;
  return shadow(CONTACT_OFFSET * axis2.slantX, CONTACT_OFFSET * axis2.slantY, CONTACT_BLUR + height * CONTACT_BLUR_PER_UNIT, alpha(tone.color, CONTACT_ALPHA * tone.strength * (1 - light.ambient * AMBIENT_FILL)));
}
function fadedShadow(s, fraction) {
  let a = (parseColor2(s.color) >>> 0 & 255) / 255;
  return shadow(s.x ?? 0, s.y ?? 0, s.blur ?? 0, alpha(s.color, a * fraction));
}

// ../../packages/components/src/glow.ts
var GLOW_ALPHA = 0.7;
var GLOW_ACTIVE = 1.35;
function glowShadow(glow, fill, active = false) {
  if (!glow)
    return;
  let color = glow.color;
  if (color == null) {
    if (typeof fill !== "string" || (parseColor2(fill) >>> 0 & 255) === 0)
      return;
    color = withAlpha(fill, GLOW_ALPHA);
  }
  return {
    x: 0,
    y: 0,
    blur: glow.radius * (active ? GLOW_ACTIVE : 1),
    color
  };
}

// ../../packages/components/src/surface.tsx
var PRESSED_KEY_SHADOW = 0.1;
var GLASS_ALPHA = 0.6;
var isOpaque = (c) => typeof c !== "string" || (parseColor2(c) >>> 0 & 255) === 255;
function roleFill(role) {
  switch (role) {
    case "control":
      return theme.color.surfaceAlt;
    case "accent":
      return theme.color.primary;
    default:
      return theme.color.surface;
  }
}
function roleMaterial(role, override) {
  let m = theme.material[role];
  return override ? {
    ...m,
    ...override
  } : m;
}
function surfaceSinks(role, elevation = "flat", material) {
  let m = roleMaterial(role, material);
  return (m.sheen ?? 0) > 0 || (m.bevel ?? 0) > 0 || theme.elevation[elevation] > 0;
}
function facePaint(o) {
  let light = theme.light;
  let axis2 = themeAxis();
  let material = roleMaterial(o.role, o.material);
  let base = o.fill ?? roleFill(o.role);
  let invert2 = o.pressed === true || o.sunken === true;
  let height = o.sunken ? 0 : theme.elevation[o.elevation ?? "flat"];
  let key2 = keyShadow(light, axis2, height, theme.shadow);
  return {
    fill: sheenFill(base, light, axis2, material.sheen, invert2),
    keyShadow: key2 && o.pressed ? fadedShadow(key2, PRESSED_KEY_SHADOW) : key2,
    contactShadow: contactShadow(light, axis2, height, theme.shadow),
    glow: glowShadow(o.glow !== undefined ? o.glow : material.glow, base, o.active),
    bevel: bevelStroke(light, axis2, material.bevel, invert2)
  };
}
function Surface(props) {
  let chrome = () => props.chrome !== false;
  let base = () => props.fill ?? roleFill(props.role);
  let material = () => roleMaterial(props.role, props.material);
  let glass = () => chrome() ? material().glass : undefined;
  let paint = () => facePaint({
    role: props.role,
    fill: base(),
    elevation: props.elevation,
    material: props.material,
    glow: props.glow,
    pressed: props.pressed,
    sunken: props.sunken,
    active: props.active ?? (props.pressed || props.hovered)
  });
  let key2 = () => {
    if (!chrome())
      return;
    if (props.lifted && props.liftTo && !props.sunken) {
      let light = theme.light;
      return keyShadow(light, themeAxis(), theme.elevation[props.liftTo], theme.shadow);
    }
    return paint().keyShadow;
  };
  let contact = () => chrome() ? paint().contactShadow : undefined;
  let glow = () => chrome() ? paint().glow : undefined;
  let fill = () => chrome() ? paint().fill : "transparent";
  let bevel = () => chrome() ? paint().bevel : undefined;
  let caster = () => glass() || !isOpaque(base()) ? "transparent" : base();
  let keyOnFill = () => !glass() && caster() !== "transparent";
  let glassFill = () => {
    let g = glass();
    let tint2 = g.tint ?? (typeof base() === "string" ? withAlpha(base(), GLASS_ALPHA) : base());
    return sheenFill(tint2, theme.light, themeAxis(), material().sheen, props.pressed === true || props.sunken === true);
  };
  let fillTransition = () => withTransitionDefaults(props.fillTransition, {
    ...colorFade(),
    ...elevationMotion()
  });
  let shadowTransition = () => elevationMotion();
  let outlineTransition = () => withTransitionDefaults(props.outlineTransition, colorFade());
  let tinted = () => props.tint !== false && (props.pressed !== undefined || props.hovered !== undefined);
  let tint = () => props.pressed ? theme.color.overlayPressed : theme.color.overlayHover;
  let tintFade = () => {
    let f = feedbackFade();
    return f === undefined ? undefined : {
      color: f
    };
  };
  return [createComponent2(Show, {
    get when() {
      return contact();
    },
    children: (shadow2) => (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: caster(),
        t: props.radius,
        a: shadow2(),
        o: shadowTransition()
      }), ({
        e,
        t,
        a,
        o
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
        a !== _p$?.a && setProp(_el$2, "shadow", a, _p$?.a);
        o !== _p$?.o && setProp(_el$2, "transition", o, _p$?.o);
      });
      return _el$2;
    })()
  }), createComponent2(Show, {
    get when() {
      return glow();
    },
    children: (shadow2) => (() => {
      var _el$3 = createElement("d-rect");
      effect3(() => ({
        e: caster(),
        t: props.radius,
        a: shadow2()
      }), ({
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "radius", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "shadow", a, _p$?.a);
      });
      return _el$3;
    })()
  }), createComponent2(Show, {
    get when() {
      return memo2(() => !keyOnFill())() && key2();
    },
    children: (shadow2) => (() => {
      var _el$4 = createElement("d-rect", {
        color: "transparent"
      });
      effect3(() => ({
        e: props.radius,
        t: shadow2(),
        a: shadowTransition()
      }), ({
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "radius", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "shadow", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "transition", a, _p$?.a);
      });
      return _el$4;
    })()
  }), createComponent2(Show, {
    get when() {
      return glass();
    },
    get fallback() {
      var _el$5 = createElement("d-rect");
      effect3(() => ({
        e: fillTransition(),
        t: props.onFillTransitionEnd,
        a: fill(),
        o: props.radius,
        i: keyOnFill() ? key2() : undefined
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
        o !== _p$?.o && setProp(_el$5, "radius", o, _p$?.o);
        i !== _p$?.i && setProp(_el$5, "shadow", i, _p$?.i);
      });
      return _el$5;
    },
    children: (g) => (() => {
      var _el$6 = createElement("view", {
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        overflow: "hidden"
      }), _el$7 = createElement("d-rect");
      insertNode2(_el$6, _el$7);
      effect3(() => ({
        e: props.radius,
        t: {
          blur: g().blur,
          saturate: 1.4
        },
        a: fillTransition(),
        o: props.onFillTransitionEnd,
        i: glassFill(),
        n: props.radius
      }), ({
        e,
        t,
        a,
        o,
        i,
        n
      }, _p$) => {
        e !== _p$?.e && setProp(_el$6, "clipRadius", e, _p$?.e);
        t !== _p$?.t && setProp(_el$6, "backdropFilter", t, _p$?.t);
        a !== _p$?.a && setProp(_el$7, "transition", a, _p$?.a);
        o !== _p$?.o && setProp(_el$7, "onTransitionEnd", o, _p$?.o);
        i !== _p$?.i && setProp(_el$7, "color", i, _p$?.i);
        n !== _p$?.n && setProp(_el$7, "radius", n, _p$?.n);
      });
      return _el$6;
    })()
  }), createComponent2(Show, {
    get when() {
      return tinted();
    },
    get children() {
      var _el$ = createElement("d-rect");
      effect3(() => ({
        e: tintFade(),
        t: props.pressed || props.hovered ? tint() : withAlpha(tint(), 0),
        a: props.radius
      }), ({
        e,
        t,
        a
      }, _p$) => {
        e !== _p$?.e && setProp(_el$, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$, "color", t, _p$?.t);
        a !== _p$?.a && setProp(_el$, "radius", a, _p$?.a);
      });
      return _el$;
    }
  }), createComponent2(Show, {
    get when() {
      return bevel();
    },
    children: (b) => (() => {
      var _el$8 = createElement("d-rect", {
        drawStyle: "stroke",
        strokeWidth: 1
      });
      effect3(() => ({
        e: b(),
        t: props.radius
      }), ({
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$8, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$8, "radius", t, _p$?.t);
      });
      return _el$8;
    })()
  }), createComponent2(Show, {
    get when() {
      return props.outline;
    },
    children: (o) => (() => {
      var _el$9 = createElement("d-rect", {
        drawStyle: "stroke"
      });
      effect3(() => ({
        e: outlineTransition(),
        t: props.onOutlineTransitionEnd,
        a: o().color,
        o: o().width,
        i: props.radius
      }), ({
        e,
        t,
        a,
        o: o2,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$9, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$9, "onTransitionEnd", t, _p$?.t);
        a !== _p$?.a && setProp(_el$9, "color", a, _p$?.a);
        o2 !== _p$?.o && setProp(_el$9, "strokeWidth", o2, _p$?.o);
        i !== _p$?.i && setProp(_el$9, "radius", i, _p$?.i);
      });
      return _el$9;
    })()
  })];
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
var CARET_BLINK_MS = 500;
function sameFont(a, b) {
  let ka = Object.keys(a);
  let kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => a[k] === b[k]);
}
function EditorField(props) {
  let [caretOn, setCaretOn] = createSignal(true);
  let node;
  let viewport;
  let blinkId = null;
  let startBlink = () => {
    setCaretOn(true);
    if (blinkId != null)
      clearInterval(blinkId);
    blinkId = setInterval(() => setCaretOn((v) => !v), CARET_BLINK_MS);
  };
  let restartBlink = () => {
    if (blinkId != null)
      startBlink();
  };
  let focused = createMemo(() => {
    let id2 = focusedNode();
    return id2 != null && id2 === node?.id;
  });
  let editing = createMemo(() => focused() && textInputActive());
  createEffect(() => editing(), (active) => {
    if (!active)
      return;
    startBlink();
    return () => {
      if (blinkId != null)
        clearInterval(blinkId);
      blinkId = null;
    };
  });
  let buffer = untrack(() => props.buffer)((_text, offset, direction) => editor.step(offset, direction));
  let value = buffer.value;
  let selectedText = () => {
    let {
      anchor,
      focus
    } = buffer.selection();
    return value().slice(Math.min(anchor, focus), Math.max(anchor, focus));
  };
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
  let offsetAt = (e) => {
    let line = editor.lineAtY(e.localY + editor.scrollY());
    return editor.offsetAtX(line, e.localX + editor.scrollX());
  };
  let dragArmed = null;
  let dragActive = null;
  let dragOwner = {
    cancel: () => {
      dragActive = null;
    }
  };
  let handleViewportPointerDown = (e) => {
    if (props.disabled)
      return;
    let offset = offsetAt(e);
    buffer.setSelection(e.shiftKey ? buffer.selection().anchor : offset, offset);
    if (e.pointerType !== "touch" && (e.button == null || e.button === 0))
      dragArmed = e.pointerId;
    restartBlink();
  };
  let handleViewportPointerMove = (e) => {
    if (props.disabled)
      return;
    if (dragArmed === e.pointerId) {
      dragArmed = null;
      if (arena.steal(e.pointerId, dragOwner))
        dragActive = e.pointerId;
    }
    if (dragActive === e.pointerId) {
      buffer.setSelection(buffer.selection().anchor, offsetAt(e));
      restartBlink();
    }
  };
  let handleViewportPointerEnd = (e) => {
    if (dragArmed === e.pointerId)
      dragArmed = null;
    if (dragActive === e.pointerId) {
      arena.release(e.pointerId, dragOwner);
      dragActive = null;
    }
  };
  let handleFocus = () => {
    untrack(() => props.onFocus)?.();
  };
  let handleBlur = () => {
    untrack(() => props.onBlur)?.();
  };
  let handleKeyDown = (e) => {
    if (props.disabled)
      return;
    let consumed = true;
    if (e.key === "Backspace") {
      buffer.deleteBackward();
      restartBlink();
    } else if (e.key === "Delete") {
      buffer.deleteForward();
      restartBlink();
    } else if (e.key === "ArrowLeft") {
      buffer.move("left", {
        extend: e.shiftKey
      });
      restartBlink();
    } else if (e.key === "ArrowRight") {
      buffer.move("right", {
        extend: e.shiftKey
      });
      restartBlink();
    } else if (e.key === "Home" || e.key === "End") {
      if (props.multiline) {
        let offset = editor.offsetAtX(editor.caretLine(), e.key === "Home" ? 0 : 1e9);
        buffer.setSelection(e.shiftKey ? buffer.selection().anchor : offset, offset);
      } else {
        buffer.move(e.key === "Home" ? "start" : "end", {
          extend: e.shiftKey
        });
      }
      restartBlink();
    } else if (props.multiline && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      moveLine(e.key === "ArrowUp" ? -1 : 1, e.shiftKey);
      restartBlink();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      buffer.setSelection(0, value().length);
      restartBlink();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
      let text = selectedText();
      if (text.length === 0)
        consumed = false;
      else
        navigator.clipboard.writeText(text).catch((err) => console.warn("Clipboard copy failed: " + err));
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "x") {
      let text = selectedText();
      if (text.length === 0)
        consumed = false;
      else {
        navigator.clipboard.writeText(text).catch((err) => console.warn("Clipboard cut failed: " + err));
        buffer.insertText("");
        restartBlink();
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
      navigator.clipboard.readText().then((text) => {
        if (text.length === 0)
          return;
        if (!props.multiline)
          text = text.replace(/\r?\n/g, " ");
        buffer.insertText(text);
        restartBlink();
      }, (err) => console.warn("Clipboard paste failed: " + err));
    } else if (props.multiline && e.key === "Enter" && textInputActive()) {
      buffer.insertText(`
`);
      restartBlink();
    } else if (e.key === "Enter" || e.code === "Select") {
      activateField();
    } else if (e.key === "Escape") {
      if (node)
        setFocus(null);
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && textInputActive()) {} else {
      consumed = false;
    }
    if (consumed)
      e.stopPropagation();
  };
  let handleTextInput = (e) => {
    if (props.disabled)
      return;
    buffer.insertText(e.text ?? "");
    restartBlink();
  };
  let moveLine = (delta, extend) => {
    let target = editor.caretLine() + delta;
    let count = editor.lines().length;
    let offset = target < 0 ? 0 : target >= count ? value().length : editor.offsetAtX(target, editor.caret().x);
    buffer.setSelection(extend ? buffer.selection().anchor : offset, offset);
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
    unregisterNav?.();
    if (dragActive != null)
      arena.release(dragActive, dragOwner);
  });
  let textColor = () => props.style?.color ?? theme.color.text;
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface;
  let ring = () => focused() && policy.focusRing;
  let borderColor = () => props.style?.borderColor ?? (ring() ? theme.color.ring : theme.color.border);
  let borderWidth = () => props.style?.borderWidth ?? (ring() ? theme.borderWidth.focus : theme.borderWidth.sm);
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.md;
  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0;
  let showCaret = () => editing() && caretOn() && !showPlaceholder();
  let layout = createMemo(() => splitTextLayout(props.layout));
  let layoutFont = () => layout().text;
  let fontSize = () => (layoutFont().fontSize ?? theme.text.body.size) * policy.textScale;
  let lineHeight = () => layoutFont().lineHeight ?? theme.text.body.lineHeight;
  let font = createMemo(() => ({
    fontFamily: layoutFont().fontFamily ?? theme.text.fontFamily,
    fontSize: fontSize(),
    lineHeight: lineHeight(),
    fontStyle: layoutFont().fontStyle,
    fontWeight: typeWeight(layoutFont().fontWeight ?? theme.text.body.weight, fontSize())
  }), {
    equals: sameFont
  });
  let rowHeight = () => Math.round(fontSize() * lineHeight());
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
    let lines = editor.lines();
    let last = lines[lines.length - 1];
    let content = Math.ceil(last.y + last.height);
    let max = props.maxRows != null ? props.maxRows * rowHeight() : Infinity;
    return Math.max(rowHeight(), Math.min(content, max));
  };
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view"), _el$2 = createElement("view", {
    alignSelf: "stretch",
    overflow: "hidden",
    onPointerDown: handleViewportPointerDown,
    onPointerMove: handleViewportPointerMove,
    onPointerUp: handleViewportPointerEnd,
    onPointerCancel: handleViewportPointerEnd
  });
  insertNode2(_el$, _el$2);
  ref(() => (n) => {
    node = n;
    unregisterNav?.();
    unregisterNav = registerNavAction(n.id, activateField);
    untrack(() => props.ref)?.(n);
  }, _el$);
  setProp(_el$, "focusable", true);
  setProp(_el$, "cursor", "text");
  setProp(_el$, "flexDirection", "column");
  setProp(_el$, "justifyContent", "center");
  spread(_el$, [{
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
    get minHeight() {
      return props.multiline ? 0 : undefined;
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
  }, () => layout().box, {
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
  }], true);
  insert(_el$, createComponent2(Surface, {
    role: "control",
    get fill() {
      return surfaceColor();
    },
    get radius() {
      return borderRadius();
    },
    get material() {
      return props.style?.material;
    },
    get glow() {
      return props.style?.glow;
    },
    sunken: true,
    get outline() {
      return {
        color: borderColor(),
        width: borderWidth()
      };
    },
    get fillTransition() {
      return split().background;
    },
    get onFillTransitionEnd() {
      return transitionEndFor("background", props.onTransitionEnd);
    },
    get outlineTransition() {
      return split().border;
    },
    get onOutlineTransitionEnd() {
      return transitionEndFor("border", props.onTransitionEnd);
    }
  }), _el$2);
  ref(() => (n) => viewport = n, _el$2);
  insert(_el$2, (() => {
    var _c$ = memo2(() => !!showPlaceholder());
    return () => _c$() ? (() => {
      var _el$3 = createElement("d-text");
      setProp(_el$3, "w", 1e9);
      spread(_el$3, [font, {
        get color() {
          return theme.color.textMuted;
        },
        maxLines: 1
      }], true);
      insert(_el$3, () => props.placeholder ?? "");
      return _el$3;
    })() : [memo2(() => memo2(() => !!focused())() ? createComponent2(For, {
      get each() {
        return editor.selectionRects(buffer.selection().anchor, buffer.selection().focus);
      },
      keyed: false,
      children: (r) => (() => {
        var _el$4 = createElement("d-rect");
        effect3(() => ({
          e: theme.color.selection,
          t: r().x,
          a: r().y,
          o: r().width,
          i: r().height
        }), ({
          e,
          t,
          a,
          o,
          i
        }, _p$) => {
          e !== _p$?.e && setProp(_el$4, "color", e, _p$?.e);
          t !== _p$?.t && setProp(_el$4, "x", t, _p$?.t);
          a !== _p$?.a && setProp(_el$4, "y", a, _p$?.a);
          o !== _p$?.o && setProp(_el$4, "w", o, _p$?.o);
          i !== _p$?.i && setProp(_el$4, "h", i, _p$?.i);
        });
        return _el$4;
      })()
    }) : null), createComponent2(For, {
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
      var _el$5 = createElement("d-rect", {
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
        e !== _p$?.e && setProp(_el$5, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$5, "x", t, _p$?.t);
        a !== _p$?.a && setProp(_el$5, "y", a, _p$?.a);
        o !== _p$?.o && setProp(_el$5, "h", o, _p$?.o);
      });
      return _el$5;
    })() : null)];
  })());
  effect3(() => ({
    e: viewportHeight(),
    t: props.multiline ? 0 : undefined,
    a: props.multiline ? 1 : 0,
    o: props.multiline ? 1 : 0,
    i: editor.scrollX(),
    n: editor.scrollY()
  }), ({
    e,
    t,
    a,
    o,
    i,
    n
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "height", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "minHeight", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "flexGrow", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "flexShrink", o, _p$?.o);
    i !== _p$?.i && setProp(_el$2, "scrollX", i, _p$?.i);
    n !== _p$?.n && setProp(_el$2, "scrollY", n, _p$?.n);
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
      spread(_el$, [{
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
      }], true);
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
var MOMENTUM_DECAY = 2;
var MOMENTUM_CURVE = [0.15, 1, 0.36, 1];
var MOMENTUM_MS = Math.round(MOMENTUM_CURVE[1] / MOMENTUM_CURVE[0] / MOMENTUM_DECAY * 1000);
var LIVE_EPSILON = 0.5;
function ScrollView(props) {
  let viewport;
  let content;
  let [dragging, setDragging] = createSignal(false);
  let [fling, setFling] = createSignal(false);
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
    onPanEnd: (v) => {
      setDragging(false);
      let speed = props.horizontal ? v.vx : v.vy;
      if (speed === 0)
        return;
      let cur = scroll.offset();
      let range = scroll.range();
      let now = props.horizontal ? cur.x : cur.y;
      let dest = Math.max(0, Math.min(now - speed / MOMENTUM_DECAY, props.horizontal ? range.x : range.y));
      if (dest === now)
        return;
      setFling(true);
      scroll.scrollTo(props.horizontal ? {
        x: dest
      } : {
        y: dest
      });
    },
    onPanCancel: () => setDragging(false)
  });
  let hold2 = (e) => {
    setFling(false);
    if (viewport && content) {
      let vb = getBoundingBoxViewport2(viewport);
      let cb = getBoundingBoxViewport2(content);
      let lb = getLayoutBox2(viewport);
      if (vb && cb && lb) {
        let scale2 = props.horizontal ? lb.width > 0 ? vb.width / lb.width : 0 : lb.height > 0 ? vb.height / lb.height : 0;
        if (scale2 > 0) {
          let live = (props.horizontal ? vb.x - cb.x : vb.y - cb.y) / scale2;
          let cur = scroll.offset();
          if (Math.abs(live - (props.horizontal ? cur.x : cur.y)) > LIVE_EPSILON) {
            scroll.scrollTo(props.horizontal ? {
              x: live,
              behavior: "instant"
            } : {
              y: live,
              behavior: "instant"
            });
          }
        }
      }
    }
    pan.handlers.onPointerDown(e);
  };
  let settled = (e) => {
    if (e.property === "scrollX" || e.property === "scrollY")
      setFling(false);
    transitionEndFor("root", props.onTransitionEnd)?.(e);
  };
  let onWheel = (e) => {
    setFling(false);
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
    if (fling()) {
      let momentum = {
        duration: MOMENTUM_MS,
        curve: MOMENTUM_CURVE
      };
      return {
        ...entries,
        scrollX: momentum,
        scrollY: momentum
      };
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
  var _el$ = createElement("view"), _el$2 = createElement("view", {
    flex: 1,
    overflow: "hidden",
    onTransitionEnd: settled,
    onPointerDown: hold2,
    onWheel
  }), _el$3 = createElement("view", {
    flexShrink: 0
  });
  insertNode2(_el$, _el$2);
  var _ref$ = props.ref;
  typeof _ref$ === "function" || Array.isArray(_ref$) ? ref(() => _ref$, _el$) : props.ref = _el$;
  spread(_el$, [{
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
    get onPointerCancel() {
      return props.onPointerCancel;
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
  }], true);
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
  effect3(() => ({
    e: props.style?.borderRadius,
    t: direction(),
    a: viewportTransition(),
    o: scroll.offset().x,
    i: scroll.offset().y,
    n: pan.handlers.onPointerMove,
    s: pan.handlers.onPointerUp,
    h: pan.handlers.onPointerCancel,
    r: direction()
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
    e !== _p$?.e && setProp(_el$2, "clipRadius", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "flexDirection", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "transition", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "scrollX", o, _p$?.o);
    i !== _p$?.i && setProp(_el$2, "scrollY", i, _p$?.i);
    n !== _p$?.n && setProp(_el$2, "onPointerMove", n, _p$?.n);
    s !== _p$?.s && setProp(_el$2, "onPointerUp", s, _p$?.s);
    h !== _p$?.h && setProp(_el$2, "onPointerCancel", h, _p$?.h);
    r !== _p$?.r && setProp(_el$3, "flexDirection", r, _p$?.r);
  });
  return _el$;
}
// ../../packages/components/src/press.ts
function createPress(options) {
  let [pressed, setPressed] = createSignal(false);
  let [hovered, setHovered] = createSignal(false);
  let node = null;
  let unregisterNav = null;
  let [pending2, setPending] = createSignal(false);
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
      return pending2();
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
  let disposed = false;
  onSettled(() => () => {
    disposed = true;
    disengage();
    unregisterNav?.();
  });
  let fireDeferred = () => {
    if (!disposed)
      activate();
  };
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
        if (fire && !arena.defer(e.pointerId, fireDeferred))
          activate();
      }
      options.onPointerUp?.(e);
    },
    onPointerCancel: (e) => {
      if (active === e.pointerId)
        cancel();
      options.onPointerCancel?.(e);
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
    pending: pending2,
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
    untrack(() => props.ref)?.(n);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  spread(_el$, [{
    get transition() {
      return withTransitionDefaults(split().root, scaleFeedback());
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
    get onPointerCancel() {
      return press.handlers.onPointerCancel;
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
    },
    get cursor() {
      return props.cursor ?? "pointer";
    }
  }], true);
  insert(_el$, (() => {
    var _c$ = memo2(() => !!hasBackground());
    return () => _c$() ? (() => {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: withTransitionDefaults(split().background, colorFade()),
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
        e: withTransitionDefaults(split().border, colorFade()),
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
  spread(_el$, [{
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
  }], true);
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
    t: colorFade(),
    a: color(),
    o: thickness()
  }), ({
    e,
    t,
    a,
    o
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "d", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "transition", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "strokeWidth", o, _p$?.o);
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
  let variant = () => props.variant ?? "primary";
  let role = () => variant() === "primary" || variant() === "danger" ? "accent" : "control";
  let styled = () => ({
    ...theme.components.button,
    ...props.style
  });
  let idleFill = () => props.disabled ? variant() === "ghost" ? "transparent" : theme.color.surface : colors().fill;
  let bg = () => styled().backgroundColor ?? idleFill();
  let radius = () => styled().borderRadius ?? theme.radius.md;
  let elevation = () => styled().elevation ?? (props.disabled || variant() === "ghost" ? "flat" : "raised");
  let label = () => props.disabled ? theme.color.textMuted : colors().label;
  let resolved2 = children(() => props.children);
  let isText = () => typeof resolved2() === "string" || typeof resolved2() === "number";
  let labelOnDark = () => lightOnDark(label(), bg());
  let press = createPress(props);
  let pressed = () => !props.disabled && press.pressed();
  let hovered = () => !props.disabled && press.hovered() && policy.interaction !== "touch";
  let sinks = () => surfaceSinks(role(), elevation(), styled().material);
  let style = () => ({
    ...styled(),
    ...press.focused() && policy.focusRing ? {
      borderWidth: theme.borderWidth.focus,
      borderColor: theme.color.ring
    } : {},
    backgroundColor: bg(),
    borderRadius: radius(),
    scale: (styled().scale ?? 1) * (sinks() ? 1 : pressScale(press.pressed()))
  });
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view");
  ref(() => (n) => {
    press.ref(n);
    untrack(() => props.ref)?.(n);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "alignItems", "center");
  setProp(_el$, "justifyContent", "center");
  setProp(_el$, "position", "relative");
  spread(_el$, [{
    get transition() {
      return withTransitionDefaults(split().root, scaleFeedback());
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
    },
    get minWidth() {
      return memo2(() => !!props.size)() ? SIZE_WIDTH[props.size] : undefined;
    }
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
    cursor: "pointer",
    get focusable() {
      return memo2(() => !!(props.focusable ?? true))() ? props.disabled !== true : props.focusable ?? true;
    },
    get pointerEvents() {
      return props.disabled ? "none" : undefined;
    }
  }], true);
  insert(_el$, createComponent2(Surface, {
    get role() {
      return role();
    },
    get elevation() {
      return elevation();
    },
    get fill() {
      return bg();
    },
    get radius() {
      return radius();
    },
    get material() {
      return styled().material;
    },
    get glow() {
      return memo2(() => !!props.disabled)() ? null : styled().glow;
    },
    get chrome() {
      return variant() !== "ghost" || pressed() || hovered();
    },
    get pressed() {
      return pressed();
    },
    get hovered() {
      return hovered();
    },
    get outline() {
      return memo2(() => (style().borderWidth ?? 0) > 0)() ? {
        color: style().borderColor ?? "transparent",
        width: style().borderWidth
      } : null;
    },
    get fillTransition() {
      return split().background;
    },
    get onFillTransitionEnd() {
      return transitionEndFor("background", props.onTransitionEnd);
    },
    get outlineTransition() {
      return split().border;
    },
    get onOutlineTransitionEnd() {
      return transitionEndFor("border", props.onTransitionEnd);
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return isText();
    },
    get fallback() {
      return resolved2();
    },
    get children() {
      var _el$2 = createElement("text");
      spread(_el$2, [{
        get transition() {
          return colorFade();
        },
        get color() {
          return memo2(() => !!press.pending())() ? withAlpha(label(), 0) : label();
        }
      }, () => typeStyle("body", labelOnDark())], true);
      insert(_el$2, resolved2);
      return _el$2;
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return press.pending();
    },
    get children() {
      var _el$3 = createElement("view", {
        position: "absolute",
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        alignItems: "center",
        justifyContent: "center"
      });
      insert(_el$3, createComponent2(Spinner, {
        size: 16,
        thickness: 2,
        get style() {
          return {
            color: label()
          };
        }
      }));
      return _el$3;
    }
  }), null);
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
  spread(_el$, [{
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
  }, () => props.layout], true);
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
var LIFTED = {
  flat: "raised",
  raised: "floating",
  floating: "overlay",
  overlay: "overlay"
};
var LIFT_RISE = 2;
var GLARE = 0.08;
function Card(props) {
  let styled = () => ({
    ...theme.components.card,
    ...props.style
  });
  let bg = () => styled().backgroundColor ?? theme.color.surface;
  let radius = () => styled().borderRadius ?? theme.radius.lg;
  let elevation = () => styled().elevation ?? "raised";
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null;
  let node;
  let [pointer, setPointer] = createSignal(null);
  let lifts = () => props.lift === true && policy.interaction !== "touch";
  let track = (e) => {
    if (!lifts() || !node)
      return;
    let box = getLayoutBox2(node);
    if (!box || box.width === 0 || box.height === 0)
      return;
    setPointer([e.localX / box.width, e.localY / box.height]);
  };
  let glare = () => {
    let p = pointer();
    if (!p)
      return "transparent";
    return createRadialGradient(p[0], p[1], 0.9, [{
      offset: 0,
      color: withAlpha(theme.light.color, GLARE)
    }, {
      offset: 1,
      color: withAlpha(theme.light.color, 0)
    }]);
  };
  let rises = () => lifts() && policy.motion === "normal" && theme.elevation[elevation()] > 0;
  let riseTransition = () => rises() ? {
    y: {
      duration: theme.motion.slow,
      bounce: 0.2
    }
  } : undefined;
  let split = () => splitTransition(props.transition);
  var _el$ = createElement("view");
  ref(() => (n) => {
    node = n;
    props.ref?.(n);
  }, _el$);
  setProp(_el$, "repaintBoundary", true);
  setProp(_el$, "flexDirection", "column");
  spread(_el$, [{
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
      return (styled().y ?? 0) - (rises() && pointer() ? LIFT_RISE : 0);
    },
    get scale() {
      return styled().scale;
    },
    get rotate() {
      return styled().rotate;
    },
    get opacity() {
      return styled().opacity;
    },
    get onPointerMove() {
      return lifts() ? track : undefined;
    },
    get onPointerEnter() {
      return lifts() ? track : undefined;
    },
    get onPointerLeave() {
      return lifts() ? () => setPointer(null) : undefined;
    }
  }], true);
  insert(_el$, createComponent2(Surface, {
    role: "surface",
    get elevation() {
      return elevation();
    },
    get fill() {
      return bg();
    },
    get radius() {
      return radius();
    },
    get material() {
      return styled().material;
    },
    get glow() {
      return styled().glow;
    },
    get liftTo() {
      return memo2(() => !!lifts())() ? LIFTED[elevation()] : undefined;
    },
    get lifted() {
      return pointer() != null;
    },
    get outline() {
      return memo2(() => !!hasBorder())() ? {
        color: styled().borderColor ?? theme.color.border,
        width: styled().borderWidth ?? theme.borderWidth.sm
      } : null;
    },
    get fillTransition() {
      return split().background;
    },
    get onFillTransitionEnd() {
      return transitionEndFor("background", props.onTransitionEnd);
    },
    get outlineTransition() {
      return split().border;
    },
    get onOutlineTransitionEnd() {
      return transitionEndFor("border", props.onTransitionEnd);
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return lifts();
    },
    get children() {
      var _el$2 = createElement("d-rect");
      effect3(() => ({
        e: glare(),
        t: radius()
      }), ({
        e,
        t
      }, _p$) => {
        e !== _p$?.e && setProp(_el$2, "color", e, _p$?.e);
        t !== _p$?.t && setProp(_el$2, "radius", t, _p$?.t);
      });
      return _el$2;
    }
  }), null);
  insert(_el$, createComponent2(Show, {
    get when() {
      return props.title != null;
    },
    get children() {
      var _el$3 = createElement("text");
      spread(_el$3, [{
        get transition() {
          return colorFade();
        },
        get color() {
          return theme.color.text;
        }
      }, () => typeStyle("title")], true);
      insert(_el$3, () => props.title);
      return _el$3;
    }
  }), null);
  insert(_el$, () => props.children, null);
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
      justifyContent: "center",
      opacity: 1
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
    effect3(() => ({
      e: popupFade(),
      t: colorFade(),
      a: props.backdropColor ?? theme.color.scrim
    }), ({
      e,
      t,
      a
    }, _p$) => {
      e !== _p$?.e && setProp(_el$, "transition", e, _p$?.e);
      t !== _p$?.t && setProp(_el$3, "transition", t, _p$?.t);
      a !== _p$?.a && setProp(_el$3, "color", a, _p$?.a);
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
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null;
  let activeFill = () => props.disabled ? theme.color.surface : theme.color.primary;
  let elevation = () => styled().elevation ?? "raised";
  let label = (active) => props.disabled ? theme.color.textMuted : active ? theme.color.onPrimary : theme.color.text;
  let root;
  let segs = new Map;
  let [boxes, setBoxes] = createSignal([]);
  let [pressedValue, setPressedValue] = createSignal(undefined);
  let [placed, setPlaced] = createSignal(false);
  onLayout(() => {
    if (!root)
      return;
    let r = getBoundingBox2(root);
    if (!r)
      return;
    let next = [];
    for (let i = 0;i < props.options.length; i++) {
      let s = segs.get(props.options[i].value);
      let b = s && getBoundingBox2(s);
      if (!b)
        return;
      next.push({
        x: b.x - r.x,
        w: b.width
      });
    }
    let cur = boxes();
    if (cur.length !== next.length || cur.some((c, i) => c.x !== next[i].x || c.w !== next[i].w))
      setBoxes(next);
    if (!placed())
      setTimeout(() => setPlaced(true), 0);
  });
  let activeIndex = () => props.options.findIndex((o) => o.value === value());
  let indicator = () => boxes()[activeIndex()];
  let indicatorPaint = () => facePaint({
    role: "accent",
    fill: activeFill(),
    elevation: elevation(),
    material: styled().material,
    glow: indicator() && !props.disabled ? styled().glow : null,
    pressed: pressedValue() !== undefined && pressedValue() === value()
  });
  let split = () => splitTransition(props.transition, ["indicator"]);
  let indicatorTransition = () => {
    if (props.transition === null)
      return null;
    let travel = placed() ? partTransition(props.transition, "indicator", "x", travelMotion()) : undefined;
    let fade = colorFade();
    if (!travel && !fade)
      return;
    return {
      ...fade,
      ...travel
    };
  };
  var _el$ = createElement("view"), _el$2 = createElement("d-rect");
  insertNode2(_el$, _el$2);
  ref(() => (n) => root = n, _el$);
  setProp(_el$, "flexDirection", "row");
  setProp(_el$, "gap", 0);
  spread(_el$, [{
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
  }], true);
  insert(_el$, createComponent2(Surface, {
    role: "control",
    get fill() {
      return idleFill();
    },
    get radius() {
      return radius();
    },
    get material() {
      return styled().material;
    },
    glow: null,
    sunken: true,
    get outline() {
      return memo2(() => !!hasBorder())() ? {
        color: styled().borderColor ?? theme.color.border,
        width: styled().borderWidth ?? theme.borderWidth.sm
      } : null;
    },
    get fillTransition() {
      return split().background;
    },
    get onFillTransitionEnd() {
      return transitionEndFor("background", props.onTransitionEnd);
    },
    get outlineTransition() {
      return split().border;
    },
    get onOutlineTransitionEnd() {
      return transitionEndFor("border", props.onTransitionEnd);
    }
  }), _el$2);
  insert(_el$, createComponent2(Show, {
    get when() {
      return indicatorPaint().contactShadow;
    },
    children: (shadow2) => (() => {
      var _el$3 = createElement("d-rect");
      effect3(() => ({
        e: indicatorTransition(),
        t: indicator() ? activeFill() : withAlpha(activeFill(), 0),
        a: indicator()?.x ?? 0,
        o: indicator()?.w ?? 0,
        i: corners(activeIndex()),
        n: shadow2()
      }), ({
        e,
        t,
        a,
        o,
        i,
        n
      }, _p$) => {
        e !== _p$?.e && setProp(_el$3, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$3, "color", t, _p$?.t);
        a !== _p$?.a && setProp(_el$3, "x", a, _p$?.a);
        o !== _p$?.o && setProp(_el$3, "w", o, _p$?.o);
        i !== _p$?.i && setProp(_el$3, "radius", i, _p$?.i);
        n !== _p$?.n && setProp(_el$3, "shadow", n, _p$?.n);
      });
      return _el$3;
    })()
  }), _el$2);
  insert(_el$, createComponent2(Show, {
    get when() {
      return indicatorPaint().keyShadow;
    },
    children: (shadow2) => (() => {
      var _el$4 = createElement("d-rect");
      effect3(() => ({
        e: indicatorTransition(),
        t: indicator() ? activeFill() : withAlpha(activeFill(), 0),
        a: indicator()?.x ?? 0,
        o: indicator()?.w ?? 0,
        i: corners(activeIndex()),
        n: shadow2()
      }), ({
        e,
        t,
        a,
        o,
        i,
        n
      }, _p$) => {
        e !== _p$?.e && setProp(_el$4, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$4, "color", t, _p$?.t);
        a !== _p$?.a && setProp(_el$4, "x", a, _p$?.a);
        o !== _p$?.o && setProp(_el$4, "w", o, _p$?.o);
        i !== _p$?.i && setProp(_el$4, "radius", i, _p$?.i);
        n !== _p$?.n && setProp(_el$4, "shadow", n, _p$?.n);
      });
      return _el$4;
    })()
  }), _el$2);
  insert(_el$, createComponent2(Show, {
    get when() {
      return indicatorPaint().bevel;
    },
    children: (bevel) => (() => {
      var _el$5 = createElement("d-rect", {
        drawStyle: "stroke",
        strokeWidth: 1
      });
      effect3(() => ({
        e: indicatorTransition(),
        t: indicator() ? bevel() : withAlpha(activeFill(), 0),
        a: indicator()?.x ?? 0,
        o: indicator()?.w ?? 0,
        i: corners(activeIndex())
      }), ({
        e,
        t,
        a,
        o,
        i
      }, _p$) => {
        e !== _p$?.e && setProp(_el$5, "transition", e, _p$?.e);
        t !== _p$?.t && setProp(_el$5, "color", t, _p$?.t);
        a !== _p$?.a && setProp(_el$5, "x", a, _p$?.a);
        o !== _p$?.o && setProp(_el$5, "w", o, _p$?.o);
        i !== _p$?.i && setProp(_el$5, "radius", i, _p$?.i);
      });
      return _el$5;
    })()
  }), null);
  insert(_el$, createComponent2(For, {
    get each() {
      return props.options;
    },
    children: (opt, i) => {
      let active = () => value() === opt.value;
      let press = createPress({
        onPress: () => select(opt.value),
        onPointerDown: () => setPressedValue(() => opt.value),
        onPointerUp: () => setPressedValue(undefined),
        onPointerCancel: () => setPressedValue(undefined)
      });
      onCleanup(() => segs.delete(opt.value));
      var _el$6 = createElement("view"), _el$8 = createElement("text");
      insertNode2(_el$6, _el$8);
      ref(() => (n) => {
        segs.set(opt.value, n);
        press.ref(n);
      }, _el$6);
      setProp(_el$6, "repaintBoundary", true);
      setProp(_el$6, "flexGrow", 1);
      setProp(_el$6, "flexBasis", 0);
      setProp(_el$6, "alignItems", "center");
      spread(_el$6, [{
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
        cursor: "pointer",
        get focusable() {
          return !props.disabled;
        },
        get pointerEvents() {
          return props.disabled ? "none" : undefined;
        }
      }], true);
      insert(_el$6, createComponent2(PressFeedback, {
        get pressed() {
          return press.pressed();
        },
        get hovered() {
          return memo2(() => !!(press.hovered() && !props.disabled))() ? policy.interaction !== "touch" : press.hovered() && !props.disabled;
        },
        get radius() {
          return corners(i());
        }
      }), _el$8);
      insert(_el$6, createComponent2(Show, {
        get when() {
          return memo2(() => !!press.focused())() ? policy.focusRing : press.focused();
        },
        get children() {
          var _el$7 = createElement("d-rect", {
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
            e !== _p$?.e && setProp(_el$7, "color", e, _p$?.e);
            t !== _p$?.t && setProp(_el$7, "strokeWidth", t, _p$?.t);
            a !== _p$?.a && setProp(_el$7, "radius", a, _p$?.a);
          });
          return _el$7;
        }
      }), _el$8);
      spread(_el$8, [{
        get transition() {
          return colorFade();
        },
        get color() {
          return label(active());
        }
      }, () => typeStyle("body", active() ? lightOnDark(label(true), activeFill()) : undefined)], true);
      insert(_el$8, () => opt.label);
      return _el$6;
    }
  }), null);
  effect3(() => ({
    e: indicatorTransition(),
    t: partTransitionEnd("indicator", "x", props.onTransitionEnd),
    a: indicator() ? indicatorPaint().fill : withAlpha(activeFill(), 0),
    o: indicator()?.x ?? 0,
    i: indicator()?.w ?? 0,
    n: corners(activeIndex()),
    s: indicatorPaint().glow
  }), ({
    e,
    t,
    a,
    o,
    i,
    n,
    s
  }, _p$) => {
    e !== _p$?.e && setProp(_el$2, "transition", e, _p$?.e);
    t !== _p$?.t && setProp(_el$2, "onTransitionEnd", t, _p$?.t);
    a !== _p$?.a && setProp(_el$2, "color", a, _p$?.a);
    o !== _p$?.o && setProp(_el$2, "x", o, _p$?.o);
    i !== _p$?.i && setProp(_el$2, "w", i, _p$?.i);
    n !== _p$?.n && setProp(_el$2, "radius", n, _p$?.n);
    s !== _p$?.s && setProp(_el$2, "shadow", s, _p$?.s);
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
      spread(_el$4, () => props.layout, true);
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
      spread(_el$, () => props.layout, true);
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
    _this2.add = function(key2) {
      if (_this2.contains(key2)) {
        throw "dup key:" + key2;
      }
      _map[key2] = _size;
      _size += 1;
    };
    _this2.size = function() {
      return _size;
    };
    _this2.indexOf = function(key2) {
      return _map[key2];
    };
    _this2.contains = function(key2) {
      return typeof _map[key2] != "undefined";
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
import { stop } from "sol:dev";
import { launch, remove, info, clearCache } from "sol:apps";

// src/parts/app-state.ts
import { available as appsAvailable, list } from "sol:apps";

// src/parts/dev-connection.ts
import { on as on6 } from "sol:events";
import { available as devAvailable, connect as devConnect, launchAddress } from "sol:dev";

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

// src/parts/dev-connection.ts
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

// src/parts/app-state.ts
var [themeMode, setThemeMode] = createSignal("system");
var [fullscreen, setFullscreen] = createSignal(false);
var [notice, setNotice] = createSignal(null);
var [apps, setApps] = createSignal(appsAvailable ? list() : []);
var installedApps = apps;
function refreshApps() {
  setApps(appsAvailable ? list() : []);
}
function dial(addr) {
  setNotice(null);
  router.navigate(home, {
    reset: true
  });
  connect(addr);
}

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

// src/parts/home-screen.tsx
var HOME_CHILD_DEPTH = 2;
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
          elevation: "flat",
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
function groupCache(entries, key2) {
  let groups = new Map;
  for (let e of entries) {
    let k = key2(e);
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
                    layout: {
                      flexGrow: 1
                    },
                    onPress: () => props.onLaunch(),
                    children: "Launch"
                  }), createComponent2(Button, {
                    layout: {
                      flexGrow: 1
                    },
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
                                    layout: {
                                      flexGrow: 1
                                    },
                                    variant: "ghost",
                                    onPress: () => setConfirming(false),
                                    children: "Cancel"
                                  }), createComponent2(Button, {
                                    layout: {
                                      flexGrow: 1
                                    },
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
function doLaunch(id2) {
  try {
    launch(id2);
  } catch (e) {
    setNotice(e instanceof Error ? e.message : String(e));
  }
}
function doRemove(id2) {
  try {
    remove(id2);
  } catch (e) {
    setNotice(e instanceof Error ? e.message : String(e));
  }
  refreshApps();
}
function MissingApp(props) {
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
              }), createComponent2(Text, {
                variant: "heading",
                children: "Not installed"
              })];
            }
          }), createComponent2(Text, {
            variant: "body",
            muted: true,
            get children() {
              return props.id;
            }
          })];
        }
      });
    }
  });
}
function AppDetailRoute() {
  let router2 = useRouter();
  let params = useParams(app);
  let app2 = createMemo2(() => installedApps().find((a) => a.id === params().id) ?? null);
  return createComponent2(Show, {
    get when() {
      return app2();
    },
    get fallback() {
      return createComponent2(MissingApp, {
        get id() {
          return params().id;
        },
        onBack: () => router2.back()
      });
    },
    children: (a) => createComponent2(AppDetail, {
      get app() {
        return a();
      },
      onLaunch: () => doLaunch(a().id),
      onRemove: () => {
        doRemove(a().id);
        router2.back();
      },
      onBack: () => router2.back()
    })
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
            children: (app2) => createComponent2(AppCard, {
              app: app2,
              get active() {
                return memo2(() => !!props.twoPane)() ? props.selectedId === app2.id : props.twoPane;
              },
              onPress: () => props.onSelect(app2.id),
              onLaunch: () => props.onLaunch(app2.id)
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
                layout: {
                  flexGrow: 1
                },
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
                layout: {
                  flexGrow: 1
                },
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
                layout: {
                  flexGrow: 1
                },
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
function HomeScreen() {
  let router2 = useRouter();
  let location = useLocation();
  let twoPane = () => policy.layout === "twoPane";
  let pane = () => location()?.matches[HOME_CHILD_DEPTH]?.route ?? null;
  let detailUp = () => pane() === settings || pane() === app;
  let selectedId = createMemo2(() => {
    let m = location()?.matches[HOME_CHILD_DEPTH];
    return m && m.route === app ? m.params.id : null;
  });
  let status = () => isConnected() ? `Connected to ${serverAddress()}${isTunneled() ? " (tunneled)" : ""}` : notice() ?? STATUS_TEXT[connectionState()];
  return createComponent2(SplitView, {
    layout: {
      flexGrow: 1
    },
    listWidth: 380,
    get showDetail() {
      return detailUp();
    },
    get list() {
      return createComponent2(Show, {
        get when() {
          return pane() !== connect2;
        },
        get fallback() {
          return createComponent2(Outlet, {});
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
                            onPress: () => router2.navigate(settings),
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
                                onPress: () => router2.navigate(scan)
                              });
                            }
                          })];
                        }
                      })];
                    }
                  }), createComponent2(Show, {
                    get when() {
                      return installedApps().length > 0;
                    },
                    get fallback() {
                      return createComponent2(NoApps, {});
                    },
                    get children() {
                      return createComponent2(AppList, {
                        get apps() {
                          return installedApps();
                        },
                        get selectedId() {
                          return selectedId();
                        },
                        get twoPane() {
                          return twoPane();
                        },
                        onSelect: (id2) => {
                          router2.navigate({
                            route: app,
                            params: {
                              id: id2
                            }
                          }, {
                            replace: pane() != null
                          });
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
                        onConnect: () => router2.navigate(connect2)
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
          return detailUp();
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
        get children() {
          return createComponent2(Outlet, {});
        }
      });
    }
  });
}

// src/parts/settings-panel.tsx
import { version as buildVersion, profile as buildProfile, platform as buildPlatform } from "sol:apps";
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
function SettingsPanel() {
  let router2 = useRouter();
  let cycleMode = () => setThemeMode(THEME_MODES[(THEME_MODES.indexOf(themeMode()) + 1) % THEME_MODES.length]);
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
                        onPress: () => router2.back()
                      }), createComponent2(Text, {
                        variant: "heading",
                        children: "Settings"
                      })];
                    }
                  }), createComponent2(Pressable, {
                    focusable: true,
                    onPress: () => setFullscreen(!fullscreen()),
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
                          return fullscreen() ? MINIMIZE_SVG : MAXIMIZE_SVG;
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
                          return themeMode();
                        },
                        onChange: (v) => setThemeMode(v)
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

// src/parts/connect-panel.tsx
var DEFAULT_PORT = "34884";
function recentLabel(entry) {
  if (!entry.includes("|"))
    return entry;
  return "ticket " + entry.split("|")[0].slice(0, 8);
}
function ConnectPanel() {
  let router2 = useRouter();
  let hostDraft = "";
  let portDraft = DEFAULT_PORT;
  let submit = () => {
    let host = hostDraft.trim();
    if (!host)
      return;
    let port = portDraft.trim();
    dial(port ? `${host}:${port}` : host);
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
                onPress: () => router2.back()
              }), createComponent2(Text, {
                variant: "heading",
                layout: {
                  flexGrow: 1
                },
                children: "Connect"
              }), createComponent2(ScanButton, {
                onPress: () => router2.navigate(scan)
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
                    layout: {
                      flexGrow: 1
                    },
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
                          onPress: () => dial(entry),
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

// ../../packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on7 } from "sol:events";
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
function ScanScreen() {
  let router2 = useRouter();
  let cam = createCamera(untrack(() => ({
    scan: ["qr"]
  })));
  createEffect(() => cam.barcode(), (b) => {
    if (b)
      dial(b.data);
  });
  createEffect(() => cam.error(), (e) => {
    if (e) {
      setNotice(`Camera: ${e.message}`);
      router2.navigate(home, {
        reset: true
      });
    }
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
    let scale2 = Math.max(w / cw, h / ch);
    let srcW = w / scale2;
    let srcH = h / scale2;
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
                        onPress: () => router2.navigate(connect2, {
                          replace: true
                        }),
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

// src/routes.ts
var APP_ID = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
var settings = createRoute({
  path: "/settings",
  component: SettingsPanel
});
var connect2 = createRoute({
  path: "/connect",
  component: ConnectPanel
});
var app = createRoute({
  path: "/app/$id",
  params: {
    parse: (raw) => {
      let id2 = raw.id ?? "";
      if (!APP_ID.test(id2))
        throw new Error("not an app id");
      return {
        id: id2
      };
    }
  },
  component: AppDetailRoute
});
var home = createRoute({
  path: "/",
  component: HomeScreen,
  children: [settings, connect2, app]
});
var scan = createRoute({
  path: "/scan",
  component: ScanScreen
});
var root = createRootRoute({
  children: [home, scan]
});
var router = createRouter({
  tree: root
});

// src/index.tsx
function App() {
  let dark = () => {
    let mode = themeMode();
    if (mode === "system")
      return env.systemTheme !== "light";
    return mode === "dark";
  };
  createEffect(() => dark(), (d) => setTheme(d ? litDarkTheme : litLightTheme));
  let nav = createFocusNav();
  return createComponent2(Window, mergeProps({
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
    }
  }, () => nav.handlers, {
    get children() {
      return createComponent2(SafeArea, {
        get children() {
          return createComponent2(Router, {
            router
          });
        }
      });
    }
  }));
}
render(() => createComponent2(App, {}));
