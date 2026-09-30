// The catch-all: a link that matches no other route lands here.
import { View, Text } from "@solidrt/components"
import { useLocation } from "@solidrt/router"

export function NotFound() {
  let location = useLocation()

  return (
    <View layout={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <Text layout={{ fontSize: 24 }} style={{ color: "#ccc" }}>
        {"Nothing at " + location()?.path}
      </Text>
    </View>
  )
}
