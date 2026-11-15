// navigator.permissions: the web's Permissions API, simplified (the
// contract is permissions.rs's module doc and
// packages/flux-types/standards/permissions.d.ts). Evaluated once per
// context by permissions.rs, which hands in the native hooks and sets what
// this source returns as `navigator.permissions`. Plain JS on purpose: a
// status is a small object with listeners, which is short here and a class
// of Persistents in rquickjs.
//
// The hooks: `stateOf(name)` is the name's state in this engine, or
// undefined for a name the query refuses; `listen(callback)` registers a
// listener on the sticky "permission" event, whose payload is the whole map
// ({ camera: "granted", microphone: "prompt" }), and returns its
// unsubscribe function.
(stateOf, listen) => {
  const API = "navigator.permissions.query"
  const CHANGE = "change"

  // The status of one query. One bus listener per status while it has a
  // listener of its own (onchange or "change"), none otherwise, so a status
  // nobody listens to holds nothing.
  class PermissionStatus {
    #name
    #seen
    #listeners = new Set()
    #onchange = null
    #unlisten = null

    constructor(name, state) {
      this.#name = name
      this.#seen = state
    }

    get name() {
      return this.#name
    }

    get state() {
      return stateOf(this.#name)
    }

    get onchange() {
      return this.#onchange
    }

    set onchange(listener) {
      this.#onchange = typeof listener === "function" ? listener : null
      this.#sync()
    }

    addEventListener(type, listener) {
      if (type !== CHANGE || typeof listener !== "function") {
        return
      }
      this.#listeners.add(listener)
      this.#sync()
    }

    removeEventListener(type, listener) {
      if (type !== CHANGE) {
        return
      }
      this.#listeners.delete(listener)
      this.#sync()
    }

    // Listen on the bus while any listener is set, and stop once none is.
    // The state seen is read afresh when listening starts, so a change made
    // while nobody listened does not fire later.
    #sync() {
      let wanted = this.#listeners.size > 0 || this.#onchange !== null
      if (wanted && this.#unlisten === null) {
        this.#seen = stateOf(this.#name)
        this.#unlisten = listen((map) => this.#changed(map))
      } else if (!wanted && this.#unlisten !== null) {
        let unlisten = this.#unlisten
        this.#unlisten = null
        unlisten()
      }
    }

    // A map arrived: fire when this name's state differs from the last one
    // this status saw. Every listener runs; the first throw is rethrown
    // after them, for the bus to report.
    #changed(map) {
      let state = map[this.#name]
      if (state === undefined || state === this.#seen) {
        return
      }
      this.#seen = state
      let event = Object.freeze({ type: CHANGE, target: this, currentTarget: this })
      let listeners = [...this.#listeners]
      if (this.#onchange !== null) {
        listeners.push(this.#onchange)
      }
      let failure = null
      for (let listener of listeners) {
        try {
          listener.call(this, event)
        } catch (e) {
          if (failure === null) {
            failure = { error: e }
          }
        }
      }
      if (failure !== null) {
        throw failure.error
      }
    }
  }

  async function query(descriptor) {
    if (typeof descriptor !== "object" || descriptor === null) {
      throw new TypeError(`${API}: pass a descriptor object with a name`)
    }
    let name = descriptor.name
    if (typeof name !== "string") {
      throw new TypeError(`${API}: pass the permission name as a string`)
    }
    let state = stateOf(name)
    if (state === undefined) {
      throw new TypeError(`${API}: unsupported permission name ${name}`)
    }
    return new PermissionStatus(name, state)
  }

  return Object.freeze({ query })
}
