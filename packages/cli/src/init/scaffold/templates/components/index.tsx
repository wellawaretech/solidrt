// Components template: built with the @solidrt/components framework.
import { render, createLinearGradient } from "@solidrt/core"
import { Window, SafeArea, View, Text } from "@solidrt/components"
import { Icon } from "./icon"

function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  return (
    <Window title="The Solid Runtime" style={{ backgroundColor }}>
      <SafeArea>
        <View layout={{ flex: 1, gap: 20, alignItems: "center", justifyContent: "center" }}>
          <Icon />
          <Text layout={{ fontSize: 40 }} style={{ color: "#ccc" }}>
            The Solid Runtime
          </Text>
        </View>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
