#!/usr/bin/env bun
//
// The runtime build base the CLI package ships (packages/cli/runtime/,
// included by every runtime's Makefile: the checkout's runtime/ and a custom
// runtime beside an app) carries two files the checkout also needs at its
// root, where cargo and rustup read them: the toolchain pin and cargo's
// config. The copies must not drift from the originals, and a hand-kept
// rule drifts, so CI runs this. Exits 1 naming each copy that differs.

import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

let REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
let BASE = join(REPO, "packages", "cli", "runtime");

let PAIRS: Array<[copy: string, original: string]> = [
  [join(BASE, "rust-toolchain.toml"), join(REPO, "rust-toolchain.toml")],
  [join(BASE, "config.toml"), join(REPO, ".cargo", "config.toml")],
];

let drifted = PAIRS.filter(([copy, original]) => readFileSync(copy, "utf8") !== readFileSync(original, "utf8"));
for (let [copy, original] of drifted) {
  console.error(`${copy} differs from ${original}: copy the original over it`);
}
if (drifted.length > 0) process.exit(1);
console.log(`Checked ${PAIRS.length} runtime base files against their originals`);
