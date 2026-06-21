// ../../node_modules/.bun/@solidjs+signals@2.0.0-beta.14/node_modules/@solidjs/signals/dist/prod.js
class NotReadyError extends Error {
  source;
  constructor(e) {
    super();
    this.source = e;
  }
}

class StatusError extends Error {
  source;
  constructor(e, t) {
    super(t instanceof Error ? t.message : String(t), { cause: t });
    this.source = e;
  }
}
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
var SUPPORTS_PROXY = typeof Proxy === "function";
var defaultContext = {};
var $REFRESH = Symbol("refresh");
var diagnosticListeners = new Set;
var diagnosticCaptures = new Set;
function actualInsertIntoHeap(e, t) {
  const n = (e.i?.t ? e.i.u?.o : e.i?.o) ?? -1;
  if (n >= e.o)
    e.o = n + 1;
  const i = e.o;
  const r = t.l[i];
  if (r === undefined)
    t.l[i] = e;
  else {
    const t2 = r.S;
    t2.T = e;
    e.S = t2;
    r.S = e;
  }
  if (i > t._)
    t._ = i;
}
function insertIntoHeap(e, t) {
  let n = e.O;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
    return;
  if (n & REACTIVE_CHECK) {
    e.O = n & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else
    e.O = n | REACTIVE_IN_HEAP;
  if (!(n & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(e, t);
}
function insertIntoHeapHeight(e, t) {
  let n = e.O;
  if (n & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
    return;
  e.O = n | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(e, t);
}
function deleteFromHeap(e, t) {
  const n = e.O;
  if (!(n & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  e.O = n & -25;
  const i = e.o;
  if (e.S === e)
    t.l[i] = undefined;
  else {
    const n2 = e.T;
    const r = t.l[i];
    const o = n2 ?? r;
    if (e === r)
      t.l[i] = n2;
    else
      e.S.T = n2;
    o.S = e.S;
  }
  e.S = e;
  e.T = undefined;
}
function markHeap(e) {
  if (e.R)
    return;
  e.R = true;
  for (let t = 0;t <= e._; t++) {
    for (let n = e.l[t];n !== undefined; n = n.T) {
      if (n.O & REACTIVE_IN_HEAP)
        markNode(n);
    }
  }
}
function markNode(e, t = REACTIVE_DIRTY) {
  const n = e.O;
  if ((n & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= t)
    return;
  e.O = n & -4 | t;
  for (let t2 = e.I;t2 !== null; t2 = t2.p) {
    markNode(t2.h, REACTIVE_CHECK);
  }
  if (e.N !== null) {
    for (let t2 = e.N;t2 !== null; t2 = t2.A) {
      for (let e2 = t2.I;e2 !== null; e2 = e2.p) {
        markNode(e2.h, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(e, t) {
  e.R = false;
  for (e.C = 0;e.C <= e._; e.C++) {
    let n = e.l[e.C];
    while (n !== undefined) {
      if (n.O & REACTIVE_IN_HEAP)
        t(n);
      else
        adjustHeight(n, e);
      n = e.l[e.C];
    }
  }
  e._ = 0;
}
function adjustHeight(e, t) {
  deleteFromHeap(e, t);
  let n = e.o;
  for (let t2 = e.P;t2; t2 = t2.D) {
    const e2 = t2.m;
    const i = e2.V || e2;
    if (i.L && i.o >= n)
      n = i.o + 1;
  }
  if (e.o !== n) {
    e.o = n;
    for (let n2 = e.I;n2 !== null; n2 = n2.p) {
      insertIntoHeapHeight(n2.h, t);
    }
  }
}
var signalLanes = new WeakMap;
var activeLanes = new Set;
function getOrCreateLane(e) {
  let t = signalLanes.get(e);
  if (t) {
    return findLane(t);
  }
  const n = e.U;
  const i = n?.G ? findLane(n.G) : null;
  t = { k: e, F: new Set, W: [[], []], H: null, M: activeTransition, j: i };
  signalLanes.set(e, t);
  activeLanes.add(t);
  e.$ = false;
  return t;
}
function findLane(e) {
  while (e.H)
    e = e.H;
  return e;
}
function mergeLanes(e, t) {
  e = findLane(e);
  t = findLane(t);
  if (e === t)
    return e;
  t.H = e;
  for (const n of t.F)
    e.F.add(n);
  e.W[0].push(...t.W[0]);
  e.W[1].push(...t.W[1]);
  return e;
}
function resolveLane(e) {
  const t = e.G;
  if (!t)
    return;
  const n = findLane(t);
  if (activeLanes.has(n))
    return n;
  e.G = undefined;
  return;
}
function resolveTransition(e) {
  return resolveLane(e)?.M ?? e.M;
}
function hasActiveOverride(e) {
  return !!(e.K !== undefined && e.K !== NOT_PENDING);
}
function assignOrMergeLane(e, t) {
  const n = findLane(t);
  const i = e.G;
  if (i) {
    if (i.H) {
      e.G = t;
      return;
    }
    const r = findLane(i);
    if (activeLanes.has(r)) {
      if (r !== n && !hasActiveOverride(e)) {
        if (n.j && findLane(n.j) === r) {
          e.G = t;
        } else if (r.j && findLane(r.j) === n)
          ;
        else
          mergeLanes(n, r);
      }
      return;
    }
  }
  e.G = t;
}
var transitions = new Set;
var dirtyQueue = { l: new Array(2000).fill(undefined), R: false, C: 0, _: 0 };
var zombieQueue = { l: new Array(2000).fill(undefined), R: false, C: 0, _: 0 };
var clock = 0;
var activeTransition = null;
var scheduled = false;
var syncDepth = 0;
var projectionWriteActive = false;
var stashedOptimisticReads = null;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(e) {
  return transitions.size === 0 && activeLanes.size === 0 && e.Y.length === 0 && e.Z.length === 0 && e.q.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const e of transientStoreNodes) {
    if (e.I !== null) {
      transientStoreNodes.delete(e);
      continue;
    }
    if (e.B !== NOT_PENDING)
      continue;
    if (e.K !== undefined && e.K !== NOT_PENDING)
      continue;
    transientStoreNodes.delete(e);
    e.X?.();
  }
}
function shouldReadStashedOptimisticValue(e) {
  return !!stashedOptimisticReads?.has(e);
}
function runLaneEffects(e) {
  for (const t of activeLanes) {
    if (t.H || t.F.size > 0)
      continue;
    const n = t.W[e - 1];
    if (n.length) {
      t.W[e - 1] = [];
      runQueue(n, e);
    }
  }
}
function queueStashedOptimisticEffects(e) {
  for (let t = e.I;t !== null; t = t.p) {
    const e2 = t.h;
    if (!e2.J)
      continue;
    if (e2.J === EFFECT_TRACKED) {
      if (!e2.ee) {
        e2.ee = true;
        e2.te.enqueue(EFFECT_USER, e2.ne);
      }
      continue;
    }
    const n = e2.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (n.C > e2.o)
      n.C = e2.o;
    insertIntoHeap(e2, n);
  }
}
function mergeTransitionState(e, t) {
  t.ie = e;
  e.re.push(...t.re);
  for (const n of activeLanes)
    if (n.M === t)
      n.M = e;
  e.Z.push(...t.Z);
  for (const n of t.q)
    e.q.add(n);
  for (const [n, i] of t.oe) {
    let t2 = e.oe.get(n);
    if (!t2)
      e.oe.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
  for (const n of t.se)
    e.se.add(n);
}
function resolveOptimisticNodes(e) {
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    n.G = undefined;
    if (n.B !== NOT_PENDING) {
      n.ue = n.B;
      n.B = NOT_PENDING;
    }
    const i = n.K;
    n.K = NOT_PENDING;
    if (i !== NOT_PENDING && n.ue !== i)
      insertSubs(n, true);
    n.M = null;
  }
  e.length = 0;
}
function cleanupCompletedLanes(e) {
  for (const t of activeLanes) {
    const n = e ? t.M === e : !t.M;
    if (!n)
      continue;
    if (!t.H) {
      if (t.W[0].length)
        runQueue(t.W[0], EFFECT_RENDER);
      if (t.W[1].length)
        runQueue(t.W[1], EFFECT_USER);
    }
    if (t.k.G === t)
      t.k.G = undefined;
    t.F.clear();
    t.W[0].length = 0;
    t.W[1].length = 0;
    activeLanes.delete(t);
    signalLanes.delete(t.k);
  }
}
function schedule() {
  if (scheduled)
    return;
  scheduled = true;
  if (!syncDepth && !globalQueue.ce && !projectionWriteActive)
    queueMicrotask(flush);
}

class Queue {
  i = null;
  le = [[], []];
  Y = [];
  created = clock;
  addChild(e) {
    this.Y.push(e);
    e.i = this;
  }
  removeChild(e) {
    const t = this.Y.indexOf(e);
    if (t >= 0) {
      this.Y.splice(t, 1);
      e.i = null;
    }
  }
  notify(e, t, n, i) {
    if (this.i)
      return this.i.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.le[e - 1].length) {
      const t = this.le[e - 1];
      this.le[e - 1] = [];
      runQueue(t, e);
    }
    for (let t = 0;t < this.Y.length; t++)
      this.Y[t].run?.(e);
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane) {
        const n = findLane(currentOptimisticLane);
        n.W[e - 1].push(t);
      } else {
        this.le[e - 1].push(t);
      }
    }
    schedule();
  }
  stashQueues(e) {
    e.le[0].push(...this.le[0]);
    e.le[1].push(...this.le[1]);
    this.le = [[], []];
    for (let t = 0;t < this.Y.length; t++) {
      let n = this.Y[t];
      let i = e.Y[t];
      if (!i) {
        i = { le: [[], []], Y: [] };
        e.Y[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.le[0].push(...e.le[0]);
    this.le[1].push(...e.le[1]);
    for (let t = 0;t < e.Y.length; t++) {
      const n = e.Y[t];
      let i = this.Y[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue extends Queue {
  ce = false;
  ae = null;
  fe = [];
  Z = [];
  q = new Set;
  static Ee;
  static Se;
  static Te;
  static de = null;
  flush() {
    if (this.ce)
      return;
    this.ce = true;
    try {
      runHeap(dirtyQueue, GlobalQueue.Ee);
      if (activeTransition) {
        const e = transitionComplete(activeTransition);
        if (!e) {
          const e2 = activeTransition;
          runHeap(zombieQueue, GlobalQueue.Ee);
          this.ae = null;
          this.fe = [];
          this.Z = [];
          this.q = new Set;
          runLaneEffects(EFFECT_RENDER);
          runLaneEffects(EFFECT_USER);
          this.stashQueues(e2._e);
          clock++;
          scheduled = dirtyQueue._ >= dirtyQueue.C;
          reassignPendingTransition(e2.fe);
          activeTransition = null;
          if (!e2.re.length && !e2.oe.size && e2.Z.length) {
            stashedOptimisticReads = new Set;
            for (let t2 = 0;t2 < e2.Z.length; t2++) {
              const n = e2.Z[t2];
              if (n.L || n.Oe & CONFIG_OWNED_WRITE)
                continue;
              stashedOptimisticReads.add(n);
              queueStashedOptimisticEffects(n);
            }
          }
          try {
            finalizePureQueue(null, true);
          } finally {
            stashedOptimisticReads = null;
          }
          return;
        }
        this.fe !== activeTransition.fe && this.fe.push(...activeTransition.fe);
        this.restoreQueues(activeTransition._e);
        transitions.delete(activeTransition);
        const t = activeTransition;
        activeTransition = null;
        reassignPendingTransition(this.fe);
        finalizePureQueue(t);
      } else {
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue._ >= dirtyQueue.C) {
            runHeap(dirtyQueue, GlobalQueue.Ee);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue.Ee);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue._ >= dirtyQueue.C;
      activeLanes.size && runLaneEffects(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && runLaneEffects(EFFECT_USER);
      this.run(EFFECT_USER);
      if (false)
        ;
    } finally {
      this.ce = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING) {
      if (n & STATUS_PENDING) {
        const t2 = i !== undefined ? i : e.Re;
        if (activeTransition && t2) {
          const n2 = t2.source;
          let i2 = activeTransition.oe.get(n2);
          if (!i2)
            activeTransition.oe.set(n2, i2 = new Set);
          const r = i2.size;
          i2.add(e);
          if (i2.size !== r)
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
      activeTransition = e ?? {
        Ie: clock,
        fe: [],
        oe: new Map,
        Z: [],
        q: new Set,
        re: [],
        _e: { le: [[], []], Y: [] },
        ie: false,
        se: new Set
      };
    } else if (e) {
      const t = activeTransition;
      mergeTransitionState(e, t);
      transitions.delete(t);
      activeTransition = e;
    }
    transitions.add(activeTransition);
    activeTransition.Ie = clock;
    if (this.ae !== null) {
      this.ae.M = activeTransition;
      activeTransition.fe.push(this.ae);
      this.ae = null;
    }
    if (this.fe !== activeTransition.fe) {
      for (let e2 = 0;e2 < this.fe.length; e2++) {
        const t = this.fe[e2];
        t.M = activeTransition;
        activeTransition.fe.push(t);
      }
      this.fe = activeTransition.fe;
    }
    if (this.Z !== activeTransition.Z) {
      for (let e2 = 0;e2 < this.Z.length; e2++) {
        const t = this.Z[e2];
        t.M = activeTransition;
        activeTransition.Z.push(t);
      }
      this.Z = activeTransition.Z;
    }
    for (const e2 of activeLanes) {
      if (!e2.M)
        e2.M = activeTransition;
    }
    if (this.q !== activeTransition.q) {
      for (const e2 of this.q)
        activeTransition.q.add(e2);
      this.q = activeTransition.q;
    }
  }
}
function queuePendingNode(e) {
  if (activeTransition) {
    globalQueue.fe.push(e);
    return;
  }
  if (globalQueue.ae === null && globalQueue.fe.length === 0) {
    globalQueue.ae = e;
    return;
  }
  if (globalQueue.ae !== null) {
    globalQueue.fe.push(globalQueue.ae);
    globalQueue.ae = null;
  }
  globalQueue.fe.push(e);
}
function insertSubs(e, t = false) {
  const n = e.G || currentOptimisticLane;
  const i = e.pe !== undefined;
  for (let r = e.I;r !== null; r = r.p) {
    if (i && r.h.Oe & CONFIG_IN_SNAPSHOT_SCOPE) {
      r.h.O |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (t && n) {
      r.h.O |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(r.h, n);
    } else if (t) {
      r.h.O |= REACTIVE_OPTIMISTIC_DIRTY;
      r.h.G = undefined;
    }
    const e2 = r.h;
    if (e2.J === EFFECT_TRACKED) {
      if (!e2.ee) {
        e2.ee = true;
        e2.te.enqueue(EFFECT_USER, e2.ne);
      }
      continue;
    }
    const o = r.h.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (o.C > r.h.o)
      o.C = r.h.o;
    insertIntoHeap(r.h, o);
  }
}
function commitPendingNode(e) {
  const t = e;
  if (!t.L) {
    if (e.B !== NOT_PENDING) {
      e.ue = e.B;
      e.B = NOT_PENDING;
    }
    return;
  }
  if (e.B !== NOT_PENDING) {
    e.ue = e.B;
    e.B = NOT_PENDING;
    if (e.J && e.J !== EFFECT_TRACKED)
      e.ee = true;
  }
  t.O &= ~REACTIVE_MANUAL_WRITE;
  if (!(t.he & STATUS_PENDING))
    t.he &= ~STATUS_UNINITIALIZED;
  if (t.Ne !== null || t.Ae !== null)
    GlobalQueue.Se(t, false, true);
}
function commitPendingNodes() {
  if (globalQueue.ae !== null) {
    commitPendingNode(globalQueue.ae);
    globalQueue.ae = null;
  }
  const e = globalQueue.fe;
  for (let t = 0;t < e.length; t++) {
    commitPendingNode(e[t]);
  }
  e.length = 0;
}
function finalizePureQueue(e = null, t = false) {
  const n = !t;
  if (n)
    commitPendingNodes();
  if (!t && globalQueue.Y.length)
    checkBoundaryChildren(globalQueue);
  const i = dirtyQueue._ >= dirtyQueue.C;
  if (i)
    runHeap(dirtyQueue, GlobalQueue.Ee);
  if (n) {
    if (i)
      commitPendingNodes();
    resolveOptimisticNodes(e ? e.Z : globalQueue.Z);
    if (e && e.se.size) {
      for (const t3 of e.se) {
        if (t3.O & REACTIVE_DISPOSED)
          continue;
        if (t3.J === EFFECT_TRACKED) {
          if (!t3.ee) {
            t3.ee = true;
            t3.te.enqueue(EFFECT_USER, t3.ne);
          }
          continue;
        }
        const e2 = t3.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
        if (e2.C > t3.o)
          e2.C = t3.o;
        insertIntoHeap(t3, e2);
      }
      e.se.clear();
    }
    const t2 = e ? e.q : globalQueue.q;
    if (GlobalQueue.de && t2.size) {
      for (const e2 of t2) {
        GlobalQueue.de(e2);
      }
      t2.clear();
      schedule();
    }
    sweepTransientStoreNodes();
    cleanupCompletedLanes(e);
  }
}
function checkBoundaryChildren(e) {
  for (const t of e.Y) {
    t.checkSources?.();
    checkBoundaryChildren(t);
  }
}
function reassignPendingTransition(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].M = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
function flush(e) {
  if (e) {
    syncDepth++;
    try {
      return e();
    } finally {
      flush();
      syncDepth--;
    }
  }
  if (globalQueue.ce) {
    return;
  }
  while (scheduled || activeTransition) {
    globalQueue.flush();
  }
}
function runQueue(e, t) {
  for (let n = 0;n < e.length; n++)
    e[n](t);
}
function reporterBlocksSource(e, t) {
  if (e.O & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (e.Ce === t || e.Pe?.has(t))
    return true;
  for (let n = e.P;n; n = n.D) {
    let e2 = n.m;
    while (e2) {
      if (e2 === t || e2.V === t)
        return true;
      e2 = e2.U;
    }
  }
  return !!(e.he & STATUS_PENDING && e.Re instanceof NotReadyError && e.Re.source === t);
}
function transitionComplete(e) {
  if (e.ie)
    return true;
  if (e.re.length)
    return false;
  let t = true;
  for (const [n, i] of e.oe) {
    let r = false;
    for (const e2 of i) {
      if (reporterBlocksSource(e2, n)) {
        r = true;
        break;
      }
      i.delete(e2);
    }
    if (!r)
      e.oe.delete(n);
    else if (n.he & STATUS_PENDING && n.Re?.source === n) {
      t = false;
      break;
    }
  }
  if (t) {
    for (let n = 0;n < e.Z.length; n++) {
      const i = e.Z[n];
      if (hasActiveOverride(i) && "he" in i && i.he & STATUS_PENDING && i.Re instanceof NotReadyError && i.Re.source !== i) {
        t = false;
        break;
      }
    }
  }
  t && (e.ie = true);
  return t;
}
function currentTransition(e) {
  while (e.ie && typeof e.ie === "object")
    e = e.ie;
  return e;
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
function markDisposal(e) {
  let t = e.ge;
  while (t) {
    t.O |= REACTIVE_ZOMBIE;
    if (t.O & REACTIVE_IN_HEAP) {
      deleteFromHeap(t, dirtyQueue);
      insertIntoHeap(t, zombieQueue);
    }
    markDisposal(t);
    t = t.De;
  }
}
function disposeChildren(e, t = false, n) {
  const i = e.O;
  if (i & REACTIVE_DISPOSED)
    return;
  if (t)
    e.O = i | REACTIVE_DISPOSED;
  if (t && e.L)
    e.ve = null;
  let r = n ? e.Ne : e.ge;
  while (r) {
    const e2 = r.De;
    if (r.P) {
      const e3 = r;
      deleteFromHeap(e3, e3.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      let t2 = e3.P;
      do {
        t2 = unlinkSubs(t2);
      } while (t2 !== null);
      e3.P = null;
      e3.ye = null;
    }
    disposeChildren(r, true);
    r = e2;
  }
  if (n) {
    e.Ne = null;
  } else {
    e.ge = null;
    e.me = 0;
  }
  if (t && !n && !(i & REACTIVE_ZOMBIE) && e.i !== null && !(e.i.O & REACTIVE_DISPOSED)) {
    const t2 = e.we;
    const n2 = e.De;
    if (t2 !== null)
      t2.De = n2;
    else
      e.i.ge = n2;
    if (n2 !== null)
      n2.we = t2;
    e.we = null;
  }
  runDisposal(e, n);
}
function runDisposal(e, t) {
  let n = t ? e.Ae : e.be;
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
  t ? e.Ae = null : e.be = null;
}
function childId(e, t) {
  let n = e;
  while (n.Oe & CONFIG_TRANSPARENT && n.i)
    n = n.i;
  if (n.id != null)
    return formatId(n.id, t ? n.me++ : n.me);
  throw new Error("Cannot get child id from owner without an id");
}
function getNextChildId(e) {
  return childId(e, true);
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
  if (!context.be)
    context.be = e;
  else if (Array.isArray(context.be))
    context.be.push(e);
  else
    context.be = [context.be, e];
  return e;
}
function disposeRootSelf(e = true) {
  disposeChildren(this, e);
}
function createOwner(e) {
  const t = context;
  const n = e?.transparent ?? false;
  const i = {
    id: e?.id ?? (n ? t?.id : t?.id != null ? getNextChildId(t) : undefined),
    Oe: n ? CONFIG_TRANSPARENT : 0,
    t: true,
    u: t?.t ? t.u : t,
    ge: null,
    De: null,
    we: null,
    be: null,
    te: t?.te ?? globalQueue,
    Ve: t?.Ve || defaultContext,
    me: 0,
    Ae: null,
    Ne: null,
    i: t,
    dispose: disposeRootSelf
  };
  if (t) {
    const e2 = t.ge;
    if (e2 === null) {
      t.ge = i;
    } else {
      i.De = e2;
      e2.we = i;
      t.ge = i;
    }
  }
  return i;
}
function createRoot(e, t) {
  const n = createOwner(t);
  return runWithOwner(n, () => e(() => n.dispose()));
}
function unlinkSubs(e) {
  const t = e.m;
  const n = e.D;
  const i = e.p;
  const r = e.Le;
  if (i !== null)
    i.Le = r;
  else
    t.Ue = r;
  if (r !== null)
    r.p = i;
  else {
    t.I = i;
    if (i === null) {
      t.X?.();
      const e2 = t;
      e2.L && e2.Oe & CONFIG_AUTO_DISPOSE && !(e2.O & REACTIVE_ZOMBIE) && unobserved(e2);
    }
  }
  return n;
}
function trimStaleDeps(e) {
  const t = e.ye;
  let n = t !== null ? t.D : e.P;
  if (n !== null) {
    do {
      n = unlinkSubs(n);
    } while (n !== null);
    if (t !== null)
      t.D = null;
    else
      e.P = null;
  }
}
function unobserved(e) {
  deleteFromHeap(e, e.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  let t = e.P;
  while (t !== null) {
    t = unlinkSubs(t);
  }
  e.P = null;
  e.ye = null;
  disposeChildren(e, true);
}
function link(e, t) {
  const n = t.ye;
  if (n !== null && n.m === e)
    return;
  let i = null;
  const r = t.O & REACTIVE_RECOMPUTING_DEPS;
  if (r) {
    i = n !== null ? n.D : t.P;
    if (i !== null && i.m === e) {
      t.ye = i;
      return;
    }
  }
  const o = e.Ue;
  if (o !== null && o.h === t && (!r || isValidLink(o, t)))
    return;
  const s = t.ye = e.Ue = { m: e, h: t, D: i, Le: o, p: null };
  if (n !== null)
    n.D = s;
  else
    t.P = s;
  if (o !== null)
    o.p = s;
  else
    e.I = s;
}
function isValidLink(e, t) {
  const n = t.ye;
  if (n !== null) {
    let i = t.P;
    do {
      if (i === e)
        return true;
      if (i === n)
        break;
      i = i.D;
    } while (i !== null);
  }
  return false;
}
function addPendingSource(e, t) {
  if (e.Ce === t || e.Pe?.has(t))
    return false;
  if (!e.Ce) {
    e.Ce = t;
    return true;
  }
  if (!e.Pe) {
    e.Pe = new Set([e.Ce, t]);
  } else {
    e.Pe.add(t);
  }
  e.Ce = undefined;
  return true;
}
function removePendingSource(e, t) {
  if (e.Ce) {
    if (e.Ce !== t)
      return false;
    e.Ce = undefined;
    return true;
  }
  if (!e.Pe?.delete(t))
    return false;
  if (e.Pe.size === 1) {
    e.Ce = e.Pe.values().next().value;
    e.Pe = undefined;
  } else if (e.Pe.size === 0) {
    e.Pe = undefined;
  }
  return true;
}
function clearPendingSources(e) {
  e.Ce = undefined;
  e.Pe?.clear();
  e.Pe = undefined;
}
function setPendingError(e, t, n) {
  if (!t) {
    e.Re = null;
    return;
  }
  if (n instanceof NotReadyError && n.source === t) {
    e.Re = n;
    return;
  }
  const i = e.Re;
  if (!(i instanceof NotReadyError) || i.source !== t) {
    e.Re = new NotReadyError(t);
  }
}
function forEachDependent(e, t) {
  for (let n = e.I;n !== null; n = n.p)
    t(n.h);
  for (let n = e.N;n !== null; n = n.A) {
    for (let e2 = n.I;e2 !== null; e2 = e2.p)
      t(e2.h);
  }
}
function settlePendingSource(e) {
  let t = false;
  const n = new Set;
  const settle = (i) => {
    if (n.has(i) || !removePendingSource(i, e))
      return;
    n.add(i);
    i.Ie = clock;
    const r = i.Ce ?? i.Pe?.values().next().value;
    if (r) {
      setPendingError(i, r);
      updatePendingSignal(i);
    } else {
      i.he &= ~STATUS_PENDING;
      setPendingError(i);
      updatePendingSignal(i);
      if (i.Ge) {
        if (i.J === EFFECT_TRACKED) {
          const e2 = i;
          if (!e2.ee) {
            e2.ee = true;
            e2.te.enqueue(EFFECT_USER, e2.ne);
          }
        } else {
          const e2 = i.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
          if (e2.C > i.o)
            e2.C = i.o;
          insertIntoHeap(i, e2);
        }
        t = true;
      }
      i.Ge = false;
    }
    forEachDependent(i, settle);
  };
  forEachDependent(e, settle);
  if (t)
    schedule();
}
function handleAsync(e, t, n) {
  let i = false;
  let r = false;
  if (typeof t === "object" && t !== null) {
    untrack(() => {
      i = t[Symbol.asyncIterator];
      r = !i && typeof t.then === "function";
    });
  }
  if (!r && !i) {
    e.ve = null;
    return t;
  }
  e.ve = t;
  let o;
  const handleError = (n2) => {
    if (e.ve !== t)
      return;
    globalQueue.initTransition(resolveTransition(e));
    notifyStatus(e, n2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, n2);
    e.Ie = clock;
  };
  const asyncWrite = (i2, r2) => {
    if (e.ve !== t)
      return;
    if (e.O & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    globalQueue.initTransition(resolveTransition(e));
    const o2 = !!(e.he & STATUS_UNINITIALIZED);
    trimStaleDeps(e);
    clearStatus(e);
    const s = resolveLane(e);
    if (s)
      s.F.delete(e);
    if (n)
      n(i2);
    else if (e.K !== undefined) {
      if (e.K !== undefined && e.K !== NOT_PENDING)
        e.B = i2;
      else {
        e.ue = i2;
        insertSubs(e);
      }
      e.Ie = clock;
    } else if (s) {
      const t2 = e.J;
      const n2 = e.ue;
      const r3 = e.ke;
      if (!t2 && o2 || !r3 || !r3(i2, n2)) {
        e.ue = i2;
        e.Ie = clock;
        if (e.Fe) {
          setSignal(e.Fe, i2);
        }
        insertSubs(e, true);
      }
    } else {
      setSignal(e, () => i2);
    }
    settlePendingSource(e);
    schedule();
    flush();
    r2?.();
  };
  if (r) {
    let n2 = false, i2 = true;
    t.then((e2) => {
      if (i2) {
        o = e2;
        n2 = true;
      } else
        asyncWrite(e2);
    }, (e2) => {
      if (!i2)
        handleError(e2);
    });
    i2 = false;
    if (!n2) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  if (i) {
    const n2 = t[Symbol.asyncIterator]();
    let i2 = false;
    let r2 = false;
    cleanup(() => {
      if (r2)
        return;
      r2 = true;
      try {
        const e2 = n2.return?.();
        if (e2 && typeof e2.then === "function") {
          e2.then(undefined, () => {});
        }
      } catch {}
    });
    const iterate = () => {
      let s2, u = false, c = true;
      n2.next().then((n3) => {
        if (c) {
          s2 = n3;
          u = true;
          if (n3.done)
            r2 = true;
        } else if (e.ve !== t) {
          return;
        } else if (!n3.done)
          asyncWrite(n3.value, iterate);
        else {
          r2 = true;
          schedule();
          flush();
        }
      }, (n3) => {
        if (!c && e.ve === t) {
          r2 = true;
          handleError(n3);
        }
      });
      c = false;
      if (u && !s2.done) {
        o = s2.value;
        i2 = true;
        return iterate();
      }
      return u && s2.done;
    };
    const s = iterate();
    if (!i2 && !s) {
      globalQueue.initTransition(resolveTransition(e));
      throw new NotReadyError(context);
    }
  }
  return o;
}
function clearStatus(e, t = false) {
  if (e.Ce || e.Pe)
    clearPendingSources(e);
  if (e.Ge)
    e.Ge = false;
  e.he = t ? 0 : e.he & STATUS_UNINITIALIZED;
  if (e.Re)
    setPendingError(e);
  if (e.We)
    updatePendingSignal(e);
  if (e.xe)
    e.xe();
}
function notifyStatus(e, t, n, i, r) {
  if (t === STATUS_ERROR && !(n instanceof StatusError) && !(n instanceof NotReadyError))
    n = new StatusError(e, n);
  const o = t === STATUS_PENDING && n instanceof NotReadyError ? n.source : undefined;
  const s = o === e;
  const u = t === STATUS_PENDING && e.K !== undefined && !s;
  const c = u && hasActiveOverride(e);
  if (!i) {
    if (t === STATUS_PENDING && o) {
      addPendingSource(e, o);
      e.he = STATUS_PENDING | e.he & STATUS_UNINITIALIZED;
      setPendingError(e, o, n);
    } else {
      clearPendingSources(e);
      e.he = t | (t !== STATUS_ERROR ? e.he & STATUS_UNINITIALIZED : 0);
      e.Re = n;
    }
    updatePendingSignal(e);
  }
  if (r && !i) {
    assignOrMergeLane(e, r);
  }
  const l = i || c;
  const a = i || u ? undefined : r;
  if (e.xe) {
    if (i && t === STATUS_PENDING) {
      return;
    }
    if (l) {
      e.xe(t, n);
    } else {
      e.xe();
    }
    return;
  }
  forEachDependent(e, (e2) => {
    e2.Ie = clock;
    if (t === STATUS_PENDING && o && e2.Ce !== o && !e2.Pe?.has(o) || t !== STATUS_PENDING && (e2.Re !== n || e2.Ce || e2.Pe)) {
      if (!l && !e2.M)
        queuePendingNode(e2);
      notifyStatus(e2, t, n, l, a);
    }
  });
}
var externalSourceConfig = null;
GlobalQueue.Ee = recompute;
GlobalQueue.Se = disposeChildren;
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var foundPending = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var pendingCheckSources = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(e) {
  while (e) {
    if (e.He)
      return true;
    e = e.i;
  }
  return false;
}
function recompute(e, t = false) {
  const n = e.J;
  if (!t) {
    if (e.M && (!n || activeTransition) && activeTransition !== e.M)
      globalQueue.initTransition(e.M);
    deleteFromHeap(e, e.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    e.ve = null;
    if (e.M || n === EFFECT_TRACKED)
      disposeChildren(e);
    else if (e.ge !== null || e.be !== null) {
      markDisposal(e);
      e.Ae = e.be;
      e.Ne = e.ge;
      e.be = null;
      e.ge = null;
      e.me = 0;
    }
  }
  let i = !!(e.O & REACTIVE_OPTIMISTIC_DIRTY);
  const r = e.K !== undefined && e.K !== NOT_PENDING;
  const o = !!(e.he & STATUS_PENDING);
  const s = !!(e.he & STATUS_UNINITIALIZED);
  const u = context;
  context = e;
  e.ye = null;
  e.O = REACTIVE_RECOMPUTING_DEPS;
  e.Ie = clock;
  let c = e.B === NOT_PENDING ? e.ue : e.B;
  let l = e.o;
  let a = tracking;
  let f = currentOptimisticLane;
  tracking = true;
  if (i) {
    const t2 = resolveLane(e);
    if (t2)
      currentOptimisticLane = t2;
  } else if (activeTransition && !t && activeTransition.Z.length) {
    for (let t2 = e.P;t2; t2 = t2.D) {
      const n2 = t2.m;
      if (n2.O & REACTIVE_OPTIMISTIC_DIRTY) {
        const t3 = resolveLane(n2);
        if (t3) {
          i = true;
          currentOptimisticLane = t3;
          e.O |= REACTIVE_OPTIMISTIC_DIRTY;
          assignOrMergeLane(e, t3);
          break;
        }
      }
    }
  }
  const E = n && n !== EFFECT_USER;
  const S = stale;
  if (E)
    stale = true;
  try {
    if (e.Oe & CONFIG_SYNC) {
      c = e.L(c);
      e.ve = null;
    } else {
      const t2 = e.ve;
      const n2 = e.L(c);
      const i2 = typeof n2 === "object" && n2 !== null;
      const r2 = e.ve !== t2;
      c = r2 || !i2 ? n2 : handleAsync(e, n2);
      if (!r2 && !i2)
        e.ve = null;
    }
    clearStatus(e, t);
    if (e.G) {
      const t2 = resolveLane(e);
      if (t2) {
        t2.F.delete(e);
        updatePendingSignal(t2.k);
      }
    }
  } catch (t2) {
    if (t2 instanceof NotReadyError && currentOptimisticLane) {
      const t3 = findLane(currentOptimisticLane);
      if (t3.k !== e) {
        t3.F.add(e);
        e.G = t3;
        updatePendingSignal(t3.k);
      }
    }
    if (t2 instanceof NotReadyError)
      e.Ge = true;
    notifyStatus(e, t2 instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, t2, undefined, t2 instanceof NotReadyError ? e.G : undefined);
  } finally {
    tracking = a;
    if (E)
      stale = S;
    e.O = REACTIVE_NONE | (t ? e.O & REACTIVE_SNAPSHOT_STALE : 0);
    context = u;
  }
  if (!e.Re) {
    trimStaleDeps(e);
    const u2 = r ? e.K : e.B === NOT_PENDING ? e.ue : e.B;
    const a2 = !n && s || !e.ke || !e.ke(u2, c);
    if (n && a2) {
      e.ee = !e.Re;
      if (!t)
        e.te.enqueue(n, GlobalQueue.Te.bind(null, e));
    }
    if (a2) {
      const s2 = r ? e.K : undefined;
      if (t || n && activeTransition !== e.M || i) {
        e.ue = c;
        if (r && i) {
          e.K = c;
          e.B = c;
        }
      } else
        e.B = c;
      if (r && !i && o && !e.$)
        e.K = c;
      if (!r || i || e.K !== s2)
        insertSubs(e, i || r);
    } else if (r) {
      e.B = c;
    } else if (e.o != l) {
      for (let t2 = e.I;t2 !== null; t2 = t2.p) {
        insertIntoHeapHeight(t2.h, t2.h.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      }
    }
  }
  currentOptimisticLane = f;
  const T = e.B !== NOT_PENDING || e.Ne !== null || e.Ae !== null || !!(e.he & (STATUS_PENDING | STATUS_UNINITIALIZED));
  T && (!t || e.he & STATUS_PENDING) && !e.M && !(activeTransition && r) && queuePendingNode(e);
  e.M && n && activeTransition !== e.M && runInTransition(e.M, () => recompute(e));
}
function updateIfNecessary(e) {
  if (e.O & REACTIVE_CHECK) {
    for (let t = e.P;t; t = t.D) {
      const n = t.m;
      const i = n.V || n;
      if (i.L) {
        updateIfNecessary(i);
      }
      if (e.O & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (e.O & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || e.Re && e.Ie < clock && !e.ve) {
    recompute(e);
  }
  e.O = e.O & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(e, t) {
  const n = t?.transparent ?? false;
  const i = {
    id: t?.id ?? (n ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    Oe: (n ? CONFIG_TRANSPARENT : 0) | (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || t?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (t?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ke: t?.equals != null ? t.equals : isEqual,
    X: t?.unobserved,
    be: null,
    te: context?.te ?? globalQueue,
    Ve: context?.Ve ?? defaultContext,
    me: 0,
    L: e,
    ue: undefined,
    o: 0,
    N: null,
    T: undefined,
    S: null,
    P: null,
    ye: null,
    I: null,
    Ue: null,
    i: context,
    De: null,
    we: null,
    ge: null,
    O: t?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    he: STATUS_UNINITIALIZED,
    Ie: clock,
    B: NOT_PENDING,
    Ae: null,
    Ne: null,
    ve: null,
    M: null
  };
  setupComputedNode(i, t);
  return i;
}
function createEffectNode(e, t, n, i, r, o) {
  const s = o?.transparent ?? false;
  const u = {
    id: o?.id ?? (s ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    Oe: (s ? CONFIG_TRANSPARENT : 0) | (o?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (o?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    ke: false,
    X: o?.unobserved,
    be: null,
    te: context?.te ?? globalQueue,
    Ve: context?.Ve ?? defaultContext,
    me: 0,
    L: e,
    ue: undefined,
    o: 0,
    N: null,
    T: undefined,
    S: null,
    P: null,
    ye: null,
    I: null,
    Ue: null,
    i: context,
    De: null,
    we: null,
    ge: null,
    O: REACTIVE_LAZY,
    he: STATUS_UNINITIALIZED,
    Ie: clock,
    B: NOT_PENDING,
    Ae: null,
    Ne: null,
    ve: null,
    M: null,
    ee: false,
    Me: undefined,
    Qe: t,
    je: n,
    $e: undefined,
    Ke: false,
    J: i,
    xe: r
  };
  setupComputedNode(u, lazyOptions);
  return u;
}
var lazyOptions = { lazy: true };
function setupComputedNode(e, t) {
  e.S = e;
  const n = context?.t ? context.u : context;
  if (context) {
    const t2 = context.ge;
    if (t2 === null) {
      context.ge = e;
    } else {
      e.De = t2;
      t2.we = e;
      context.ge = e;
    }
  }
  if (n)
    e.o = n.o + 1;
  if (externalSourceConfig) {
    const t2 = signal(undefined, { equals: false, ownedWrite: true });
    const n2 = externalSourceConfig.factory(e.L, () => {
      setSignal(t2, undefined);
    });
    cleanup(() => n2.dispose());
    e.L = (e2) => {
      read(t2);
      return n2.track(e2);
    };
  }
  !t?.lazy && recompute(e, true);
  if (snapshotCaptureActive && !t?.lazy) {
    if (!(e.he & STATUS_PENDING)) {
      e.pe = e.ue === undefined ? NO_SNAPSHOT : e.ue;
      snapshotSources.add(e);
    }
  }
}
function signal(e, t, n = null) {
  const i = {
    ke: t?.equals != null ? t.equals : isEqual,
    Oe: (t?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (t?.Ye ? CONFIG_NO_SNAPSHOT : 0),
    X: t?.unobserved,
    ue: e,
    I: null,
    Ue: null,
    Ie: clock,
    V: n,
    A: n?.N || null,
    B: NOT_PENDING
  };
  n && (n.N = i);
  if (snapshotCaptureActive && !(i.Oe & CONFIG_NO_SNAPSHOT) && !((n?.he ?? 0) & STATUS_PENDING)) {
    i.pe = e === undefined ? NO_SNAPSHOT : e;
    snapshotSources.add(i);
  }
  return i;
}
function optimisticComputed(e, t) {
  const n = computed(e, t);
  n.K = NOT_PENDING;
  return n;
}
function isEqual(e, t) {
  return e === t;
}
function untrack(e, t) {
  if (!externalSourceConfig && !tracking && true)
    return e();
  const n = tracking;
  tracking = false;
  try {
    if (externalSourceConfig)
      return externalSourceConfig.untrack(e);
    return e();
  } finally {
    tracking = n;
  }
}
function read(e) {
  if (latestReadActive) {
    const t2 = getLatestValueComputed(e);
    const n2 = latestReadActive;
    latestReadActive = false;
    const i2 = e.K !== undefined && e.K !== NOT_PENDING ? e.K : e.ue;
    let r2;
    try {
      r2 = read(t2);
    } catch (e2) {
      if (!context && e2 instanceof NotReadyError)
        return i2;
      throw e2;
    } finally {
      latestReadActive = n2;
    }
    if (t2.he & STATUS_PENDING)
      return i2;
    if (stale && currentOptimisticLane && t2.G) {
      const e2 = findLane(t2.G);
      const n3 = findLane(currentOptimisticLane);
      if (e2 !== n3 && e2.F.size > 0) {
        return i2;
      }
    }
    return r2;
  }
  if (pendingCheckActive) {
    const t2 = e.V;
    const n2 = pendingCheckActive;
    pendingCheckActive = false;
    let i2 = context;
    if (i2?.t)
      i2 = i2.u;
    const r2 = t2 || e;
    const o = e;
    if (typeof o.L === "function") {
      const t3 = e;
      if (t3.O & REACTIVE_LAZY) {
        t3.O &= ~REACTIVE_LAZY;
        recompute(t3, true);
      } else if (t3.O & REACTIVE_DISPOSED) {
        recompute(t3, true);
      } else {
        updateIfNecessary(t3);
      }
    }
    if (i2 && r2.he & STATUS_PENDING && r2.he & STATUS_UNINITIALIZED) {
      if (tracking && e !== i2)
        link(e, i2);
      pendingCheckActive = n2;
      throw r2.Re;
    }
    if (t2 && e.K !== undefined) {
      if (e.K !== NOT_PENDING && (t2.ve || !!(t2.he & STATUS_PENDING))) {
        foundPending = true;
      }
      collectPendingSources(e);
      collectPendingSources(t2);
      if (i2 && tracking)
        link(e, i2);
    } else {
      collectPendingSources(e);
      if (t2)
        collectPendingSources(t2);
    }
    pendingCheckActive = n2;
  }
  let t = context;
  if (t?.t)
    t = t.u;
  const n = e;
  if (typeof n.L === "function") {
    const t2 = e;
    if (t2.O & REACTIVE_LAZY) {
      t2.O &= ~REACTIVE_LAZY;
      recompute(t2, true);
    } else if (t2.O & REACTIVE_DISPOSED) {
      recompute(t2, true);
    }
  }
  const i = e.V || e;
  if (!n.L && i === e && e.K === undefined && e.pe === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && true) {
    if (t && tracking)
      link(e, t);
    return !t || e.B === NOT_PENDING ? e.ue : e.B;
  }
  if (t && tracking) {
    link(e, t);
    if (i.L) {
      const n2 = e.O & REACTIVE_ZOMBIE;
      if (i.o >= (n2 ? zombieQueue.C : dirtyQueue.C)) {
        markNode(t);
        markHeap(n2 ? zombieQueue : dirtyQueue);
        updateIfNecessary(i);
      }
      const r2 = i.o;
      if (r2 >= t.o && e.i !== t) {
        t.o = r2 + 1;
      }
    }
  }
  if (i.he & STATUS_PENDING) {
    if (t && !(stale && i.M && activeTransition !== i.M)) {
      if (currentOptimisticLane) {
        const n2 = i.G;
        const r2 = findLane(currentOptimisticLane);
        if (n2 && findLane(n2) === r2 && !hasActiveOverride(i)) {
          if (!tracking && e !== t)
            link(e, t);
          throw i.Re;
        }
      } else {
        if (!tracking && e !== t)
          link(e, t);
        throw i.Re;
      }
    } else if (t && i !== e && i.he & STATUS_UNINITIALIZED) {
      if (!tracking && e !== t)
        link(e, t);
      throw i.Re;
    } else if (!t && i.he & STATUS_UNINITIALIZED) {
      throw i.Re;
    }
  }
  if (e.L && e.he & STATUS_ERROR) {
    if (e.Ie < clock) {
      recompute(e);
      return read(e);
    } else
      throw e.Re;
  }
  if (snapshotCaptureActive && t && t.Oe & CONFIG_IN_SNAPSHOT_SCOPE) {
    const n2 = e.pe;
    if (n2 !== undefined) {
      const i2 = n2 === NO_SNAPSHOT ? undefined : n2;
      const r2 = e.B !== NOT_PENDING ? e.B : e.ue;
      if (r2 !== i2)
        t.O |= REACTIVE_SNAPSHOT_STALE;
      return i2;
    }
  }
  if (e.K !== undefined && e.K !== NOT_PENDING) {
    if (t && stale && shouldReadStashedOptimisticValue(e))
      return e.ue;
    return e.K;
  }
  if (activeTransition !== null && currentOptimisticLane !== null && !latestReadActive && e.B !== NOT_PENDING && i === e && !e.L && t) {
    activeTransition.se.add(t);
    return e.ue;
  }
  const r = !t || currentOptimisticLane !== null && (e.K !== undefined || e.G || i === e && stale || !!(i.he & STATUS_PENDING)) || e.B === NOT_PENDING || stale && e.M && activeTransition !== e.M ? e.ue : e.B;
  if (!t && i === e && typeof n.L === "function" && e.Oe & CONFIG_AUTO_DISPOSE && !(i.he & STATUS_PENDING) && !e.I) {
    unobserved(e);
  }
  return r;
}
function setSignal(e, t) {
  if (e.M && activeTransition !== e.M)
    globalQueue.initTransition(e.M);
  const n = e.K !== undefined && !projectionWriteActive;
  const i = e.K !== undefined && e.K !== NOT_PENDING;
  const r = n ? i ? e.K : e.ue : e.B === NOT_PENDING ? e.ue : e.B;
  if (typeof t === "function")
    t = t(r);
  const o = !e.ke || !e.ke(r, t) || !!(e.he & STATUS_UNINITIALIZED);
  if (!o) {
    if (n && i && e.L) {
      insertSubs(e, true);
      schedule();
    }
    return t;
  }
  if (n) {
    const n2 = e.K === NOT_PENDING;
    if (!n2)
      globalQueue.initTransition(resolveTransition(e));
    if (n2) {
      e.B = e.ue;
      globalQueue.Z.push(e);
    }
    e.$ = true;
    const i2 = getOrCreateLane(e);
    e.G = i2;
    e.K = t;
  } else {
    if (e.B === NOT_PENDING)
      queuePendingNode(e);
    e.B = t;
  }
  if (e.We)
    updatePendingSignal(e);
  if (e.Fe) {
    setSignal(e.Fe, t);
  }
  e.Ie = clock;
  insertSubs(e, n);
  schedule();
  return t;
}
function suppressComputedRecompute(e) {
  deleteFromHeap(e, e.O & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  if (!(e.O & REACTIVE_MANUAL_WRITE) && e.B === NOT_PENDING)
    queuePendingNode(e);
  e.O = e.O & -4 | REACTIVE_MANUAL_WRITE;
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
function collectPendingSources(e) {
  pendingCheckSources?.add(e);
  const t = e.V || e;
  if (t !== e)
    pendingCheckSources?.add(t);
}
function computePendingState(e) {
  const t = e;
  const n = e.V;
  if (e.U) {
    const n2 = e.U;
    if (n2.he & STATUS_PENDING && !(n2.he & STATUS_UNINITIALIZED))
      return true;
    return e.B !== NOT_PENDING && !(t.he & STATUS_UNINITIALIZED);
  }
  if (n && e.B !== NOT_PENDING) {
    return !n.ve && !(n.he & STATUS_PENDING);
  }
  if (e.K !== undefined && e.K !== NOT_PENDING) {
    if (t.he & STATUS_PENDING && !(t.he & STATUS_UNINITIALIZED))
      return true;
    if (e.U) {
      const t2 = e.G ? findLane(e.G) : null;
      return !!(t2 && t2.F.size > 0);
    }
    return true;
  }
  if (e.K !== undefined && e.K === NOT_PENDING && !e.U) {
    return false;
  }
  if (e.B !== NOT_PENDING && !(t.he & STATUS_UNINITIALIZED))
    return true;
  return !!(t.he & STATUS_PENDING && !(t.he & STATUS_UNINITIALIZED));
}
function updatePendingSignal(e) {
  if (e.We) {
    const t = computePendingState(e);
    const n = e.We;
    setSignal(n, t);
    if (!t && n.G) {
      const t2 = resolveLane(e);
      if (t2 && t2.F.size > 0) {
        const e2 = findLane(n.G);
        if (e2 !== t2) {
          mergeLanes(t2, e2);
        }
      }
      signalLanes.delete(n);
      n.G = undefined;
    }
  }
}
function getLatestValueComputed(e) {
  if (!e.Fe) {
    const t = latestReadActive;
    latestReadActive = false;
    const n = pendingCheckActive;
    pendingCheckActive = false;
    const i = context;
    context = null;
    e.Fe = optimisticComputed(() => read(e));
    e.Fe.U = e;
    context = i;
    pendingCheckActive = n;
    latestReadActive = t;
  }
  return e.Fe;
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
function createContext(e, t) {
  return { id: Symbol(t), defaultValue: e };
}
function effect(e, t, n, i) {
  const r = !!i?.user;
  const o = createEffectNode(e, t, n, r ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, i);
  recompute(o, true);
  !i?.defer && (o.J === EFFECT_USER || i?.schedule ? o.te.enqueue(o.J, runEffect.bind(null, o)) : runEffect(o));
}
function notifyEffectStatus(e, t) {
  const n = e !== undefined ? e : this.he;
  const i = t !== undefined ? t : this.Re;
  if (n & STATUS_ERROR) {
    let e2 = i;
    this.te.notify(this, STATUS_PENDING, 0);
    if (this.J === EFFECT_USER) {
      try {
        return this.je ? this.je(e2, () => {
          this.$e?.();
          this.$e = undefined;
        }) : console.error(e2);
      } catch (t2) {
        e2 = t2;
      }
    }
    if (!this.te.notify(this, STATUS_ERROR, STATUS_ERROR))
      throw e2;
  } else if (this.J === EFFECT_RENDER) {
    this.te.notify(this, STATUS_PENDING | STATUS_ERROR, n, i);
  }
}
function runEffect(e) {
  if (!e.ee || e.O & REACTIVE_DISPOSED)
    return;
  e.$e?.();
  e.$e = undefined;
  try {
    const t = e.Qe(e.ue, e.Me);
    if (false)
      ;
    e.$e = t;
    if (e.$e && !e.Ke) {
      e.Ke = true;
      runWithOwner(e.i, () => cleanup(() => e.$e?.()));
    }
  } catch (t) {
    e.Re = new StatusError(e, t);
    e.he |= STATUS_ERROR;
    if (!e.te.notify(e, STATUS_ERROR, STATUS_ERROR))
      throw t;
  } finally {
    e.Me = e.ue;
    e.ee = false;
  }
}
GlobalQueue.Te = runEffect;
function trackedEffect(e, t) {
  const run = () => {
    if (!n.ee || n.O & REACTIVE_DISPOSED)
      return;
    try {
      n.ee = false;
      recompute(n);
    } finally {}
  };
  const n = computed(() => {
    n.$e?.();
    n.$e = undefined;
    const t2 = staleValues(e);
    n.$e = t2;
  }, { ...t, lazy: true });
  n.$e = undefined;
  n.Oe = n.Oe & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
  n.ee = true;
  n.J = EFFECT_TRACKED;
  n.xe = (e2, t2) => {
    const i = e2 !== undefined ? e2 : n.he;
    if (i & STATUS_ERROR) {
      n.te.notify(n, STATUS_PENDING, 0);
      const e3 = t2 !== undefined ? t2 : n.Re;
      if (!n.te.notify(n, STATUS_ERROR, STATUS_ERROR))
        throw e3;
    }
  };
  n.ne = run;
  n.te.enqueue(EFFECT_USER, run);
  cleanup(() => n.$e?.());
}
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
    n2.Oe &= ~CONFIG_AUTO_DISPOSE;
    return [accessor(n2), setMemo.bind(null, n2)];
  }
  const n = signal(e, t);
  return [accessor(n), setSignal.bind(null, n)];
}
function createMemo(e, t) {
  return accessor(computed(e, t));
}
function createEffect(e, t, n) {
  effect(e, t.effect || t, t.error, { user: true, ...n });
}
function createRenderEffect(e, t, n) {
  effect(e, t, undefined, n);
}
function createTrackedEffect(e, t) {
  trackedEffect(e, t);
}
function onSettled(e) {
  const t = getOwner();
  t && !(t.Oe & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(e), undefined) : globalQueue.enqueue(EFFECT_USER, () => {
    const t2 = e();
    t2?.();
  });
}
var $TRACK = Symbol(0);
var $TARGET = Symbol(0);
var $PROXY = Symbol(0);
var $DELETED = Symbol(0);
var storeLookup = new WeakMap;
function isWrappable(e) {
  if (e == null || typeof e !== "object" || Object.isFrozen(e))
    return false;
  return typeof Node === "undefined" || !(e instanceof Node);
}
var DELETE = Symbol(0);
function isPrototypePollutionKey(e) {
  return e === "__proto__" || e === "constructor" || e === "prototype";
}
function updatePath(e, t, n = 0) {
  let i, r = e;
  if (n < t.length - 1) {
    i = t[n];
    const o2 = typeof i;
    const s = Array.isArray(e);
    if (o2 === "string" && isPrototypePollutionKey(i))
      return;
    if (Array.isArray(i)) {
      for (let r2 = 0;r2 < i.length; r2++) {
        t[n] = i[r2];
        updatePath(e, t, n);
      }
      t[n] = i;
      return;
    } else if (s && o2 === "function") {
      for (let r2 = 0;r2 < e.length; r2++) {
        if (i(e[r2], r2)) {
          t[n] = r2;
          updatePath(e, t, n);
        }
      }
      t[n] = i;
      return;
    } else if (s && o2 === "object") {
      const { from: r2 = 0, to: o3 = e.length - 1, by: s2 = 1 } = i;
      for (let i2 = r2;i2 <= o3; i2 += s2) {
        t[n] = i2;
        updatePath(e, t, n);
      }
      t[n] = i;
      return;
    } else if (n < t.length - 2) {
      updatePath(e[i], t, n + 1);
      return;
    }
    r = e[i];
  }
  let o = t[t.length - 1];
  if (typeof o === "function") {
    o = o(r);
    if (o === r)
      return;
  }
  if (i === undefined && o == undefined)
    return;
  if (o === DELETE) {
    delete e[i];
  } else if (i === undefined || isWrappable(r) && isWrappable(o) && !Array.isArray(o)) {
    const t2 = i !== undefined ? e[i] : e;
    const n2 = Object.keys(o);
    for (let e2 = 0;e2 < n2.length; e2++) {
      const i2 = n2[e2];
      if (isPrototypePollutionKey(i2))
        continue;
      const r2 = Object.getOwnPropertyDescriptor(o, i2);
      if (r2.get || r2.set)
        Object.defineProperty(t2, i2, r2);
      else
        t2[i2] = r2.value;
    }
  } else {
    e[i] = o;
  }
}
var storePath = Object.assign(function storePath2(...e) {
  return (t) => {
    updatePath(t, e);
  };
}, { DELETE });
function trueFn() {
  return true;
}
var propTraps = {
  get(e, t, n) {
    if (t === $PROXY)
      return n;
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
  const n = [];
  for (let i2 = 0;i2 < e.length; i2++) {
    const r2 = e[i2];
    t = t || !!r2 && $PROXY in r2;
    const o2 = !!r2 && r2[$SOURCES];
    if (o2) {
      for (let e2 = 0;e2 < o2.length; e2++)
        n.push(o2[e2]);
    } else
      n.push(typeof r2 === "function" ? (t = true, createMemo(r2)) : r2);
  }
  if (SUPPORTS_PROXY && t) {
    return new Proxy({
      get(e2) {
        if (e2 === $SOURCES)
          return n;
        for (let t2 = n.length - 1;t2 >= 0; t2--) {
          const i2 = resolveSource(n[t2]);
          if (e2 in i2)
            return i2[e2];
        }
      },
      has(e2) {
        for (let t2 = n.length - 1;t2 >= 0; t2--) {
          if (e2 in resolveSource(n[t2]))
            return true;
        }
        return false;
      },
      keys() {
        const e2 = new Set;
        for (let t2 = 0;t2 < n.length; t2++) {
          const i2 = Object.keys(resolveSource(n[t2]));
          for (let t3 = 0;t3 < i2.length; t3++)
            e2.add(i2[t3]);
        }
        return [...e2];
      }
    }, propTraps);
  }
  const i = Object.create(null);
  let r = false;
  let o = n.length - 1;
  for (let e2 = o;e2 >= 0; e2--) {
    const t2 = n[e2];
    if (!t2) {
      e2 === o && o--;
      continue;
    }
    const s2 = Object.getOwnPropertyNames(t2);
    for (let n2 = s2.length - 1;n2 >= 0; n2--) {
      const u2 = s2[n2];
      if (u2 === "__proto__" || u2 === "constructor")
        continue;
      if (!i[u2]) {
        r = r || e2 !== o;
        const n3 = Object.getOwnPropertyDescriptor(t2, u2);
        i[u2] = n3.get ? { enumerable: true, configurable: true, get: n3.get.bind(t2) } : n3;
      }
    }
  }
  if (!r)
    return n[o];
  const s = {};
  const u = Object.keys(i);
  for (let e2 = u.length - 1;e2 >= 0; e2--) {
    const t2 = u[e2], n2 = i[t2];
    if (n2.get)
      Object.defineProperty(s, t2, n2);
    else
      s[t2] = n2.value;
  }
  s[$SOURCES] = n;
  return s;
}
function mapArray(e, t, n) {
  const i = typeof n?.keyed === "function" ? n.keyed : undefined;
  const r = t.length > 1;
  const o = t;
  const s = {
    Ze: createOwner(),
    qe: 0,
    Be: e,
    ze: [],
    Xe: o,
    Je: [],
    et: [],
    tt: i,
    nt: i || n?.keyed === false ? [] : undefined,
    it: r && n?.keyed !== false ? [] : undefined,
    rt: n?.keyed === false,
    ot: n?.fallback
  };
  const u = computed(updateKeyedMap.bind(s));
  s.Ze.u = u;
  u.Oe &= ~CONFIG_AUTO_DISPOSE;
  return accessor(u);
}
var pureOptions = { ownedWrite: true };
function updateKeyedMap() {
  const e = this.Be() || [], t = e.length;
  e[$TRACK];
  runWithOwner(this.Ze, () => {
    let n, i, r = this.nt ? this.rt ? () => {
      this.nt[i] = signal(e[i], pureOptions);
      return this.Xe(accessor(this.nt[i]), i);
    } : () => {
      this.nt[i] = signal(e[i], pureOptions);
      this.it && (this.it[i] = signal(i, pureOptions));
      return this.Xe(accessor(this.nt[i]), this.it ? accessor(this.it[i]) : undefined);
    } : this.it ? () => {
      const t2 = e[i];
      this.it[i] = signal(i, pureOptions);
      return this.Xe(t2, accessor(this.it[i]));
    } : () => {
      const t2 = e[i];
      return this.Xe(t2);
    };
    if (t === 0) {
      if (this.qe !== 0) {
        this.Ze.dispose(false);
        this.et = [];
        this.ze = [];
        this.Je = [];
        this.qe = 0;
        this.nt && (this.nt = []);
        this.it && (this.it = []);
      }
      if (this.ot && !this.Je[0]) {
        this.Je[0] = runWithOwner(this.et[0] = createOwner(), this.ot);
      }
    } else if (this.qe === 0) {
      if (this.et[0])
        this.et[0].dispose();
      this.Je = new Array(t);
      for (i = 0;i < t; i++) {
        this.ze[i] = e[i];
        this.Je[i] = runWithOwner(this.et[i] = createOwner(), r);
      }
      this.qe = t;
    } else {
      let o, s, u, c, l, a, f, E = new Array(t), S = new Array(t), T = this.nt ? new Array(t) : undefined, d = this.it ? new Array(t) : undefined;
      for (o = 0, s = Math.min(this.qe, t);o < s && (this.ze[o] === e[o] || this.nt && compare(this.tt, this.ze[o], e[o])); o++) {
        if (this.nt)
          setSignal(this.nt[o], e[o]);
      }
      for (s = this.qe - 1, u = t - 1;s >= o && u >= o && (this.ze[s] === e[u] || this.nt && compare(this.tt, this.ze[s], e[u])); s--, u--) {
        E[u] = this.Je[s];
        S[u] = this.et[s];
        T && (T[u] = this.nt[s]);
        d && (d[u] = this.it[s]);
      }
      a = new Map;
      f = new Array(u + 1);
      for (i = u;i >= o; i--) {
        c = e[i];
        l = this.tt ? this.tt(c) : c;
        n = a.get(l);
        f[i] = n === undefined ? -1 : n;
        a.set(l, i);
      }
      for (n = o;n <= s; n++) {
        c = this.ze[n];
        l = this.tt ? this.tt(c) : c;
        i = a.get(l);
        if (i !== undefined && i !== -1) {
          E[i] = this.Je[n];
          S[i] = this.et[n];
          T && (T[i] = this.nt[n]);
          d && (d[i] = this.it[n]);
          i = f[i];
          a.set(l, i);
        } else
          this.et[n].dispose();
      }
      for (i = o;i < t; i++) {
        if (i in E) {
          this.Je[i] = E[i];
          this.et[i] = S[i];
          if (T) {
            this.nt[i] = T[i];
            setSignal(this.nt[i], e[i]);
          }
          if (d) {
            this.it[i] = d[i];
            setSignal(this.it[i], i);
          }
        } else {
          this.Je[i] = runWithOwner(this.et[i] = createOwner(), r);
        }
      }
      this.Je = this.Je.slice(0, this.qe = t);
      this.ze = e.slice(0);
    }
  });
  return this.Je;
}
function compare(e, t, n) {
  return e ? e(t) === e(n) : true;
}
var ON_INIT = Symbol();
var RevealControllerContext = createContext(null);
var _revealUsed = false;
function isRevealController(e) {
  return e instanceof RevealController;
}
function isSlotReady(e) {
  return isRevealController(e) ? e.isReady() : e.ft.size === 0 && !e.Et;
}
function isSlotMinimallyReady(e) {
  return isRevealController(e) ? e.isMinimallyReady() : isSlotReady(e);
}
function setSlotState(e, t, n, i) {
  setSignal(e.St, n);
  setSignal(e.Tt, i);
  if (isRevealController(e)) {
    if (!n && e.dt === t)
      e.dt = undefined;
    return e.evaluate(n, i);
  }
  if (!n && e._t === t && e.Ot)
    e._t = undefined;
}

class RevealController {
  Rt;
  It;
  ht = [];
  dt;
  St = signal(false, { ownedWrite: true, Ye: true });
  Tt = signal(false, { ownedWrite: true, Ye: true });
  Nt = true;
  At = true;
  Ct = false;
  constructor(e, t) {
    this.Rt = e;
    this.It = t;
  }
  Pt(e) {
    for (let t = 0;t < this.ht.length; t++) {
      const n = this.ht[t];
      if ((isRevealController(n) ? n.dt : n._t) !== this)
        continue;
      if (e(n) === false)
        return false;
    }
    return true;
  }
  isReady() {
    return this.Pt(isSlotReady);
  }
  isMinimallyReady() {
    const e = untrack(this.Rt);
    if (e === "together")
      return this.isReady();
    if (e === "natural") {
      let e2 = false;
      let t2 = false;
      this.Pt((n) => {
        e2 = true;
        if (isSlotMinimallyReady(n)) {
          t2 = true;
          return false;
        }
      });
      return !e2 || t2;
    }
    let t = true;
    this.Pt((e2) => {
      t = isSlotMinimallyReady(e2);
      return false;
    });
    return t;
  }
  register(e) {
    if (this.ht.includes(e))
      return;
    this.ht.push(e);
    const t = untrack(this.Rt);
    setSignal(e.St, true), setSignal(e.Tt, t === "sequential" ? !!untrack(this.It) : false);
    untrack(() => this.evaluate());
  }
  unregister(e) {
    const t = this.ht.indexOf(e);
    if (t >= 0)
      this.ht.splice(t, 1);
    untrack(() => this.evaluate());
  }
  evaluate(e, t) {
    if (this.Ct)
      return;
    this.Ct = true;
    const n = this.Nt;
    const i = this.At;
    try {
      const n2 = e ?? read(this.St), i2 = untrack(this.Rt), r = i2 === "sequential" && !!untrack(this.It), o = t ?? r;
      if (n2) {
        this.Pt((e2) => setSlotState(e2, this, true, o));
      } else if (i2 === "natural") {
        this.Pt((e2) => {
          if (isRevealController(e2)) {
            setSignal(e2.Tt, false);
            setSignal(e2.St, false);
            e2.evaluate(false, false);
          } else {
            setSlotState(e2, this, !isSlotReady(e2), false);
          }
        });
      } else if (i2 === "together") {
        const e2 = this.Pt(isSlotMinimallyReady);
        this.Pt((t2) => setSlotState(t2, this, !e2, false));
      } else {
        let e2 = false;
        this.Pt((t2) => {
          if (e2)
            return setSlotState(t2, this, true, r);
          if (isSlotReady(t2))
            return setSlotState(t2, this, false, false);
          e2 = true;
          if (isRevealController(t2)) {
            setSignal(t2.Tt, false);
            setSignal(t2.St, false);
            t2.evaluate(false, false);
          } else {
            setSlotState(t2, this, true, false);
          }
        });
      }
    } finally {
      this.Nt = this.isReady();
      this.At = this.isMinimallyReady();
      this.Ct = false;
    }
    if (this.dt && (n !== this.Nt || i !== this.At))
      this.dt.evaluate();
  }
}

class CollectionQueue extends Queue {
  gt;
  ft = new Set;
  Dt;
  Et = true;
  St = signal(false, { ownedWrite: true, Ye: true });
  Re;
  Tt = signal(false, { ownedWrite: true, Ye: true });
  _t;
  Ot = false;
  yt;
  vt = ON_INIT;
  constructor(e) {
    super();
    this.gt = e;
  }
  run(e) {
    if (!e || read(this.St) && (!_revealUsed || read(this.Tt)))
      return;
    return super.run(e);
  }
  notify(e, t, n, i) {
    if (!(t & this.gt))
      return super.notify(e, t, n, i);
    if (this.Ot && this.yt) {
      const e2 = untrack(() => {
        try {
          return this.yt();
        } catch {
          return ON_INIT;
        }
      });
      if (e2 !== this.vt) {
        this.vt = e2;
        this.Ot = false;
        this.ft.clear();
      }
    }
    if (this.gt & STATUS_PENDING && this.Ot)
      return super.notify(e, t, n, i);
    if (this.gt & STATUS_PENDING && n & STATUS_ERROR) {
      return super.notify(e, STATUS_ERROR, n, i);
    }
    if (n & this.gt) {
      this.Et = true;
      const t2 = i?.source || e.Re?.source;
      if (t2) {
        const e2 = this.ft.size === 0;
        this.ft.add(t2);
        if (e2)
          setSignal(this.St, true);
        if (this.gt & STATUS_ERROR) {
          setSignal(this.Re, t2.Re?.cause ?? t2.Re);
        }
      }
    }
    t &= ~this.gt;
    return t ? super.notify(e, t, n, i) : true;
  }
  checkSources() {
    for (const e of this.ft) {
      if (e.O & REACTIVE_DISPOSED || !(e.he & this.gt) && !(this.gt & STATUS_ERROR && e.he & STATUS_PENDING))
        this.ft.delete(e);
    }
    if (!this.ft.size) {
      if (this.gt & STATUS_PENDING && this.Et && !this.Ot && this.Dt) {
        this.Et = !!(this.Dt.he & this.gt);
      } else {
        this.Et = false;
      }
      if (!this.Et) {
        setSignal(this.St, false);
        if (this.yt) {
          try {
            this.vt = untrack(() => this.yt());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this._t?.evaluate();
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
    let n = [];
    if (flattenArray(e, n, t)) {
      return () => {
        let e2 = [];
        flattenArray(n, e2, { ...t, doNotUnwrap: false });
        return e2;
      };
    }
    return n;
  }
  return e;
}
function flattenArray(e, t = [], n) {
  let i = null;
  let r = false;
  for (let o = 0;o < e.length; o++) {
    try {
      let i2 = e[o];
      if (typeof i2 === "function" && !i2.length) {
        if (n?.doNotUnwrap) {
          t.push(i2);
          r = true;
          continue;
        }
        do {
          i2 = i2();
        } while (typeof i2 === "function" && !i2.length);
      }
      if (Array.isArray(i2)) {
        r = flattenArray(i2, t, n);
      } else if (n?.skipNonRendered && (i2 == null || i2 === true || i2 === false || i2 === "")) {} else
        t.push(i2);
    } catch (e2) {
      if (!(e2 instanceof NotReadyError))
        throw e2;
      i = e2;
    }
  }
  if (i)
    throw i;
  return r;
}

// ../../node_modules/.bun/solid-js@2.0.0-beta.14/node_modules/solid-js/dist/solid.js
var IS_DEV = false;
var $DEVCOMP = Symbol(0);
var NoHydrateContext = {
  id: Symbol("NoHydrateContext"),
  defaultValue: false
};
var _createMemo;
var _createRenderEffect;
class MockPromise {
  static {
    for (const k of ["all", "allSettled", "any", "race", "reject", "resolve"]) {
      MockPromise[k] = () => new MockPromise;
    }
  }
  catch() {
    return new MockPromise;
  }
  then() {
    return new MockPromise;
  }
  finally() {
    return new MockPromise;
  }
}
var NO_HYDRATED_VALUE = Symbol("NO_HYDRATED_VALUE");
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

// ../../node_modules/.bun/@solidjs+universal@2.0.0-beta.14+4805d24c3c460789/node_modules/@solidjs/universal/dist/universal.js
var transparentOptions = {
  transparent: true,
  sync: true
};
var syncOptions = {
  sync: true
};
var effect2 = (fn, effectFn, options) => createRenderEffect2(fn, effectFn, options ? {
  transparent: true,
  sync: true,
  ...options
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
  setProperty,
  getParentNode,
  getFirstChild,
  getNextSibling
}) {
  function insert(parent, accessor2, marker, initial, options) {
    const multi = marker !== undefined;
    if (multi && !initial)
      initial = [];
    if (typeof accessor2 !== "function") {
      accessor2 = normalize(accessor2, multi, true);
      if (typeof accessor2 !== "function")
        return insertExpression(parent, accessor2, initial, marker);
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
      }, prev !== undefined && !(options && options.schedule) ? {
        ...options,
        schedule: true
      } : options);
      return INNER_OWNED;
    }, (value) => {
      if (value === INNER_OWNED)
        return;
      insertExpression(parent, value, current, marker);
      current = value;
    }, options);
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
      let disposer;
      try {
        createRoot((dispose) => {
          disposer = dispose;
          insert(element, code());
        });
      } catch (err) {
        if (disposer)
          disposer();
        throw err;
      }
      return disposer;
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
      let dispose;
      createRoot((d) => {
        dispose = d;
        const tree = code();
        baseInsert(element, () => tree, undefined, undefined, {
          schedule: true
        });
      });
      flush();
      return dispose;
    }
  };
}

// ../../packages/core/src/window.ts
import { on, once } from "srt:events";

// ../../packages/core/src/core.ts
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
    ffi.setTextInputActive(wantActive);
  }
}
function getFocusedNodeId() {
  return focusedNodeId;
}

// ../../packages/core/src/window.ts
var nextFrameId = 1;
var animationFrames = new Map;
var refreshRate = 60;
function onFrame(fn) {
  let frameId = null;
  let extendedFn = (tick, frame, rate) => {
    fn(tick, frame, rate);
    frameId = nextFrameId++;
    animationFrames.set(frameId, extendedFn);
    ffi.requestFrame();
  };
  frameId = nextFrameId++;
  animationFrames.set(frameId, extendedFn);
  ffi.requestFrame();
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
  let [size, setSize] = createSignal({ width: 0, height: 0 });
  let [safe, setSafe] = createSignal({ top: 0, left: 0, right: 0, bottom: 0 });
  let [scale, setScale] = createSignal(1);
  on("resize", (e) => {
    setSize({ width: e.width, height: e.height });
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
    ffi.renderFrame();
  }
  onSettled(() => {
    unsubRefreshRate = on("displayRefreshRate", ({ hz }) => {
      if (hz > 0)
        refreshRate = hz;
    });
    unsubscribe = on("render", ({ time, frame }) => {
      runFrame(time * 1000, frame);
    });
    unsubDown = on("pointerDown", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerDown")?.(e);
      }
      let focused = getFocusedNodeId();
      if (focused != null && !targets.includes(focused)) {
        setFocus(null);
      }
    });
    unsubUp = on("pointerUp", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerUp")?.(e);
      }
    });
    unsubMove = on("pointerMove", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerMove")?.(e);
      }
    });
    unsubEnter = on("pointerEnter", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerEnter")?.(e);
      }
    });
    unsubLeave = on("pointerLeave", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerLeave")?.(e);
      }
    });
    unsubWheel = on("wheel", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onWheel")?.(e);
      }
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
    unsubTextInput = on("textInput", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e);
      }
    });
    unsubKeyboardVisibility = on("keyboardVisibility", ({ shown }) => {
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

// ../../node_modules/.bun/colord@2.9.3/node_modules/colord/index.mjs
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

// ../../node_modules/.bun/colord@2.9.3/node_modules/colord/plugins/names.mjs
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

// ../../packages/core/src/color.ts
k([names_default]);
function parseColor(color) {
  let { r: r2, g: g2, b: b2, a: a2 } = w(color).toRgb();
  return ((r2 & 255) << 24 | (g2 & 255) << 16 | (b2 & 255) << 8 | a2 * 255 & 255) >>> 0;
}
function isGradient(value) {
  return typeof value === "object" && value !== null && "__gradient" in value;
}

// ../../packages/core/src/renderer.ts
var nodes = new Map;
var id = 1;
function createProxyNode(elementType) {
  let node = { id, elementType, children: [] };
  nodes.set(id, node);
  id += 1;
  return node;
}
var {
  effect: effect3,
  memo: memo2,
  createComponent: createComponent2,
  createElement,
  createTextNode,
  insertNode,
  insert,
  spread,
  setProp,
  mergeProps,
  applyRef,
  ref
} = createRenderer({
  createElement: (elementType) => {
    let proxy = createProxyNode(elementType);
    if (elementType === "window")
      ffi.createRoot(proxy.id);
    else
      ffi.createNode(proxy.id, elementType);
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode("d-span");
    ffi.createNode(proxy.id, "d-span");
    ffi.setProperty(proxy.id, "text", "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    ffi.setProperty(node.id, "text", "" + value);
  },
  isTextNode: (node) => node?.elementType === "d-span",
  setProperty: (node, name, value) => {
    if (!node)
      return;
    if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
      setEventHandler(node.id, name, value);
      return;
    }
    if (name === "color" && isGradient(value)) {
      ffi.setProperty(node.id, name, value);
      return;
    }
    if (name === "color" && typeof value === "string") {
      ffi.setProperty(node.id, name, parseColor(value));
      return;
    }
    ffi.setProperty(node.id, name, value);
  },
  insertNode: (parent, node, anchor) => {
    if (!node)
      return;
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
        ffi.insertNode(parent.id, node.id, anchor.id);
      else
        ffi.insertNode(parent.id, node.id);
    }
  },
  removeNode: (parent, node) => {
    if (!node || !parent)
      return;
    let index = parent.children.indexOf(node);
    if (index !== -1) {
      parent.children.splice(index, 1);
    }
    node.parent = undefined;
    ffi.deleteNode(parent.id, node.id);
    let cleanup2 = (n2) => {
      for (let child of n2.children)
        cleanup2(child);
      if (n2.id === getFocusedNodeId())
        setFocus(null);
      nodes.delete(n2.id);
      cleanupNodeHandlers(n2.id);
    };
    cleanup2(node);
  },
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
function render(code) {
  createRoot(() => {
    let root = code();
    if (!root || root.elementType !== "window") {
      throw new Error("render() root must be a <window> element");
    }
    attachWindow(root.id);
    insert(null, root);
  });
}
// ../../packages/core/src/gpu.ts
import * as gpu from "flux:gpu";
// ../../packages/core/src/camera.ts
import { listCameras, open } from "flux:camera";
import { on as on2 } from "srt:events";
var devicesAccessor;
function cameraDevices() {
  if (!devicesAccessor) {
    let [devices, setDevices] = createSignal(listCameras());
    on2("cameraDeviceChange", () => setDevices(listCameras()));
    devicesAccessor = devices;
  }
  return devicesAccessor();
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
  }).catch((e2) => setError(e2 instanceof Error ? e2 : new Error(String(e2))));
  onCleanup(() => {
    disposed = true;
    if (session) {
      session.close();
      session = undefined;
    }
  });
  return { texture, width, height, barcode, error };
}

// app.tsx
import { platform } from "flux:process";
import { on as on3 } from "srt:events";
import { available as devAvailable, canDiscover, connect, discover, stop, recents as initialRecents } from "srt:dev";

// logo.tsx
var EXPLODE_DIST = 3;
var STAGGER_DELAY = 100;
var ANIM_DURATION = 600;
var HOLD_ASSEMBLED = 5000;
var HOLD_EXPLODED = 0;
var SOLID_COLORS = {
  dark: "rgba(26,51,128)",
  mid: "rgba(51,102,179)",
  light: "rgba(102,153,230)"
};
var RT_COLORS = {
  dark: "rgba(100,100,100)",
  mid: "rgba(140,140,140)",
  light: "rgba(180,180,180)"
};
var M2 = 25;
var R = M2 * Math.SQRT2;
var T = -0.5 * R;
var sq = [[0, 0], [2 * M2, 0], [2 * M2, 2 * M2], [0, 2 * M2]];
var tri1 = [[0, 0], [2 * M2, 0], [0, 2 * M2]];
var tri2 = [[0, 0], [2 * R, 0], [0, 2 * R]];
var tri3 = [[0, 0], [4 * M2, 0], [0, 4 * M2]];
var par1 = [[0, 0], [2 * M2, 0], [4 * M2, 2 * M2], [2 * M2, 2 * M2]];
var par2 = [[2 * M2, 0], [4 * M2, 0], [2 * M2, 2 * M2], [0, 2 * M2]];
function shapeCenter(shape, rotate) {
  let radians = rotate * Math.PI / 4;
  let cos = Math.cos(radians);
  let sin = Math.sin(radians);
  let pts = shape.map(([x2, y2]) => [x2 * cos - y2 * sin, x2 * sin + y2 * cos]);
  let minX = Math.min(...pts.map(([x2]) => x2));
  let minY = Math.min(...pts.map(([, y2]) => y2));
  pts = pts.map(([x2, y2]) => [x2 - minX, y2 - minY]);
  let area = 0;
  let cx = 0;
  let cy = 0;
  for (let i2 = 0;i2 < pts.length; i2++) {
    let [x0, y0] = pts[i2];
    let [x1, y1] = pts[(i2 + 1) % pts.length];
    let cross = x0 * y1 - x1 * y0;
    area += cross;
    cx += (x0 + x1) * cross;
    cy += (y0 + y1) * cross;
  }
  area *= 0.5;
  cx /= 6 * area;
  cy /= 6 * area;
  return [cx, cy];
}
function path(shape, rotate) {
  let radians = rotate * Math.PI / 4;
  let cos = Math.cos(radians);
  let sin = Math.sin(radians);
  let rotated = shape.map(([x2, y2]) => [x2 * cos - y2 * sin, x2 * sin + y2 * cos]);
  let minX = Math.min(...rotated.map(([x2]) => x2));
  let minY = Math.min(...rotated.map(([, y2]) => y2));
  let d2 = "M" + rotated.map(([x2, y2]) => `${x2 - minX} ${y2 - minY}`).join("L") + "Z";
  return d2;
}
var letters = [
  {
    width: 5 * R - 0.5 * R,
    height: 6 * R,
    pieces: [{
      shape: tri1,
      x: R,
      y: 5 * R,
      rot: 1,
      shade: "light"
    }, {
      shape: sq,
      x: 0,
      y: 4 * R,
      rot: 1,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R,
      y: 4 * R,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: 3 * R,
      y: 2 * R,
      rot: 3,
      shade: "mid"
    }, {
      shape: tri3,
      x: R,
      y: 0,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri2,
      x: 3 * R,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: par2,
      x: 5 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "light"
    }]
  },
  {
    width: 4 * R + 2 * M2 - 0.5 * R,
    height: 2 * M2 + 4 * R,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: sq,
      x: 2 * R,
      y: 4 * R,
      rot: 0,
      shade: "light"
    }, {
      shape: tri3,
      x: 2 * R + 2 * M2,
      y: 0,
      rot: 3,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R - 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri1,
      x: 2 * R,
      y: 0,
      rot: 2,
      shade: "dark"
    }, {
      shape: par1,
      x: 2 * R + 2 * M2,
      y: 4 * R - 2 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri2,
      x: 2 * R - 2 * M2,
      y: 0,
      rot: 1,
      shade: "light"
    }]
  },
  {
    width: 4 * M2 + 2 * R,
    height: 4 * M2 + 4 * R,
    pieces: [{
      shape: sq,
      x: 2 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "light"
    }, {
      shape: tri1,
      x: 2 * R - 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: 2 * R - 2 * M2,
      y: 4 * R,
      rot: -2,
      shade: "mid"
    }, {
      shape: par1,
      x: 2 * R,
      y: 4 * R + 2 * M2,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri2,
      x: 4 * M2,
      y: 2 * R + 4 * M2,
      rot: 2,
      shade: "mid"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: R + 4 * M2,
      rot: 1,
      shade: "light"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: sq,
      x: 4 * M2,
      y: 0,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "light"
    }, {
      shape: par2,
      x: 2 * M2,
      y: 2 * M2,
      rot: -2,
      shade: "light"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 0,
      rot: -1,
      shade: "mid"
    }, {
      shape: tri3,
      x: 2 * M2,
      y: 4 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri1,
      x: 0,
      y: 6 * M2,
      rot: 4,
      shade: "mid"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 6 * M2,
      rot: 2,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 4 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: tri1,
      x: 2 * M2,
      y: 0,
      rot: 4,
      shade: "dark"
    }, {
      shape: par2,
      x: 4 * M2,
      y: 0,
      rot: 2,
      shade: "light"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 2 * M2,
      rot: -2,
      shade: "dark"
    }, {
      shape: sq,
      x: 4 * M2,
      y: 4 * M2,
      rot: 0,
      shade: "light"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 6 * M2,
      rot: -3,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 8 * M2,
    pieces: [{
      shape: tri3,
      x: 0,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: 0,
      y: 4 * M2,
      rot: 0,
      shade: "dark"
    }, {
      shape: tri2,
      x: 2 * M2,
      y: 0,
      rot: 1,
      shade: "dark"
    }, {
      shape: sq,
      x: 4 * M2 - R,
      y: 4 * M2,
      rot: 1,
      shade: "light"
    }, {
      shape: tri1,
      x: 0,
      y: 6 * M2,
      rot: 4,
      shade: "light"
    }, {
      shape: tri1,
      x: 4 * M2,
      y: 4 * M2 + R,
      rot: -1,
      shade: "mid"
    }, {
      shape: par2,
      x: 2 * M2,
      y: 2 * M2,
      rot: 0,
      shade: "mid"
    }]
  },
  {
    width: 6 * M2,
    height: 4 * M2 + 4 * R,
    pieces: [{
      shape: par1,
      x: T + 2 * R - 2 * M2,
      y: 0,
      rot: -2,
      shade: "light"
    }, {
      shape: tri1,
      x: T + 2 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "mid"
    }, {
      shape: tri3,
      x: T + 0,
      y: 2 * M2,
      rot: -1,
      shade: "dark"
    }, {
      shape: tri3,
      x: T + 2 * R - 2 * M2,
      y: 4 * R,
      rot: -2,
      shade: "mid"
    }, {
      shape: tri2,
      x: T + 2 * R,
      y: 2 * M2,
      rot: -3,
      shade: "light"
    }, {
      shape: tri1,
      x: T + 2 * R,
      y: 2 * M2,
      rot: -2,
      shade: "mid"
    }, {
      shape: sq,
      x: T + 2 * M2 + R,
      y: 4 * M2 + 2 * R,
      rot: 1,
      shade: "dark"
    }]
  }
];
function TangramLetter(props) {
  let [dist, setDist] = createSignal(EXPLODE_DIST);
  let letterCx = props.letter.width / 2;
  let letterCy = props.letter.height / 2;
  let pieceVectors = props.letter.pieces.map((p2) => {
    let [scx, scy] = shapeCenter(p2.shape, p2.rot);
    return [p2.x + scx - letterCx, p2.y + scy - letterCy];
  });
  let pieceSpins = props.letter.pieces.map((_, i2) => ((i2 * 7 + 3) % 11 - 5) * 30);
  onFrame((tick, frame) => {
    let cycleLen = ANIM_DURATION + HOLD_ASSEMBLED + ANIM_DURATION + HOLD_EXPLODED;
    let t2 = (tick - props.delay) % cycleLen;
    if (t2 < 0) {
      setDist(EXPLODE_DIST);
    } else if (t2 < ANIM_DURATION) {
      let p2 = t2 / ANIM_DURATION;
      let ease = p2 * p2 * (3 - 2 * p2);
      setDist((1 - ease) * EXPLODE_DIST);
    } else if (t2 < ANIM_DURATION + HOLD_ASSEMBLED) {
      setDist(0);
    } else if (t2 < 2 * ANIM_DURATION + HOLD_ASSEMBLED) {
      let p2 = (t2 - ANIM_DURATION - HOLD_ASSEMBLED) / ANIM_DURATION;
      let ease = p2 * p2 * (3 - 2 * p2);
      setDist(ease * EXPLODE_DIST);
    } else {
      setDist(EXPLODE_DIST);
    }
  });
  var _el$ = createElement("view");
  insert(_el$, () => props.letter.pieces.map((p2, i2) => (() => {
    var _el$2 = createElement("view"), _el$3 = createElement("d-path");
    insertNode(_el$2, _el$3);
    effect3(() => ({
      e: pieceVectors[i2][0] * dist(),
      t: pieceVectors[i2][1] * dist(),
      a: 1 + dist() * 0.5,
      o: pieceSpins[i2] * dist() / EXPLODE_DIST / 150,
      i: props.colors[p2.shade],
      n: p2.x,
      s: p2.y,
      h: path(p2.shape, p2.rot)
    }), ({
      e: e2,
      t: t2,
      a: a2,
      o: o2,
      i: i3,
      n: n2,
      s: s2,
      h: h2
    }, _p$) => {
      e2 !== _p$?.e && setProp(_el$2, "x", e2, _p$?.e);
      t2 !== _p$?.t && setProp(_el$2, "y", t2, _p$?.t);
      a2 !== _p$?.a && setProp(_el$2, "scale", a2, _p$?.a);
      o2 !== _p$?.o && setProp(_el$2, "rotate", o2, _p$?.o);
      i3 !== _p$?.i && setProp(_el$3, "color", i3, _p$?.i);
      n2 !== _p$?.n && setProp(_el$3, "x", n2, _p$?.n);
      s2 !== _p$?.s && setProp(_el$3, "y", s2, _p$?.s);
      h2 !== _p$?.h && setProp(_el$3, "d", h2, _p$?.h);
    });
    return _el$2;
  })()));
  effect3(() => ({
    e: props.letter.width,
    t: props.letter.height,
    a: props.letter.scale
  }), ({
    e: e2,
    t: t2,
    a: a2
  }, _p$) => {
    e2 !== _p$?.e && setProp(_el$, "width", e2, _p$?.e);
    t2 !== _p$?.t && setProp(_el$, "height", t2, _p$?.t);
    a2 !== _p$?.a && setProp(_el$, "scale", a2, _p$?.a);
  });
  return _el$;
}
var LOGO_HEIGHT = Math.max(...letters.map((l2) => l2.height));
function Logo() {
  let scale = () => windowSize().width * 1.12 / 1500;
  var _el$4 = createElement("view"), _el$5 = createElement("view");
  insertNode(_el$4, _el$5);
  setProp(_el$4, "justifyContent", "center");
  setProp(_el$4, "alignItems", "center");
  setProp(_el$4, "width", 1500);
  setProp(_el$5, "gap", 30);
  setProp(_el$5, "flexDirection", "row");
  setProp(_el$5, "alignItems", "flex-end");
  insert(_el$5, () => letters.map((letter, i2) => createComponent2(TangramLetter, {
    letter,
    colors: i2 < 5 ? SOLID_COLORS : RT_COLORS,
    delay: i2 * STAGGER_DELAY
  })));
  effect3(() => ({
    e: LOGO_HEIGHT * scale(),
    t: scale()
  }), ({
    e: e2,
    t: t2
  }, _p$) => {
    e2 !== _p$?.e && setProp(_el$4, "height", e2, _p$?.e);
    t2 !== _p$?.t && setProp(_el$4, "scale", t2, _p$?.t);
  });
  return _el$4;
}

// app.tsx
var LOOPBACK = "127.0.0.1:15194";
var STATUS_TEXT = {
  idle: "not connected",
  searching: "searching...",
  connecting: "connecting...",
  connected: "connected"
};
function normalizeAddress(raw) {
  return raw.trim().replace(/^(ws|http):\/\//, "").replace(/\/+$/, "");
}
function Button(props) {
  var _el$ = createElement("view"), _el$2 = createElement("d-rect"), _el$3 = createElement("text");
  insertNode(_el$, _el$2);
  insertNode(_el$, _el$3);
  setProp(_el$, "paddingLeft", 18);
  setProp(_el$, "paddingRight", 18);
  setProp(_el$, "paddingTop", 10);
  setProp(_el$, "paddingBottom", 10);
  setProp(_el$, "justifyContent", "center");
  setProp(_el$, "alignItems", "center");
  setProp(_el$2, "radius", 8);
  setProp(_el$3, "color", "white");
  insert(_el$3, () => props.label);
  effect3(() => ({
    e: props.onTap,
    t: props.color
  }), ({
    e: e2,
    t: t2
  }, _p$) => {
    e2 !== _p$?.e && setProp(_el$, "onPointerDown", e2, _p$?.e);
    t2 !== _p$?.t && setProp(_el$2, "color", t2, _p$?.t);
  });
  return _el$;
}
function CameraView(props) {
  let cam = createCamera(untrack(() => ({
    width: props.width,
    scan: props.scan
  })));
  createEffect(() => cam.barcode(), (b2) => {
    if (b2)
      props.onBarcode?.(b2);
  });
  createEffect(() => cam.error(), (e2) => {
    if (e2)
      props.onError?.(e2);
  });
  var _el$4 = createElement("texture");
  effect3(() => ({
    e: cam.texture(),
    t: props.width
  }), ({
    e: e2,
    t: t2
  }, _p$) => {
    e2 !== _p$?.e && setProp(_el$4, "src", e2, _p$?.e);
    t2 !== _p$?.t && setProp(_el$4, "width", t2, _p$?.t);
  });
  return _el$4;
}
function App() {
  let dev = devAvailable;
  let hasCamera = () => cameraDevices().length > 0;
  let isAndroid = platform === "android";
  let [state, setState] = createSignal("idle");
  let [address, setAddress] = createSignal(null);
  let [recents, setRecents] = createSignal(initialRecents);
  let [scanning, setScanning] = createSignal(false);
  let [scanError, setScanError] = createSignal(null);
  if (dev) {
    on3("dev", (e2) => {
      setState(e2.state);
      setAddress(e2.address);
      if (e2.recents) {
        setRecents(e2.recents);
        console.log("got recents", e2.recents);
      }
    });
  }
  let idle = () => state() === "idle";
  let busy = () => state() === "searching" || state() === "connecting";
  let connected = () => state() === "connected";
  let status = () => scanning() ? "scan the dev server QR code" : connected() ? `connected to ${address()}` : scanError() ?? STATUS_TEXT[state()];
  let startScan = () => {
    setScanError(null);
    setScanning(true);
  };
  let onScanned = (data) => {
    setScanning(false);
    connect(normalizeAddress(data));
  };
  var _el$5 = createElement("window"), _el$6 = createElement("d-rect"), _el$7 = createElement("view"), _el$8 = createElement("view"), _el$9 = createElement("text"), _el$0 = createElement("view");
  insertNode(_el$5, _el$6);
  insertNode(_el$5, _el$7);
  setProp(_el$5, "title", "solidrt-go");
  setProp(_el$6, "color", "#111");
  insertNode(_el$7, _el$8);
  setProp(_el$7, "flexGrow", 1);
  setProp(_el$7, "justifyContent", "center");
  setProp(_el$7, "alignItems", "center");
  setProp(_el$7, "flexDirection", "column-reverse");
  setProp(_el$7, "gap", 40);
  insertNode(_el$8, _el$9);
  insertNode(_el$8, _el$0);
  setProp(_el$8, "flexDirection", "column");
  setProp(_el$8, "alignItems", "center");
  setProp(_el$8, "gap", 16);
  insert(_el$8, createComponent2(Show, {
    get when() {
      return scanning();
    },
    get children() {
      return createComponent2(CameraView, {
        width: 280,
        scan: ["qr"],
        onBarcode: (r2) => onScanned(r2.data),
        onError: (e2) => {
          setScanError(`camera: ${e2.message}`);
          setScanning(false);
        }
      });
    }
  }), _el$9);
  setProp(_el$9, "color", "lightgrey");
  insert(_el$9, status);
  setProp(_el$0, "flexDirection", "row");
  setProp(_el$0, "gap", 12);
  insert(_el$0, (() => {
    var _c$ = memo2(() => !!(idle() && !scanning() && canDiscover));
    return () => _c$() && createComponent2(Button, {
      label: "Discover",
      color: "#3366b3",
      onTap: () => discover()
    });
  })(), null);
  insert(_el$0, (() => {
    var _c$2 = memo2(() => !!(idle() && !scanning() && dev && hasCamera()));
    return () => _c$2() && createComponent2(Button, {
      label: "Scan QR",
      color: "#3366b3",
      onTap: startScan
    });
  })(), null);
  insert(_el$0, (() => {
    var _c$3 = memo2(() => !!(idle() && !scanning() && isAndroid));
    return () => _c$3() && createComponent2(Button, {
      label: "Connect (adb)",
      color: "#3366b3",
      onTap: () => connect(LOOPBACK)
    });
  })(), null);
  insert(_el$0, (() => {
    var _c$4 = memo2(() => !!scanning());
    return () => _c$4() && createComponent2(Button, {
      label: "Cancel",
      color: "#555",
      onTap: () => setScanning(false)
    });
  })(), null);
  insert(_el$0, (() => {
    var _c$5 = memo2(() => !!busy());
    return () => _c$5() && createComponent2(Button, {
      label: "Cancel",
      color: "#555",
      onTap: () => stop()
    });
  })(), null);
  insert(_el$0, (() => {
    var _c$6 = memo2(() => !!connected());
    return () => _c$6() && createComponent2(Button, {
      label: "Disconnect",
      color: "#555",
      onTap: () => stop()
    });
  })(), null);
  insert(_el$8, createComponent2(Show, {
    get when() {
      return memo2(() => !!(idle() && !scanning()))() && recents().length > 0;
    },
    get children() {
      var _el$1 = createElement("view"), _el$10 = createElement("text");
      insertNode(_el$1, _el$10);
      setProp(_el$1, "flexDirection", "column");
      setProp(_el$1, "alignItems", "center");
      setProp(_el$1, "gap", 8);
      insertNode(_el$10, createTextNode(`recent`));
      setProp(_el$10, "color", "grey");
      insert(_el$1, createComponent2(For, {
        get each() {
          return recents();
        },
        children: (addr) => createComponent2(Button, {
          label: addr,
          color: "#333",
          onTap: () => connect(addr)
        })
      }), null);
      return _el$1;
    }
  }), null);
  insert(_el$7, createComponent2(Logo, {}), null);
  return _el$5;
}
render(() => createComponent2(App, {}));
