<section>
  <hgroup>
    <h1>SolidRT</h1>
    <p>The Solid Runtime</p>
  </hgroup>
  <p>
    SolidRT lets you build cross-platform native apps with familiar web
    technologies. Built from the ground up for developers and AI agents to
    work together.
  </p>
  <p><a href="/start/" role="button">Get started</a></p>
</section>

<section>
  <!-- Placeholder: one live Core example (code + captured recording) lands
       here once the capture pipeline exists. -->
  <pre><code class="hljs language-tsx">import { render, createSignal } from "@solidrt/core"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    &lt;window&gt;
      &lt;view padding={12} onPointerDown={() => setCount(count() + 1)}&gt;
        &lt;text&gt;Clicked {count()} times&lt;/text&gt;
      &lt;/view&gt;
    &lt;/window&gt;
  )
}

render(() => &lt;App /&gt;)</code></pre>
  <p>
    A whole app. No build config, no markup language, no virtual DOM: reading
    <code>count()</code> in the JSX wires that one piece of text to that one
    signal, and a click writes it straight to the native node.
  </p>
</section>

<section>
  <h2>What you get</h2>
  <ul>
    <li>
      <strong>Native rendering.</strong> A GPU renderer behind a small
      element vocabulary, with flexbox layout and demand-driven frames.
      Nothing draws when nothing changed.
    </li>
    <li>
      <strong>A familiar model.</strong> JSX, signals, and components, from
      <a href="https://www.solidjs.com/">SolidJS</a>. If you have written a
      web app, you already know how to write this one.
    </li>
    <li>
      <strong>Batteries in the runtime.</strong> Files, HTTP, SQLite,
      sockets, subprocesses, and peer-to-peer ship with
      <a href="/runtime/">Flux</a>, the JavaScript runtime underneath.
    </li>
    <li>
      <strong>Made to be inspected.</strong> The running app exposes its
      logs, render tree, and live screenshots over MCP, so a coding agent
      can see what your app is actually doing.
    </li>
  </ul>
</section>

<section>
  <h2>From example to app</h2>
  <p>
    The <a href="/tools/">tools</a> take you from a running example to a
    standalone app: one CLI for scaffolding, live development, and packaging.
    The same source runs on Linux, macOS, Windows, and Android, and the dev
    server drives a desktop window and a phone at the same time.
  </p>
  <p><a href="/start/" role="button">Get started</a></p>
</section>
