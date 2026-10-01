/**
 * The node verbs of a gui test: finding and reading the nodes of the render
 * tree. A GUI build of Flux with the test module has them, which is the
 * SolidRT dev client; an app test reaches them through
 * `@solidrt/core/test`, whose locators are built on this module.
 *
 * A node reads as the control API's `/tree` reports it: records cross as
 * JSON text, shaped in one place for both readers.
 */
declare module "flux:test/gui" {
  /**
   * One field of a query: a string matches exactly (case-sensitive), `true`
   * matches every node that has the field (for a pattern the caller checks
   * itself), absent is no constraint.
   */
  export type FieldQuery = string | true

  export interface NodeQuery {
    /** The element kind's name: "view", "text", ... */
    kind?: string
    /** The text a text or span node shows. */
    text?: FieldQuery
    /** The node's `label` prop. */
    label?: FieldQuery
  }

  /**
   * The nodes under `root` (the tree root when null, `root` included) that
   * the query matches, depth first, at most `limit`: a JSON array of node
   * records without children, each with `path`, the ids from the search
   * root down to it. A node whose text matches under an ancestor showing
   * the very same text is left out. Null when `root` is no current node.
   */
  export function find(root: number | null, query: NodeQuery, limit: number): string | null

  /**
   * The record of node `id` with `depth` levels of children (0: the node
   * alone), as JSON; null when `id` is no current node.
   */
  export function node(id: number, depth: number): string | null

  /**
   * Whether the node is painted where a user could see it: mounted and not
   * exiting, its painted box still has area once every clipping ancestor
   * and the window have cut it, and the opacity multiplied up its ancestor
   * chain is above 0. It says nothing of what is painted over it; `hit`
   * answers that.
   */
  export function visible(id: number): boolean

  /** The ids from the tree root down to `id`; null when `id` is no current node. */
  export function path(id: number): number[] | null

  /**
   * The ids of the nodes a pointer at the window point reaches, root first,
   * the node it lands on last; empty when nothing is hit.
   */
  export function hit(x: number, y: number): number[]

  /**
   * The GPU resource inventory as JSON, the record the control API's `/gpu`
   * answers with: textures, buffers, pipelines, programs and each draw
   * target's entries. `label` keeps the resources created with exactly
   * that label; `draw` is the draw entry id whose params are reported in
   * full (matrix-valued ones are elided elsewhere).
   */
  export function gpu(label: string | null, draw: number | null): string
}
