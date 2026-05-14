// node_modules/.bun/@solidjs+signals@2.0.0-beta.7/node_modules/@solidjs/signals/dist/dev.js
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

// node_modules/.bun/solid-js@2.0.0-beta.7/node_modules/solid-js/dist/dev.js
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

// node_modules/.bun/@solidjs+universal@2.0.0-beta.7+b8339d087ca3085b/node_modules/@solidjs/universal/dist/dev.js
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

// packages/core/src/window.ts
var animationFrames = new Map;
function attachWindow(_nodeId) {
  let unsubscribe = null;
  onSettled(() => {
    unsubscribe = Flux.on("render", (time) => {
      if (animationFrames.size > 0) {
        let frames = animationFrames;
        animationFrames = new Map;
        let t = time * 1000 | 0;
        for (let fn of frames.values())
          fn(t);
      }
      draw();
    });
    draw();
  });
  onCleanup(() => {
    if (unsubscribe)
      unsubscribe();
  });
}

// packages/core/src/renderer.ts
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
    console.log("createElement", proxy.id, elementType);
    if (elementType === "window")
      ffi.createRoot(proxy.id);
    else
      ffi.createNode(proxy.id, elementType);
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode("span");
    console.log("createTextNode", proxy.id, value);
    ffi.createNode(proxy.id, "span");
    ffi.setProperty(proxy.id, "text", "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    console.log("replaceText", node.id, value);
    ffi.setProperty(node.id, "text", "" + value);
  },
  isTextNode: (node) => node?.elementType === "span",
  setProperty: (node, name, value, prev) => {
    if (!node)
      return;
    console.log("setProperty", node.id, name, value);
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
      console.log("insertNode", parent.id, node.id, anchor?.id ?? "");
      if (anchor)
        ffi.insertNode(parent.id, node.id, anchor.id);
      else
        ffi.insertNode(parent.id, node.id);
    }
  },
  removeNode: (parent, node) => {
    if (!node || !parent)
      return;
    console.log("removeNode", parent.id, node.id);
    let index = parent.children.indexOf(node);
    if (index !== -1) {
      parent.children.splice(index, 1);
    }
    node.parent = undefined;
    ffi.deleteNode(parent.id, node.id);
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
// lattice/default-app/app.tsx
function App() {
  let [count, setCount] = createSignal(0);
  setInterval(() => setCount((c) => c + 1), 1000);
  return (() => {
    var _el$ = createElement("window"), _el$2 = createElement("text"), _el$3 = createTextNode(`Hello, World `);
    insertNode(_el$, _el$2);
    insertNode(_el$2, _el$3);
    setProp(_el$2, "fontSize", 100);
    setProp(_el$2, "color", 8355839);
    insert(_el$2, count, null);
    return _el$;
  })();
}
render(() => createComponent2(App, {}));
