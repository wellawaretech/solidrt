// One chat's memory: what was said, oldest first, and how each command
// turned out. App keeps one per party - a client, or a server as the group
// its clients are in - for as long as the console runs, so a chat keeps its
// history while you talk to another.
import { createSignal } from "@solidrt/core"
import type { Block, Entry } from "./blocks"

/** Who a chat is with: a client on a server, or (client null) the server
 * itself, as the group its clients are in. A client is named by its stable
 * key (machine + storage tree, servers.ts clientKey), not its connection id,
 * so a restart in the same slot continues the same chat. */
export type Party = { server: string; client: string | null }

/** The key a party's chat is kept under. */
export function partyKey(party: Party): string {
  return party.client === null ? party.server : `${party.server}/${party.client}`
}

export type Conversation = {
  /** Who this chat is with, as its header names it. Fixed at open: a
   * client's label does not change, and a gone client still has a name. */
  title: string
  blocks: () => Entry[]
  /** Appends, never replaces; returns the block's id so a command can be
   * marked once it settles. */
  say: (block: Block) => number
  settle: (id: number, ok: boolean) => void
  /** How a command turned out: true a check, false a cross, undefined while
   * it runs or when the block never acted. */
  outcome: (id: number) => boolean | undefined
}

export function createConversation(title: string, opening: Block[]): Conversation {
  let seq = 0
  let [blocks, setBlocks] = createSignal<Entry[]>(opening.map((block) => ({ ...block, id: ++seq })))
  // Kept beside the blocks rather than in them, so a block stays plain data.
  let [outcomes, setOutcomes] = createSignal<Record<number, boolean>>({})
  return {
    title,
    blocks,
    // The updater form rather than a read: two says in one handler run
    // before the read of the first has flushed.
    say: (block) => {
      let id = ++seq
      setBlocks((list) => [...list, { ...block, id }])
      return id
    },
    settle: (id, ok) => setOutcomes((all) => ({ ...all, [id]: ok })),
    outcome: (id) => outcomes()[id],
  }
}
