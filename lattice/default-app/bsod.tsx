import { render } from "@solidrt/core"

// Shown by the runtime when an app fails to start (a module/startup error means
// render() never ran, so nothing kept the engine alive). Deliberately tiny and
// dependency-free so it always bundles and starts; shipped as compiled bytecode
// (include_bytes! in lattice) and evaluated like a packed app. Calls render() so
// it stays alive and can be replaced by a later reload of the fixed app.
function Bsod() {
  return (
    <window title="solidrt">
      <d-rect color="#1144bb" />
      <view
        flexGrow={1}
        justifyContent="center"
        alignItems="center"
        flexDirection="column"
        gap={16}
      >
        <text color="white" fontSize={64} fontWeight={700}>:(</text>
        <text color="white" fontSize={22}>Something went wrong</text>
        <text color="#aac2ff" fontSize={15}>The application could not be started.</text>
      </view>
    </window>
  )
}

render(() => <Bsod />)
