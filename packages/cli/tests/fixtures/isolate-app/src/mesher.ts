"use isolate"
// The fixture app's isolate module: its id is "mesher", counted from the
// source root (src/), which is what the test in tests/ relies on.

export function sum(values: number[]): number {
  return values.reduce((total, value) => total + value, 0)
}
