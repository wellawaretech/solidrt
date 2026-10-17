"use isolate"
// A stream for settle.test.tsx: the numbers 1 to n, a pause before each, on
// the isolate's own clock (an isolate is not stepped with the app).

// The pause before each item: long enough that a settle which did not wait
// for a stream step would return between two items.
const PAUSE_MS = 20

export async function* count(n: number): AsyncGenerator<number> {
  for (let i = 1; i <= n; i++) {
    await new Promise<void>((resolve) => setTimeout(() => resolve(), PAUSE_MS))
    yield i
  }
}
