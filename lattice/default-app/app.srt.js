// ../../../node_modules/.bun/@solidjs+signals@2.0.0-beta.7/node_modules/@solidjs/signals/dist/dev.js
class NotReadyError extends Error {
  source;
  constructor(source) {
    super();
    this.source = source;
  }
}

class StatusError extends Error {
  source;
  constructor(source, original) {
    super(original instanceof Error ? original.message : String(original), { cause: original });
    this.source = source;
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
function actualInsertIntoHeap(n, heap) {
  const parentHeight = (n._parent?._root ? n._parent._parentComputed?._height : n._parent?._height) ?? -1;
  if (parentHeight >= n._height)
    n._height = parentHeight + 1;
  const height = n._height;
  const heapAtHeight = heap._heap[height];
  if (heapAtHeight === undefined)
    heap._heap[height] = n;
  else {
    const tail = heapAtHeight._prevHeap;
    tail._nextHeap = n;
    n._prevHeap = tail;
    heapAtHeight._prevHeap = n;
  }
  if (height > heap._max)
    heap._max = height;
}
function insertIntoHeap(n, heap) {
  let flags = n._flags;
  if (flags & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS))
    return;
  if (flags & REACTIVE_CHECK) {
    n._flags = flags & -4 | REACTIVE_DIRTY | REACTIVE_IN_HEAP;
  } else
    n._flags = flags | REACTIVE_IN_HEAP;
  if (!(flags & REACTIVE_IN_HEAP_HEIGHT))
    actualInsertIntoHeap(n, heap);
}
function insertIntoHeapHeight(n, heap) {
  let flags = n._flags;
  if (flags & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT))
    return;
  n._flags = flags | REACTIVE_IN_HEAP_HEIGHT;
  actualInsertIntoHeap(n, heap);
}
function deleteFromHeap(n, heap) {
  const flags = n._flags;
  if (!(flags & (REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT)))
    return;
  n._flags = flags & -25;
  const height = n._height;
  if (n._prevHeap === n)
    heap._heap[height] = undefined;
  else {
    const next = n._nextHeap;
    const dhh = heap._heap[height];
    const end = next ?? dhh;
    if (n === dhh)
      heap._heap[height] = next;
    else
      n._prevHeap._nextHeap = next;
    end._prevHeap = n._prevHeap;
  }
  n._prevHeap = n;
  n._nextHeap = undefined;
}
function markHeap(heap) {
  if (heap._marked)
    return;
  heap._marked = true;
  for (let i = 0;i <= heap._max; i++) {
    for (let el = heap._heap[i];el !== undefined; el = el._nextHeap) {
      if (el._flags & REACTIVE_IN_HEAP)
        markNode(el);
    }
  }
}
function markNode(el, newState = REACTIVE_DIRTY) {
  const flags = el._flags;
  if ((flags & (REACTIVE_CHECK | REACTIVE_DIRTY)) >= newState)
    return;
  el._flags = flags & -4 | newState;
  for (let link = el._subs;link !== null; link = link._nextSub) {
    markNode(link._sub, REACTIVE_CHECK);
  }
  if (el._child !== null) {
    for (let child = el._child;child !== null; child = child._nextChild) {
      for (let link = child._subs;link !== null; link = link._nextSub) {
        markNode(link._sub, REACTIVE_CHECK);
      }
    }
  }
}
function runHeap(heap, recompute) {
  heap._marked = false;
  for (heap._min = 0;heap._min <= heap._max; heap._min++) {
    let el = heap._heap[heap._min];
    while (el !== undefined) {
      if (el._flags & REACTIVE_IN_HEAP)
        recompute(el);
      else
        adjustHeight(el, heap);
      el = heap._heap[heap._min];
    }
  }
  heap._max = 0;
}
function adjustHeight(el, heap) {
  deleteFromHeap(el, heap);
  let newHeight = el._height;
  for (let d = el._deps;d; d = d._nextDep) {
    const dep1 = d._dep;
    const dep = dep1._firewall || dep1;
    if (dep._fn && dep._height >= newHeight)
      newHeight = dep._height + 1;
  }
  if (el._height !== newHeight) {
    el._height = newHeight;
    for (let s = el._subs;s !== null; s = s._nextSub) {
      insertIntoHeapHeight(s._sub, heap);
    }
  }
}
var hooks = {};
var diagnosticListeners = new Set;
var diagnosticCaptures = new Set;
var diagnosticSequence = 0;
var diagnostics = {
  subscribe(listener) {
    diagnosticListeners.add(listener);
    return () => diagnosticListeners.delete(listener);
  },
  capture() {
    const events = [];
    diagnosticCaptures.add(events);
    return {
      get events() {
        return events;
      },
      clear() {
        events.length = 0;
      },
      stop() {
        diagnosticCaptures.delete(events);
        return [...events];
      }
    };
  }
};
var DEV$1 = {
  hooks,
  diagnostics,
  getChildren,
  getSignals,
  getParent,
  getSources,
  getObservers
};
function emitDiagnostic(event) {
  const entry = { sequence: ++diagnosticSequence, ...event };
  for (const listener of diagnosticListeners)
    listener(entry);
  for (const capture of diagnosticCaptures)
    capture.push(entry);
  return entry;
}
function registerGraph(value, owner) {
  value._owner = owner;
  if (owner) {
    if (!owner._signals)
      owner._signals = [];
    owner._signals.push(value);
  }
  DEV$1.hooks.onGraph?.(value, owner);
}
function clearSignals(node) {
  node._signals = undefined;
}
function getChildren(owner) {
  const children = [];
  let child = owner._firstChild;
  while (child) {
    children.push(child);
    child = child._nextSibling;
  }
  return children;
}
function getSignals(owner) {
  return owner._signals ? [...owner._signals] : [];
}
function getParent(owner) {
  return owner._parent;
}
function getSources(computation) {
  const sources = [];
  let link = computation._deps;
  while (link) {
    sources.push(link._dep);
    link = link._nextDep;
  }
  return sources;
}
function getObservers(node) {
  const observers = [];
  let link = node._subs;
  while (link) {
    observers.push(link._sub);
    link = link._nextSub;
  }
  return observers;
}
var transitions = new Set;
var dirtyQueue = { _heap: new Array(2000).fill(undefined), _marked: false, _min: 0, _max: 0 };
var zombieQueue = { _heap: new Array(2000).fill(undefined), _marked: false, _min: 0, _max: 0 };
var clock = 0;
var activeTransition = null;
var scheduled = false;
var projectionWriteActive = false;
var inTrackedQueueCallback = false;
var _enforceLoadingBoundary = false;
var _hitUnhandledAsync = false;
var stashedOptimisticReads = null;
function resetUnhandledAsync() {
  _hitUnhandledAsync = false;
}
function shouldReadStashedOptimisticValue(node) {
  return !!stashedOptimisticReads?.has(node);
}
function runLaneEffects(type) {
  for (const lane of activeLanes) {
    if (lane._mergedInto || lane._pendingAsync.size > 0)
      continue;
    const effects = lane._effectQueues[type - 1];
    if (effects.length) {
      lane._effectQueues[type - 1] = [];
      runQueue(effects, type);
    }
  }
}
function queueStashedOptimisticEffects(node) {
  for (let s = node._subs;s !== null; s = s._nextSub) {
    const sub = s._sub;
    if (!sub._type)
      continue;
    if (sub._type === EFFECT_TRACKED) {
      if (!sub._modified) {
        sub._modified = true;
        sub._queue.enqueue(EFFECT_USER, sub._run);
      }
      continue;
    }
    const queue = sub._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (queue._min > sub._height)
      queue._min = sub._height;
    insertIntoHeap(sub, queue);
  }
}
function setTrackedQueueCallback(value) {
  inTrackedQueueCallback = value;
}
function mergeTransitionState(target, outgoing) {
  outgoing._done = target;
  target._actions.push(...outgoing._actions);
  for (const lane of activeLanes)
    if (lane._transition === outgoing)
      lane._transition = target;
  target._optimisticNodes.push(...outgoing._optimisticNodes);
  for (const store of outgoing._optimisticStores)
    target._optimisticStores.add(store);
  for (const [source, reporters] of outgoing._asyncReporters) {
    let targetReporters = target._asyncReporters.get(source);
    if (!targetReporters)
      target._asyncReporters.set(source, targetReporters = new Set);
    for (const reporter of reporters)
      targetReporters.add(reporter);
  }
}
function resolveOptimisticNodes(nodes) {
  for (let i = 0;i < nodes.length; i++) {
    const node = nodes[i];
    node._optimisticLane = undefined;
    if (node._pendingValue !== NOT_PENDING) {
      node._value = node._pendingValue;
      node._pendingValue = NOT_PENDING;
    }
    const prevOverride = node._overrideValue;
    node._overrideValue = NOT_PENDING;
    if (prevOverride !== NOT_PENDING && node._value !== prevOverride)
      insertSubs(node, true);
    node._transition = null;
  }
  nodes.length = 0;
}
function cleanupCompletedLanes(completingTransition) {
  for (const lane of activeLanes) {
    const owned = completingTransition ? lane._transition === completingTransition : !lane._transition;
    if (!owned)
      continue;
    if (!lane._mergedInto) {
      if (lane._effectQueues[0].length)
        runQueue(lane._effectQueues[0], EFFECT_RENDER);
      if (lane._effectQueues[1].length)
        runQueue(lane._effectQueues[1], EFFECT_USER);
    }
    if (lane._source._optimisticLane === lane)
      lane._source._optimisticLane = undefined;
    lane._pendingAsync.clear();
    lane._effectQueues[0].length = 0;
    lane._effectQueues[1].length = 0;
    activeLanes.delete(lane);
    signalLanes.delete(lane._source);
  }
}
function schedule() {
  if (scheduled)
    return;
  scheduled = true;
  if (!globalQueue._running && !projectionWriteActive)
    queueMicrotask(flush);
}

class Queue {
  _parent = null;
  _queues = [[], []];
  _children = [];
  created = clock;
  addChild(child) {
    this._children.push(child);
    child._parent = this;
  }
  removeChild(child) {
    const index = this._children.indexOf(child);
    if (index >= 0) {
      this._children.splice(index, 1);
      child._parent = null;
    }
  }
  notify(node, mask, flags, error) {
    if (this._parent)
      return this._parent.notify(node, mask, flags, error);
    return false;
  }
  run(type) {
    if (this._queues[type - 1].length) {
      const effects = this._queues[type - 1];
      this._queues[type - 1] = [];
      runQueue(effects, type);
    }
    for (let i = 0;i < this._children.length; i++)
      this._children[i].run?.(type);
  }
  enqueue(type, fn) {
    if (type) {
      if (currentOptimisticLane) {
        const lane = findLane(currentOptimisticLane);
        lane._effectQueues[type - 1].push(fn);
      } else {
        this._queues[type - 1].push(fn);
      }
    }
    schedule();
  }
  stashQueues(stub) {
    stub._queues[0].push(...this._queues[0]);
    stub._queues[1].push(...this._queues[1]);
    this._queues = [[], []];
    for (let i = 0;i < this._children.length; i++) {
      let child = this._children[i];
      let childStub = stub._children[i];
      if (!childStub) {
        childStub = { _queues: [[], []], _children: [] };
        stub._children[i] = childStub;
      }
      child.stashQueues(childStub);
    }
  }
  restoreQueues(stub) {
    this._queues[0].push(...stub._queues[0]);
    this._queues[1].push(...stub._queues[1]);
    for (let i = 0;i < stub._children.length; i++) {
      const childStub = stub._children[i];
      let child = this._children[i];
      if (child)
        child.restoreQueues(childStub);
    }
  }
}

