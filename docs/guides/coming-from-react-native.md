# Coming from Expo/React Native

The element model is close to React Native's: a small set of native
elements, flexbox layout, and styles as props. If you build with Expo, the
workflow is familiar too: a dev server, a client app on the phone that joins
it, and one command to build an installable app.

SolidRT apps use SolidJS instead of React. See
[React and SolidJS 2](/guides/react-and-solid/) for that side.

SolidRT runs on Android, Linux, macOS and Windows. There is no iOS build yet.

## Elements and layout

The difference to learn first: layout and paint are separate. A `view` lays
out and takes input, but never paints.

| React Native | SolidRT |
| --- | --- |
| `<View style={{ backgroundColor }}>` | `<view>` with a `<rect color=...>` inside it, which fills the view |
| `<Text>` | `<text>`, with `<span>` for a styled run inside it |
| `<Image>`, `expo-image` | `<texture src=...>` with `createImage` from `@solidrt/core/image` |
| `StyleSheet.create` | Layout and paint props go directly on the element |
| `flexDirection` defaults to `column` | Same |
| `<Pressable onPress>` | `onPointerDown`/`onPointerUp` on any element, or a button from [Components](/extensions/) |
| `Animated`, Reanimated | `onFrame` driving the [detached elements](/core/detached/) (`d-rect`, `d-view`, ...), which move without a layout pass |
| `useWindowDimensions` | `windowSize()` |
| `SafeAreaView` | `safeArea()` insets, used as padding |
| `BackHandler` | `onBack` |
| React Navigation, Expo Router | `@solidrt/router` |

There is no JavaScript-to-native bridge to think about. A prop update writes
to the native node directly.

## Device APIs

| Expo | SolidRT |
| --- | --- |
| `expo-camera` | `createCamera` from `@solidrt/core/camera` |
| `expo-audio`, `expo-av` | `createSound` from `@solidrt/core/audio`, `createVideo` from `@solidrt/core/video` |
| `expo-file-system` | `flux:fs` |
| `expo-sqlite` | `flux:sqlite` |
| `fetch` | `fetch`, the same standard API |

The `flux:*` modules are documented under [Runtime](/runtime/).

## Workflow

| Expo | SolidRT |
| --- | --- |
| `npx create-expo-app` | `bun create solidrt` |
| `npx expo start` | `bun run dev` |
| Expo Go | The SolidRT player, which joins your dev server |
| Fast Refresh | A save reloads the app; state starts fresh (see [The dev loop](/guides/dev-loop/)) |
| `app.json` | The `solidrt` key in `package.json`: `appId`, `icon`, `capabilities`, `android.versionCode` |
| EAS Build | `bun run pack --apk`, on your own machine, with no Android SDK |

A packed APK is signed with a development key, which is fine for sideloading.
Publishing to a store needs your own key. See [sol pack](/tools/pack/).
