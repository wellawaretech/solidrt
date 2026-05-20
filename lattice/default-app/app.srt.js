// node_modules/.bun/@solidjs+signals@2.0.0-beta.13/node_modules/@solidjs/signals/dist/dev.js
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
  if (flags & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_MANUAL_WRITE))
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
  if (flags & (REACTIVE_IN_HEAP | REACTIVE_RECOMPUTING_DEPS | REACTIVE_IN_HEAP_HEIGHT | REACTIVE_MANUAL_WRITE))
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
var transitions = new Set;
var dirtyQueue = { _heap: new Array(2000).fill(undefined), _marked: false, _min: 0, _max: 0 };
var zombieQueue = { _heap: new Array(2000).fill(undefined), _marked: false, _min: 0, _max: 0 };
var clock = 0;
var activeTransition = null;
var scheduled = false;
var syncDepth = 0;
var projectionWriteActive = false;
var inTrackedQueueCallback = false;
var _enforceLoadingBoundary = false;
var _hitUnhandledAsync = false;
var stashedOptimisticReads = null;
var transientStoreNodes = new Set;
function canUseSimpleSyncFlush(queue) {
  return transitions.size === 0 && activeLanes.size === 0 && queue._children.length === 0 && queue._optimisticNodes.length === 0 && queue._optimisticStores.size === 0 && transientStoreNodes.size === 0;
}
function sweepTransientStoreNodes() {
  if (transientStoreNodes.size === 0)
    return;
  for (const node of transientStoreNodes) {
    if (node._subs !== null) {
      transientStoreNodes.delete(node);
      continue;
    }
    if (node._pendingValue !== NOT_PENDING)
      continue;
    if (node._overrideValue !== undefined && node._overrideValue !== NOT_PENDING)
      continue;
    transientStoreNodes.delete(node);
    node._unobserved?.();
  }
}
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
  for (const sub of outgoing._gatedSubs)
    target._gatedSubs.add(sub);
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
  if (!syncDepth && !globalQueue._running && !projectionWriteActive)
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
  _pendingNode = null;
  _pendingNodes = [];
  _optimisticNodes = [];
  _optimisticStores = new Set;
  static _update;
  static _dispose;
  static _runEffect;
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
          this._pendingNode = null;
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
          if (!stashedTransition._actions.length && !stashedTransition._asyncReporters.size && stashedTransition._optimisticNodes.length) {
            stashedOptimisticReads = new Set;
            for (let i = 0;i < stashedTransition._optimisticNodes.length; i++) {
              const node = stashedTransition._optimisticNodes[i];
              if (node._fn || node._config & CONFIG_OWNED_WRITE)
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
        if (canUseSimpleSyncFlush(this)) {
          commitPendingNodes();
          if (dirtyQueue._max >= dirtyQueue._min) {
            runHeap(dirtyQueue, GlobalQueue._update);
            commitPendingNodes();
          }
        } else {
          if (transitions.size)
            runHeap(zombieQueue, GlobalQueue._update);
          finalizePureQueue();
        }
      }
      clock++;
      scheduled = dirtyQueue._max >= dirtyQueue._min;
      activeLanes.size && runLaneEffects(EFFECT_RENDER);
      this.run(EFFECT_RENDER);
      activeLanes.size && runLaneEffects(EFFECT_USER);
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
        _done: false,
        _gatedSubs: new Set
      };
    } else if (transition) {
      const outgoing = activeTransition;
      mergeTransitionState(transition, outgoing);
      transitions.delete(outgoing);
      activeTransition = transition;
    }
    transitions.add(activeTransition);
    activeTransition._time = clock;
    if (this._pendingNode !== null) {
      this._pendingNode._transition = activeTransition;
      activeTransition._pendingNodes.push(this._pendingNode);
      this._pendingNode = null;
    }
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
function queuePendingNode(node) {
  if (activeTransition) {
    globalQueue._pendingNodes.push(node);
    return;
  }
  if (globalQueue._pendingNode === null && globalQueue._pendingNodes.length === 0) {
    globalQueue._pendingNode = node;
    return;
  }
  if (globalQueue._pendingNode !== null) {
    globalQueue._pendingNodes.push(globalQueue._pendingNode);
    globalQueue._pendingNode = null;
  }
  globalQueue._pendingNodes.push(node);
}
function insertSubs(node, optimistic = false) {
  const sourceLane = node._optimisticLane || currentOptimisticLane;
  const hasSnapshot = node._snapshotValue !== undefined;
  for (let s = node._subs;s !== null; s = s._nextSub) {
    if (hasSnapshot && s._sub._config & CONFIG_IN_SNAPSHOT_SCOPE) {
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
function commitPendingNode(n) {
  const c = n;
  if (!c._fn) {
    if (n._pendingValue !== NOT_PENDING) {
      n._value = n._pendingValue;
      n._pendingValue = NOT_PENDING;
    }
    return;
  }
  if (n._pendingValue !== NOT_PENDING) {
    n._value = n._pendingValue;
    n._pendingValue = NOT_PENDING;
    if (n._type && n._type !== EFFECT_TRACKED)
      n._modified = true;
  }
  c._flags &= ~REACTIVE_MANUAL_WRITE;
  if (!(c._statusFlags & STATUS_PENDING))
    c._statusFlags &= ~STATUS_UNINITIALIZED;
  if (c._pendingFirstChild !== null || c._pendingDisposal !== null)
    GlobalQueue._dispose(c, false, true);
}
function commitPendingNodes() {
  if (globalQueue._pendingNode !== null) {
    commitPendingNode(globalQueue._pendingNode);
    globalQueue._pendingNode = null;
  }
  const pendingNodes = globalQueue._pendingNodes;
  for (let i = 0;i < pendingNodes.length; i++) {
    commitPendingNode(pendingNodes[i]);
  }
  pendingNodes.length = 0;
}
function finalizePureQueue(completingTransition = null, incomplete = false) {
  const resolvePending = !incomplete;
  if (resolvePending)
    commitPendingNodes();
  if (!incomplete && globalQueue._children.length)
    checkBoundaryChildren(globalQueue);
  const ranHeap = dirtyQueue._max >= dirtyQueue._min;
  if (ranHeap)
    runHeap(dirtyQueue, GlobalQueue._update);
  if (resolvePending) {
    if (ranHeap)
      commitPendingNodes();
    resolveOptimisticNodes(completingTransition ? completingTransition._optimisticNodes : globalQueue._optimisticNodes);
    if (completingTransition && completingTransition._gatedSubs.size) {
      for (const sub of completingTransition._gatedSubs) {
        if (sub._flags & REACTIVE_DISPOSED)
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
      completingTransition._gatedSubs.clear();
    }
    const optimisticStores = completingTransition ? completingTransition._optimisticStores : globalQueue._optimisticStores;
    if (GlobalQueue._clearOptimisticStore && optimisticStores.size) {
      for (const store of optimisticStores) {
        GlobalQueue._clearOptimisticStore(store);
      }
      optimisticStores.clear();
      schedule();
    }
    sweepTransientStoreNodes();
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
function flush(fn) {
  if (fn) {
    syncDepth++;
    try {
      return fn();
    } finally {
      flush();
      syncDepth--;
    }
  }
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
  const flags = node._flags;
  if (flags & REACTIVE_DISPOSED)
    return;
  if (self)
    node._flags = flags | REACTIVE_DISPOSED;
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
  if (self && !zombie && !(flags & REACTIVE_ZOMBIE) && node._parent !== null && !(node._parent._flags & REACTIVE_DISPOSED)) {
    const prev = node._prevSibling;
    const next = node._nextSibling;
    if (prev !== null)
      prev._nextSibling = next;
    else
      node._parent._firstChild = next;
    if (next !== null)
      next._prevSibling = prev;
    node._prevSibling = null;
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
  while (counter._config & CONFIG_TRANSPARENT && counter._parent)
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
function disposeRootSelf(self = true) {
  disposeChildren(this, self);
}
function createOwner(options) {
  const parent = context;
  const transparent = options?.transparent ?? false;
  const owner = {
    id: options?.id ?? (transparent ? parent?.id : parent?.id != null ? getNextChildId(parent) : undefined),
    _config: transparent ? CONFIG_TRANSPARENT : 0,
    _root: true,
    _parentComputed: parent?._root ? parent._parentComputed : parent,
    _firstChild: null,
    _nextSibling: null,
    _prevSibling: null,
    _disposal: null,
    _queue: parent?._queue ?? globalQueue,
    _context: parent?._context || defaultContext,
    _childCount: 0,
    _pendingDisposal: null,
    _pendingFirstChild: null,
    _parent: parent,
    dispose: disposeRootSelf
  };
  if (parent && parent._config & CONFIG_CHILDREN_FORBIDDEN) {
    emitDiagnostic({
      code: "PRIMITIVE_IN_FORBIDDEN_SCOPE",
      kind: "lifecycle",
      severity: "error",
      message: PRIMITIVE_IN_FORBIDDEN_SCOPE_MESSAGE,
      ownerId: parent.id,
      ownerName: parent._name
    });
    throw new Error(PRIMITIVE_IN_FORBIDDEN_SCOPE_MESSAGE);
  }
  if (parent) {
    const lastChild = parent._firstChild;
    if (lastChild === null) {
      parent._firstChild = owner;
    } else {
      owner._nextSibling = lastChild;
      lastChild._prevSibling = owner;
      parent._firstChild = owner;
    }
  }
  DEV$1.hooks.onOwner?.(owner);
  return owner;
}
function createRoot(init, options) {
  const owner = createOwner(options);
  return runWithOwner(owner, () => init(() => owner.dispose()));
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
      const c = dep;
      c._fn && c._config & CONFIG_AUTO_DISPOSE && !(c._flags & REACTIVE_ZOMBIE) && unobserved(c);
    }
  }
  return nextDep;
}
function trimStaleDeps(el) {
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
  let iterator = false;
  let isThenable = false;
  if (typeof result === "object" && result !== null) {
    untrack(() => {
      iterator = result[Symbol.asyncIterator];
      isThenable = !iterator && typeof result.then === "function";
    });
  }
  if (!isThenable && !iterator) {
    el._inFlight = null;
    return result;
  }
  if (el._config & CONFIG_SYNC) {
    const message = `[SYNC_NODE_RECEIVED_ASYNC] A computed/effect created with \`sync: true\` returned ` + `${isThenable ? "a Promise" : "an AsyncIterable"}. The value would be stored as-is and ` + `never awaited in production; remove \`sync: true\` to use async-aware behavior, or ` + `unwrap the value before returning.`;
    emitDiagnostic({
      code: "SYNC_NODE_RECEIVED_ASYNC",
      kind: "lifecycle",
      severity: "error",
      message,
      ownerId: el.id,
      ownerName: el._name
    });
    throw new Error(message);
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
    trimStaleDeps(el);
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
  if (el._pendingSource || el._pendingSources)
    clearPendingSources(el);
  if (el._blocked)
    el._blocked = false;
  el._statusFlags = clearUninitialized ? 0 : el._statusFlags & STATUS_UNINITIALIZED;
  if (el._error)
    setPendingError(el);
  if (el._pendingSignal)
    updatePendingSignal(el);
  if (el._notifyStatus)
    el._notifyStatus();
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
        queuePendingNode(sub);
      notifyStatus(sub, status, error, downstreamBlockStatus, downstreamLane);
    }
  });
}
var externalSourceConfig = null;
GlobalQueue._update = recompute;
GlobalQueue._dispose = disposeChildren;
var PRIMITIVE_IN_FORBIDDEN_SCOPE_MESSAGE = "[PRIMITIVE_IN_FORBIDDEN_SCOPE] Cannot create reactive primitives inside createTrackedEffect or owner-backed onSettled";
var tracking = false;
var stale = false;
var pendingCheckActive = false;
var foundPending = false;
var latestReadActive = false;
var context = null;
var currentOptimisticLane = null;
var pendingCheckLoadingPath = false;
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
    else if (el._firstChild !== null || el._disposal !== null) {
      markDisposal(el);
      el._pendingDisposal = el._disposal;
      el._pendingFirstChild = el._firstChild;
      el._disposal = null;
      el._firstChild = null;
      el._childCount = 0;
      clearSignals(el);
    } else
      clearSignals(el);
  }
  let isOptimisticDirty = !!(el._flags & REACTIVE_OPTIMISTIC_DIRTY);
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
  } else if (activeTransition && !create && activeTransition._optimisticNodes.length) {
    for (let d = el._deps;d; d = d._nextDep) {
      const dep = d._dep;
      if (dep._flags & REACTIVE_OPTIMISTIC_DIRTY) {
        const depLane = resolveLane(dep);
        if (depLane) {
          isOptimisticDirty = true;
          currentOptimisticLane = depLane;
          el._flags |= REACTIVE_OPTIMISTIC_DIRTY;
          assignOrMergeLane(el, depLane);
          break;
        }
      }
    }
  }
  const isStaleEffect = isEffect && isEffect !== EFFECT_USER;
  const prevStale = stale;
  if (isStaleEffect)
    stale = true;
  try {
    if (false)
      ;
    else {
      const prevInFlight = el._inFlight;
      const fnResult = el._fn(value);
      const isAsyncResult = typeof fnResult === "object" && fnResult !== null;
      const inFlightChanged = el._inFlight !== prevInFlight;
      value = inFlightChanged || !isAsyncResult ? fnResult : handleAsync(el, fnResult);
      if (!inFlightChanged && !isAsyncResult)
        el._inFlight = null;
    }
    clearStatus(el, create);
    if (el._optimisticLane) {
      const resolvedLane = resolveLane(el);
      if (resolvedLane) {
        resolvedLane._pendingAsync.delete(el);
        updatePendingSignal(resolvedLane._source);
      }
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
    if (isStaleEffect)
      stale = prevStale;
    el._flags = REACTIVE_NONE | (create ? el._flags & REACTIVE_SNAPSHOT_STALE : 0);
    context = oldcontext;
  }
  if (!el._error) {
    trimStaleDeps(el);
    const compareValue = hasOverride ? el._overrideValue : el._pendingValue === NOT_PENDING ? el._value : el._pendingValue;
    const valueChanged = !isEffect && wasUninitialized || !el._equals || !el._equals(compareValue, value);
    if (isEffect && valueChanged) {
      el._modified = !el._error;
      if (!create)
        el._queue.enqueue(isEffect, GlobalQueue._runEffect.bind(null, el));
    }
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
  const needsPendingCommit = el._pendingValue !== NOT_PENDING || el._pendingFirstChild !== null || el._pendingDisposal !== null || !!(el._statusFlags & (STATUS_PENDING | STATUS_UNINITIALIZED));
  needsPendingCommit && (!create || el._statusFlags & STATUS_PENDING) && !el._transition && !(activeTransition && hasOverride) && queuePendingNode(el);
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
function computed(fn, options) {
  const transparent = options?.transparent ?? false;
  const self = {
    id: options?.id ?? (transparent ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    _config: (transparent ? CONFIG_TRANSPARENT : 0) | (options?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (!context || options?.lazy ? CONFIG_AUTO_DISPOSE : 0) | (options?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    _equals: options?.equals != null ? options.equals : isEqual,
    _unobserved: options?.unobserved,
    _disposal: null,
    _queue: context?._queue ?? globalQueue,
    _context: context?._context ?? defaultContext,
    _childCount: 0,
    _fn: fn,
    _value: undefined,
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
    _prevSibling: null,
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
  setupComputedNode(self, options);
  return self;
}
function createEffectNode(fn, effectFn, errorFn, type, notifyStatus2, options) {
  const transparent = options?.transparent ?? false;
  const self = {
    id: options?.id ?? (transparent ? context?.id : context?.id != null ? getNextChildId(context) : undefined),
    _config: (transparent ? CONFIG_TRANSPARENT : 0) | (options?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (options?.sync ? CONFIG_SYNC : 0) | (snapshotCaptureActive && ownerInSnapshotScope(context) ? CONFIG_IN_SNAPSHOT_SCOPE : 0),
    _equals: false,
    _unobserved: options?.unobserved,
    _disposal: null,
    _queue: context?._queue ?? globalQueue,
    _context: context?._context ?? defaultContext,
    _childCount: 0,
    _fn: fn,
    _value: undefined,
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
    _prevSibling: null,
    _firstChild: null,
    _flags: REACTIVE_LAZY,
    _statusFlags: STATUS_UNINITIALIZED,
    _time: clock,
    _pendingValue: NOT_PENDING,
    _pendingDisposal: null,
    _pendingFirstChild: null,
    _inFlight: null,
    _transition: null,
    _modified: false,
    _prevValue: undefined,
    _effectFn: effectFn,
    _errorFn: errorFn,
    _cleanup: undefined,
    _cleanupRegistered: false,
    _type: type,
    _notifyStatus: notifyStatus2
  };
  self._name = options?.name ?? "effect";
  setupComputedNode(self, lazyOptions);
  return self;
}
var lazyOptions = { lazy: true };
function setupComputedNode(self, options) {
  self._prevHeap = self;
  const parent = context?._root ? context._parentComputed : context;
  if (context && context._config & CONFIG_CHILDREN_FORBIDDEN) {
    emitDiagnostic({
      code: "PRIMITIVE_IN_FORBIDDEN_SCOPE",
      kind: "lifecycle",
      severity: "error",
      message: PRIMITIVE_IN_FORBIDDEN_SCOPE_MESSAGE,
      ownerId: context.id,
      ownerName: context._name
    });
    throw new Error(PRIMITIVE_IN_FORBIDDEN_SCOPE_MESSAGE);
  }
  if (context) {
    const lastChild = context._firstChild;
    if (lastChild === null) {
      context._firstChild = self;
    } else {
      self._nextSibling = lastChild;
      lastChild._prevSibling = self;
      context._firstChild = self;
    }
  }
  DEV$1.hooks.onOwner?.(self);
  if (parent)
    self._height = parent._height + 1;
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
}
function signal(v, options, firewall = null) {
  const s = {
    _equals: options?.equals != null ? options.equals : isEqual,
    _config: (options?.ownedWrite ? CONFIG_OWNED_WRITE : 0) | (options?._noSnapshot ? CONFIG_NO_SNAPSHOT : 0),
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
  if (snapshotCaptureActive && !(s._config & CONFIG_NO_SNAPSHOT) && !((firewall?._statusFlags ?? 0) & STATUS_PENDING)) {
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
function optimisticComputed(fn, options) {
  const c = computed(fn, options);
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
    const owner2 = firewall || el;
    if (pendingCheckLoadingPath && owner2._statusFlags & STATUS_PENDING && owner2._statusFlags & STATUS_UNINITIALIZED) {
      let c2 = context;
      if (c2?._root)
        c2 = c2._parentComputed;
      if (c2 && tracking)
        link(el, c2);
      pendingCheckActive = prevCheck;
      throw owner2._error;
    }
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
    if (comp._flags & REACTIVE_LAZY) {
      comp._flags &= ~REACTIVE_LAZY;
      recompute(comp, true);
    } else if (comp._flags & REACTIVE_DISPOSED) {
      recompute(comp, true);
    }
  }
  const owner = el._firewall || el;
  if (!computed2._fn && owner === el && el._overrideValue === undefined && el._snapshotValue === undefined && activeTransition === null && currentOptimisticLane === null && !snapshotCaptureActive && !strictRead) {
    if (c && tracking)
      link(el, c);
    return !c || el._pendingValue === NOT_PENDING ? el._value : el._pendingValue;
  }
  if (strictRead && owner._statusFlags & STATUS_PENDING) {
    const message = `[PENDING_ASYNC_UNTRACKED_READ] Reading a pending async value directly in ${strictRead}. ` + `Async values must be read within a tracking scope (JSX, a memo, or an effect's compute function).`;
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
      if (c && c._config & CONFIG_CHILDREN_FORBIDDEN) {
        const message = "[PENDING_ASYNC_FORBIDDEN_SCOPE] Reading a pending async value inside createTrackedEffect or onSettled will throw. " + "Use createEffect instead which supports async-aware reactivity.";
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
  if (snapshotCaptureActive && c && c._config & CONFIG_IN_SNAPSHOT_SCOPE) {
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
    const message = `[STRICT_READ_UNTRACKED] Reactive value read directly in ${strictRead} will not update. ` + `Move it into a tracking scope (JSX, a memo, or an effect's compute function).`;
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
  if (activeTransition !== null && currentOptimisticLane !== null && !latestReadActive && el._pendingValue !== NOT_PENDING && owner === el && !el._fn && c) {
    activeTransition._gatedSubs.add(c);
    return el._value;
  }
  const value = !c || currentOptimisticLane !== null && (el._overrideValue !== undefined || el._optimisticLane || owner === el && stale || !!(owner._statusFlags & STATUS_PENDING)) || el._pendingValue === NOT_PENDING || stale && el._transition && activeTransition !== el._transition ? el._value : el._pendingValue;
  if (!c && owner === el && typeof computed2._fn === "function" && el._config & CONFIG_AUTO_DISPOSE && !(owner._statusFlags & STATUS_PENDING) && !el._subs) {
    unobserved(el);
  }
  return value;
}
function setSignal(el, v) {
  if (!(el._config & CONFIG_OWNED_WRITE) && !(context && context._config & CONFIG_CHILDREN_FORBIDDEN) && context && el._firewall !== context) {
    const message = "[SIGNAL_WRITE_IN_OWNED_SCOPE] Writing to a Signal inside an owned scope (component, computation) is not allowed. " + "Move the write outside or set the `ownedWrite` option if this is intentional.";
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
      queuePendingNode(el);
    el._pendingValue = v;
  }
  if (el._pendingSignal)
    updatePendingSignal(el);
  if (el._latestValueComputed) {
    setSignal(el._latestValueComputed, v);
  }
  el._time = clock;
  insertSubs(el, isOptimistic);
  schedule();
  return v;
}
function suppressComputedRecompute(el) {
  deleteFromHeap(el, el._flags & REACTIVE_ZOMBIE ? zombieQueue : dirtyQueue);
  if (!(el._flags & REACTIVE_MANUAL_WRITE) && el._pendingValue === NOT_PENDING)
    queuePendingNode(el);
  el._flags = el._flags & -4 | REACTIVE_MANUAL_WRITE;
}
function setMemo(el, v) {
  const result = setSignal(el, v);
  suppressComputedRecompute(el);
  return result;
}
function runWithOwner(owner, fn) {
  if (owner && owner._flags & REACTIVE_DISPOSED) {
    const message = "[RUN_WITH_DISPOSED_OWNER] runWithOwner called with a disposed owner. Children created inside will never be disposed.";
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
function effect(compute, effect2, error, options) {
  const isUser = !!options?.user;
  const node = createEffectNode(compute, effect2, error, isUser ? EFFECT_USER : EFFECT_RENDER, notifyEffectStatus, options);
  recompute(node, true);
  !options?.defer && (node._type === EFFECT_USER || options?.schedule ? node._queue.enqueue(node._type, runEffect.bind(null, node)) : runEffect(node));
  if (!node._parent) {
    const message = "[NO_OWNER_EFFECT] Effects created outside a reactive context will never be disposed";
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
function notifyEffectStatus(status, error) {
  const actualStatus = status !== undefined ? status : this._statusFlags;
  const actualError = error !== undefined ? error : this._error;
  if (actualStatus & STATUS_ERROR) {
    let err = actualError;
    this._queue.notify(this, STATUS_PENDING, 0);
    if (this._type === EFFECT_USER) {
      try {
        return this._errorFn ? this._errorFn(err, () => {
          this._cleanup?.();
          this._cleanup = undefined;
        }) : console.error(err);
      } catch (e) {
        err = e;
      }
    }
    if (!this._queue.notify(this, STATUS_ERROR, STATUS_ERROR))
      throw err;
  } else if (this._type === EFFECT_RENDER) {
    this._queue.notify(this, STATUS_PENDING | STATUS_ERROR, actualStatus, actualError);
    if (_hitUnhandledAsync) {
      resetUnhandledAsync();
      if (!this._queue.notify(this, STATUS_ERROR, STATUS_ERROR)) {
        const message = "[ASYNC_OUTSIDE_LOADING_BOUNDARY] An async value was read outside a Loading boundary. The root mount will be deferred until all pending async settles.";
        emitDiagnostic({
          code: "ASYNC_OUTSIDE_LOADING_BOUNDARY",
          kind: "async",
          severity: "warn",
          message,
          ownerId: this.id,
          ownerName: this._name
        });
        console.warn(message);
      }
    }
  }
}
function runEffect(node) {
  if (!node._modified || node._flags & REACTIVE_DISPOSED)
    return;
  let prevStrictRead = false;
  {
    prevStrictRead = setStrictRead("an effect callback");
  }
  node._cleanup?.();
  node._cleanup = undefined;
  try {
    const nextCleanup = node._effectFn(node._value, node._prevValue);
    if (nextCleanup !== undefined && typeof nextCleanup !== "function") {
      throw new Error(`${node._name || "effect"} callback returned an invalid cleanup value. Return a cleanup function or undefined.`);
    }
    node._cleanup = nextCleanup;
    if (node._cleanup && !node._cleanupRegistered) {
      node._cleanupRegistered = true;
      runWithOwner(node._parent, () => cleanup(() => node._cleanup?.()));
    }
  } catch (error) {
    node._error = new StatusError(node, error);
    node._statusFlags |= STATUS_ERROR;
    if (!node._queue.notify(node, STATUS_ERROR, STATUS_ERROR))
      throw error;
  } finally {
    setStrictRead(prevStrictRead);
    node._prevValue = node._value;
    node._modified = false;
  }
}
GlobalQueue._runEffect = runEffect;
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
  }, { ...options, lazy: true });
  node._cleanup = undefined;
  node._config = node._config & ~CONFIG_AUTO_DISPOSE | CONFIG_CHILDREN_FORBIDDEN;
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
    const message = "[NO_OWNER_EFFECT] Effects created outside a reactive context will never be disposed";
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
      const message = "[NO_OWNER_CLEANUP] onCleanup called outside a reactive context will never be run";
      emitDiagnostic({
        code: "NO_OWNER_CLEANUP",
        kind: "lifecycle",
        severity: "warn",
        message
      });
      console.warn(message);
    } else if (owner._config & CONFIG_CHILDREN_FORBIDDEN) {
      const message = "[CLEANUP_IN_FORBIDDEN_SCOPE] Cannot use onCleanup inside createTrackedEffect or onSettled; return a cleanup function instead";
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
  fn[$REFRESH] = node;
  return fn;
}
function createSignal(first, second) {
  if (typeof first === "function") {
    const node2 = computed(first, second);
    node2._config &= ~CONFIG_AUTO_DISPOSE;
    return [accessor(node2), setMemo.bind(null, node2)];
  }
  const node = signal(first, second);
  registerGraph(node, getOwner());
  return [accessor(node), setSignal.bind(null, node)];
}
function createMemo(compute, options) {
  return accessor(computed(compute, options));
}
function createRenderEffect(compute, effectFn, options) {
  effect(compute, effectFn, undefined, { ...options, name: options?.name ?? "effect" });
}
function createTrackedEffect(compute, options) {
  trackedEffect(compute, { ...options, name: options?.name ?? "trackedEffect" });
}
function onSettled(callback) {
  const owner = getOwner();
  owner && !(owner._config & CONFIG_CHILDREN_FORBIDDEN) ? createTrackedEffect(() => untrack(callback), { name: "onSettled" }) : globalQueue.enqueue(EFFECT_USER, () => {
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
  if (obj == null || typeof obj !== "object" || Object.isFrozen(obj))
    return false;
  return typeof Node === "undefined" || !(obj instanceof Node);
}
var DELETE = Symbol("STORE_PATH_DELETE");
function isPrototypePollutionKey(part) {
  return part === "__proto__" || part === "constructor" || part === "prototype";
}
function updatePath(current, args, i = 0) {
  let part, prev = current;
  if (i < args.length - 1) {
    part = args[i];
    const partType = typeof part;
    const isArray = Array.isArray(current);
    if (partType === "string" && isPrototypePollutionKey(part))
      return;
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
      if (isPrototypePollutionKey(key))
        continue;
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
    if (childSources) {
      for (let i2 = 0;i2 < childSources.length; i2++)
        flattened.push(childSources[i2]);
    } else
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
        const keys = new Set;
        for (let i = 0;i < flattened.length; i++) {
          const sourceKeys = Object.keys(resolveSource(flattened[i]));
          for (let j = 0;j < sourceKeys.length; j++)
            keys.add(sourceKeys[j]);
        }
        return [...keys];
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
function isSlotMinimallyReady(slot) {
  return isRevealController(slot) ? slot.isMinimallyReady() : isSlotReady(slot);
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
  _orderAccessor;
  _collapsedAccessor;
  _slots = [];
  _parentController;
  _disabled = signal(false, { ownedWrite: true, _noSnapshot: true });
  _collapsed = signal(false, { ownedWrite: true, _noSnapshot: true });
  _ready = true;
  _minimallyReady = true;
  _evaluating = false;
  constructor(order, collapsed) {
    this._orderAccessor = order;
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
  isMinimallyReady() {
    const order = untrack(this._orderAccessor);
    if (order === "together")
      return this.isReady();
    if (order === "natural") {
      let hasSlot = false;
      let anyReady = false;
      this._forEachOwnedSlot((slot) => {
        hasSlot = true;
        if (isSlotMinimallyReady(slot)) {
          anyReady = true;
          return false;
        }
      });
      return !hasSlot || anyReady;
    }
    let firstReady = true;
    this._forEachOwnedSlot((slot) => {
      firstReady = isSlotMinimallyReady(slot);
      return false;
    });
    return firstReady;
  }
  register(slot) {
    if (this._slots.includes(slot))
      return;
    this._slots.push(slot);
    const order = untrack(this._orderAccessor);
    setSignal(slot._disabled, true), setSignal(slot._collapsed, order === "sequential" ? !!untrack(this._collapsedAccessor) : false);
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
    const wasMinReady = this._minimallyReady;
    try {
      const disabled = disabledOverride ?? read(this._disabled), order = untrack(this._orderAccessor), collapseTail = order === "sequential" && !!untrack(this._collapsedAccessor), collapsed = collapsedOverride ?? collapseTail;
      if (disabled) {
        this._forEachOwnedSlot((slot) => setSlotState(slot, this, true, collapsed));
      } else if (order === "natural") {
        this._forEachOwnedSlot((slot) => {
          if (isRevealController(slot)) {
            setSignal(slot._collapsed, false);
            setSignal(slot._disabled, false);
            slot.evaluate(false, false);
          } else {
            setSlotState(slot, this, !isSlotReady(slot), false);
          }
        });
      } else if (order === "together") {
        const minReady = this._forEachOwnedSlot(isSlotMinimallyReady);
        this._forEachOwnedSlot((slot) => setSlotState(slot, this, !minReady, false));
      } else {
        let pendingSeen = false;
        this._forEachOwnedSlot((slot) => {
          if (pendingSeen)
            return setSlotState(slot, this, true, collapseTail);
          if (isSlotReady(slot))
            return setSlotState(slot, this, false, false);
          pendingSeen = true;
          if (isRevealController(slot)) {
            setSignal(slot._collapsed, false);
            setSignal(slot._disabled, false);
            slot.evaluate(false, false);
          } else {
            setSlotState(slot, this, true, false);
          }
        });
      }
    } finally {
      this._ready = this.isReady();
      this._minimallyReady = this.isMinimallyReady();
      this._evaluating = false;
    }
    if (this._parentController && (wasReady !== this._ready || wasMinReady !== this._minimallyReady))
      this._parentController.evaluate();
  }
}

class CollectionQueue extends Queue {
  _collectionType;
  _sources = new Set;
  _tree;
  _pending = true;
  _disabled = signal(false, { ownedWrite: true, _noSnapshot: true });
  _error;
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
        if (this._collectionType & STATUS_ERROR) {
          setSignal(this._error, source._error?.cause ?? source._error);
        }
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

// node_modules/.bun/solid-js@2.0.0-beta.13/node_modules/solid-js/dist/dev.js
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

// node_modules/.bun/@solidjs+universal@2.0.0-beta.13+97643fd58f54a293/node_modules/@solidjs/universal/dist/dev.js
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

// packages/core/src/events.ts
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

// packages/core/src/focus.ts
var focusedNodeId = null;
function getFocusedNodeId() {
  return focusedNodeId;
}

// packages/core/src/window.ts
var nextFrameId = 1;
var animationFrames = new Map;
function onRender(fn) {
  let frameId = null;
  let extendedFn = (tick, frame) => {
    fn(tick, frame);
    frameId = nextFrameId++;
    animationFrames.set(frameId, extendedFn);
  };
  frameId = nextFrameId++;
  animationFrames.set(frameId, extendedFn);
  let cleanup2 = () => animationFrames.delete(frameId);
  onCleanup(cleanup2);
  return cleanup2;
}
function onResize(fn) {
  let unsubscribe = Flux.on("resize", fn);
  onCleanup(unsubscribe);
  return unsubscribe;
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
  onSettled(() => {
    unsubscribe = Flux.on("render", ({ time, frame }) => {
      if (animationFrames.size > 0) {
        let frames = animationFrames;
        animationFrames = new Map;
        let t = time * 1000 | 0;
        for (let fn of frames.values())
          fn(t, frame);
      }
      draw();
    });
    unsubDown = Flux.on("pointerDown", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerDown")?.(e);
      }
    });
    unsubUp = Flux.on("pointerUp", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerUp")?.(e);
      }
    });
    unsubMove = Flux.on("pointerMove", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerMove")?.(e);
      }
    });
    unsubEnter = Flux.on("pointerEnter", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerEnter")?.(e);
      }
    });
    unsubLeave = Flux.on("pointerLeave", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerLeave")?.(e);
      }
    });
    unsubWheel = Flux.on("wheel", ({ targets, ...e }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onWheel")?.(e);
      }
    });
    unsubKeyDown = Flux.on("keydown", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onKeyDown")?.(e);
      }
    });
    unsubKeyUp = Flux.on("keyup", (e) => {
      let id = getFocusedNodeId();
      if (id != null) {
        getEventHandler(id, "onKeyUp")?.(e);
      }
    });
    draw();
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

// packages/core/src/color.ts
k([names_default]);
function parseColorToU32(color) {
  let { r: r2, g: g2, b: b2, a: a2 } = w(color).toRgb();
  return ((r2 & 255) << 24 | (g2 & 255) << 16 | (b2 & 255) << 8 | a2 * 255 & 255) >>> 0;
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
    if (elementType === "window")
      ffi.createRoot(proxy.id);
    else
      ffi.createNode(proxy.id, elementType);
    return proxy;
  },
  createTextNode: (value) => {
    let proxy = createProxyNode("span");
    ffi.createNode(proxy.id, "span");
    ffi.setProperty(proxy.id, "text", "" + value);
    return proxy;
  },
  replaceText: (node, value) => {
    ffi.setProperty(node.id, "text", "" + value);
  },
  isTextNode: (node) => node?.elementType === "span",
  setProperty: (node, name, value) => {
    if (!node)
      return;
    if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
      setEventHandler(node.id, name, value);
      return;
    }
    if (name === "color" && typeof value === "string") {
      ffi.setProperty(node.id, name, parseColorToU32(value));
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
// lattice/default-app/logo.tsx
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
      x: 5 * R - 2 * M2,
      y: 0,
      rot: 0,
      shade: "light"
    }]
  },
  {
    width: 4 * R + 2 * M2,
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
var EXPLODE_DIST = 10;
var STAGGER_DELAY = 50;
var ANIM_DURATION = 600;
var HOLD_ASSEMBLED = 5000;
var HOLD_EXPLODED = 0;
function TangramLetter(props) {
  let [dist, setDist] = createSignal(EXPLODE_DIST);
  let start = null;
  let letterCx = props.letter.width / 2;
  let letterCy = props.letter.height / 2;
  let pieceVectors = props.letter.pieces.map((p2) => {
    let [scx, scy] = shapeCenter(p2.shape, p2.rot);
    return [p2.x + scx - letterCx, p2.y + scy - letterCy];
  });
  let pieceSpins = props.letter.pieces.map((_, i2) => ((i2 * 7 + 3) % 11 - 5) * 30);
  onRender((_tick) => {
    if (start === null)
      start = _tick;
    let tick = _tick - start;
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
  let [scale, setScale] = createSignal(1);
  onResize(({
    width
  }) => {
    setScale(width * 0.8 / 1500);
  });
  var _el$4 = createElement("view"), _el$5 = createElement("view");
  insertNode(_el$4, _el$5);
  setProp(_el$4, "justifyContent", "center");
  setProp(_el$4, "width", 1500);
  setProp(_el$5, "gap", 50);
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

// lattice/default-app/app.tsx
function App() {
  var _el$ = createElement("window"), _el$2 = createElement("view"), _el$3 = createElement("d-rect"), _el$4 = createElement("view"), _el$5 = createElement("text"), _el$7 = createElement("view");
  insertNode(_el$, _el$2);
  setProp(_el$, "title", "Solid-RT Demo");
  insertNode(_el$2, _el$3);
  insertNode(_el$2, _el$4);
  insertNode(_el$2, _el$7);
  setProp(_el$2, "flexGrow", 1);
  setProp(_el$2, "justifyContent", "center");
  setProp(_el$2, "alignItems", "center");
  setProp(_el$2, "flexDirection", "column-reverse");
  setProp(_el$2, "gap", 20);
  setProp(_el$3, "color", "#111");
  insertNode(_el$4, _el$5);
  insertNode(_el$5, createTextNode(`waiting for connection...`));
  setProp(_el$5, "color", "lightgrey");
  insert(_el$7, createComponent2(Logo, {}));
  return _el$;
}
render(() => createComponent2(App, {}));
