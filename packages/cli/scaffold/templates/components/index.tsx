// Components template: built with the @solidrt/components framework.
import { render } from "@solidrt/core"
import { Window, SafeArea, Text } from "@solidrt/components"

function App() {
  return (
    <Window>
      <SafeArea>
        <Text>Hello, World!</Text>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
