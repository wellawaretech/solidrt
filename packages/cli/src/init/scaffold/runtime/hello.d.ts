// The types of runtime:hello (runtime/src/hello.rs); tsconfig.json includes
// this folder. One file per module, a `declare module "runtime:<name>"`.
declare module "runtime:hello" {
  /** The greeting from the runtime. */
  export function hello(): string
}
