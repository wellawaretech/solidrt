// Packages with no bundled or DefinitelyTyped declarations. @babel/core is
// covered by @types/babel__core; plugins and presets have no dedicated types
// by convention - they are consumed as opaque PluginItem values.
declare module "@babel/plugin-syntax-jsx" {
  import { type PluginItem } from "@babel/core"
  let plugin: PluginItem
  export default plugin
}
declare module "@babel/preset-typescript" {
  import { type PluginItem } from "@babel/core"
  let preset: PluginItem
  export default preset
}
declare module "babel-preset-solid" {
  import { type PluginItem } from "@babel/core"
  let preset: PluginItem
  export default preset
}
