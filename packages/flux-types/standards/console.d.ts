// The web-standard `console` global. flux provides exactly these four methods
// (no info/trace/table/group/...). A global script file: these declarations
// stand in for the ones lib.dom would otherwise supply.

declare let console: {
  debug(...args: any[]): void
  log(...args: any[]): void
  warn(...args: any[]): void
  error(...args: any[]): void
}