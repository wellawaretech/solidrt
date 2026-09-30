// The entry: the window and the route tree. App (app.tsx) is the layout
// around every screen; the screens live in routes/, one file each.
import { render } from "@solidrt/core"
import { Window } from "@solidrt/components"
import { Router, Route } from "@solidrt/router"
import { App } from "./app"
import { Home } from "./routes/home"
import { NotFound } from "./routes/not-found"

render(() => (
  <Window title="The Solid Runtime">
    <Router initial="/">
      <Route path="/" component={App}>
        <Route path="/" component={Home} />
        <Route path="/$" component={NotFound} />
      </Route>
    </Router>
  </Window>
))