class GlobalQueue extends Queue {
  _running = false;
  _pendingNodes = [];
  _optimisticNodes = [];
  _optimisticStores = new Set;
  static _update;
  static _dispose;
  static _clearOptimisticStore = null;
  flush() {
    if (this._running)
      return;
    this._running = true;
    try {
      runHeap(dirtyQueue, GlobalQueue._update);
      if (activeTransition) {
        const isComplete = transitionComplete(activeTransition);
        if (!isComplete) {
          const stashedTransition = activeTransition;
          runHeap(zombieQueue, GlobalQueue._update);
          this._pendingNodes = [];
          this._optimisticNodes = [];
          this._optimisticStores = new Set;
          runLaneEffects(EFFECT_RENDER);
          runLaneEffects(EFFECT_USER);
          this.stashQueues(stashedTransition._queueStash);
          clock++;
          scheduled = dirtyQueue._max >= dirtyQueue._min;
          reassignPendingTransition(stashedTransition._pendingNodes);
          activeTransition = null;
          if (!stashedTransition._actions.length && stashedTransition._optimisticNodes.length) {
            stashedOptimisticReads = new Set;
            for (let i = 0;i < stashedTransition._optimisticNodes.length; i++) {
              const node = stashedTransition._optimisticNodes[i];
              if (node._fn || node._ownedWrite)
                continue;
              stashedOptimisticReads.add(node);
              queueStashedOptimisticEffects(node);
            }
          }
          try {
            finalizePureQueue(null, true);
          } finally {
            stashedOptimisticReads = null;
          }
          return;
        }
        this._pendingNodes !== activeTransition._pendingNodes && this._pendingNodes.push(...activeTransition._pendingNodes);
        this.restoreQueues(activeTransition._queueStash);
        transitions.delete(activeTransition);
        const completingTransition = activeTransition;
        activeTransition = null;
        reassignPendingTransition(this._pendingNodes);
        finalizePureQueue(completingTransition);
      } else {
        if (transitions.size)
          runHeap(zombieQueue, GlobalQueue._update);
        finalizePureQueue();
      }
      clock++;
      scheduled = dirtyQueue._max >= dirtyQueue._min;
      runLaneEffects(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      runLaneEffects(EFFECT_USER);
      this.run(EFFECT_USER);
      if (true)
        DEV$1.hooks.onUpdate?.();
    } finally {
      this._running = false;
    }
  }
  notify(node, mask, flags, error) {
    if (mask & STATUS_PENDING) {
      if (flags & STATUS_PENDING) {
        const actualError = error !== undefined ? error : node._error;
        if (activeTransition && actualError) {
          const source = actualError.source;
          let reporters = activeTransition._asyncReporters.get(source);
          if (!reporters)
            activeTransition._asyncReporters.set(source, reporters = new Set);
          const prevSize = reporters.size;
          reporters.add(node);
          if (reporters.size !== prevSize)
            schedule();
        }
        if (_enforceLoadingBoundary)
          _hitUnhandledAsync = true;
      }
      return true;
    }
    return false;
  }
  initTransition(transition) {
    if (transition)
      transition = currentTransition(transition);
    if (transition && transition === activeTransition)
      return;
    if (!transition && activeTransition && activeTransition._time === clock)
      return;
    if (!activeTransition) {
      activeTransition = transition ?? {
        _time: clock,
        _pendingNodes: [],
        _asyncReporters: new Map,
        _optimisticNodes: [],
        _optimisticStores: new Set,
        _actions: [],
        _queueStash: { _queues: [[], []], _children: [] },
        _done: false
      };
    } else if (transition) {
      const outgoing = activeTransition;
      mergeTransitionState(transition, outgoing);
      transitions.delete(outgoing);
      activeTransition = transition;
    }
    transitions.add(activeTransition);
    activeTransition._time = clock;
    if (this._pendingNodes !== activeTransition._pendingNodes) {
      for (let i = 0;i < this._pendingNodes.length; i++) {
        const node = this._pendingNodes[i];
        node._transition = activeTransition;
        activeTransition._pendingNodes.push(node);
      }
      this._pendingNodes = activeTransition._pendingNodes;
    }
    if (this._optimisticNodes !== activeTransition._optimisticNodes) {
      for (let i = 0;i < this._optimisticNodes.length; i++) {
        const node = this._optimisticNodes[i];
        node._transition = activeTransition;
        activeTransition._optimisticNodes.push(node);
      }
      this._optimisticNodes = activeTransition._optimisticNodes;
    }
    for (const lane of activeLanes) {
      if (!lane._transition)
        lane._transition = activeTransition;
    }
    if (this._optimisticStores !== activeTransition._optimisticStores) {
      for (const store of this._optimisticStores)
        activeTransition._optimisticStores.add(store);
      this._optimisticStores = activeTransition._optimisticStores;
    }
  }
}
function insertSubs(node, optimistic = false) {
  const sourceLane = node._optimisticLane || currentOptimisticLane;
  const hasSnapshot = node._snapshotValue !== undefined;
  for (let s = node._subs;s !== null; s = s._nextSub) {
    if (hasSnapshot && s._sub._inSnapshotScope) {
      s._sub._flags |= REACTIVE_SNAPSHOT_STALE;
      continue;
    }
    if (optimistic && sourceLane) {
      s._sub._flags |= REACTIVE_OPTIMISTIC_DIRTY;
      assignOrMergeLane(s._sub, sourceLane);
    } else if (optimistic) {
      s._sub._flags |= REACTIVE_OPTIMISTIC_DIRTY;
      s._sub._optimisticLane = undefined;
    }
    const sub = s._sub;
    if (sub._type === EFFECT_TRACKED) {
      if (!sub._modified) {
        sub._modified = true;
        sub._queue.enqueue(EFFECT_USER, sub._run);
      }
      continue;
    }
    const queue = s._sub._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
    if (queue._min > s._sub._height)
      queue._min = s._sub._height;
    insertIntoHeap(s._sub, queue);
  }
}
function commitPendingNodes() {
  const pendingNodes = globalQueue._pendingNodes;
  for (let i = 0;i < pendingNodes.length; i++) {
    const n = pendingNodes[i];
    if (n._pendingValue !== NOT_PENDING) {
      n._value = n._pendingValue;
      n._pendingValue = NOT_PENDING;
      if (n._type && n._type !== EFFECT_TRACKED)
        n._modified = true;
    }
    if (!(n._statusFlags & STATUS_PENDING))
      n._statusFlags &= ~STATUS_UNINITIALIZED;
    if (n._fn)
      GlobalQueue._dispose(n, false, true);
  }
  pendingNodes.length = 0;
}
function finalizePureQueue(completingTransition = null, incomplete = false) {
  const resolvePending = !incomplete;
  if (resolvePending)
    commitPendingNodes();
  if (!incomplete)
    checkBoundaryChildren(globalQueue);
  if (dirtyQueue._max >= dirtyQueue._min)
    runHeap(dirtyQueue, GlobalQueue._update);
  if (resolvePending) {
    commitPendingNodes();
    resolveOptimisticNodes(completingTransition ? completingTransition._optimisticNodes : globalQueue._optimisticNodes);
    const optimisticStores = completingTransition ? completingTransition._optimisticStores : globalQueue._optimisticStores;
    if (GlobalQueue._clearOptimisticStore && optimisticStores.size) {
      for (const store of optimisticStores) {
        GlobalQueue._clearOptimisticStore(store);
      }
      optimisticStores.clear();
      schedule();
    }
    cleanupCompletedLanes(completingTransition);
  }
}
function checkBoundaryChildren(queue) {
  for (const child of queue._children) {
    child.checkSources?.();
    checkBoundaryChildren(child);
  }
}
function reassignPendingTransition(pendingNodes) {
  for (let i = 0;i < pendingNodes.length; i++) {
    pendingNodes[i]._transition = activeTransition;
  }
}
var globalQueue = new GlobalQueue;
function flush() {
  if (globalQueue._running) {
    if (inTrackedQueueCallback) {
      throw new Error("Cannot call flush() from inside onSettled or createTrackedEffect. flush() is not reentrant there.");
    }
    return;
  }
  let count = 0;
  while (scheduled || activeTransition) {
    if (++count === 1e5)
      throw new Error("Potential Infinite Loop Detected.");
    globalQueue.flush();
  }
}
function runQueue(queue, type) {
  for (let i = 0;i < queue.length; i++)
    queue[i](type);
}
function reporterBlocksSource(reporter, source) {
  if (reporter._flags & (REACTIVE_ZOMBIE | REACTIVE_DISPOSED))
    return false;
  if (reporter._pendingSource === source || reporter._pendingSources?.has(source))
    return true;
  for (let dep = reporter._deps;dep; dep = dep._nextDep) {
    let current = dep._dep;
    while (current) {
      if (current === source || current._firewall === source)
        return true;
      current = current._parentSource;
    }
  }
  return !!(reporter._statusFlags & STATUS_PENDING && reporter._error instanceof NotReadyError && reporter._error.source === source);
}
function transitionComplete(transition) {
  if (transition._done)
    return true;
  if (transition._actions.length)
    return false;
  let done = true;
  for (const [source, reporters] of transition._asyncReporters) {
    let hasLive = false;
    for (const reporter of reporters) {
      if (reporterBlocksSource(reporter, source)) {
        hasLive = true;
        break;
      }
      reporters.delete(reporter);
    }
    if (!hasLive)
      transition._asyncReporters.delete(source);
    else if (source._statusFlags & STATUS_PENDING && source._error?.source === source) {
      done = false;
      break;
    }
  }
  if (done) {
    for (let i = 0;i < transition._optimisticNodes.length; i++) {
      const node = transition._optimisticNodes[i];
      if (hasActiveOverride(node) && "_statusFlags" in node && node._statusFlags & STATUS_PENDING && node._error instanceof NotReadyError && node._error.source !== node) {
        done = false;
        break;
      }
    }
  }
  done && (transition._done = true);
  return done;
}
function currentTransition(transition) {
  while (transition._done && typeof transition._done === "object")
    transition = transition._done;
  return transition;
}
function runInTransition(transition, fn) {
  const prevTransition = activeTransition;
  try {
    activeTransition = currentTransition(transition);
    return fn();
  } finally {
    activeTransition = prevTransition;
  }
}
var signalLanes = new WeakMap;
var activeLanes = new Set;
function getOrCreateLane(signal) {
  let lane = signalLanes.get(signal);
  if (lane) {
    return findLane(lane);
  }
  const parentSource = signal._parentSource;
  const parentLane = parentSource?._optimisticLane ? findLane(parentSource._optimisticLane) : null;
  lane = {
    _source: signal,
    _pendingAsync: new Set,
    _effectQueues: [[], []],
    _mergedInto: null,
    _transition: activeTransition,
    _parentLane: parentLane
  };
  signalLanes.set(signal, lane);
  activeLanes.add(lane);
  signal._overrideSinceLane = false;
  return lane;
}
function findLane(lane) {
  while (lane._mergedInto)
    lane = lane._mergedInto;
  return lane;
}
function mergeLanes(lane1, lane2) {
  lane1 = findLane(lane1);
  lane2 = findLane(lane2);
  if (lane1 === lane2)
    return lane1;
  lane2._mergedInto = lane1;
  for (const node of lane2._pendingAsync)
    lane1._pendingAsync.add(node);
  lane1._effectQueues[0].push(...lane2._effectQueues[0]);
  lane1._effectQueues[1].push(...lane2._effectQueues[1]);
  return lane1;
}
function resolveLane(el) {
  const lane = el._optimisticLane;
  if (!lane)
    return;
  const root = findLane(lane);
  if (activeLanes.has(root))
    return root;
  el._optimisticLane = undefined;
  return;
}
function resolveTransition(el) {
  return resolveLane(el)?._transition ?? el._transition;
}
function hasActiveOverride(el) {
  return !!(el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING);
}
function assignOrMergeLane(el, sourceLane) {
  const sourceRoot = findLane(sourceLane);
  const existing = el._optimisticLane;
  if (existing) {
    if (existing._mergedInto) {
      el._optimisticLane = sourceLane;
      return;
    }
    const existingRoot = findLane(existing);
    if (activeLanes.has(existingRoot)) {
      if (existingRoot !== sourceRoot && !hasActiveOverride(el)) {
        if (sourceRoot._parentLane && findLane(sourceRoot._parentLane) === existingRoot) {
          el._optimisticLane = sourceLane;
        } else if (existingRoot._parentLane && findLane(existingRoot._parentLane) === sourceRoot)
          ;
        else
          mergeLanes(sourceRoot, existingRoot);
      }
      return;
    }
  }
  el._optimisticLane = sourceLane;
}
function unlinkSubs(link) {
  const dep = link._dep;
  const nextDep = link._nextDep;
  const nextSub = link._nextSub;
  const prevSub = link._prevSub;
  if (nextSub !== null)
    nextSub._prevSub = prevSub;
  else
    dep._subsTail = prevSub;
  if (prevSub !== null)
    prevSub._nextSub = nextSub;
  else {
    dep._subs = nextSub;
    if (nextSub === null) {
      dep._unobserved?.();
      dep._fn && !dep._preventAutoDisposal && !(dep._flags & REACTIVE_ZOMBIE) && unobserved(dep);
    }
  }
  return nextDep;
}
function unobserved(el) {
  deleteFromHeap(el, el._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  let dep = el._deps;
  while (dep !== null) {
    dep = unlinkSubs(dep);
  }
  el._deps = null;
  el._depsTail = null;
  disposeChildren(el, true);
}
function link(dep, sub) {
  const prevDep = sub._depsTail;
  if (prevDep !== null && prevDep._dep === dep)
    return;
  let nextDep = null;
  const isRecomputing = sub._flags & REACTIVE_RECOMPUTING_DEPS;
  if (isRecomputing) {
    nextDep = prevDep !== null ? prevDep._nextDep : sub._deps;
    if (nextDep !== null && nextDep._dep === dep) {
      sub._depsTail = nextDep;
      return;
    }
  }
  const prevSub = dep._subsTail;
  if (prevSub !== null && prevSub._sub === sub && (!isRecomputing || isValidLink(prevSub, sub)))
    return;
  const newLink = sub._depsTail = dep._subsTail = { _dep: dep, _sub: sub, _nextDep: nextDep, _prevSub: prevSub, _nextSub: null };
  if (prevDep !== null)
    prevDep._nextDep = newLink;
  else
    sub._deps = newLink;
  if (prevSub !== null)
    prevSub._nextSub = newLink;
  else
    dep._subs = newLink;
}
function isValidLink(checkLink, sub) {
  const depsTail = sub._depsTail;
  if (depsTail !== null) {
    let link2 = sub._deps;
    do {
      if (link2 === checkLink)
        return true;
      if (link2 === depsTail)
        break;
      link2 = link2._nextDep;
    } while (link2 !== null);
  }
  return false;
}
function markDisposal(el) {
  let child = el._firstChild;
  while (child) {
    child._flags |= REACTIVE_ZOMBIE;
    if (child._flags & REACTIVE_IN_HEAP) {
      deleteFromHeap(child, dirtyQueue);
      insertIntoHeap(child, zombieQueue);
    }
    markDisposal(child);
    child = child._nextSibling;
  }
}
function disposeChildren(node, self = false, zombie) {
  if (node._flags & REACTIVE_DISPOSED)
    return;
  if (self)
    node._flags = REACTIVE_DISPOSED;
  if (self && true)
    clearSignals(node);
  if (self && node._fn)
    node._inFlight = null;
  let child = zombie ? node._pendingFirstChild : node._firstChild;
  while (child) {
    const nextChild = child._nextSibling;
    if (child._deps) {
      const n = child;
      deleteFromHeap(n, n._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      let toRemove = n._deps;
      do {
        toRemove = unlinkSubs(toRemove);
      } while (toRemove !== null);
      n._deps = null;
      n._depsTail = null;
    }
    disposeChildren(child, true);
    child = nextChild;
  }
  if (zombie) {
    node._pendingFirstChild = null;
  } else {
    node._firstChild = null;
    node._childCount = 0;
  }
  runDisposal(node, zombie);
}
function runDisposal(node, zombie) {
  let disposal = zombie ? node._pendingDisposal : node._disposal;
  if (!disposal)
    return;
  if (Array.isArray(disposal)) {
    for (let i = 0;i < disposal.length; i++) {
      const callable = disposal[i];
      callable.call(callable);
    }
  } else {
    disposal.call(disposal);
  }
  zombie ? node._pendingDisposal = null : node._disposal = null;
}
function childId(owner, consume) {
  let counter = owner;
  while (counter._transparent && counter._parent)
    counter = counter._parent;
  if (counter.id != null)
    return formatId(counter.id, consume ? counter._childCount++ : counter._childCount);
  throw new Error("Cannot get child id from owner without an id");
}
function getNextChildId(owner) {
  return childId(owner, true);
}
function formatId(prefix, id) {
  const num = id.toString(36), len = num.length - 1;
  return prefix + (len ? String.fromCharCode(64 + len) : "") + num;
}
function getOwner() {
  return context;
}
function cleanup(fn) {
  if (!context)
    return fn;
  if (!context._disposal)
    context._disposal = fn;
  else if (Array.isArray(context._disposal))
    context._disposal.push(fn);
  else
    context._disposal = [context._disposal, fn];
  return fn;
}
function createOwner(options) {
  const parent = context;
  const transparent = options?.transparent ?? false;
  const owner = {
    id: options?.id ?? (transparent ? parent?.id : parent?.id != null ? getNextChildId(parent) : undefined),
    _transparent: transparent || undefined,
    _root: true,
    _parentComputed: parent?._root ? parent._parentComputed : parent,
    _firstChild: null,
    _nextSibling: null,
    _disposal: null,
    _queue: parent?._queue ?? globalQueue,
    _context: parent?._context || defaultContext,
    _childCount: 0,
    _pendingDisposal: null,
    _pendingFirstChild: null,
    _parent: parent,
    dispose(self = true) {
      disposeChildren(owner, self);
    }
  };
  if (parent?._childrenForbidden) {
    throw new Error("Cannot create reactive primitives inside createTrackedEffect or owner-backed onSettled");
  }
  if (parent) {
    const lastChild = parent._firstChild;
    if (lastChild === null) {
      parent._firstChild = owner;
    } else {
      owner._nextSibling = lastChild;
      parent._firstChild = owner;
    }
  }
  DEV$1.hooks.onOwner?.(owner);
  return owner;
}
function createRoot(init, options) {
  const owner = createOwner(options);
  return runWithOwner(owner, () => init(owner.dispose));
}
function addPendingSource(el, source) {
  if (el._pendingSource === source || el._pendingSources?.has(source))
    return false;
  if (!el._pendingSource) {
    el._pendingSource = source;
    return true;
  }
  if (!el._pendingSources) {
    el._pendingSources = new Set([el._pendingSource, source]);
  } else {
    el._pendingSources.add(source);
  }
  el._pendingSource = undefined;
  return true;
}
function removePendingSource(el, source) {
  if (el._pendingSource) {
    if (el._pendingSource !== source)
      return false;
    el._pendingSource = undefined;
    return true;
  }
  if (!el._pendingSources?.delete(source))
    return false;
  if (el._pendingSources.size === 1) {
    el._pendingSource = el._pendingSources.values().next().value;
    el._pendingSources = undefined;
  } else if (el._pendingSources.size === 0) {
    el._pendingSources = undefined;
  }
  return true;
}
function clearPendingSources(el) {
  el._pendingSource = undefined;
  el._pendingSources?.clear();
  el._pendingSources = undefined;
}
function setPendingError(el, source, error) {
  if (!source) {
    el._error = null;
    return;
  }
  if (error instanceof NotReadyError && error.source === source) {
    el._error = error;
    return;
  }
  const current = el._error;
  if (!(current instanceof NotReadyError) || current.source !== source) {
    el._error = new NotReadyError(source);
  }
}
function forEachDependent(el, fn) {
  for (let s = el._subs;s !== null; s = s._nextSub)
    fn(s._sub);
  for (let child = el._child;child !== null; child = child._nextChild) {
    for (let s = child._subs;s !== null; s = s._nextSub)
      fn(s._sub);
  }
}
function settlePendingSource(el) {
  let scheduled2 = false;
  const visited = new Set;
  const settle = (node) => {
    if (visited.has(node) || !removePendingSource(node, el))
      return;
    visited.add(node);
    node._time = clock;
    const source = node._pendingSource ?? node._pendingSources?.values().next().value;
    if (source) {
      setPendingError(node, source);
      updatePendingSignal(node);
    } else {
      node._statusFlags &= ~STATUS_PENDING;
      setPendingError(node);
      updatePendingSignal(node);
      if (node._blocked) {
        if (node._type === EFFECT_TRACKED) {
          const tracked = node;
          if (!tracked._modified) {
            tracked._modified = true;
            tracked._queue.enqueue(EFFECT_USER, tracked._run);
          }
        } else {
          const queue = node._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue;
          if (queue._min > node._height)
            queue._min = node._height;
          insertIntoHeap(node, queue);
        }
        scheduled2 = true;
      }
      node._blocked = false;
    }
    forEachDependent(node, settle);
  };
  forEachDependent(el, settle);
  if (scheduled2)
    schedule();
}
function handleAsync(el, result, setter) {
  const isObject = typeof result === "object" && result !== null;
  const iterator = isObject && untrack(() => result[Symbol.asyncIterator]);
  const isThenable = !iterator && isObject && untrack(() => typeof result.then === "function");
  if (!isThenable && !iterator) {
    el._inFlight = null;
    return result;
  }
  el._inFlight = result;
  let syncValue;
  const handleError = (error) => {
    if (el._inFlight !== result)
      return;
    globalQueue.initTransition(resolveTransition(el));
    notifyStatus(el, error instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, error);
    el._time = clock;
  };
  const asyncWrite = (value, then) => {
    if (el._inFlight !== result)
      return;
    if (el._flags & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY))
      return;
    globalQueue.initTransition(resolveTransition(el));
    const wasUninitialized = !!(el._statusFlags & STATUS_UNINITIALIZED);
    clearStatus(el);
    const lane = resolveLane(el);
    if (lane)
      lane._pendingAsync.delete(el);
    if (setter)
      setter(value);
    else if (el._overrideValue !== undefined) {
      if (el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING)
        el._pendingValue = value;
      else {
        el._value = value;
        insertSubs(el);
      }
      el._time = clock;
    } else if (lane) {
      const isEffect = el._type;
      const prevValue = el._value;
      const equals = el._equals;
      if (!isEffect && wasUninitialized || !equals || !equals(value, prevValue)) {
        el._value = value;
        el._time = clock;
        if (el._latestValueComputed) {
          setSignal(el._latestValueComputed, value);
        }
        insertSubs(el, true);
      }
    } else {
      setSignal(el, () => value);
    }
    settlePendingSource(el);
    schedule();
    flush();
    then?.();
  };
  if (isThenable) {
    let resolved = false, isSync = true;
    result.then((v) => {
      if (isSync) {
        syncValue = v;
        resolved = true;
      } else
        asyncWrite(v);
    }, (e) => {
      if (!isSync)
        handleError(e);
    });
    isSync = false;
    if (!resolved) {
      globalQueue.initTransition(resolveTransition(el));
      throw new NotReadyError(context);
    }
  }
  if (iterator) {
    const it = result[Symbol.asyncIterator]();
    let hadSyncValue = false;
    let completed = false;
    cleanup(() => {
      if (completed)
        return;
      completed = true;
      try {
        const returned = it.return?.();
        if (returned && typeof returned.then === "function") {
          returned.then(undefined, () => {});
        }
      } catch {}
    });
    const iterate = () => {
      let syncResult, resolved = false, isSync = true;
      it.next().then((r) => {
        if (isSync) {
          syncResult = r;
          resolved = true;
          if (r.done)
            completed = true;
        } else if (el._inFlight !== result) {
          return;
        } else if (!r.done)
          asyncWrite(r.value, iterate);
        else {
          completed = true;
          schedule();
          flush();
        }
      }, (e) => {
        if (!isSync && el._inFlight === result) {
          completed = true;
          handleError(e);
        }
      });
      isSync = false;
      if (resolved && !syncResult.done) {
        syncValue = syncResult.value;
        hadSyncValue = true;
        return iterate();
      }
      return resolved && syncResult.done;
    };
    const immediatelyDone = iterate();
    if (!hadSyncValue && !immediatelyDone) {
      globalQueue.initTransition(resolveTransition(el));
      throw new NotReadyError(context);
    }
  }
  return syncValue;
}
function clearStatus(el, clearUninitialized = false) {
  clearPendingSources(el);
  el._blocked = false;
  el._statusFlags = clearUninitialized ? 0 : el._statusFlags & STATUS_UNINITIALIZED;
  setPendingError(el);
  updatePendingSignal(el);
  el._notifyStatus?.();
}
function notifyStatus(el, status, error, blockStatus, lane) {
  if (status === STATUS_ERROR && !(error instanceof StatusError) && !(error instanceof NotReadyError))
    error = new StatusError(el, error);
  const pendingSource = status === STATUS_PENDING && error instanceof NotReadyError ? error.source : undefined;
  const isSource = pendingSource === el;
  const isOptimisticBoundary = status === STATUS_PENDING && el._overrideValue !== undefined && !isSource;
  const startsBlocking = isOptimisticBoundary && hasActiveOverride(el);
  if (!blockStatus) {
    if (status === STATUS_PENDING && pendingSource) {
      addPendingSource(el, pendingSource);
      el._statusFlags = STATUS_PENDING | el._statusFlags & STATUS_UNINITIALIZED;
      setPendingError(el, pendingSource, error);
    } else {
      clearPendingSources(el);
      el._statusFlags = status | (status !== STATUS_ERROR ? el._statusFlags & STATUS_UNINITIALIZED : 0);
      el._error = error;
    }
    updatePendingSignal(el);
  }
  if (lane && !blockStatus) {
    assignOrMergeLane(el, lane);
  }
  const downstreamBlockStatus = blockStatus || startsBlocking;
  const downstreamLane = blockStatus || isOptimisticBoundary ? undefined : lane;
  if (el._notifyStatus) {
    if (blockStatus && status === STATUS_PENDING) {
      return;
    }
    if (downstreamBlockStatus) {
      el._notifyStatus(status, error);
    } else {
      el._notifyStatus();
    }
    return;
  }
  forEachDependent(el, (sub) => {
    sub._time = clock;
    if (status === STATUS_PENDING && pendingSource && sub._pendingSource !== pendingSource && !sub._pendingSources?.has(pendingSource) || status !== STATUS_PENDING && (sub._error !== error || sub._pendingSource || sub._pendingSources)) {
      if (!downstreamBlockStatus && !sub._transition)
        globalQueue._pendingNodes.push(sub);
      notifyStatus(sub, status, error, downstreamBlockStatus, downstreamLane);
    }
  });
}
var externalSourceConfig = null;
GlobalQueue._update = recompute;
GlobalQueue._dispose = disposeChildren;
var tracking = false;
var stale = false;
var refreshing = false;
var pendingCheckActive = false;
var foundPending = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var snapshotCaptureActive = false;
var snapshotSources = null;
function ownerInSnapshotScope(owner) {
  while (owner) {
    if (owner._snapshotScope)
      return true;
    owner = owner._parent;
  }
  return false;
}
function recompute(el, create = false) {
  const isEffect = el._type;
  if (!create) {
    if (el._transition && (!isEffect || activeTransition) && activeTransition !== el._transition)
      globalQueue.initTransition(el._transition);
    deleteFromHeap(el, el._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
    el._inFlight = null;
    if (el._transition || isEffect === EFFECT_TRACKED)
      disposeChildren(el);
    else {
      markDisposal(el);
      el._pendingDisposal = el._disposal;
      el._pendingFirstChild = el._firstChild;
      el._disposal = null;
      el._firstChild = null;
      el._childCount = 0;
      clearSignals(el);
    }
  }
  const isOptimisticDirty = !!(el._flags & REACTIVE_OPTIMISTIC_DIRTY);
  const hasOverride = el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING;
  const wasPending = !!(el._statusFlags & STATUS_PENDING);
  const wasUninitialized = !!(el._statusFlags & STATUS_UNINITIALIZED);
  const oldcontext = context;
  context = el;
  el._depsTail = null;
  el._flags = REACTIVE_RECOMPUTING_DEPS;
  el._time = clock;
  let value = el._pendingValue === NOT_PENDING ? el._value : el._pendingValue;
  let oldHeight = el._height;
  let prevTracking = tracking;
  let prevLane = currentOptimisticLane;
  let prevStrictRead = false;
  {
    prevStrictRead = strictRead;
    strictRead = false;
  }
  tracking = true;
  if (isOptimisticDirty) {
    const lane = resolveLane(el);
    if (lane)
      currentOptimisticLane = lane;
  }
  try {
    value = handleAsync(el, el._fn(value));
    clearStatus(el, create);
    const resolvedLane = resolveLane(el);
    if (resolvedLane) {
      resolvedLane._pendingAsync.delete(el);
      updatePendingSignal(resolvedLane._source);
    }
  } catch (e) {
    if (e instanceof NotReadyError && currentOptimisticLane) {
      const lane = findLane(currentOptimisticLane);
      if (lane._source !== el) {
        lane._pendingAsync.add(el);
        el._optimisticLane = lane;
        updatePendingSignal(lane._source);
      }
    }
    if (e instanceof NotReadyError)
      el._blocked = true;
    notifyStatus(el, e instanceof NotReadyError ? STATUS_PENDING : STATUS_ERROR, e, undefined, e instanceof NotReadyError ? el._optimisticLane : undefined);
  } finally {
    tracking = prevTracking;
    strictRead = prevStrictRead;
    el._flags = REACTIVE_NONE | (create ? el._flags & REACTIVE_SNAPSHOT_STALE : 0);
    context = oldcontext;
  }
  if (!el._error) {
    const depsTail = el._depsTail;
    let toRemove = depsTail !== null ? depsTail._nextDep : el._deps;
    if (toRemove !== null) {
      do {
        toRemove = unlinkSubs(toRemove);
      } while (toRemove !== null);
      if (depsTail !== null)
        depsTail._nextDep = null;
      else
        el._deps = null;
    }
    const compareValue = hasOverride ? el._overrideValue : el._pendingValue === NOT_PENDING ? el._value : el._pendingValue;
    const valueChanged = !isEffect && wasUninitialized || !el._equals || !el._equals(compareValue, value);
    if (valueChanged) {
      const prevVisible = hasOverride ? el._overrideValue : undefined;
      if (create || isEffect && activeTransition !== el._transition || isOptimisticDirty) {
        el._value = value;
        if (hasOverride && isOptimisticDirty) {
          el._overrideValue = value;
          el._pendingValue = value;
        }
      } else
        el._pendingValue = value;
      if (hasOverride && !isOptimisticDirty && wasPending && !el._overrideSinceLane)
        el._overrideValue = value;
      if (!hasOverride || isOptimisticDirty || el._overrideValue !== prevVisible)
        insertSubs(el, isOptimisticDirty || hasOverride);
    } else if (hasOverride) {
      el._pendingValue = value;
    } else if (el._height != oldHeight) {
      for (let s = el._subs;s !== null; s = s._nextSub) {
        insertIntoHeapHeight(s._sub, s._sub._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
      }
    }
  }
  currentOptimisticLane = prevLane;
  (!create || el._statusFlags & STATUS_PENDING) && !el._transition && !(activeTransition && hasOverride) && globalQueue._pendingNodes.push(el);
  el._transition && isEffect && activeTransition !== el._transition && runInTransition(el._transition, () => recompute(el));
}
function updateIfNecessary(el) {
  if (el._flags & REACTIVE_CHECK) {
    for (let d = el._deps;d; d = d._nextDep) {
      const dep1 = d._dep;
      const dep = dep1._firewall || dep1;
      if (dep._fn) {
        updateIfNecessary(dep);
      }
      if (el._flags & REACTIVE_DIRTY) {
        break;
      }
    }
  }
  if (el._flags & (REACTIVE_DIRTY | REACTIVE_OPTIMISTIC_DIRTY) || el._error && el._time < clock && !el._inFlight) {
    recompute(el);
  }
  el._flags = el._flags & (REACTIVE_SNAPSHOT_STALE | REACTIVE_IN_HEAP | REACTIVE_IN_HEAP_HEIGHT);
}
function computed(fn, initialValue, options) {
  const transparent = options?.transparent ?? false;
  const self = {
    id: options?.id ?? (transparent ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    _transparent: transparent || undefined,
    _equals: options?.equals != null ? options.equals : isEqual,
    _ownedWrite: !!options?.ownedWrite,
    _unobserved: options?.unobserved,
    _disposal: null,
    _queue: context?._queue ?? globalQueue,
    _context: context?._context ?? defaultContext,
    _childCount: 0,
    _fn: fn,
    _value: initialValue,
    _height: 0,
    _child: null,
    _nextHeap: undefined,
    _prevHeap: null,
    _deps: null,
    _depsTail: null,
    _subs: null,
    _subsTail: null,
    _parent: context,
    _nextSibling: null,
    _firstChild: null,
    _flags: options?.lazy ? REACTIVE_LAZY : REACTIVE_NONE,
    _statusFlags: STATUS_UNINITIALIZED,
    _time: clock,
    _pendingValue: NOT_PENDING,
    _pendingDisposal: null,
    _pendingFirstChild: null,
    _inFlight: null,
    _transition: null
  };
  self._name = options?.name ?? "computed";
  self._prevHeap = self;
  const parent = context?._root ? context._parentComputed : context;
  if (context?._childrenForbidden) {
    throw new Error("Cannot create reactive primitives inside createTrackedEffect or owner-backed onSettled");
  }
  if (context) {
    const lastChild = context._firstChild;
    if (lastChild === null) {
      context._firstChild = self;
    } else {
      self._nextSibling = lastChild;
      context._firstChild = self;
    }
  }
  DEV$1.hooks.onOwner?.(self);
  if (parent)
    self._height = parent._height + 1;
  if (snapshotCaptureActive && ownerInSnapshotScope(context))
    self._inSnapshotScope = true;
  if (externalSourceConfig) {
    const bridgeSignal = signal(undefined, { equals: false, ownedWrite: true });
    const source = externalSourceConfig.factory(self._fn, () => {
      setSignal(bridgeSignal, undefined);
    });
    cleanup(() => source.dispose());
    self._fn = (prev) => {
      read(bridgeSignal);
      return source.track(prev);
    };
  }
  !options?.lazy && recompute(self, true);
  if (snapshotCaptureActive && !options?.lazy) {
    if (!(self._statusFlags & STATUS_PENDING)) {
      self._snapshotValue = self._value === undefined ? NO_SNAPSHOT : self._value;
      snapshotSources.add(self);
    }
  }
  return self;
}
function signal(v, options, firewall = null) {
  const s = {
    _equals: options?.equals != null ? options.equals : isEqual,
    _ownedWrite: !!options?.ownedWrite,
    _noSnapshot: !!options?._noSnapshot,
    _unobserved: options?.unobserved,
    _value: v,
    _subs: null,
    _subsTail: null,
    _time: clock,
    _firewall: firewall,
    _nextChild: firewall?._child || null,
    _pendingValue: NOT_PENDING
  };
  {
    s._name = options?.name ?? "signal";
    s._internal = !!firewall;
  }
  firewall && (firewall._child = s);
  if (snapshotCaptureActive && !s._noSnapshot && !((firewall?._statusFlags ?? 0) & STATUS_PENDING)) {
    s._snapshotValue = v === undefined ? NO_SNAPSHOT : v;
    snapshotSources.add(s);
  }
  return s;
}
function optimisticSignal(v, options) {
  const s = signal(v, options);
  s._overrideValue = NOT_PENDING;
  return s;
}
function optimisticComputed(fn, initialValue, options) {
  const c = computed(fn, initialValue, options);
  c._overrideValue = NOT_PENDING;
  return c;
}
function isEqual(a, b) {
  return a === b;
}
var strictRead = false;
function setStrictRead(v) {
  const prev = strictRead;
  strictRead = v;
  return prev;
}
function untrack(fn, strictReadLabel) {
  if (!externalSourceConfig && !tracking && !strictRead && !strictReadLabel)
    return fn();
  const prevTracking = tracking;
  const prevStrictRead = strictRead;
  tracking = false;
  strictRead = strictReadLabel || false;
  try {
    if (externalSourceConfig)
      return externalSourceConfig.untrack(fn);
    return fn();
  } finally {
    tracking = prevTracking;
    strictRead = prevStrictRead;
  }
}
function read(el) {
  if (latestReadActive) {
    const pendingComputed = getLatestValueComputed(el);
    const prevPending = latestReadActive;
    latestReadActive = false;
    const visibleValue = el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING ? el._overrideValue : el._value;
    let value2;
    try {
      value2 = read(pendingComputed);
    } catch (e) {
      if (!context && e instanceof NotReadyError)
        return visibleValue;
      throw e;
    } finally {
      latestReadActive = prevPending;
    }
    if (pendingComputed._statusFlags & STATUS_PENDING)
      return visibleValue;
    if (stale && currentOptimisticLane && pendingComputed._optimisticLane) {
      const pcLane = findLane(pendingComputed._optimisticLane);
      const curLane = findLane(currentOptimisticLane);
      if (pcLane !== curLane && pcLane._pendingAsync.size > 0) {
        return visibleValue;
      }
    }
    return value2;
  }
  if (pendingCheckActive) {
    const firewall = el._firewall;
    const prevCheck = pendingCheckActive;
    pendingCheckActive = false;
    if (firewall && el._overrideValue !== undefined) {
      if (el._overrideValue !== NOT_PENDING && (firewall._inFlight || !!(firewall._statusFlags & STATUS_PENDING))) {
        foundPending = true;
      }
      let c2 = context;
      if (c2?._root)
        c2 = c2._parentComputed;
      if (c2 && tracking)
        link(el, c2);
      read(getPendingSignal(el));
      read(getPendingSignal(firewall));
    } else {
      if (read(getPendingSignal(el)))
        foundPending = true;
      if (firewall && read(getPendingSignal(firewall)))
        foundPending = true;
    }
    pendingCheckActive = prevCheck;
    return el._value;
  }
  let c = context;
  if (c?._root)
    c = c._parentComputed;
  const computed2 = el;
  if (typeof computed2._fn === "function") {
    const comp = el;
    if (refreshing && !(comp._flags & REACTIVE_DISPOSED))
      recompute(comp);
    if (comp._flags & REACTIVE_LAZY) {
      comp._flags &= ~REACTIVE_LAZY;
      recompute(comp, true);
    } else if (comp._flags & REACTIVE_DISPOSED) {
      recompute(comp, true);
    }
  }
  const owner = el._firewall || el;
  if (strictRead && owner._statusFlags & STATUS_PENDING) {
    const message = `Reading a pending async value directly in ${strictRead}. ` + `Async values must be read within a tracking scope (JSX, a memo, or an effect's compute function).`;
    emitDiagnostic({
      code: "PENDING_ASYNC_UNTRACKED_READ",
      kind: "async",
      severity: "error",
      message,
      ownerId: c?.id,
      ownerName: c?._name,
      nodeName: owner?._name,
      data: { strictRead }
    });
    throw new Error(message);
  }
  if (c && tracking) {
    link(el, c);
    if (owner._fn) {
      const isZombie = el._flags & REACTIVE_ZOMBIE;
      if (owner._height >= (isZombie ? zombieQueue._min : dirtyQueue._min)) {
        markNode(c);
        markHeap(isZombie ? zombieQueue : dirtyQueue);
        updateIfNecessary(owner);
      }
      const height = owner._height;
      if (height >= c._height && el._parent !== c) {
        c._height = height + 1;
      }
    }
  }
  if (owner._statusFlags & STATUS_PENDING) {
    if (c && !(stale && owner._transition && activeTransition !== owner._transition)) {
      if (c?._childrenForbidden) {
        const message = "Reading a pending async value inside createTrackedEffect or onSettled will throw. " + "Use createEffect instead which supports async-aware reactivity.";
        emitDiagnostic({
          code: "PENDING_ASYNC_FORBIDDEN_SCOPE",
          kind: "async",
          severity: "warn",
          message,
          ownerId: c.id,
          ownerName: c._name,
          nodeName: owner?._name
        });
        console.warn(message);
      }
      if (currentOptimisticLane) {
        const pendingLane = owner._optimisticLane;
        const lane = findLane(currentOptimisticLane);
        if (pendingLane && findLane(pendingLane) === lane && !hasActiveOverride(owner)) {
          if (!tracking && el !== c)
            link(el, c);
          throw owner._error;
        }
      } else {
        if (!tracking && el !== c)
          link(el, c);
        throw owner._error;
      }
    } else if (c && owner !== el && owner._statusFlags & STATUS_UNINITIALIZED) {
      if (!tracking && el !== c)
        link(el, c);
      throw owner._error;
    } else if (!c && owner._statusFlags & STATUS_UNINITIALIZED) {
      throw owner._error;
    }
  }
  if (el._fn && el._statusFlags & STATUS_ERROR) {
    if (el._time < clock) {
      recompute(el);
      return read(el);
    } else
      throw el._error;
  }
  if (snapshotCaptureActive && c && c._inSnapshotScope) {
    const sv = el._snapshotValue;
    if (sv !== undefined) {
      const snapshot = sv === NO_SNAPSHOT ? undefined : sv;
      const current = el._pendingValue !== NOT_PENDING ? el._pendingValue : el._value;
      if (current !== snapshot)
        c._flags |= REACTIVE_SNAPSHOT_STALE;
      return snapshot;
    }
  }
  if (strictRead) {
    const message = `Reactive value read directly in ${strictRead} will not update. ` + `Move it into a tracking scope (JSX, a memo, or an effect's compute function).`;
    emitDiagnostic({
      code: "STRICT_READ_UNTRACKED",
      kind: "strict-read",
      severity: "warn",
      message,
      ownerId: c?.id,
      ownerName: c?._name,
      nodeName: owner?._name,
      data: { strictRead }
    });
    console.warn(message);
  }
  if (el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING) {
    if (c && stale && shouldReadStashedOptimisticValue(el))
      return el._value;
    return el._overrideValue;
  }
  const value = !c || currentOptimisticLane !== null && (el._overrideValue !== undefined || el._optimisticLane || owner === el && stale || !!(owner._statusFlags & STATUS_PENDING)) || el._pendingValue === NOT_PENDING || stale && el._transition && activeTransition !== el._transition ? el._value : el._pendingValue;
  if (!c && owner === el && typeof computed2._fn === "function" && !computed2._preventAutoDisposal && !(owner._statusFlags & STATUS_PENDING) && !computed2._parent && !el._subs) {
    unobserved(el);
  }
  return value;
}
function setSignal(el, v) {
  if (!el._ownedWrite && !context?._childrenForbidden && context && el._firewall !== context) {
    const message = "Writing to a Signal inside an owned scope (component, computation) is not allowed. " + "Move the write outside or set the `ownedWrite` option if this is intentional.";
    emitDiagnostic({
      code: "SIGNAL_WRITE_IN_OWNED_SCOPE",
      kind: "write",
      severity: "error",
      message,
      ownerId: context.id,
      ownerName: context._name,
      nodeName: el._name
    });
    throw new Error(message);
  }
  if (el._transition && activeTransition !== el._transition)
    globalQueue.initTransition(el._transition);
  const isOptimistic = el._overrideValue !== undefined && !projectionWriteActive;
  const hasOverride = el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING;
  const currentValue = isOptimistic ? hasOverride ? el._overrideValue : el._value : el._pendingValue === NOT_PENDING ? el._value : el._pendingValue;
  if (typeof v === "function")
    v = v(currentValue);
  const valueChanged = !el._equals || !el._equals(currentValue, v) || !!(el._statusFlags & STATUS_UNINITIALIZED);
  if (!valueChanged) {
    if (isOptimistic && hasOverride && el._fn) {
      insertSubs(el, true);
      schedule();
    }
    return v;
  }
  if (isOptimistic) {
    const firstOverride = el._overrideValue === NOT_PENDING;
    if (!firstOverride)
      globalQueue.initTransition(resolveTransition(el));
    if (firstOverride) {
      el._pendingValue = el._value;
      globalQueue._optimisticNodes.push(el);
    }
    el._overrideSinceLane = true;
    const lane = getOrCreateLane(el);
    el._optimisticLane = lane;
    el._overrideValue = v;
  } else {
    if (el._pendingValue === NOT_PENDING)
      globalQueue._pendingNodes.push(el);
    el._pendingValue = v;
  }
  updatePendingSignal(el);
  if (el._latestValueComputed) {
    setSignal(el._latestValueComputed, v);
  }
  el._time = clock;
  insertSubs(el, isOptimistic);
  schedule();
  return v;
}
function runWithOwner(owner, fn) {
  if (owner && owner._flags & REACTIVE_DISPOSED) {
    const message = "runWithOwner called with a disposed owner. Children created inside will never be disposed.";
    emitDiagnostic({
      code: "RUN_WITH_DISPOSED_OWNER",
      kind: "owner",
      severity: "warn",
      message,
      ownerId: owner.id,
      ownerName: owner._name
    });
    console.warn(message);
  }
  const oldContext = context;
  const prevTracking = tracking;
  context = owner;
  tracking = false;
  try {
    return fn();
  } finally {
    context = oldContext;
    tracking = prevTracking;
  }
}
function getPendingSignal(el) {
  if (!el._pendingSignal) {
    el._pendingSignal = optimisticSignal(false, { ownedWrite: true });
    if (el._parentSource) {
      el._pendingSignal._parentSource = el;
    }
    if (computePendingState(el))
      setSignal(el._pendingSignal, true);
  }
  return el._pendingSignal;
}
function computePendingState(el) {
  const comp = el;
  const firewall = el._firewall;
  if (firewall && el._pendingValue !== NOT_PENDING) {
    return !firewall._inFlight && !(firewall._statusFlags & STATUS_PENDING);
  }
  if (el._overrideValue !== undefined && el._overrideValue !== NOT_PENDING) {
    if (comp._statusFlags & STATUS_PENDING && !(comp._statusFlags & STATUS_UNINITIALIZED))
      return true;
    if (el._parentSource) {
      const lane = el._optimisticLane ? findLane(el._optimisticLane) : null;
      return !!(lane && lane._pendingAsync.size > 0);
    }
    return true;
  }
  if (el._overrideValue !== undefined && el._overrideValue === NOT_PENDING && !el._parentSource) {
    return false;
  }
  if (el._pendingValue !== NOT_PENDING && !(comp._statusFlags & STATUS_UNINITIALIZED))
    return true;
  return !!(comp._statusFlags & STATUS_PENDING && !(comp._statusFlags & STATUS_UNINITIALIZED));
}
function updatePendingSignal(el) {
  if (el._pendingSignal) {
    const pending = computePendingState(el);
    const sig = el._pendingSignal;
    setSignal(sig, pending);
    if (!pending && sig._optimisticLane) {
      const sourceLane = resolveLane(el);
      if (sourceLane && sourceLane._pendingAsync.size > 0) {
        const sigLane = findLane(sig._optimisticLane);
        if (sigLane !== sourceLane) {
          mergeLanes(sourceLane, sigLane);
        }
      }
      signalLanes.delete(sig);
      sig._optimisticLane = undefined;
    }
  }
}
function getLatestValueComputed(el) {
  if (!el._latestValueComputed) {
    const prevPending = latestReadActive;
    latestReadActive = false;
    const prevCheck = pendingCheckActive;
    pendingCheckActive = false;
    const prevContext = context;
    context = null;
    el._latestValueComputed = optimisticComputed(() => read(el));
    el._latestValueComputed._parentSource = el;
    context = prevContext;
    pendingCheckActive = prevCheck;
    latestReadActive = prevPending;
  }
  return el._latestValueComputed;
}
function staleValues(fn, set = true) {
  const prevStale = stale;
  stale = set;
  try {
    return fn();
  } finally {
    stale = prevStale;
  }
}
function createContext(defaultValue, description) {
  return { id: Symbol(description), defaultValue };
}
function effect(compute, effect2, error, initialValue, options) {
  let initialized = false;
  const node = computed(options?.render ? (p) => staleValues(() => compute(p)) : compute, initialValue, {
    ...options,
    equals: () => {
      node._modified = !node._error;
      if (initialized)
        node._queue.enqueue(node._type, runEffect.bind(node));
      return false;
    },
    lazy: true
  });
  node._prevValue = initialValue;
  node._effectFn = effect2;
  node._errorFn = error;
  node._cleanup = undefined;
  node._type = options?.render ? EFFECT_RENDER : EFFECT_USER;
  node._notifyStatus = (status, error2) => {
    const actualStatus = status !== undefined ? status : node._statusFlags;
    const actualError = error2 !== undefined ? error2 : node._error;
    if (actualStatus & STATUS_ERROR) {
      let err = actualError;
      node._queue.notify(node, STATUS_PENDING, 0);
      if (node._type === EFFECT_USER) {
        try {
          return node._errorFn ? node._errorFn(err, () => {
            node._cleanup?.();
            node._cleanup = undefined;
          }) : console.error(err);
        } catch (e) {
          err = e;
        }
      }
      if (!node._queue.notify(node, STATUS_ERROR, STATUS_ERROR))
        throw err;
    } else if (node._type === EFFECT_RENDER) {
      node._queue.notify(node, STATUS_PENDING | STATUS_ERROR, actualStatus, actualError);
      if (_hitUnhandledAsync) {
        resetUnhandledAsync();
        if (!node._queue.notify(node, STATUS_ERROR, STATUS_ERROR)) {
          const message = "An async value must be rendered inside a Loading boundary.";
          emitDiagnostic({
            code: "ASYNC_OUTSIDE_LOADING_BOUNDARY",
            kind: "async",
            severity: "error",
            message,
            ownerId: node.id,
            ownerName: node._name
          });
          throw new Error(message);
        }
      }
    }
  };
  recompute(node, true);
  !options?.defer && (node._type === EFFECT_USER ? node._queue.enqueue(node._type, runEffect.bind(node)) : runEffect.call(node));
  initialized = true;
  cleanup(() => node._cleanup?.());
  if (!node._parent) {
    const message = "Effects created outside a reactive context will never be disposed";
    emitDiagnostic({
      code: "NO_OWNER_EFFECT",
      kind: "lifecycle",
      severity: "warn",
      message,
      ownerId: node.id,
      ownerName: node._name,
      data: { effectType: "effect" }
    });
    console.warn(message);
  }
}
function runEffect() {
  if (!this._modified || this._flags & REACTIVE_DISPOSED)
    return;
  let prevStrictRead = false;
  {
    prevStrictRead = setStrictRead("an effect callback");
  }
  this._cleanup?.();
  this._cleanup = undefined;
  try {
    const cleanup2 = this._effectFn(this._value, this._prevValue);
    if (cleanup2 !== undefined && typeof cleanup2 !== "function") {
      throw new Error(`${this._name || "effect"} callback returned an invalid cleanup value. Return a cleanup function or undefined.`);
    }
    this._cleanup = cleanup2;
  } catch (error) {
    this._error = new StatusError(this, error);
    this._statusFlags |= STATUS_ERROR;
    if (!this._queue.notify(this, STATUS_ERROR, STATUS_ERROR))
      throw error;
  } finally {
    setStrictRead(prevStrictRead);
    this._prevValue = this._value;
    this._modified = false;
  }
}
function trackedEffect(fn, options) {
  const run = () => {
    if (!node._modified || node._flags & REACTIVE_DISPOSED)
      return;
    setTrackedQueueCallback(true);
    try {
      node._modified = false;
      recompute(node);
    } finally {
      setTrackedQueueCallback(false);
    }
  };
  const node = computed(() => {
    node._cleanup?.();
    node._cleanup = undefined;
    const cleanup2 = staleValues(fn);
    if (cleanup2 !== undefined && typeof cleanup2 !== "function") {
      throw new Error(`${node._name || "trackedEffect"} callback returned an invalid cleanup value. Return a cleanup function or undefined.`);
    }
    node._cleanup = cleanup2;
  }, undefined, { ...options, lazy: true });
  node._cleanup = undefined;
  node._childrenForbidden = true;
  node._modified = true;
  node._type = EFFECT_TRACKED;
  node._notifyStatus = (status, error) => {
    const actualStatus = status !== undefined ? status : node._statusFlags;
    if (actualStatus & STATUS_ERROR) {
      node._queue.notify(node, STATUS_PENDING, 0);
      const err = error !== undefined ? error : node._error;
      if (!node._queue.notify(node, STATUS_ERROR, STATUS_ERROR))
        throw err;
    }
  };
  node._run = run;
  node._queue.enqueue(EFFECT_USER, run);
  cleanup(() => node._cleanup?.());
  if (!node._parent) {
    const message = "Effects created outside a reactive context will never be disposed";
    emitDiagnostic({
      code: "NO_OWNER_EFFECT",
      kind: "lifecycle",
      severity: "warn",
      message,
      ownerId: node.id,
      ownerName: node._name,
      data: { effectType: "trackedEffect" }
    });
    console.warn(message);
  }
}
function onCleanup(fn) {
  {
    const owner = getOwner();
    if (!owner) {
      const message = "onCleanup called outside a reactive context will never be run";
      emitDiagnostic({
        code: "NO_OWNER_CLEANUP",
        kind: "lifecycle",
        severity: "warn",
        message
      });
      console.warn(message);
    } else if (owner._childrenForbidden) {
      const message = "Cannot use onCleanup inside createTrackedEffect or onSettled; return a cleanup function instead";
      emitDiagnostic({
        code: "CLEANUP_IN_FORBIDDEN_SCOPE",
        kind: "lifecycle",
        severity: "error",
        message,
        ownerId: owner.id,
        ownerName: owner._name
      });
      throw new Error(message);
    }
  }
  return cleanup(fn);
}
function accessor(node) {
  const fn = read.bind(null, node);
  fn.$r = true;
  return fn;
}
function createSignal(first, second) {
  if (typeof first === "function") {
    const node2 = computed(first, undefined, second);
    node2._preventAutoDisposal = true;
    return [accessor(node2), setSignal.bind(null, node2)];
  }
  const node = signal(first, second);
  registerGraph(node, getOwner());
  return [accessor(node), setSignal.bind(null, node)];
}
function createMemo(compute, options) {
  let node = computed(compute, undefined, options);
  return accessor(node);
}
function createEffect(compute, effectFn, options) {
  effect(compute, effectFn.effect || effectFn, effectFn.error, undefined, {
    ...options,
    name: options?.name ?? "effect"
  });
}
function createRenderEffect(compute, effectFn, options) {
  effect(compute, effectFn, undefined, undefined, {
    render: true,
    ...{ ...options, name: options?.name ?? "effect" }
  });
}
function createTrackedEffect(compute, options) {
  trackedEffect(compute, { ...options, name: options?.name ?? "trackedEffect" });
}
function onSettled(callback) {
  const owner = getOwner();
  owner && !owner._childrenForbidden ? createTrackedEffect(() => untrack(callback), { name: "onSettled" }) : globalQueue.enqueue(EFFECT_USER, () => {
    const cleanup2 = callback();
    if (cleanup2 !== undefined && typeof cleanup2 !== "function") {
      throw new Error("onSettled callback returned an invalid cleanup value. Return a cleanup function or undefined.");
    }
    cleanup2?.();
  });
}
var $TRACK = Symbol("STORE_TRACK");
var $TARGET = Symbol("STORE_TARGET");
var $PROXY = Symbol("STORE_PROXY");
var $DELETED = Symbol("STORE_DELETED");
var storeLookup = new WeakMap;
function isWrappable(obj) {
  return obj != null && typeof obj === "object" && !Object.isFrozen(obj) && !(typeof Node !== "undefined" && obj instanceof Node);
}
var DELETE = Symbol("STORE_PATH_DELETE");
function updatePath(current, args, i = 0) {
  let part, prev = current;
  if (i < args.length - 1) {
    part = args[i];
    const partType = typeof part;
    const isArray = Array.isArray(current);
    if (Array.isArray(part)) {
      for (let j = 0;j < part.length; j++) {
        args[i] = part[j];
        updatePath(current, args, i);
      }
      args[i] = part;
      return;
    } else if (isArray && partType === "function") {
      for (let j = 0;j < current.length; j++) {
        if (part(current[j], j)) {
          args[i] = j;
          updatePath(current, args, i);
        }
      }
      args[i] = part;
      return;
    } else if (isArray && partType === "object") {
      const { from = 0, to = current.length - 1, by = 1 } = part;
      for (let j = from;j <= to; j += by) {
        args[i] = j;
        updatePath(current, args, i);
      }
      args[i] = part;
      return;
    } else if (i < args.length - 2) {
      updatePath(current[part], args, i + 1);
      return;
    }
    prev = current[part];
  }
  let value = args[args.length - 1];
  if (typeof value === "function") {
    value = value(prev);
    if (value === prev)
      return;
  }
  if (part === undefined && value == undefined)
    return;
  if (value === DELETE) {
    delete current[part];
  } else if (part === undefined || isWrappable(prev) && isWrappable(value) && !Array.isArray(value)) {
    const target = part !== undefined ? current[part] : current;
    const keys = Object.keys(value);
    for (let i2 = 0;i2 < keys.length; i2++) {
      const key = keys[i2];
      const desc = Object.getOwnPropertyDescriptor(value, key);
      if (desc.get || desc.set)
        Object.defineProperty(target, key, desc);
      else
        target[key] = desc.value;
    }
  } else {
    current[part] = value;
  }
}
var storePath = Object.assign(function storePath2(...args) {
  return (state) => {
    updatePath(state, args);
  };
}, { DELETE });
function trueFn() {
  return true;
}
var propTraps = {
  get(_, property, receiver) {
    if (property === $PROXY)
      return receiver;
    return _.get(property);
  },
  has(_, property) {
    if (property === $PROXY)
      return true;
    return _.has(property);
  },
  set: trueFn,
  deleteProperty: trueFn,
  getOwnPropertyDescriptor(_, property) {
    return {
      configurable: true,
      enumerable: true,
      get() {
        return _.get(property);
      },
      set: trueFn,
      deleteProperty: trueFn
    };
  },
  ownKeys(_) {
    return _.keys();
  }
};
function resolveSource(s) {
  return !(s = typeof s === "function" ? s() : s) ? {} : s;
}
var $SOURCES = Symbol("MERGE_SOURCE");
function merge(...sources) {
  if (sources.length === 1 && typeof sources[0] !== "function")
    return sources[0];
  let proxy = false;
  const flattened = [];
  for (let i = 0;i < sources.length; i++) {
    const s = sources[i];
    proxy = proxy || !!s && $PROXY in s;
    const childSources = !!s && s[$SOURCES];
    if (childSources)
      flattened.push(...childSources);
    else
      flattened.push(typeof s === "function" ? (proxy = true, createMemo(s)) : s);
  }
  if (SUPPORTS_PROXY && proxy) {
    return new Proxy({
      get(property) {
        if (property === $SOURCES)
          return flattened;
        for (let i = flattened.length - 1;i >= 0; i--) {
          const s = resolveSource(flattened[i]);
          if (property in s)
            return s[property];
        }
      },
      has(property) {
        for (let i = flattened.length - 1;i >= 0; i--) {
          if (property in resolveSource(flattened[i]))
            return true;
        }
        return false;
      },
      keys() {
        const keys = [];
        for (let i = 0;i < flattened.length; i++)
          keys.push(...Object.keys(resolveSource(flattened[i])));
        return [...new Set(keys)];
      }
    }, propTraps);
  }
  const defined = Object.create(null);
  let nonTargetKey = false;
  let lastIndex = flattened.length - 1;
  for (let i = lastIndex;i >= 0; i--) {
    const source = flattened[i];
    if (!source) {
      i === lastIndex && lastIndex--;
      continue;
    }
    const sourceKeys = Object.getOwnPropertyNames(source);
    for (let j = sourceKeys.length - 1;j >= 0; j--) {
      const key = sourceKeys[j];
      if (key === "__proto__" || key === "constructor")
        continue;
      if (!defined[key]) {
        nonTargetKey = nonTargetKey || i !== lastIndex;
        const desc = Object.getOwnPropertyDescriptor(source, key);
        defined[key] = desc.get ? { enumerable: true, configurable: true, get: desc.get.bind(source) } : desc;
      }
    }
  }
  if (!nonTargetKey)
    return flattened[lastIndex];
  const target = {};
  const definedKeys = Object.keys(defined);
  for (let i = definedKeys.length - 1;i >= 0; i--) {
    const key = definedKeys[i], desc = defined[key];
    if (desc.get)
      Object.defineProperty(target, key, desc);
    else
      target[key] = desc.value;
  }
  target[$SOURCES] = flattened;
  return target;
}
var ON_INIT = Symbol();
var RevealControllerContext = createContext(null);
var _revealUsed = false;
function isRevealController(slot) {
  return slot instanceof RevealController;
}
function isSlotReady(slot) {
  return isRevealController(slot) ? slot.isReady() : slot._sources.size === 0 && !slot._pending;
}
function setSlotState(slot, controller, disabled, collapsed) {
  setSignal(slot._disabled, disabled);
  setSignal(slot._collapsed, collapsed);
  if (isRevealController(slot)) {
    if (!disabled && slot._parentController === controller)
      slot._parentController = undefined;
    return slot.evaluate(disabled, collapsed);
  }
  if (!disabled && slot._revealController === controller && slot._initialized)
    slot._revealController = undefined;
}

class RevealController {
  _togetherAccessor;
  _collapsedAccessor;
  _slots = [];
  _parentController;
  _disabled = signal(false, { ownedWrite: true, _noSnapshot: true });
  _collapsed = signal(false, { ownedWrite: true, _noSnapshot: true });
  _ready = true;
  _evaluating = false;
  constructor(together, collapsed) {
    this._togetherAccessor = together;
    this._collapsedAccessor = collapsed;
  }
  _forEachOwnedSlot(fn) {
    for (let i = 0;i < this._slots.length; i++) {
      const slot = this._slots[i];
      if ((isRevealController(slot) ? slot._parentController : slot._revealController) !== this)
        continue;
      if (fn(slot) === false)
        return false;
    }
    return true;
  }
  isReady() {
    return this._forEachOwnedSlot(isSlotReady);
  }
  register(slot) {
    if (this._slots.includes(slot))
      return;
    this._slots.push(slot);
    const together = !!untrack(this._togetherAccessor);
    setSignal(slot._disabled, true), setSignal(slot._collapsed, together ? false : !!untrack(this._collapsedAccessor));
    untrack(() => this.evaluate());
  }
  unregister(slot) {
    const index = this._slots.indexOf(slot);
    if (index >= 0)
      this._slots.splice(index, 1);
    untrack(() => this.evaluate());
  }
  evaluate(disabledOverride, collapsedOverride) {
    if (this._evaluating)
      return;
    this._evaluating = true;
    const wasReady = this._ready;
    try {
      const disabled = disabledOverride ?? read(this._disabled), collapseTail = !!untrack(this._collapsedAccessor), collapsed = collapsedOverride ?? collapseTail;
      if (disabled && collapsed)
        this._forEachOwnedSlot((slot) => setSlotState(slot, this, true, true));
      else if (!!untrack(this._togetherAccessor)) {
        const ready = this.isReady();
        this._forEachOwnedSlot((slot) => setSlotState(slot, this, !ready, false));
      } else {
        let pendingSeen = false;
        this._forEachOwnedSlot((slot) => {
          if (pendingSeen)
            return setSlotState(slot, this, true, collapseTail);
          if (isSlotReady(slot))
            return setSlotState(slot, this, false, false);
          pendingSeen = true;
          setSlotState(slot, this, true, false);
        });
      }
    } finally {
      this._ready = this.isReady();
      this._evaluating = false;
    }
    if (this._parentController && wasReady !== this._ready)
      this._parentController.evaluate();
  }
}

class CollectionQueue extends Queue {
  _collectionType;
  _sources = new Set;
  _tree;
  _pending = true;
  _disabled = signal(false, { ownedWrite: true, _noSnapshot: true });
  _collapsed = signal(false, { ownedWrite: true, _noSnapshot: true });
  _revealController;
  _initialized = false;
  _onFn;
  _prevOn = ON_INIT;
  constructor(type) {
    super();
    this._collectionType = type;
  }
  run(type) {
    if (!type || read(this._disabled) && (!_revealUsed || read(this._collapsed)))
      return;
    return super.run(type);
  }
  notify(node, type, flags, error) {
    if (!(type & this._collectionType))
      return super.notify(node, type, flags, error);
    if (this._initialized && this._onFn) {
      const currentOn = untrack(() => {
        try {
          return this._onFn();
        } catch {
          return ON_INIT;
        }
      });
      if (currentOn !== this._prevOn) {
        this._prevOn = currentOn;
        this._initialized = false;
        this._sources.clear();
      }
    }
    if (this._collectionType & STATUS_PENDING && this._initialized)
      return super.notify(node, type, flags, error);
    if (this._collectionType & STATUS_PENDING && flags & STATUS_ERROR) {
      return super.notify(node, STATUS_ERROR, flags, error);
    }
    if (flags & this._collectionType) {
      this._pending = true;
      const source = error?.source || node._error?.source;
      if (source) {
        const wasEmpty = this._sources.size === 0;
        this._sources.add(source);
        if (wasEmpty)
          setSignal(this._disabled, true);
      }
    }
    type &= ~this._collectionType;
    return type ? super.notify(node, type, flags, error) : true;
  }
  checkSources() {
    for (const source of this._sources) {
      if (source._flags & REACTIVE_DISPOSED || !(source._statusFlags & this._collectionType) && !(this._collectionType & STATUS_ERROR && source._statusFlags & STATUS_PENDING))
        this._sources.delete(source);
    }
    if (!this._sources.size) {
      if (this._collectionType & STATUS_PENDING && this._pending && !this._initialized && this._tree) {
        this._pending = !!(this._tree._statusFlags & this._collectionType);
      } else {
        this._pending = false;
      }
      if (!this._pending) {
        setSignal(this._disabled, false);
        if (this._onFn) {
          try {
            this._prevOn = untrack(() => this._onFn());
          } catch {}
        }
      }
    }
    if (_revealUsed)
      this._revealController?.evaluate();
  }
}
function flatten(children, options) {
  if (typeof children === "function" && !children.length) {
    if (options?.doNotUnwrap)
      return children;
    do {
      children = children();
    } while (typeof children === "function" && !children.length);
  }
  if (options?.skipNonRendered && (children == null || children === true || children === false || children === ""))
    return;
  if (Array.isArray(children)) {
    let results = [];
    if (flattenArray(children, results, options)) {
      return () => {
        let nested = [];
        flattenArray(results, nested, { ...options, doNotUnwrap: false });
        return nested;
      };
    }
    return results;
  }
  return children;
}
function flattenArray(children, results = [], options) {
  let notReady = null;
  let needsUnwrap = false;
  for (let i = 0;i < children.length; i++) {
    try {
      let child = children[i];
      if (typeof child === "function" && !child.length) {
        if (options?.doNotUnwrap) {
          results.push(child);
          needsUnwrap = true;
          continue;
        }
        do {
          child = child();
        } while (typeof child === "function" && !child.length);
      }
      if (Array.isArray(child)) {
        needsUnwrap = flattenArray(child, results, options);
      } else if (options?.skipNonRendered && (child == null || child === true || child === false || child === "")) {} else
        results.push(child);
    } catch (e) {
      if (!(e instanceof NotReadyError))
        throw e;
      notReady = e;
    }
  }
  if (notReady)
    throw notReady;
  return needsUnwrap;
}

// ../../../node_modules/.bun/solid-js@2.0.0-beta.7/node_modules/solid-js/dist/dev.js
var $DEVCOMP = Symbol("COMPONENT_DEV");
function devComponent(Comp, props) {
  return createRoot(() => {
    const owner = getOwner();
    owner._component = {
      fn: Comp,
      props,
      name: Comp.name
    };
    Object.assign(Comp, {
      [$DEVCOMP]: true
    });
    return untrack(() => Comp(props), `<${Comp.name || "Anonymous"}>`);
  }, {
    transparent: true
  });
}
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
  return devComponent(Comp, props || {});
}
if (globalThis) {
  if (!globalThis.Solid$$)
    globalThis.Solid$$ = true;
  else
    console.warn("You appear to have multiple instances of Solid. This can lead to unexpected behavior.");
}

// ../../../node_modules/.bun/@solidjs+universal@2.0.0-beta.7+b8339d087ca3085b/node_modules/@solidjs/universal/dist/dev.js
class NotReadyError2 extends Error {
  source;
  constructor(e) {
    super();
    this.source = e;
  }
}

class StatusError2 extends Error {
  source;
  constructor(e, t) {
    super(t instanceof Error ? t.message : String(t), {
      cause: t
    });
    this.source = e;
  }
}
var REACTIVE_NONE2 = 0;
var REACTIVE_CHECK2 = 1 << 0;
var REACTIVE_DIRTY2 = 1 << 1;
var REACTIVE_RECOMPUTING_DEPS2 = 1 << 2;
var REACTIVE_IN_HEAP2 = 1 << 3;
var REACTIVE_IN_HEAP_HEIGHT2 = 1 << 4;
var REACTIVE_ZOMBIE2 = 1 << 5;
var REACTIVE_DISPOSED2 = 1 << 6;
var REACTIVE_OPTIMISTIC_DIRTY2 = 1 << 7;
var REACTIVE_SNAPSHOT_STALE2 = 1 << 8;
var REACTIVE_LAZY2 = 1 << 9;
var STATUS_PENDING2 = 1 << 0;
var STATUS_ERROR2 = 1 << 1;
var STATUS_UNINITIALIZED2 = 1 << 2;
var EFFECT_RENDER2 = 1;
var EFFECT_USER2 = 2;
var EFFECT_TRACKED2 = 3;
var NOT_PENDING2 = {};
var defaultContext2 = {};
function actualInsertIntoHeap2(e, t) {
  const n = (e.i?.t ? e.i.u?.o : e.i?.o) ?? -1;
  if (n >= e.o)
    e.o = n + 1;
  const i = e.o;
  const r = t.l[i];
  if (r === undefined)
    t.l[i] = e;
  else {
    const t2 = r.T;
    t2.S = e;
    e.T = t2;
    r.T = e;
  }
  if (i > t.R)
    t.R = i;
}
function insertIntoHeap2(e, t) {
  let n = e.O;
  if (n & (REACTIVE_IN_HEAP2 | REACTIVE_RECOMPUTING_DEPS2))
    return;
  if (n & REACTIVE_CHECK2) {
    e.O = n & -4 | REACTIVE_DIRTY2 | REACTIVE_IN_HEAP2;
  } else
    e.O = n | REACTIVE_IN_HEAP2;
  if (!(n & REACTIVE_IN_HEAP_HEIGHT2))
    actualInsertIntoHeap2(e, t);
}
function insertIntoHeapHeight2(e, t) {
  let n = e.O;
  if (n & (REACTIVE_IN_HEAP2 | REACTIVE_RECOMPUTING_DEPS2 | REACTIVE_IN_HEAP_HEIGHT2))
    return;
  e.O = n | REACTIVE_IN_HEAP_HEIGHT2;
  actualInsertIntoHeap2(e, t);
}
function deleteFromHeap2(e, t) {
  const n = e.O;
  if (!(n & (REACTIVE_IN_HEAP2 | REACTIVE_IN_HEAP_HEIGHT2)))
    return;
  e.O = n & -25;
  const i = e.o;
  if (e.T === e)
    t.l[i] = undefined;
  else {
    const n2 = e.S;
    const r = t.l[i];
    const s = n2 ?? r;
    if (e === r)
      t.l[i] = n2;
    else
      e.T.S = n2;
    s.T = e.T;
  }
  e.T = e;
  e.S = undefined;
}
function markHeap2(e) {
  if (e._)
    return;
  e._ = true;
  for (let t = 0;t <= e.R; t++) {
    for (let n = e.l[t];n !== undefined; n = n.S) {
      if (n.O & REACTIVE_IN_HEAP2)
        markNode2(n);
    }
  }
}
function markNode2(e, t = REACTIVE_DIRTY2) {
  const n = e.O;
  if ((n & (REACTIVE_CHECK2 | REACTIVE_DIRTY2)) >= t)
    return;
  e.O = n & -4 | t;
  for (let t2 = e.I;t2 !== null; t2 = t2.h) {
    markNode2(t2.p, REACTIVE_CHECK2);
  }
  if (e.A !== null) {
    for (let t2 = e.A;t2 !== null; t2 = t2.N) {
      for (let e2 = t2.I;e2 !== null; e2 = e2.h) {
        markNode2(e2.p, REACTIVE_CHECK2);
      }
    }
  }
}
function runHeap2(e, t) {
  e._ = false;
  for (e.P = 0;e.P <= e.R; e.P++) {
    let n = e.l[e.P];
    while (n !== undefined) {
      if (n.O & REACTIVE_IN_HEAP2)
        t(n);
      else
        adjustHeight2(n, e);
      n = e.l[e.P];
    }
  }
  e.R = 0;
}
function adjustHeight2(e, t) {
  deleteFromHeap2(e, t);
  let n = e.o;
  for (let t2 = e.C;t2; t2 = t2.D) {
    const e2 = t2.m;
    const i = e2.V || e2;
    if (i.L && i.o >= n)
      n = i.o + 1;
  }
  if (e.o !== n) {
    e.o = n;
    for (let n2 = e.I;n2 !== null; n2 = n2.h) {
      insertIntoHeapHeight2(n2.p, t);
    }
  }
}
var transitions2 = new Set;
var dirtyQueue2 = {
  l: new Array(2000).fill(undefined),
  _: false,
  P: 0,
  R: 0
};
var zombieQueue2 = {
  l: new Array(2000).fill(undefined),
  _: false,
  P: 0,
  R: 0
};
var clock2 = 0;
var activeTransition2 = null;
var scheduled2 = false;
var stashedOptimisticReads2 = null;
function runLaneEffects2(e) {
  for (const t of activeLanes2) {
    if (t.U || t.k.size > 0)
      continue;
    const n = t.G[e - 1];
    if (n.length) {
      t.G[e - 1] = [];
      runQueue2(n, e);
    }
  }
}
function queueStashedOptimisticEffects2(e) {
  for (let t = e.I;t !== null; t = t.h) {
    const e2 = t.p;
    if (!e2.W)
      continue;
    if (e2.W === EFFECT_TRACKED2) {
      if (!e2.H) {
        e2.H = true;
        e2.F.enqueue(EFFECT_USER2, e2.M);
      }
      continue;
    }
    const n = e2.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2;
    if (n.P > e2.o)
      n.P = e2.o;
    insertIntoHeap2(e2, n);
  }
}
function mergeTransitionState2(e, t) {
  t.j = e;
  e.$.push(...t.$);
  for (const n of activeLanes2)
    if (n.K === t)
      n.K = e;
  e.Y.push(...t.Y);
  for (const n of t.Z)
    e.Z.add(n);
  for (const [n, i] of t.B) {
    let t2 = e.B.get(n);
    if (!t2)
      e.B.set(n, t2 = new Set);
    for (const e2 of i)
      t2.add(e2);
  }
}
function resolveOptimisticNodes2(e) {
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    n.q = undefined;
    if (n.X !== NOT_PENDING2) {
      n.J = n.X;
      n.X = NOT_PENDING2;
    }
    const i = n.ee;
    n.ee = NOT_PENDING2;
    if (i !== NOT_PENDING2 && n.J !== i)
      insertSubs2(n, true);
    n.K = null;
  }
  e.length = 0;
}
function cleanupCompletedLanes2(e) {
  for (const t of activeLanes2) {
    const n = e ? t.K === e : !t.K;
    if (!n)
      continue;
    if (!t.U) {
      if (t.G[0].length)
        runQueue2(t.G[0], EFFECT_RENDER2);
      if (t.G[1].length)
        runQueue2(t.G[1], EFFECT_USER2);
    }
    if (t.te.q === t)
      t.te.q = undefined;
    t.k.clear();
    t.G[0].length = 0;
    t.G[1].length = 0;
    activeLanes2.delete(t);
    signalLanes2.delete(t.te);
  }
}
function schedule2() {
  if (scheduled2)
    return;
  scheduled2 = true;
  if (!globalQueue2.ne && true)
    queueMicrotask(flush2);
}

class Queue2 {
  i = null;
  ie = [[], []];
  re = [];
  created = clock2;
  addChild(e) {
    this.re.push(e);
    e.i = this;
  }
  removeChild(e) {
    const t = this.re.indexOf(e);
    if (t >= 0) {
      this.re.splice(t, 1);
      e.i = null;
    }
  }
  notify(e, t, n, i) {
    if (this.i)
      return this.i.notify(e, t, n, i);
    return false;
  }
  run(e) {
    if (this.ie[e - 1].length) {
      const t = this.ie[e - 1];
      this.ie[e - 1] = [];
      runQueue2(t, e);
    }
    for (let t = 0;t < this.re.length; t++)
      this.re[t].run?.(e);
  }
  enqueue(e, t) {
    if (e) {
      if (currentOptimisticLane2) {
        const n = findLane2(currentOptimisticLane2);
        n.G[e - 1].push(t);
      } else {
        this.ie[e - 1].push(t);
      }
    }
    schedule2();
  }
  stashQueues(e) {
    e.ie[0].push(...this.ie[0]);
    e.ie[1].push(...this.ie[1]);
    this.ie = [[], []];
    for (let t = 0;t < this.re.length; t++) {
      let n = this.re[t];
      let i = e.re[t];
      if (!i) {
        i = {
          ie: [[], []],
          re: []
        };
        e.re[t] = i;
      }
      n.stashQueues(i);
    }
  }
  restoreQueues(e) {
    this.ie[0].push(...e.ie[0]);
    this.ie[1].push(...e.ie[1]);
    for (let t = 0;t < e.re.length; t++) {
      const n = e.re[t];
      let i = this.re[t];
      if (i)
        i.restoreQueues(n);
    }
  }
}

class GlobalQueue2 extends Queue2 {
  ne = false;
  se = [];
  Y = [];
  Z = new Set;
  static oe;
  static ue;
  static ce = null;
  flush() {
    if (this.ne)
      return;
    this.ne = true;
    try {
      runHeap2(dirtyQueue2, GlobalQueue2.oe);
      if (activeTransition2) {
        const e = transitionComplete2(activeTransition2);
        if (!e) {
          const e2 = activeTransition2;
          runHeap2(zombieQueue2, GlobalQueue2.oe);
          this.se = [];
          this.Y = [];
          this.Z = new Set;
          runLaneEffects2(EFFECT_RENDER2);
          runLaneEffects2(EFFECT_USER2);
          this.stashQueues(e2.ae);
          clock2++;
          scheduled2 = dirtyQueue2.R >= dirtyQueue2.P;
          reassignPendingTransition2(e2.se);
          activeTransition2 = null;
          if (!e2.$.length && e2.Y.length) {
            stashedOptimisticReads2 = new Set;
            for (let t2 = 0;t2 < e2.Y.length; t2++) {
              const n = e2.Y[t2];
              if (n.L || n.le)
                continue;
              stashedOptimisticReads2.add(n);
              queueStashedOptimisticEffects2(n);
            }
          }
          try {
            finalizePureQueue2(null, true);
          } finally {
            stashedOptimisticReads2 = null;
          }
          return;
        }
        this.se !== activeTransition2.se && this.se.push(...activeTransition2.se);
        this.restoreQueues(activeTransition2.ae);
        transitions2.delete(activeTransition2);
        const t = activeTransition2;
        activeTransition2 = null;
        reassignPendingTransition2(this.se);
        finalizePureQueue2(t);
      } else {
        if (transitions2.size)
          runHeap2(zombieQueue2, GlobalQueue2.oe);
        finalizePureQueue2();
      }
      clock2++;
      scheduled2 = dirtyQueue2.R >= dirtyQueue2.P;
      runLaneEffects2(EFFECT_RENDER2);
      this.run(EFFECT_RENDER2);
      runLaneEffects2(EFFECT_USER2);
      this.run(EFFECT_USER2);
      if (false)
        ;
    } finally {
      this.ne = false;
    }
  }
  notify(e, t, n, i) {
    if (t & STATUS_PENDING2) {
      if (n & STATUS_PENDING2) {
        const t2 = i !== undefined ? i : e.fe;
        if (activeTransition2 && t2) {
          const n2 = t2.source;
          let i2 = activeTransition2.B.get(n2);
          if (!i2)
            activeTransition2.B.set(n2, i2 = new Set);
          const r = i2.size;
          i2.add(e);
          if (i2.size !== r)
            schedule2();
        }
      }
      return true;
    }
    return false;
  }
  initTransition(e) {
    if (e)
      e = currentTransition2(e);
    if (e && e === activeTransition2)
      return;
    if (!e && activeTransition2 && activeTransition2.Ee === clock2)
      return;
    if (!activeTransition2) {
      activeTransition2 = e ?? {
        Ee: clock2,
        se: [],
        B: new Map,
        Y: [],
        Z: new Set,
        $: [],
        ae: {
          ie: [[], []],
          re: []
        },
        j: false
      };
    } else if (e) {
      const t = activeTransition2;
      mergeTransitionState2(e, t);
      transitions2.delete(t);
      activeTransition2 = e;
    }
    transitions2.add(activeTransition2);
    activeTransition2.Ee = clock2;
    if (this.se !== activeTransition2.se) {
      for (let e2 = 0;e2 < this.se.length; e2++) {
        const t = this.se[e2];
        t.K = activeTransition2;
        activeTransition2.se.push(t);
      }
      this.se = activeTransition2.se;
    }
    if (this.Y !== activeTransition2.Y) {
      for (let e2 = 0;e2 < this.Y.length; e2++) {
        const t = this.Y[e2];
        t.K = activeTransition2;
        activeTransition2.Y.push(t);
      }
      this.Y = activeTransition2.Y;
    }
    for (const e2 of activeLanes2) {
      if (!e2.K)
        e2.K = activeTransition2;
    }
    if (this.Z !== activeTransition2.Z) {
      for (const e2 of this.Z)
        activeTransition2.Z.add(e2);
      this.Z = activeTransition2.Z;
    }
  }
}
function insertSubs2(e, t = false) {
  const n = e.q || currentOptimisticLane2;
  const i = e.de !== undefined;
  for (let r = e.I;r !== null; r = r.h) {
    if (i && r.p.Te) {
      r.p.O |= REACTIVE_SNAPSHOT_STALE2;
      continue;
    }
    if (t && n) {
      r.p.O |= REACTIVE_OPTIMISTIC_DIRTY2;
      assignOrMergeLane2(r.p, n);
    } else if (t) {
      r.p.O |= REACTIVE_OPTIMISTIC_DIRTY2;
      r.p.q = undefined;
    }
    const e2 = r.p;
    if (e2.W === EFFECT_TRACKED2) {
      if (!e2.H) {
        e2.H = true;
        e2.F.enqueue(EFFECT_USER2, e2.M);
      }
      continue;
    }
    const s = r.p.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2;
    if (s.P > r.p.o)
      s.P = r.p.o;
    insertIntoHeap2(r.p, s);
  }
}
function commitPendingNodes2() {
  const e = globalQueue2.se;
  for (let t = 0;t < e.length; t++) {
    const n = e[t];
    if (n.X !== NOT_PENDING2) {
      n.J = n.X;
      n.X = NOT_PENDING2;
      if (n.W && n.W !== EFFECT_TRACKED2)
        n.H = true;
    }
    if (!(n.Se & STATUS_PENDING2))
      n.Se &= ~STATUS_UNINITIALIZED2;
    if (n.L)
      GlobalQueue2.ue(n, false, true);
  }
  e.length = 0;
}
function finalizePureQueue2(e = null, t = false) {
  const n = !t;
  if (n)
    commitPendingNodes2();
  if (!t)
    checkBoundaryChildren2(globalQueue2);
  if (dirtyQueue2.R >= dirtyQueue2.P)
    runHeap2(dirtyQueue2, GlobalQueue2.oe);
  if (n) {
    commitPendingNodes2();
    resolveOptimisticNodes2(e ? e.Y : globalQueue2.Y);
    e ? e.Z : globalQueue2.Z;
    cleanupCompletedLanes2(e);
  }
}
function checkBoundaryChildren2(e) {
  for (const t of e.re) {
    t.checkSources?.();
    checkBoundaryChildren2(t);
  }
}
function reassignPendingTransition2(e) {
  for (let t = 0;t < e.length; t++) {
    e[t].K = activeTransition2;
  }
}
var globalQueue2 = new GlobalQueue2;
function flush2() {
  if (globalQueue2.ne) {
    return;
  }
  while (scheduled2 || activeTransition2) {
    globalQueue2.flush();
  }
}
function runQueue2(e, t) {
  for (let n = 0;n < e.length; n++)
    e[n](t);
}
function reporterBlocksSource2(e, t) {
  if (e.O & (REACTIVE_ZOMBIE2 | REACTIVE_DISPOSED2))
    return false;
  if (e.Re === t || e.Oe?.has(t))
    return true;
  for (let n = e.C;n; n = n.D) {
    let e2 = n.m;
    while (e2) {
      if (e2 === t || e2.V === t)
        return true;
      e2 = e2._e;
    }
  }
  return !!(e.Se & STATUS_PENDING2 && e.fe instanceof NotReadyError2 && e.fe.source === t);
}
function transitionComplete2(e) {
  if (e.j)
    return true;
  if (e.$.length)
    return false;
  let t = true;
  for (const [n, i] of e.B) {
    let r = false;
    for (const e2 of i) {
      if (reporterBlocksSource2(e2, n)) {
        r = true;
        break;
      }
      i.delete(e2);
    }
    if (!r)
      e.B.delete(n);
    else if (n.Se & STATUS_PENDING2 && n.fe?.source === n) {
      t = false;
      break;
    }
  }
  if (t) {
    for (let n = 0;n < e.Y.length; n++) {
      const i = e.Y[n];
      if (hasActiveOverride2(i) && "Se" in i && i.Se & STATUS_PENDING2 && i.fe instanceof NotReadyError2 && i.fe.source !== i) {
        t = false;
        break;
      }
    }
  }
  t && (e.j = true);
  return t;
}
function currentTransition2(e) {
  while (e.j && typeof e.j === "object")
    e = e.j;
  return e;
}
function runInTransition2(e, t) {
  const n = activeTransition2;
  try {
    activeTransition2 = currentTransition2(e);
    return t();
  } finally {
    activeTransition2 = n;
  }
}
var signalLanes2 = new WeakMap;
var activeLanes2 = new Set;
function getOrCreateLane2(e) {
  let t = signalLanes2.get(e);
  if (t) {
    return findLane2(t);
  }
  const n = e._e;
  const i = n?.q ? findLane2(n.q) : null;
  t = {
    te: e,
    k: new Set,
    G: [[], []],
    U: null,
    K: activeTransition2,
    Ie: i
  };
  signalLanes2.set(e, t);
  activeLanes2.add(t);
  e.he = false;
  return t;
}
function findLane2(e) {
  while (e.U)
    e = e.U;
  return e;
}
function mergeLanes2(e, t) {
  e = findLane2(e);
  t = findLane2(t);
  if (e === t)
    return e;
  t.U = e;
  for (const n of t.k)
    e.k.add(n);
  e.G[0].push(...t.G[0]);
  e.G[1].push(...t.G[1]);
  return e;
}
function resolveLane2(e) {
  const t = e.q;
  if (!t)
    return;
  const n = findLane2(t);
  if (activeLanes2.has(n))
    return n;
  e.q = undefined;
  return;
}
function resolveTransition2(e) {
  return resolveLane2(e)?.K ?? e.K;
}
function hasActiveOverride2(e) {
  return !!(e.ee !== undefined && e.ee !== NOT_PENDING2);
}
function assignOrMergeLane2(e, t) {
  const n = findLane2(t);
  const i = e.q;
  if (i) {
    if (i.U) {
      e.q = t;
      return;
    }
    const r = findLane2(i);
    if (activeLanes2.has(r)) {
      if (r !== n && !hasActiveOverride2(e)) {
        if (n.Ie && findLane2(n.Ie) === r) {
          e.q = t;
        } else if (r.Ie && findLane2(r.Ie) === n)
          ;
        else
          mergeLanes2(n, r);
      }
      return;
    }
  }
  e.q = t;
}
function unlinkSubs2(e) {
  const t = e.m;
  const n = e.D;
  const i = e.h;
  const r = e.pe;
  if (i !== null)
    i.pe = r;
  else
    t.Ae = r;
  if (r !== null)
    r.h = i;
  else {
    t.I = i;
    if (i === null) {
      t.Ne?.();
      t.L && !t.Pe && !(t.O & REACTIVE_ZOMBIE2) && unobserved2(t);
    }
  }
  return n;
}
function unobserved2(e) {
  deleteFromHeap2(e, e.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2);
  let t = e.C;
  while (t !== null) {
    t = unlinkSubs2(t);
  }
  e.C = null;
  e.ge = null;
  disposeChildren2(e, true);
}
function link2(e, t) {
  const n = t.ge;
  if (n !== null && n.m === e)
    return;
  let i = null;
  const r = t.O & REACTIVE_RECOMPUTING_DEPS2;
  if (r) {
    i = n !== null ? n.D : t.C;
    if (i !== null && i.m === e) {
      t.ge = i;
      return;
    }
  }
  const s = e.Ae;
  if (s !== null && s.p === t && (!r || isValidLink2(s, t)))
    return;
  const o = t.ge = e.Ae = {
    m: e,
    p: t,
    D: i,
    pe: s,
    h: null
  };
  if (n !== null)
    n.D = o;
  else
    t.C = o;
  if (s !== null)
    s.h = o;
  else
    e.I = o;
}
function isValidLink2(e, t) {
  const n = t.ge;
  if (n !== null) {
    let i = t.C;
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
function markDisposal2(e) {
  let t = e.Ce;
  while (t) {
    t.O |= REACTIVE_ZOMBIE2;
    if (t.O & REACTIVE_IN_HEAP2) {
      deleteFromHeap2(t, dirtyQueue2);
      insertIntoHeap2(t, zombieQueue2);
    }
    markDisposal2(t);
    t = t.De;
  }
}
function disposeChildren2(e, t = false, n) {
  if (e.O & REACTIVE_DISPOSED2)
    return;
  if (t)
    e.O = REACTIVE_DISPOSED2;
  if (t && e.L)
    e.ye = null;
  let i = n ? e.ve : e.Ce;
  while (i) {
    const e2 = i.De;
    if (i.C) {
      const e3 = i;
      deleteFromHeap2(e3, e3.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2);
      let t2 = e3.C;
      do {
        t2 = unlinkSubs2(t2);
      } while (t2 !== null);
      e3.C = null;
      e3.ge = null;
    }
    disposeChildren2(i, true);
    i = e2;
  }
  if (n) {
    e.ve = null;
  } else {
    e.Ce = null;
    e.we = 0;
  }
  runDisposal2(e, n);
}
function runDisposal2(e, t) {
  let n = t ? e.me : e.Ve;
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
  t ? e.me = null : e.Ve = null;
}
function cleanup2(e) {
  if (!context2)
    return e;
  if (!context2.Ve)
    context2.Ve = e;
  else if (Array.isArray(context2.Ve))
    context2.Ve.push(e);
  else
    context2.Ve = [context2.Ve, e];
  return e;
}
function addPendingSource2(e, t) {
  if (e.Re === t || e.Oe?.has(t))
    return false;
  if (!e.Re) {
    e.Re = t;
    return true;
  }
  if (!e.Oe) {
    e.Oe = new Set([e.Re, t]);
  } else {
    e.Oe.add(t);
  }
  e.Re = undefined;
  return true;
}
function removePendingSource2(e, t) {
  if (e.Re) {
    if (e.Re !== t)
      return false;
    e.Re = undefined;
    return true;
  }
  if (!e.Oe?.delete(t))
    return false;
  if (e.Oe.size === 1) {
    e.Re = e.Oe.values().next().value;
    e.Oe = undefined;
  } else if (e.Oe.size === 0) {
    e.Oe = undefined;
  }
  return true;
}
function clearPendingSources2(e) {
  e.Re = undefined;
  e.Oe?.clear();
  e.Oe = undefined;
}
function setPendingError2(e, t, n) {
  if (!t) {
    e.fe = null;
    return;
  }
  if (n instanceof NotReadyError2 && n.source === t) {
    e.fe = n;
    return;
  }
  const i = e.fe;
  if (!(i instanceof NotReadyError2) || i.source !== t) {
    e.fe = new NotReadyError2(t);
  }
}
function forEachDependent2(e, t) {
  for (let n = e.I;n !== null; n = n.h)
    t(n.p);
  for (let n = e.A;n !== null; n = n.N) {
    for (let e2 = n.I;e2 !== null; e2 = e2.h)
      t(e2.p);
  }
}
function settlePendingSource2(e) {
  let t = false;
  const n = new Set;
  const settle = (i) => {
    if (n.has(i) || !removePendingSource2(i, e))
      return;
    n.add(i);
    i.Ee = clock2;
    const r = i.Re ?? i.Oe?.values().next().value;
    if (r) {
      setPendingError2(i, r);
      updatePendingSignal2(i);
    } else {
      i.Se &= ~STATUS_PENDING2;
      setPendingError2(i);
      updatePendingSignal2(i);
      if (i.Ue) {
        if (i.W === EFFECT_TRACKED2) {
          const e2 = i;
          if (!e2.H) {
            e2.H = true;
            e2.F.enqueue(EFFECT_USER2, e2.M);
          }
        } else {
          const e2 = i.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2;
          if (e2.P > i.o)
            e2.P = i.o;
          insertIntoHeap2(i, e2);
        }
        t = true;
      }
      i.Ue = false;
    }
    forEachDependent2(i, settle);
  };
  forEachDependent2(e, settle);
  if (t)
    schedule2();
}
function handleAsync2(e, t, n) {
  const i = typeof t === "object" && t !== null;
  const r = i && untrack2(() => t[Symbol.asyncIterator]);
  const s = !r && i && untrack2(() => typeof t.then === "function");
  if (!s && !r) {
    e.ye = null;
    return t;
  }
  e.ye = t;
  let o;
  const handleError = (n2) => {
    if (e.ye !== t)
      return;
    globalQueue2.initTransition(resolveTransition2(e));
    notifyStatus2(e, n2 instanceof NotReadyError2 ? STATUS_PENDING2 : STATUS_ERROR2, n2);
    e.Ee = clock2;
  };
  const asyncWrite = (i2, r2) => {
    if (e.ye !== t)
      return;
    if (e.O & (REACTIVE_DIRTY2 | REACTIVE_OPTIMISTIC_DIRTY2))
      return;
    globalQueue2.initTransition(resolveTransition2(e));
    const s2 = !!(e.Se & STATUS_UNINITIALIZED2);
    clearStatus2(e);
    const o2 = resolveLane2(e);
    if (o2)
      o2.k.delete(e);
    if (e.ee !== undefined) {
      if (e.ee !== undefined && e.ee !== NOT_PENDING2)
        e.X = i2;
      else {
        e.J = i2;
        insertSubs2(e);
      }
      e.Ee = clock2;
    } else if (o2) {
      const t2 = e.W;
      const n2 = e.J;
      const r3 = e.ke;
      if (!t2 && s2 || !r3 || !r3(i2, n2)) {
        e.J = i2;
        e.Ee = clock2;
        if (e.xe) {
          setSignal2(e.xe, i2);
        }
        insertSubs2(e, true);
      }
    } else {
      setSignal2(e, () => i2);
    }
    settlePendingSource2(e);
    schedule2();
    flush2();
    r2?.();
  };
  if (s) {
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
      globalQueue2.initTransition(resolveTransition2(e));
      throw new NotReadyError2(context2);
    }
  }
  if (r) {
    const n2 = t[Symbol.asyncIterator]();
    let i2 = false;
    let r2 = false;
    cleanup2(() => {
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
      let s3, u = false, c = true;
      n2.next().then((n3) => {
        if (c) {
          s3 = n3;
          u = true;
          if (n3.done)
            r2 = true;
        } else if (e.ye !== t) {
          return;
        } else if (!n3.done)
          asyncWrite(n3.value, iterate);
        else {
          r2 = true;
          schedule2();
          flush2();
        }
      }, (n3) => {
        if (!c && e.ye === t) {
          r2 = true;
          handleError(n3);
        }
      });
      c = false;
      if (u && !s3.done) {
        o = s3.value;
        i2 = true;
        return iterate();
      }
      return u && s3.done;
    };
    const s2 = iterate();
    if (!i2 && !s2) {
      globalQueue2.initTransition(resolveTransition2(e));
      throw new NotReadyError2(context2);
    }
  }
  return o;
}
function clearStatus2(e, t = false) {
  clearPendingSources2(e);
  e.Ue = false;
  e.Se = t ? 0 : e.Se & STATUS_UNINITIALIZED2;
  setPendingError2(e);
  updatePendingSignal2(e);
  e.Ge?.();
}
function notifyStatus2(e, t, n, i, r) {
  if (t === STATUS_ERROR2 && !(n instanceof StatusError2) && !(n instanceof NotReadyError2))
    n = new StatusError2(e, n);
  const s = t === STATUS_PENDING2 && n instanceof NotReadyError2 ? n.source : undefined;
  const o = s === e;
  const u = t === STATUS_PENDING2 && e.ee !== undefined && !o;
  const c = u && hasActiveOverride2(e);
  if (!i) {
    if (t === STATUS_PENDING2 && s) {
      addPendingSource2(e, s);
      e.Se = STATUS_PENDING2 | e.Se & STATUS_UNINITIALIZED2;
      setPendingError2(e, s, n);
    } else {
      clearPendingSources2(e);
      e.Se = t | (t !== STATUS_ERROR2 ? e.Se & STATUS_UNINITIALIZED2 : 0);
      e.fe = n;
    }
    updatePendingSignal2(e);
  }
  if (r && !i) {
    assignOrMergeLane2(e, r);
  }
  const a = i || c;
  const l = i || u ? undefined : r;
  if (e.Ge) {
    if (i && t === STATUS_PENDING2) {
      return;
    }
    if (a) {
      e.Ge(t, n);
    } else {
      e.Ge();
    }
    return;
  }
  forEachDependent2(e, (e2) => {
    e2.Ee = clock2;
    if (t === STATUS_PENDING2 && s && e2.Re !== s && !e2.Oe?.has(s) || t !== STATUS_PENDING2 && (e2.fe !== n || e2.Re || e2.Oe)) {
      if (!a && !e2.K)
        globalQueue2.se.push(e2);
      notifyStatus2(e2, t, n, a, l);
    }
  });
}
var externalSourceConfig2 = null;
GlobalQueue2.oe = recompute2;
GlobalQueue2.ue = disposeChildren2;
var tracking2 = false;
var stale2 = false;
var context2 = null;
var currentOptimisticLane2 = null;
function recompute2(e, t = false) {
  const n = e.W;
  if (!t) {
    if (e.K && (!n || activeTransition2) && activeTransition2 !== e.K)
      globalQueue2.initTransition(e.K);
    deleteFromHeap2(e, e.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2);
    e.ye = null;
    if (e.K || n === EFFECT_TRACKED2)
      disposeChildren2(e);
    else {
      markDisposal2(e);
      e.me = e.Ve;
      e.ve = e.Ce;
      e.Ve = null;
      e.Ce = null;
      e.we = 0;
    }
  }
  const i = !!(e.O & REACTIVE_OPTIMISTIC_DIRTY2);
  const r = e.ee !== undefined && e.ee !== NOT_PENDING2;
  const s = !!(e.Se & STATUS_PENDING2);
  const o = !!(e.Se & STATUS_UNINITIALIZED2);
  const u = context2;
  context2 = e;
  e.ge = null;
  e.O = REACTIVE_RECOMPUTING_DEPS2;
  e.Ee = clock2;
  let c = e.X === NOT_PENDING2 ? e.J : e.X;
  let a = e.o;
  let l = tracking2;
  let f = currentOptimisticLane2;
  tracking2 = true;
  if (i) {
    const t2 = resolveLane2(e);
    if (t2)
      currentOptimisticLane2 = t2;
  }
  try {
    c = handleAsync2(e, e.L(c));
    clearStatus2(e, t);
    const n2 = resolveLane2(e);
    if (n2) {
      n2.k.delete(e);
      updatePendingSignal2(n2.te);
    }
  } catch (t2) {
    if (t2 instanceof NotReadyError2 && currentOptimisticLane2) {
      const t3 = findLane2(currentOptimisticLane2);
      if (t3.te !== e) {
        t3.k.add(e);
        e.q = t3;
        updatePendingSignal2(t3.te);
      }
    }
    if (t2 instanceof NotReadyError2)
      e.Ue = true;
    notifyStatus2(e, t2 instanceof NotReadyError2 ? STATUS_PENDING2 : STATUS_ERROR2, t2, undefined, t2 instanceof NotReadyError2 ? e.q : undefined);
  } finally {
    tracking2 = l;
    e.O = REACTIVE_NONE2 | (t ? e.O & REACTIVE_SNAPSHOT_STALE2 : 0);
    context2 = u;
  }
  if (!e.fe) {
    const u2 = e.ge;
    let l2 = u2 !== null ? u2.D : e.C;
    if (l2 !== null) {
      do {
        l2 = unlinkSubs2(l2);
      } while (l2 !== null);
      if (u2 !== null)
        u2.D = null;
      else
        e.C = null;
    }
    const f2 = r ? e.ee : e.X === NOT_PENDING2 ? e.J : e.X;
    const E = !n && o || !e.ke || !e.ke(f2, c);
    if (E) {
      const o2 = r ? e.ee : undefined;
      if (t || n && activeTransition2 !== e.K || i) {
        e.J = c;
        if (r && i) {
          e.ee = c;
          e.X = c;
        }
      } else
        e.X = c;
      if (r && !i && s && !e.he)
        e.ee = c;
      if (!r || i || e.ee !== o2)
        insertSubs2(e, i || r);
    } else if (r) {
      e.X = c;
    } else if (e.o != a) {
      for (let t2 = e.I;t2 !== null; t2 = t2.h) {
        insertIntoHeapHeight2(t2.p, t2.p.O & REACTIVE_ZOMBIE2 ? zombieQueue2 : dirtyQueue2);
      }
    }
  }
  currentOptimisticLane2 = f;
  (!t || e.Se & STATUS_PENDING2) && !e.K && !(activeTransition2 && r) && globalQueue2.se.push(e);
  e.K && n && activeTransition2 !== e.K && runInTransition2(e.K, () => recompute2(e));
}
function updateIfNecessary2(e) {
  if (e.O & REACTIVE_CHECK2) {
    for (let t = e.C;t; t = t.D) {
      const n = t.m;
      const i = n.V || n;
      if (i.L) {
        updateIfNecessary2(i);
      }
      if (e.O & REACTIVE_DIRTY2) {
        break;
      }
    }
  }
  if (e.O & (REACTIVE_DIRTY2 | REACTIVE_OPTIMISTIC_DIRTY2) || e.fe && e.Ee < clock2 && !e.ye) {
    recompute2(e);
  }
  e.O = e.O & (REACTIVE_SNAPSHOT_STALE2 | REACTIVE_IN_HEAP2 | REACTIVE_IN_HEAP_HEIGHT2);
}
function computed2(e, t, n) {
  const i = n?.transparent;
  const r = {
    id: n?.id ?? context2?.id,
    be: i,
    ke: n?.equals != null ? n.equals : isEqual2,
    le: !!n?.ownedWrite,
    Ne: n?.unobserved,
    Ve: null,
    F: context2?.F ?? globalQueue2,
    Le: context2?.Le ?? defaultContext2,
    we: 0,
    L: e,
    J: t,
    o: 0,
    A: null,
    S: undefined,
    T: null,
    C: null,
    ge: null,
    I: null,
    Ae: null,
    i: context2,
    De: null,
    Ce: null,
    O: n?.lazy ? REACTIVE_LAZY2 : REACTIVE_NONE2,
    Se: STATUS_UNINITIALIZED2,
    Ee: clock2,
    X: NOT_PENDING2,
    me: null,
    ve: null,
    ye: null,
    K: null
  };
  r.T = r;
  const s = context2?.t ? context2.u : context2;
  if (context2) {
    const e2 = context2.Ce;
    if (e2 === null) {
      context2.Ce = r;
    } else {
      r.De = e2;
      context2.Ce = r;
    }
  }
  if (s)
    r.o = s.o + 1;
  !n?.lazy && recompute2(r, true);
  return r;
}
function isEqual2(e, t) {
  return e === t;
}
function untrack2(e, t) {
  if (!tracking2 && true)
    return e();
  const n = tracking2;
  tracking2 = false;
  try {
    if (externalSourceConfig2)
      ;
    return e();
  } finally {
    tracking2 = n;
  }
}
function read2(e) {
  let t = context2;
  if (t?.t)
    t = t.u;
  const n = e;
  if (typeof n.L === "function") {
    const t2 = e;
    if (t2.O & REACTIVE_LAZY2) {
      t2.O &= ~REACTIVE_LAZY2;
      recompute2(t2, true);
    } else if (t2.O & REACTIVE_DISPOSED2) {
      recompute2(t2, true);
    }
  }
  const i = e.V || e;
  if (t && tracking2) {
    link2(e, t);
    if (i.L) {
      const n2 = e.O & REACTIVE_ZOMBIE2;
      if (i.o >= (n2 ? zombieQueue2.P : dirtyQueue2.P)) {
        markNode2(t);
        markHeap2(n2 ? zombieQueue2 : dirtyQueue2);
        updateIfNecessary2(i);
      }
      const r2 = i.o;
      if (r2 >= t.o && e.i !== t) {
        t.o = r2 + 1;
      }
    }
  }
  if (i.Se & STATUS_PENDING2) {
    if (t && true) {
      if (currentOptimisticLane2) {
        const n2 = i.q;
        const r2 = findLane2(currentOptimisticLane2);
        if (n2 && findLane2(n2) === r2 && !hasActiveOverride2(i)) {
          if (!tracking2 && e !== t)
            link2(e, t);
          throw i.fe;
        }
      } else {
        if (!tracking2 && e !== t)
          link2(e, t);
        throw i.fe;
      }
    } else if (t && i !== e && i.Se & STATUS_UNINITIALIZED2) {
      if (!tracking2 && e !== t)
        link2(e, t);
      throw i.fe;
    } else if (!t && i.Se & STATUS_UNINITIALIZED2) {
      throw i.fe;
    }
  }
  if (e.L && e.Se & STATUS_ERROR2) {
    if (e.Ee < clock2) {
      recompute2(e);
      return read2(e);
    } else
      throw e.fe;
  }
  if (e.ee !== undefined && e.ee !== NOT_PENDING2) {
    return e.ee;
  }
  const r = !t || currentOptimisticLane2 !== null && (e.ee !== undefined || e.q || i === e && stale2 || !!(i.Se & STATUS_PENDING2)) || e.X === NOT_PENDING2 || stale2 ? e.J : e.X;
  if (!t && i === e && typeof n.L === "function" && !n.Pe && !(i.Se & STATUS_PENDING2) && !n.i && !e.I) {
    unobserved2(e);
  }
  return r;
}
function setSignal2(e, t) {
  if (e.K && activeTransition2 !== e.K)
    globalQueue2.initTransition(e.K);
  const n = e.ee !== undefined && true;
  const i = e.ee !== undefined && e.ee !== NOT_PENDING2;
  const r = n ? i ? e.ee : e.J : e.X === NOT_PENDING2 ? e.J : e.X;
  if (typeof t === "function")
    t = t(r);
  const s = !e.ke || !e.ke(r, t) || !!(e.Se & STATUS_UNINITIALIZED2);
  if (!s) {
    if (n && i && e.L) {
      insertSubs2(e, true);
      schedule2();
    }
    return t;
  }
  if (n) {
    const n2 = e.ee === NOT_PENDING2;
    if (!n2)
      globalQueue2.initTransition(resolveTransition2(e));
    if (n2) {
      e.X = e.J;
      globalQueue2.Y.push(e);
    }
    e.he = true;
    const i2 = getOrCreateLane2(e);
    e.q = i2;
    e.ee = t;
  } else {
    if (e.X === NOT_PENDING2)
      globalQueue2.se.push(e);
    e.X = t;
  }
  updatePendingSignal2(e);
  if (e.xe) {
    setSignal2(e.xe, t);
  }
  e.Ee = clock2;
  insertSubs2(e, n);
  schedule2();
  return t;
}
function computePendingState2(e) {
  const t = e;
  const n = e.V;
  if (n && e.X !== NOT_PENDING2) {
    return !n.ye && !(n.Se & STATUS_PENDING2);
  }
  if (e.ee !== undefined && e.ee !== NOT_PENDING2) {
    if (t.Se & STATUS_PENDING2 && !(t.Se & STATUS_UNINITIALIZED2))
      return true;
    if (e._e) {
      const t2 = e.q ? findLane2(e.q) : null;
      return !!(t2 && t2.k.size > 0);
    }
    return true;
  }
  if (e.ee !== undefined && e.ee === NOT_PENDING2 && !e._e) {
    return false;
  }
  if (e.X !== NOT_PENDING2 && !(t.Se & STATUS_UNINITIALIZED2))
    return true;
  return !!(t.Se & STATUS_PENDING2 && !(t.Se & STATUS_UNINITIALIZED2));
}
function updatePendingSignal2(e) {
  if (e.Fe) {
    const t = computePendingState2(e);
    const n = e.Fe;
    setSignal2(n, t);
    if (!t && n.q) {
      const t2 = resolveLane2(e);
      if (t2 && t2.k.size > 0) {
        const e2 = findLane2(n.q);
        if (e2 !== t2) {
          mergeLanes2(t2, e2);
        }
      }
      signalLanes2.delete(n);
      n.q = undefined;
    }
  }
}
function accessor2(e) {
  const t = read2.bind(null, e);
  t.$r = true;
  return t;
}
function createMemo3(e, t) {
  let n = computed2(e, undefined, t);
  return accessor2(n);
}
function isWrappable2(e) {
  return e != null && typeof e === "object" && !Object.isFrozen(e) && !(typeof Node !== "undefined" && e instanceof Node);
}
var DELETE2 = Symbol(0);
function updatePath2(e, t, n = 0) {
  let i, r = e;
  if (n < t.length - 1) {
    i = t[n];
    const s2 = typeof i;
    const o = Array.isArray(e);
    if (Array.isArray(i)) {
      for (let r2 = 0;r2 < i.length; r2++) {
        t[n] = i[r2];
        updatePath2(e, t, n);
      }
      t[n] = i;
      return;
    } else if (o && s2 === "function") {
      for (let r2 = 0;r2 < e.length; r2++) {
        if (i(e[r2], r2)) {
          t[n] = r2;
          updatePath2(e, t, n);
        }
      }
      t[n] = i;
      return;
    } else if (o && s2 === "object") {
      const {
        from: r2 = 0,
        to: s3 = e.length - 1,
        by: o2 = 1
      } = i;
      for (let i2 = r2;i2 <= s3; i2 += o2) {
        t[n] = i2;
        updatePath2(e, t, n);
      }
      t[n] = i;
      return;
    } else if (n < t.length - 2) {
      updatePath2(e[i], t, n + 1);
      return;
    }
    r = e[i];
  }
  let s = t[t.length - 1];
  if (typeof s === "function") {
    s = s(r);
    if (s === r)
      return;
  }
  if (i === undefined && s == undefined)
    return;
  if (s === DELETE2) {
    delete e[i];
  } else if (i === undefined || isWrappable2(r) && isWrappable2(s) && !Array.isArray(s)) {
    const t2 = i !== undefined ? e[i] : e;
    const n2 = Object.keys(s);
    for (let e2 = 0;e2 < n2.length; e2++) {
      const i2 = n2[e2];
      const r2 = Object.getOwnPropertyDescriptor(s, i2);
      if (r2.get || r2.set)
        Object.defineProperty(t2, i2, r2);
      else
        t2[i2] = r2.value;
    }
  } else {
    e[i] = s;
  }
}
Object.assign(function storePath3(...e) {
  return (t) => {
    updatePath2(t, e);
  };
}, {
  DELETE: DELETE2
});
var effect2 = (fn, effectFn) => createRenderEffect2(fn, effectFn, {
  transparent: true
});
var memo = (fn, transparent) => transparent ? fn.$r ? fn : createMemo3(() => fn(), {
  transparent: true
}) : createMemo2(() => fn());
function createRenderer({
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
  function insert(parent, accessor3, marker, initial) {
    const multi = marker !== undefined;
    if (multi && !initial)
      initial = [];
    if (typeof accessor3 !== "function") {
      accessor3 = normalize(accessor3, multi, true);
      if (typeof accessor3 !== "function")
        return insertExpression(parent, accessor3, initial, marker);
    }
    accessor3 = memo(accessor3, true);
    if (multi && initial.length === 0) {
      const sentinel = createSentinel();
      insertNode(parent, sentinel, marker);
      initial = [sentinel];
    }
    effect2(() => normalize(accessor3, multi), (value, current = initial) => insertExpression(parent, value, current, marker));
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
        const node = getNextSibling(a[--aEnd]);
        insertNode(parentNode, b[bStart++], getNextSibling(a[aStart++]));
        insertNode(parentNode, b[--bEnd], node);
        a[aEnd] = b[bEnd];
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
      createRoot((dispose) => {
        disposer = dispose;
        insert(element, code());
      });
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

// ../../core/src/constants.ts
var BufferUsage = {
  Uniform: 1 << 0,
  Storage: 1 << 1,
  Vertex: 1 << 2,
  Index: 1 << 3,
  Indirect: 1 << 4,
  CopyDst: 1 << 5,
  CopySrc: 1 << 6,
  MapRead: 1 << 7,
  MapWrite: 1 << 8
};
var ElementTypeMap = {
  window: 0 /* Window */,
  rect: 1 /* Rectangle */,
  rectangle: 1 /* Rectangle */,
  oval: 3 /* Oval */,
  path: 5 /* Path */,
  view: 6 /* View */,
  text: 7 /* Text */,
  string: 8 /* String */,
  texture: 9 /* Texture */,
  audio: 10 /* Audio */,
  "d-rect": 11 /* DetachedRectangle */,
  "d-oval": 12 /* DetachedOval */,
  "d-path": 13 /* DetachedPath */,
  "d-texture": 14 /* DetachedTexture */,
  "d-text": 15 /* DetachedText */
};
var PropertyNameMap = {
  width: 1 /* Width */,
  height: 2 /* Height */,
  x: 10 /* X */,
  y: 11 /* Y */,
  w: 12 /* W */,
  h: 13 /* H */,
  r: 14 /* R */,
  fill: 20 /* FillColor */,
  color: 20 /* FillColor */,
  stroke: 21 /* StrokeColor */,
  strokeWidth: 22 /* StrokeWidth */,
  style: 23 /* Style */,
  blendMode: 24 /* BlendMode */,
  drawStyle: 25 /* DrawStyle */,
  strokeCap: 26 /* StrokeCap */,
  strokeJoin: 27 /* StrokeJoin */,
  strokeMiter: 28 /* StrokeMiter */,
  rotate: 29 /* Rotate */,
  scale: 30 /* Scale */,
  cx: 33 /* Cx */,
  cy: 34 /* Cy */,
  d: 100 /* D */,
  title: 101 /* Title */,
  fillRule: 102 /* FillRule */,
  vsync: 3 /* Vsync */,
  trace: 5 /* Trace */,
  fps: 4 /* FPS */,
  display: 200 /* Display */,
  flexDirection: 201 /* FlexDirection */,
  gap: 202 /* Gap */,
  flexGrow: 203 /* FlexGrow */,
  alignItems: 204 /* AlignItems */,
  justifyContent: 205 /* JustifyContent */,
  alignContent: 206 /* AlignContent */,
  alignSelf: 207 /* AlignSelf */,
  flexWrap: 208 /* FlexWrap */,
  flexShrink: 209 /* FlexShrink */,
  flexBasis: 210 /* FlexBasis */,
  rowGap: 211 /* RowGap */,
  columnGap: 212 /* ColumnGap */,
  gridAutoFlow: 213 /* GridAutoFlow */,
  gridAutoColumns: 214 /* GridAutoColumns */,
  gridAutoRows: 215 /* GridAutoRows */,
  gridColumnStart: 216 /* GridColumnStart */,
  gridColumnEnd: 217 /* GridColumnEnd */,
  gridRowStart: 218 /* GridRowStart */,
  gridRowEnd: 219 /* GridRowEnd */,
  gridTemplateColumns: 220 /* GridTemplateColumns */,
  gridTemplateRows: 221 /* GridTemplateRows */,
  padding: 222 /* Padding */,
  paddingTop: 223 /* PaddingTop */,
  paddingRight: 224 /* PaddingRight */,
  paddingBottom: 225 /* PaddingBottom */,
  paddingLeft: 226 /* PaddingLeft */,
  margin: 227 /* Margin */,
  marginTop: 228 /* MarginTop */,
  marginRight: 229 /* MarginRight */,
  marginBottom: 230 /* MarginBottom */,
  marginLeft: 231 /* MarginLeft */,
  position: 232 /* Position */,
  top: 233 /* Top */,
  right: 234 /* Right */,
  bottom: 235 /* Bottom */,
  left: 236 /* Left */,
  overflow: 237 /* Overflow */,
  pointerEvents: 238 /* PointerEvents */,
  minWidth: 239 /* MinWidth */,
  minHeight: 240 /* MinHeight */,
  maxWidth: 241 /* MaxWidth */,
  maxHeight: 242 /* MaxHeight */,
  fontSize: 301 /* FontSize */,
  fontStyle: 302 /* FontStyle */,
  maxLines: 303 /* MaxLines */,
  textAlign: 304 /* TextAlign */,
  src: 403 /* BinarySrc */,
  textureId: 404 /* TextureId */,
  srcX: 405 /* SrcX */,
  srcY: 406 /* SrcY */,
  srcW: 407 /* SrcW */,
  srcH: 408 /* SrcH */,
  params: 409 /* Params */,
  play: 500 /* Play */
};
var PropertyTypeMap = {
  [1 /* Width */]: 1 /* U32 */,
  [2 /* Height */]: 1 /* U32 */,
  [20 /* FillColor */]: 4 /* Color */,
  [21 /* StrokeColor */]: 4 /* Color */,
  [23 /* Style */]: 1 /* U32 */,
  [24 /* BlendMode */]: 1 /* U32 */,
  [25 /* DrawStyle */]: 1 /* U32 */,
  [26 /* StrokeCap */]: 1 /* U32 */,
  [27 /* StrokeJoin */]: 1 /* U32 */,
  [10 /* X */]: 0 /* F32 */,
  [11 /* Y */]: 0 /* F32 */,
  [12 /* W */]: 0 /* F32 */,
  [13 /* H */]: 0 /* F32 */,
  [14 /* R */]: 0 /* F32 */,
  [22 /* StrokeWidth */]: 0 /* F32 */,
  [28 /* StrokeMiter */]: 0 /* F32 */,
  [29 /* Rotate */]: 0 /* F32 */,
  [30 /* Scale */]: 0 /* F32 */,
  [33 /* Cx */]: 0 /* F32 */,
  [34 /* Cy */]: 0 /* F32 */,
  [100 /* D */]: 2 /* String */,
  [101 /* Title */]: 2 /* String */,
  [102 /* FillRule */]: 1 /* U32 */,
  [3 /* Vsync */]: 3 /* Boolean */,
  [4 /* FPS */]: 3 /* Boolean */,
  [5 /* Trace */]: 3 /* Boolean */,
  [200 /* Display */]: 1 /* U32 */,
  [201 /* FlexDirection */]: 1 /* U32 */,
  [202 /* Gap */]: 0 /* F32 */,
  [203 /* FlexGrow */]: 0 /* F32 */,
  [204 /* AlignItems */]: 1 /* U32 */,
  [205 /* JustifyContent */]: 1 /* U32 */,
  [206 /* AlignContent */]: 1 /* U32 */,
  [207 /* AlignSelf */]: 1 /* U32 */,
  [208 /* FlexWrap */]: 1 /* U32 */,
  [209 /* FlexShrink */]: 0 /* F32 */,
  [210 /* FlexBasis */]: 0 /* F32 */,
  [211 /* RowGap */]: 0 /* F32 */,
  [212 /* ColumnGap */]: 0 /* F32 */,
  [213 /* GridAutoFlow */]: 1 /* U32 */,
  [214 /* GridAutoColumns */]: 0 /* F32 */,
  [215 /* GridAutoRows */]: 0 /* F32 */,
  [216 /* GridColumnStart */]: 1 /* U32 */,
  [217 /* GridColumnEnd */]: 1 /* U32 */,
  [218 /* GridRowStart */]: 1 /* U32 */,
  [219 /* GridRowEnd */]: 1 /* U32 */,
  [220 /* GridTemplateColumns */]: 2 /* String */,
  [221 /* GridTemplateRows */]: 2 /* String */,
  [222 /* Padding */]: 0 /* F32 */,
  [223 /* PaddingTop */]: 0 /* F32 */,
  [224 /* PaddingRight */]: 0 /* F32 */,
  [225 /* PaddingBottom */]: 0 /* F32 */,
  [226 /* PaddingLeft */]: 0 /* F32 */,
  [227 /* Margin */]: 0 /* F32 */,
  [228 /* MarginTop */]: 0 /* F32 */,
  [229 /* MarginRight */]: 0 /* F32 */,
  [230 /* MarginBottom */]: 0 /* F32 */,
  [231 /* MarginLeft */]: 0 /* F32 */,
  [232 /* Position */]: 1 /* U32 */,
  [237 /* Overflow */]: 1 /* U32 */,
  [238 /* PointerEvents */]: 1 /* U32 */,
  [239 /* MinWidth */]: 0 /* F32 */,
  [240 /* MinHeight */]: 0 /* F32 */,
  [241 /* MaxWidth */]: 0 /* F32 */,
  [242 /* MaxHeight */]: 0 /* F32 */,
  [233 /* Top */]: 0 /* F32 */,
  [234 /* Right */]: 0 /* F32 */,
  [235 /* Bottom */]: 0 /* F32 */,
  [236 /* Left */]: 0 /* F32 */,
  [300 /* Text */]: 2 /* String */,
  [301 /* FontSize */]: 0 /* F32 */,
  [302 /* FontStyle */]: 1 /* U32 */,
  [303 /* MaxLines */]: 1 /* U32 */,
  [304 /* TextAlign */]: 1 /* U32 */,
  [403 /* BinarySrc */]: 1 /* U32 */,
  [404 /* TextureId */]: 1 /* U32 */,
  [405 /* SrcX */]: 0 /* F32 */,
  [406 /* SrcY */]: 0 /* F32 */,
  [407 /* SrcW */]: 0 /* F32 */,
  [408 /* SrcH */]: 0 /* F32 */,
  [409 /* Params */]: 6 /* ParamMap */,
  [500 /* Play */]: 1 /* U32 */
};
var mappings = {
  display: {
    block: 0,
    flex: 1,
    grid: 2,
    none: 3
  },
  flexDirection: {
    row: 0,
    column: 1,
    "row-reverse": 2,
    "column-reverse": 3
  },
  flexWrap: {
    nowrap: 0,
    wrap: 1,
    "wrap-reverse": 2
  },
  alignItems: {
    start: 0,
    end: 1,
    "flex-start": 2,
    "flex-end": 3,
    center: 4,
    baseline: 5,
    stretch: 6
  },
  alignSelf: {
    start: 0,
    end: 1,
    "flex-start": 2,
    "flex-end": 3,
    center: 4,
    baseline: 5,
    stretch: 6
  },
  alignContent: {
    start: 0,
    end: 1,
    "flex-start": 2,
    "flex-end": 3,
    center: 4,
    stretch: 5,
    "space-between": 6,
    "space-evenly": 7,
    "space-around": 8
  },
  justifyContent: {
    start: 0,
    end: 1,
    "flex-start": 2,
    "flex-end": 3,
    center: 4,
    stretch: 5,
    "space-between": 6,
    "space-evenly": 7,
    "space-around": 8
  },
  gridAutoFlow: {
    row: 0,
    column: 1,
    "row-dense": 2,
    "column-dense": 3
  },
  drawStyle: {
    fill: 0,
    stroke: 1,
    "stroke-and-fill": 2
  },
  strokeCap: {
    butt: 0,
    round: 1,
    square: 2
  },
  strokeJoin: {
    miter: 0,
    round: 1,
    bevel: 2
  },
  blendMode: {
    clear: 0,
    source: 1,
    destination: 2,
    "source-over": 3,
    "destination-over": 4,
    "source-in": 5,
    "destination-in": 6,
    "source-out": 7,
    "destination-out": 8,
    "source-atop": 9,
    "destination-atop": 10,
    xor: 11,
    plus: 12,
    modulate: 13,
    screen: 14,
    overlay: 15,
    darken: 16,
    lighten: 17,
    "color-dodge": 18,
    "color-burn": 19,
    "hard-light": 20,
    "soft-light": 21,
    difference: 22,
    exclusion: 23,
    multiply: 24,
    hue: 25,
    saturation: 26,
    color: 27,
    luminosity: 28
  },
  fontStyle: {
    normal: 0,
    italic: 1
  },
  textAlign: {
    left: 0,
    right: 1,
    center: 2,
    justify: 3
  },
  position: {
    relative: 0,
    absolute: 1
  },
  overflow: {
    visible: 0,
    clip: 1,
    hidden: 2,
    scroll: 3
  },
  pointerEvents: {
    auto: 0,
    none: 1,
    all: 2
  },
  fillRule: {
    nonZero: 0,
    evenOdd: 1
  }
};

// ../../../node_modules/.bun/colord@2.9.3/node_modules/colord/index.mjs
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

// ../../../node_modules/.bun/colord@2.9.3/node_modules/colord/plugins/names.mjs
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

// ../../../node_modules/.bun/colord@2.9.3/node_modules/colord/plugins/lch.mjs
var r2 = { grad: 0.9, turn: 360, rad: 360 / (2 * Math.PI) };
var t2 = function(r3) {
  return typeof r3 == "string" ? r3.length > 0 : typeof r3 == "number";
};
var a2 = function(r3, t3, a3) {
  return t3 === undefined && (t3 = 0), a3 === undefined && (a3 = Math.pow(10, t3)), Math.round(a3 * r3) / a3 + 0;
};
var n2 = function(r3, t3, a3) {
  return t3 === undefined && (t3 = 0), a3 === undefined && (a3 = 1), r3 > a3 ? a3 : r3 > t3 ? r3 : t3;
};
var u2 = function(r3) {
  var t3 = r3 / 255;
  return t3 < 0.04045 ? t3 / 12.92 : Math.pow((t3 + 0.055) / 1.055, 2.4);
};
var h2 = function(r3) {
  return 255 * (r3 > 0.0031308 ? 1.055 * Math.pow(r3, 1 / 2.4) - 0.055 : 12.92 * r3);
};
var o2 = 96.422;
var e2 = 100;
var c2 = 82.521;
var i2 = function(r3) {
  var t3, a3, u3 = { x: 0.9555766 * (t3 = r3).x + -0.0230393 * t3.y + 0.0631636 * t3.z, y: -0.0282895 * t3.x + 1.0099416 * t3.y + 0.0210077 * t3.z, z: 0.0122982 * t3.x + -0.020483 * t3.y + 1.3299098 * t3.z };
  return a3 = { r: h2(0.032404542 * u3.x - 0.015371385 * u3.y - 0.004985314 * u3.z), g: h2(-0.00969266 * u3.x + 0.018760108 * u3.y + 0.00041556 * u3.z), b: h2(0.000556434 * u3.x - 0.002040259 * u3.y + 0.010572252 * u3.z), a: r3.a }, { r: n2(a3.r, 0, 255), g: n2(a3.g, 0, 255), b: n2(a3.b, 0, 255), a: n2(a3.a) };
};
var l2 = function(r3) {
  var t3 = u2(r3.r), a3 = u2(r3.g), h3 = u2(r3.b);
  return function(r4) {
    return { x: n2(r4.x, 0, o2), y: n2(r4.y, 0, e2), z: n2(r4.z, 0, c2), a: n2(r4.a) };
  }(function(r4) {
    return { x: 1.0478112 * r4.x + 0.0228866 * r4.y + -0.050127 * r4.z, y: 0.0295424 * r4.x + 0.9904844 * r4.y + -0.0170491 * r4.z, z: -0.0092345 * r4.x + 0.0150436 * r4.y + 0.7521316 * r4.z, a: r4.a };
  }({ x: 100 * (0.4124564 * t3 + 0.3575761 * a3 + 0.1804375 * h3), y: 100 * (0.2126729 * t3 + 0.7151522 * a3 + 0.072175 * h3), z: 100 * (0.0193339 * t3 + 0.119192 * a3 + 0.9503041 * h3), a: r3.a }));
};
var f2 = 216 / 24389;
var b2 = 24389 / 27;
var d2 = function(r3) {
  return { l: n2(r3.l, 0, 100), c: r3.c, h: (t3 = r3.h, (t3 = isFinite(t3) ? t3 % 360 : 0) > 0 ? t3 : t3 + 360), a: r3.a };
  var t3;
};
var p2 = function(r3) {
  return { l: a2(r3.l, 2), c: a2(r3.c, 2), h: a2(r3.h, 2), a: a2(r3.a, 3) };
};
var v2 = function(r3) {
  var { l: a3, c: n3, h: u3, a: h3 } = r3, o3 = h3 === undefined ? 1 : h3;
  if (!t2(a3) || !t2(n3) || !t2(u3))
    return null;
  var e3 = d2({ l: Number(a3), c: Number(n3), h: Number(u3), a: Number(o3) });
  return M2(e3);
};
var y2 = function(r3) {
  var t3 = function(r4) {
    var t4 = l2(r4), a3 = t4.x / o2, n4 = t4.y / e2, u4 = t4.z / c2;
    return a3 = a3 > f2 ? Math.cbrt(a3) : (b2 * a3 + 16) / 116, { l: 116 * (n4 = n4 > f2 ? Math.cbrt(n4) : (b2 * n4 + 16) / 116) - 16, a: 500 * (a3 - n4), b: 200 * (n4 - (u4 = u4 > f2 ? Math.cbrt(u4) : (b2 * u4 + 16) / 116)), alpha: t4.a };
  }(r3), n3 = a2(t3.a, 3), u3 = a2(t3.b, 3), h3 = Math.atan2(u3, n3) / Math.PI * 180;
  return { l: t3.l, c: Math.sqrt(n3 * n3 + u3 * u3), h: h3 < 0 ? h3 + 360 : h3, a: t3.alpha };
};
var M2 = function(r3) {
  return t3 = { l: r3.l, a: r3.c * Math.cos(r3.h * Math.PI / 180), b: r3.c * Math.sin(r3.h * Math.PI / 180), alpha: r3.a }, n3 = t3.a / 500 + (a3 = (t3.l + 16) / 116), u3 = a3 - t3.b / 200, i2({ x: (Math.pow(n3, 3) > f2 ? Math.pow(n3, 3) : (116 * n3 - 16) / b2) * o2, y: (t3.l > 8 ? Math.pow((t3.l + 16) / 116, 3) : t3.l / b2) * e2, z: (Math.pow(u3, 3) > f2 ? Math.pow(u3, 3) : (116 * u3 - 16) / b2) * c2, a: t3.alpha });
  var t3, a3, n3, u3;
};
var x2 = /^lch\(\s*([+-]?\d*\.?\d+)%\s+([+-]?\d*\.?\d+)\s+([+-]?\d*\.?\d+)(deg|rad|grad|turn)?\s*(?:\/\s*([+-]?\d*\.?\d+)(%)?\s*)?\)$/i;
var s2 = function(t3) {
  var a3 = x2.exec(t3);
  if (!a3)
    return null;
  var n3, u3, h3 = d2({ l: Number(a3[1]), c: Number(a3[2]), h: (n3 = a3[3], u3 = a3[4], u3 === undefined && (u3 = "deg"), Number(n3) * (r2[u3] || 1)), a: a3[5] === undefined ? 1 : Number(a3[5]) / (a3[6] ? 100 : 1) });
  return M2(h3);
};
function lch_default(r3, t3) {
  r3.prototype.toLch = function() {
    return p2(y2(this.rgba));
  }, r3.prototype.toLchString = function() {
    return r4 = p2(y2(this.rgba)), t4 = r4.l, a3 = r4.c, n3 = r4.h, (u3 = r4.a) < 1 ? "lch(" + t4 + "% " + a3 + " " + n3 + " / " + u3 + ")" : "lch(" + t4 + "% " + a3 + " " + n3 + ")";
    var r4, t4, a3, n3, u3;
  }, t3.string.push([s2, "lch"]), t3.object.push([v2, "lch"]);
}

// ../../core/src/color.ts
k([names_default, lch_default]);
function parseColor(color) {
  let { l: l3, c: c3, h: h3, a: a3 } = w(color).toLch();
  return [l3, c3, h3, a3];
}
function lchToRgba(lch) {
  let { r: r3, g: g2, b: b3, a: a3 } = w({ l: lch[0], c: lch[1], h: lch[2], a: lch[3] }).toRgb();
  return [r3 / 255, g2 / 255, b3 / 255, a3];
}

// ../../core/src/ffi.ts
function frame() {
  return __native_frame();
}
function createElement(elementType, nodeId) {
  __native_createElement(elementType, nodeId);
}
function createTextElement(nodeId, value) {
  createElement(8 /* String */, nodeId);
  setProperty(nodeId, 300 /* Text */, value);
}
function insertNode(parentId, childId2, anchorId) {
  __native_insertNode(parentId, childId2, anchorId);
}
function removeNode(parentId, childId2) {
  __native_removeNode(parentId, childId2);
}
function setProperty(nodeId, propertyId, value) {
  if (value === undefined)
    return;
  if (Array.isArray(value)) {
    let [r3, g2, b3, a3] = lchToRgba(value);
    __native_setColorProperty(nodeId, propertyId, r3, g2, b3, a3);
    return;
  }
  if (value instanceof Uint8Array) {
    __native_setBlobProperty(nodeId, propertyId, value);
    return;
  }
  if (typeof value === "object" && value !== null) {
    __native_setParamMapProperty(nodeId, propertyId, value);
    return;
  }
  if (typeof value === "string") {
    let propertyType2 = PropertyTypeMap[propertyId];
    if (propertyType2 === 4 /* Color */) {
      let [r3, g2, b3, a3] = lchToRgba(parseColor(value));
      __native_setColorProperty(nodeId, propertyId, r3, g2, b3, a3);
      return;
    }
    __native_setStringProperty(nodeId, propertyId, value);
    return;
  }
  if (typeof value === "boolean") {
    __native_setBooleanProperty(nodeId, propertyId, value);
    return;
  }
  if (typeof value !== "number") {
    console.warn(`Property ${propertyId} expects number value, got ${typeof value}`);
    return;
  }
  let propertyType = PropertyTypeMap[propertyId];
  if (propertyType === 1 /* U32 */) {
    __native_setNumberProperty(nodeId, propertyId, value);
  } else {
    __native_setFloatProperty(nodeId, propertyId, value);
  }
}

// ../../core/src/window.ts
var nextFrameId = 1;
var animationFrames = new Map;
var requestAnimationFrame = (fn) => {
  let id = nextFrameId++;
  animationFrames.set(id, fn);
  return id;
};
var cancelAnimationFrame = (id) => {
  animationFrames.delete(id);
};
var resizeHandlers = [];
var keyDownHandlers = [];
var keyUpHandlers = [];
var textInputHandlers = [];
var pointerMoveHandlers = [];
var pointerUpHandlers = [];
var resolvePointerEvent = (ev, index) => ({
  target: ev.targets[index],
  clientX: ev.clientX,
  clientY: ev.clientY,
  localX: ev.localX[index],
  localY: ev.localY[index],
  parentX: index === 0 ? ev.clientX : ev.localX[index - 1],
  parentY: index === 0 ? ev.clientY : ev.localY[index - 1],
  button: ev.button,
  deltaX: ev.deltaX,
  deltaY: ev.deltaY
});
var eventRegistry = (() => {
  let eventMap = {
    onpointerenter: "enter",
    onpointerleave: "leave",
    onpointerdown: "down",
    onpointerup: "up",
    onpointermove: "move",
    onpointermovelocal: "moveLocal",
    onwheel: "wheel"
  };
  let lookup = new Map;
  let register = (id, event, fn) => {
    let key = eventMap[event.toLowerCase()];
    if (!key)
      return;
    let handlers = lookup.get(id);
    if (!handlers) {
      handlers = new Map;
      lookup.set(id, handlers);
    }
    handlers.set(key, fn);
  };
  let cleanup3 = (id) => {
    lookup.delete(id);
    if (capturedNodeId === id)
      capturedNodeId = 0;
  };
  let capturedNodeId = 0;
  let execute = (eventType, id, event) => {
    let fn = lookup.get(id)?.get(eventType);
    if (fn) {
      fn(event);
      return true;
    }
    return false;
  };
  let capture = (id) => capturedNodeId = id;
  let release = () => capturedNodeId = 0;
  let getCaptured = () => capturedNodeId;
  let has = (event) => Object.keys(eventMap).includes(event.toLowerCase());
  return { register, cleanup: cleanup3, execute, capture, release, getCaptured, has };
})();
function onRender(fn) {
  let frameId = null;
  let extendedFn = (tick) => {
    fn(tick);
    frameId = requestAnimationFrame(extendedFn);
  };
  frameId = requestAnimationFrame(extendedFn);
  onCleanup(() => cancelAnimationFrame(frameId));
}
var dispatchEvent = (ev, nodeId) => {
  switch (ev.type) {
    case "resize": {
      let [w2, h3] = ev.size;
      setProperty(nodeId, 1 /* Width */, w2);
      setProperty(nodeId, 2 /* Height */, h3);
      for (let fn of resizeHandlers)
        fn(w2, h3, ev.area);
      break;
    }
    case "keyDown":
      for (let h3 of keyDownHandlers)
        h3(ev);
      break;
    case "keyUp":
      for (let h3 of keyUpHandlers)
        h3(ev);
      break;
    case "textInput":
      for (let h3 of textInputHandlers)
        h3(ev);
      break;
    case "pointerEnter":
      for (let i3 = 0;i3 < ev.targets.length; i3++) {
        eventRegistry.execute("enter", ev.targets[i3], resolvePointerEvent(ev, i3));
      }
      break;
    case "pointerLeave":
      for (let i3 = 0;i3 < ev.targets.length; i3++) {
        eventRegistry.execute("leave", ev.targets[i3], resolvePointerEvent(ev, i3));
      }
      break;
    case "pointerMove":
      for (let h3 of pointerMoveHandlers)
        h3(ev);
      for (let i3 = 0;i3 < ev.targets.length; i3++) {
        eventRegistry.execute("move", ev.targets[i3], resolvePointerEvent(ev, i3));
      }
      break;
    case "pointerMoveLocal":
      for (let i3 = 0;i3 < ev.targets.length; i3++) {
        eventRegistry.execute("moveLocal", ev.targets[i3], resolvePointerEvent(ev, i3));
      }
      break;
    case "pointerDown":
      for (let i3 = ev.targets.length - 1;i3 >= 0; i3--) {
        let resolved = resolvePointerEvent(ev, i3);
        if (eventRegistry.execute("down", ev.targets[i3], resolved)) {
          eventRegistry.capture(ev.targets[i3]);
          break;
        }
      }
      break;
    case "pointerUp": {
      let captured = eventRegistry.getCaptured();
      for (let h3 of pointerUpHandlers)
        h3(ev);
      if (captured) {
        let idx = ev.targets.indexOf(captured);
        let resolved = idx >= 0 ? resolvePointerEvent(ev, idx) : ev;
        eventRegistry.execute("up", captured, resolved);
        eventRegistry.release();
      }
      break;
    }
    case "wheel":
      for (let i3 = ev.targets.length - 1;i3 >= 0; i3--) {
        let resolved = resolvePointerEvent(ev, i3);
        if (eventRegistry.execute("wheel", ev.targets[i3], resolved))
          break;
      }
      break;
  }
};

class Window {
  listenerId = 0;
  constructor(nodeId) {
    onSettled(() => {
      this.listenerId = on("render", (time) => {
        let frames = animationFrames;
        animationFrames = new Map;
        let t3 = time * 1000 | 0;
        for (let fn of frames.values()) {
          fn(t3);
        }
        let events = frame();
        for (let ev of events) {
          dispatchEvent(ev, nodeId);
        }
      });
    });
    onCleanup(() => {
      if (this.listenerId)
        off("render", this.listenerId);
    });
  }
}

// ../../core/src/renderer.ts
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
  createElement: createElement2,
  createTextNode,
  insertNode: insertNode2,
  insert,
  spread,
  setProp,
  mergeProps,
  applyRef,
  ref
} = createRenderer({
  createElement: (element) => {
    let elementType = ElementTypeMap[element];
    if (elementType === undefined) {
      throw new Error(`Unknown element type: ${element}`);
    }
    let proxy = createProxyNode(elementType);
    createElement(elementType, proxy.id);
    if (elementType === 0 /* Window */)
      new Window(proxy.id);
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode(8 /* String */);
    createTextElement(proxy.id, "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    setProperty(node.id, 300 /* Text */, "" + value);
  },
  isTextNode: (node) => node?.elementType === 8 /* String */,
  setProperty: (node, name, value, prev) => {
    if (!node)
      return;
    if (eventRegistry.has(name)) {
      eventRegistry.register(node.id, name, value);
      return;
    }
    let propertyId = PropertyNameMap[name];
    if (propertyId === undefined) {
      console.warn(`Unknown property: ${name}`);
      return;
    }
    let mapped = mappings[name];
    if (typeof value === "function") {
      createEffect(() => {
        let v3 = value();
        if (typeof v3 === "string" && mapped) {
          v3 = mapped[v3];
        }
        setProperty(node.id, propertyId, v3);
      });
      return;
    }
    if (typeof value === "string" && mapped) {
      setProperty(node.id, propertyId, mapped[value]);
      return;
    }
    setProperty(node.id, propertyId, value);
  },
  insertNode: (parent, node, anchor) => {
    if (!node)
      return;
    let parentId = parent?.id || 0;
    let anchorId = anchor?.id || 0;
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
    }
    insertNode(parentId, node.id, anchorId);
  },
  removeNode: (parent, node) => {
    if (!node || !parent)
      return;
    let index = parent.children.indexOf(node);
    if (index !== -1) {
      parent.children.splice(index, 1);
    }
    node.parent = undefined;
    removeNode(parent.id, node.id);
    let cleanup3 = (n3) => {
      for (let child of n3.children)
        cleanup3(child);
      nodes.delete(n3.id);
      eventRegistry.cleanup(n3.id);
    };
    cleanup3(node);
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
    insert(null, code());
  });
}
// logo.tsx
var DEFAULT_COLORS = {
  dark: "rgba(51,51,51)",
  mid: "rgba(102,102,102)",
  light: "rgba(153,153,153)"
};
var M3 = 25;
var R = M3 * Math.SQRT2;
var T = -0.5 * R;
var sq = [[0, 0], [2 * M3, 0], [2 * M3, 2 * M3], [0, 2 * M3]];
var tri1 = [[0, 0], [2 * M3, 0], [0, 2 * M3]];
var tri2 = [[0, 0], [2 * R, 0], [0, 2 * R]];
var tri3 = [[0, 0], [4 * M3, 0], [0, 4 * M3]];
var par1 = [[0, 0], [2 * M3, 0], [4 * M3, 2 * M3], [2 * M3, 2 * M3]];
var par2 = [[2 * M3, 0], [4 * M3, 0], [2 * M3, 2 * M3], [0, 2 * M3]];
function shapeCenter(shape, rotate) {
  let radians = rotate * Math.PI / 4;
  let cos = Math.cos(radians);
  let sin = Math.sin(radians);
  let pts = shape.map(([x3, y3]) => [x3 * cos - y3 * sin, x3 * sin + y3 * cos]);
  let minX = Math.min(...pts.map(([x3]) => x3));
  let minY = Math.min(...pts.map(([, y3]) => y3));
  pts = pts.map(([x3, y3]) => [x3 - minX, y3 - minY]);
  let area = 0;
  let cx = 0;
  let cy = 0;
  for (let i3 = 0;i3 < pts.length; i3++) {
    let [x0, y0] = pts[i3];
    let [x1, y1] = pts[(i3 + 1) % pts.length];
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
  let rotated = shape.map(([x3, y3]) => [x3 * cos - y3 * sin, x3 * sin + y3 * cos]);
  let minX = Math.min(...rotated.map(([x3]) => x3));
  let minY = Math.min(...rotated.map(([, y3]) => y3));
  let d3 = "M" + rotated.map(([x3, y3]) => `${x3 - minX} ${y3 - minY}`).join("L") + "Z";
  return d3;
}
var letters = [{
  width: 5 * R,
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
    x: 5 * R - 2 * M3,
    y: 0,
    rot: 0,
    shade: "light"
  }]
}, {
  width: 4 * R + 2 * M3,
  height: 2 * M3 + 4 * R,
  pieces: [{
    shape: tri3,
    x: 0,
    y: 2 * M3,
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
    x: 2 * R + 2 * M3,
    y: 0,
    rot: 3,
    shade: "mid"
  }, {
    shape: tri1,
    x: 2 * R - 2 * M3,
    y: 2 * M3,
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
    x: 2 * R + 2 * M3,
    y: 4 * R - 2 * M3,
    rot: -2,
    shade: "dark"
  }, {
    shape: tri2,
    x: 2 * R - 2 * M3,
    y: 0,
    rot: 1,
    shade: "light"
  }]
}, {
  width: 4 * M3 + 2 * R,
  height: 4 * M3 + 4 * R,
  pieces: [{
    shape: sq,
    x: 2 * R - 2 * M3,
    y: 0,
    rot: 0,
    shade: "light"
  }, {
    shape: tri1,
    x: 2 * R - 2 * M3,
    y: 2 * M3,
    rot: 0,
    shade: "mid"
  }, {
    shape: tri3,
    x: 0,
    y: 2 * M3,
    rot: -1,
    shade: "dark"
  }, {
    shape: tri3,
    x: 2 * R - 2 * M3,
    y: 4 * R,
    rot: -2,
    shade: "mid"
  }, {
    shape: par1,
    x: 2 * R,
    y: 4 * R + 2 * M3,
    rot: 0,
    shade: "dark"
  }, {
    shape: tri2,
    x: 4 * M3,
    y: 2 * R + 4 * M3,
    rot: 2,
    shade: "mid"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: R + 4 * M3,
    rot: 1,
    shade: "light"
  }]
}, {
  width: 6 * M3,
  height: 8 * M3,
  pieces: [{
    shape: sq,
    x: 4 * M3,
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
    x: 2 * M3,
    y: 2 * M3,
    rot: -2,
    shade: "light"
  }, {
    shape: tri2,
    x: 2 * M3,
    y: 0,
    rot: -1,
    shade: "mid"
  }, {
    shape: tri3,
    x: 2 * M3,
    y: 4 * M3,
    rot: -2,
    shade: "dark"
  }, {
    shape: tri1,
    x: 0,
    y: 6 * M3,
    rot: 4,
    shade: "mid"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: 6 * M3,
    rot: 2,
    shade: "mid"
  }]
}, {
  width: 6 * M3,
  height: 8 * M3,
  pieces: [{
    shape: tri3,
    x: 0,
    y: 0,
    rot: 0,
    shade: "mid"
  }, {
    shape: tri3,
    x: 0,
    y: 4 * M3,
    rot: -2,
    shade: "dark"
  }, {
    shape: tri1,
    x: 2 * M3,
    y: 0,
    rot: 4,
    shade: "dark"
  }, {
    shape: par2,
    x: 4 * M3,
    y: 0,
    rot: 2,
    shade: "light"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: 2 * M3,
    rot: -2,
    shade: "dark"
  }, {
    shape: sq,
    x: 4 * M3,
    y: 4 * M3,
    rot: 0,
    shade: "light"
  }, {
    shape: tri2,
    x: 2 * M3,
    y: 6 * M3,
    rot: -3,
    shade: "mid"
  }]
}, {
  width: 8 * M3,
  height: 8 * M3,
  scale: 0.5,
  pieces: [{
    shape: tri3,
    x: 0,
    y: 0,
    rot: 4,
    shade: "dark"
  }, {
    shape: tri3,
    x: 0,
    y: 4 * M3,
    rot: 2,
    shade: "mid"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: 0,
    rot: -2,
    shade: "light"
  }, {
    shape: sq,
    x: 4 * M3,
    y: 2 * M3,
    rot: 0,
    shade: "mid"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: 4 * M3,
    rot: 0,
    shade: "light"
  }, {
    shape: par1,
    x: 4 * M3,
    y: 4 * M3,
    rot: 2,
    shade: "dark"
  }, {
    shape: tri2,
    x: 6 * M3,
    y: 2 * M3,
    rot: 3,
    shade: "light"
  }]
}, {
  width: 6 * M3,
  height: 8 * M3,
  pieces: [{
    shape: tri3,
    x: 0,
    y: 0,
    rot: 0,
    shade: "mid"
  }, {
    shape: tri3,
    x: 0,
    y: 4 * M3,
    rot: 0,
    shade: "dark"
  }, {
    shape: tri2,
    x: 2 * M3,
    y: 0,
    rot: 1,
    shade: "dark"
  }, {
    shape: sq,
    x: 4 * M3 - R,
    y: 4 * M3,
    rot: 1,
    shade: "light"
  }, {
    shape: tri1,
    x: 0,
    y: 6 * M3,
    rot: 4,
    shade: "light"
  }, {
    shape: tri1,
    x: 4 * M3,
    y: 4 * M3 + R,
    rot: -1,
    shade: "mid"
  }, {
    shape: par2,
    x: 2 * M3,
    y: 2 * M3,
    rot: 0,
    shade: "mid"
  }]
}, {
  width: 6 * M3,
  height: 4 * M3 + 4 * R,
  pieces: [{
    shape: par1,
    x: T + 2 * R - 2 * M3,
    y: 0,
    rot: -2,
    shade: "light"
  }, {
    shape: tri1,
    x: T + 2 * R - 2 * M3,
    y: 0,
    rot: 0,
    shade: "mid"
  }, {
    shape: tri3,
    x: T + 0,
    y: 2 * M3,
    rot: -1,
    shade: "dark"
  }, {
    shape: tri3,
    x: T + 2 * R - 2 * M3,
    y: 4 * R,
    rot: -2,
    shade: "mid"
  }, {
    shape: tri2,
    x: T + 2 * R,
    y: 2 * M3,
    rot: -3,
    shade: "light"
  }, {
    shape: tri1,
    x: T + 2 * R,
    y: 2 * M3,
    rot: -2,
    shade: "mid"
  }, {
    shape: sq,
    x: T + 2 * M3 + R,
    y: 4 * M3 + 2 * R,
    rot: 1,
    shade: "dark"
  }]
}];
var EXPLODE_DIST = 10;
var STAGGER_DELAY = 50;
var ANIM_DURATION = 600;
var HOLD_ASSEMBLED = 5000;
var HOLD_EXPLODED = 0;
function TangramLetter(props) {
  let [dist, setDist] = createSignal(EXPLODE_DIST);
  let letterCx = props.letter.width / 2;
  let letterCy = props.letter.height / 2;
  let pieceVectors = props.letter.pieces.map((p3) => {
    let [scx, scy] = shapeCenter(p3.shape, p3.rot);
    return [p3.x + scx - letterCx, p3.y + scy - letterCy];
  });
  let pieceSpins = props.letter.pieces.map((_, i3) => ((i3 * 7 + 3) % 11 - 5) * 30);
  onRender((tick) => {
    let cycleLen = ANIM_DURATION + HOLD_ASSEMBLED + ANIM_DURATION + HOLD_EXPLODED;
    let t3 = (tick - props.delay) % cycleLen;
    if (t3 < 0) {
      setDist(EXPLODE_DIST);
    } else if (t3 < ANIM_DURATION) {
      let p3 = t3 / ANIM_DURATION;
      let ease = p3 * p3 * (3 - 2 * p3);
      setDist((1 - ease) * EXPLODE_DIST);
    } else if (t3 < ANIM_DURATION + HOLD_ASSEMBLED) {
      setDist(0);
    } else if (t3 < 2 * ANIM_DURATION + HOLD_ASSEMBLED) {
      let p3 = (t3 - ANIM_DURATION - HOLD_ASSEMBLED) / ANIM_DURATION;
      let ease = p3 * p3 * (3 - 2 * p3);
      setDist(ease * EXPLODE_DIST);
    } else {
      setDist(EXPLODE_DIST);
    }
    flush();
  });
  return (() => {
    var _el$ = createElement2("view");
    insert(_el$, () => props.letter.pieces.map((p3, i3) => (() => {
      var _el$2 = createElement2("view"), _el$3 = createElement2("d-path");
      insertNode2(_el$2, _el$3);
      effect3(() => ({
        e: pieceVectors[i3][0] * dist(),
        t: pieceVectors[i3][1] * dist(),
        a: 1 + dist() * 0.5,
        o: pieceSpins[i3] * dist() / EXPLODE_DIST / 150,
        i: props.colors[p3.shade],
        n: p3.x,
        s: p3.y,
        h: path(p3.shape, p3.rot)
      }), ({
        e: e3,
        t: t3,
        a: a3,
        o: o3,
        i: i4,
        n: n3,
        s: s3,
        h: h3
      }, _p$ = {
        e: undefined,
        t: undefined,
        a: undefined,
        o: undefined,
        i: undefined,
        n: undefined,
        s: undefined,
        h: undefined
      }) => {
        e3 !== _p$.e && setProp(_el$2, "x", e3, _p$.e);
        t3 !== _p$.t && setProp(_el$2, "y", t3, _p$.t);
        a3 !== _p$.a && setProp(_el$2, "scale", a3, _p$.a);
        o3 !== _p$.o && setProp(_el$2, "rotate", o3, _p$.o);
        i4 !== _p$.i && setProp(_el$3, "color", i4, _p$.i);
        n3 !== _p$.n && setProp(_el$3, "x", n3, _p$.n);
        s3 !== _p$.s && setProp(_el$3, "y", s3, _p$.s);
        h3 !== _p$.h && setProp(_el$3, "d", h3, _p$.h);
      });
      return _el$2;
    })()));
    effect3(() => ({
      e: props.letter.width,
      t: props.letter.height,
      a: props.letter.scale
    }), ({
      e: e3,
      t: t3,
      a: a3
    }, _p$ = {
      e: undefined,
      t: undefined,
      a: undefined
    }) => {
      e3 !== _p$.e && setProp(_el$, "width", e3, _p$.e);
      t3 !== _p$.t && setProp(_el$, "height", t3, _p$.t);
      a3 !== _p$.a && setProp(_el$, "scale", a3, _p$.a);
    });
    return _el$;
  })();
}
function Logo(props) {
  let colors = () => ({
    dark: props.dark ?? DEFAULT_COLORS.dark,
    mid: props.mid ?? DEFAULT_COLORS.mid,
    light: props.light ?? DEFAULT_COLORS.light
  });
  return (() => {
    var _el$4 = createElement2("view"), _el$5 = createElement2("view");
    insertNode2(_el$4, _el$5);
    setProp(_el$4, "justifyContent", "center");
    setProp(_el$4, "width", 1500);
    setProp(_el$5, "gap", 50);
    setProp(_el$5, "flexDirection", "row");
    setProp(_el$5, "alignItems", "flex-end");
    insert(_el$5, () => letters.map((letter, i3) => createComponent2(TangramLetter, {
      letter,
      get colors() {
        return colors();
      },
      delay: i3 * STAGGER_DELAY
    })));
    effect3(() => props.width / 1000, (_v$, _$p) => {
      setProp(_el$4, "scale", _v$, _$p);
    });
    return _el$4;
  })();
}

// app.tsx
var palettes = [
  {
    bg: "rgba(217,217,217)",
    dark: "rgba(51,51,51)",
    mid: "rgba(102,102,102)",
    light: "rgba(153,153,153)"
  },
  {
    bg: "rgba(217,230,250)",
    dark: "rgba(26,51,128)",
    mid: "rgba(51,102,179)",
    light: "rgba(102,153,230)"
  },
  {
    bg: "rgba(250,224,224)",
    dark: "rgba(128,26,26)",
    mid: "rgba(179,51,51)",
    light: "rgba(230,102,102)"
  },
  {
    bg: "rgba(224,245,230)",
    dark: "rgba(26,102,51)",
    mid: "rgba(51,153,77)",
    light: "rgba(102,204,128)"
  },
  {
    bg: "rgba(250,240,217)",
    dark: "rgba(128,77,26)",
    mid: "rgba(179,128,51)",
    light: "rgba(230,179,77)"
  },
  {
    bg: "rgba(242,224,250)",
    dark: "rgba(102,26,128)",
    mid: "rgba(153,51,179)",
    light: "rgba(204,102,230)"
  },
  {
    bg: "rgba(224,245,250)",
    dark: "rgba(26,102,128)",
    mid: "rgba(51,153,179)",
    light: "rgba(102,204,230)"
  },
  {
    bg: "rgba(250,230,237)",
    dark: "rgba(153,51,102)",
    mid: "rgba(204,77,128)",
    light: "rgba(230,128,179)"
  },
  {
    bg: "rgba(250,245,224)",
    dark: "rgba(128,102,26)",
    mid: "rgba(179,153,51)",
    light: "rgba(230,204,102)"
  },
  {
    bg: "rgba(242,230,217)",
    dark: "rgba(38,38,38)",
    mid: "rgba(128,51,26)",
    light: "rgba(230,128,51)"
  }
];
function App() {
  let [index, setIndex] = createSignal(0);
  let [statusText, setStatusText] = createSignal("");
  setInterval(() => {
    setIndex((i3) => (i3 + 1) % palettes.length);
  }, 1000);
  let palette = () => palettes[index()];
  return (() => {
    var _el$ = createElement2("window"), _el$2 = createElement2("view"), _el$3 = createElement2("text");
    insertNode2(_el$, _el$2);
    setProp(_el$, "title", "Solid-RT Demo");
    setProp(_el$, "width", 1600);
    setProp(_el$, "height", 400);
    insertNode2(_el$2, _el$3);
    setProp(_el$2, "flexGrow", 1);
    setProp(_el$2, "justifyContent", "center");
    setProp(_el$2, "alignItems", "center");
    setProp(_el$2, "flexDirection", "column");
    setProp(_el$2, "gap", 20);
    insert(_el$2, createComponent2(Logo, {
      width: 500,
      get dark() {
        return palette().dark;
      },
      get mid() {
        return palette().mid;
      },
      get light() {
        return palette().light;
      }
    }), _el$3);
    setProp(_el$3, "fontSize", 20);
    insert(_el$3, statusText);
    effect3(() => palette().mid, (_v$, _$p) => {
      setProp(_el$3, "color", _v$, _$p);
    });
    return _el$;
  })();
}
render(() => createComponent2(App, {}));
