---
order: guides core tools runtime extensions architecture
---
<!-- The aurora behind the whole landing page (website/assets/js/aurora.js).
     It is dark in either color scheme, so the page pins the dark theme; the
     inline script runs while the page is parsed, before the first paint. The
     canvas's gradient is the aurora's navy ground, for when WebGL2 is out. -->
<div>
<script>document.documentElement.dataset.theme = "dark"</script>
<canvas class="aurora" aria-hidden="true" style="position: fixed; inset: 0; width: 100%; height: 100%; z-index: -1; background: linear-gradient(#05080f, #070b1a)"></canvas>
<script type="module" src="/js/aurora.js"></script>
</div>
<style>
  /* The landing hero, after the intro demo's opening screen. Landing-only,
     so it lives here rather than in site.css. */
  /* The demo's palette, for the hero and the sections below it. */
  article:has(> .hero) {
    --hero-text: #eef2ff;
    --hero-text-2: #a7b2d4;
    --hero-text-3: #66729a;
    --hero-panel: #0e1629;
    /* A panel over the aurora, which shows through it. */
    --hero-panel-glass: rgb(10 17 34 / 0.8);
    --hero-stroke: #1e2a4a;
    --hero-blue: #7ea9ea;
    --hero-blue-light: #9fb8ff;
    --hero-blue-hover: #9dc0f5;
    --hero-ink: #07101f;
    --hero-mint: #5eead4;
    --hero-amber: #ffb86b;
    --hero-violet: #a78bfa;
  }
  .hero {
    /* Headline size: scales with the viewport between these bounds. */
    --hero-title-size: clamp(2.25rem, 6vw, 3.6rem);
    /* The logo's size, and its size once stacked on a narrow screen. */
    --hero-mark-size: 24rem;
    --hero-mark-size-stacked: 19rem;
    /* The enter: rise distance, duration, and the delay between blocks. */
    --hero-rise-from: 1.5rem;
    --hero-rise-duration: 0.8s;
    --hero-rise-stagger: 70ms;
    display: flex;
    align-items: center;
    gap: calc(var(--sol-spacing-xl) * 2);
    padding: calc(var(--sol-spacing-xl) * 2) 0 calc(var(--sol-spacing-xl) * 3);
    color: var(--hero-text-2);
  }
  /* The mark's glow reaches past the screen edge; clip it there rather than
     let it widen the page. */
  body:has(.hero) {
    overflow-x: clip;
  }
  /* The hero spans the page. */
  article:has(> .hero) {
    max-width: none;
  }
  /* Sections keep the reading width, on the hero's left edge. */
  article:has(> .hero) > section {
    max-width: var(--sol-content-width);
    margin-inline: 0;
    margin-bottom: calc(var(--sol-spacing-xl) * 3);
    padding: 0;
    border-radius: 0;
    background: none;
  }
  /* A section's heading is its eyebrow. */
  article:has(> .hero) > section > h2.eyebrow {
    margin: 0 0 var(--sol-spacing-xl);
  }
  .hero-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: calc(var(--sol-spacing-xl) * 1.4);
  }
  .hero-text > * {
    margin: 0;
    animation: hero-rise var(--hero-rise-duration) cubic-bezier(0.2, 0.9, 0.3, 1.1) both;
  }
  .hero-text > :nth-child(2) { animation-delay: calc(var(--hero-rise-stagger) * 1); }
  .hero-text > :nth-child(3) { animation-delay: calc(var(--hero-rise-stagger) * 2); }
  .hero-text > :nth-child(4) { animation-delay: calc(var(--hero-rise-stagger) * 3); }
  .hero-text > :nth-child(5) { animation-delay: calc(var(--hero-rise-stagger) * 4); }
  @keyframes hero-rise {
    from {
      opacity: 0;
      transform: translateY(var(--hero-rise-from));
    }
  }
  /* The mint label with a dash that opens the hero and each section. */
  .eyebrow {
    display: flex;
    align-items: center;
    gap: 0.9em;
    color: var(--hero-mint);
    font-family: var(--sol-font-mono);
    font-size: 0.85rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .eyebrow::before {
    content: "";
    width: 1.75rem;
    height: 3px;
    border-radius: 2px;
    background: currentColor;
  }
  .hero-title {
    color: var(--hero-text);
    font-size: var(--hero-title-size);
    font-weight: 800;
    line-height: 1.04;
    letter-spacing: -0.01em;
  }
  .hero-title > span {
    display: block;
  }
  .hero-accent {
    background: linear-gradient(90deg, var(--hero-blue), var(--hero-blue-light) 55%, var(--hero-mint));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .hero-lead {
    max-width: 36rem;
    font-size: 1.2rem;
    line-height: 1.55;
    text-wrap: pretty;
  }
  .hero-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sol-spacing-md);
    padding: 0;
    list-style: none;
  }
  .hero-chips > li {
    display: flex;
    align-items: center;
    gap: 0.6em;
    margin: 0;
    padding: 0.4em 1em 0.4em 0.85em;
    border: 1.5px solid var(--hero-stroke);
    border-radius: var(--sol-radius-full);
    background: var(--hero-panel);
    font-size: 0.85rem;
    font-weight: 500;
  }
  .hero-chips > li::before {
    content: "";
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--dot);
  }
  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sol-spacing-lg);
  }
  /* The landing's buttons, the hero's and the sections', as the demo's
     primary pill. */
  article:has(> .hero) a[role="button"] {
    padding: 0.8em 1.6em;
    border-radius: var(--sol-radius-full);
    background: var(--hero-blue);
    color: var(--hero-ink);
    font-size: 1.05rem;
    font-weight: 700;
  }
  article:has(> .hero) a[role="button"]:hover {
    background: var(--hero-blue-hover);
    color: var(--hero-ink);
  }
  .hero-command {
    padding: 0.8em 1.4em;
    border: 1.5px solid var(--hero-stroke);
    border-radius: var(--sol-radius-full);
    background: var(--hero-panel-glass);
    color: var(--hero-text);
    font-family: var(--sol-font-mono);
    font-size: 0.95rem;
  }
  .hero-command > span {
    color: var(--hero-text-3);
    margin-right: 0.6em;
  }
  /* A section that spans the hero's width instead of the reading width. */
  article:has(> .hero) > section.wide {
    max-width: none;
  }
  /* What you get: one card per result, three columns (one on a narrow
     screen, with the hero). The grid spans the window up to a cap, inset by
     the page gutter: its section spans the centered page block, so half the
     block less half the grid's width centers it on the window. */
  .outcomes {
    /* The widest the grid gets, gutters included, so lines stay readable on
       a wide window. */
    --outcomes-max-width: 80rem;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sol-spacing-lg);
    margin: 0 calc(50% - min(50vw, var(--outcomes-max-width) / 2));
    padding: 0 var(--sol-spacing-xl);
    list-style: none;
  }
  .outcomes > li {
    margin: 0;
    padding: var(--sol-spacing-lg) var(--sol-spacing-xl);
    border: 1.5px solid var(--hero-stroke);
    border-radius: var(--sol-radius-lg);
    background: var(--hero-panel-glass);
    color: var(--hero-text-2);
    line-height: 1.55;
  }
  .outcomes strong {
    display: block;
    margin-bottom: var(--sol-spacing-sm);
    color: var(--hero-text);
    font-size: 1.05rem;
  }
  /* The closing band: the hero's call to action again. */
  .closing {
    /* Headline size: scales with the viewport between these bounds. */
    --closing-title-size: clamp(2rem, 5vw, 3rem);
  }
  .closing-title {
    margin: 0 0 var(--sol-spacing-xl);
    color: var(--hero-text);
    font-size: var(--closing-title-size);
    font-weight: 800;
    line-height: 1.08;
    letter-spacing: -0.01em;
  }
  .closing-title > span {
    display: block;
  }
  .closing > .hero-lead {
    margin: 0 0 calc(var(--sol-spacing-xl) * 2);
  }
  /* The mark: the logo over a soft glow twice its size, as in the demo. */
  .hero-mark {
    position: relative;
    flex: 0 0 auto;
    width: var(--hero-mark-size);
    aspect-ratio: 1;
    display: grid;
    place-items: center;
  }
  .hero-mark::before {
    content: "";
    position: absolute;
    inset: -50%;
    border-radius: 50%;
    background: radial-gradient(closest-side, #3f6fd455, #2a3f8f22 55%, transparent);
  }
  .hero-logo {
    position: relative;
    width: 100%;
    height: auto;
  }
  /* Below this width the columns stack, the mark on top, and the outcome
     cards drop to one column. */
  @media (max-width: 52rem) {
    .hero {
      flex-direction: column-reverse;
      align-items: stretch;
      padding-top: var(--sol-spacing-xl);
    }
    .hero-mark {
      width: var(--hero-mark-size-stacked);
      align-self: center;
    }
    .outcomes {
      grid-template-columns: 1fr;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .hero-text > * {
      animation: none;
    }
  }
</style>

<div class="hero">
<div class="hero-text">
<p class="eyebrow">The Solid Runtime</p>
<p class="hero-title" role="heading" aria-level="1"><span>One codebase.</span><span>Every screen.</span><span class="hero-accent">Every pixel, yours.</span></p>
<p class="hero-lead">SolidRT lets you build cross-platform native apps with familiar web technologies. Built from the ground up for developers and AI agents to work together.</p>
<ul class="hero-chips">
<li style="--dot: var(--hero-amber)">Rust engine, GPU rendering</li>
<li style="--dot: var(--hero-blue)">SolidJS 2.0</li>
<li style="--dot: var(--hero-violet)">2D and 3D built in</li>
<li style="--dot: var(--hero-mint)">Linux &middot; Windows &middot; macOS &middot; Android</li>
</ul>
<div class="hero-actions">
<a href="/guides/the-basics/" role="button">Get started &rarr;</a>
<span class="hero-command"><span>$</span>bun create solidrt my-app</span>
</div>
</div>
<div class="hero-mark" aria-hidden="true">
<svg class="hero-logo" viewBox="-6 -6 112 112" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="logo-a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3f5494"/><stop offset="1" stop-color="#162b6c"/></linearGradient><linearGradient id="logo-b" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#547ebf"/><stop offset="1" stop-color="#2b5696"/></linearGradient><linearGradient id="logo-c" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7ea9ea"/><stop offset="1" stop-color="#5681c1"/></linearGradient></defs><path d="M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z" fill="url(#logo-a)"/><path d="M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z" fill="url(#logo-b)"/><path d="M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z" fill="url(#logo-c)"/><path d="M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z" fill="url(#logo-b)"/><path d="M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z" fill="url(#logo-c)"/><path d="M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z" fill="url(#logo-a)"/><path d="M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z" fill="url(#logo-c)"/></svg>
</div>
</div>

<section>
  <h2 class="eyebrow">A whole app</h2>
  <!-- Placeholder: one live Core example (code + captured recording) lands
       here once the capture pipeline exists. -->



```tsx
import { render, createSignal } from "@solidrt/core"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    <window>
      <view padding={12} onPointerDown={() => setCount(count() + 1)}>
        <text>Clicked {count()} times</text>
      </view>
    </window>
  )
}

render(() => <App />)
```

  <p>
    This is all there is to it: a fully working application, on desktop
    and mobile.
  </p>
</section>

<section class="wide">
  <h2 class="eyebrow">What you get</h2>
  <ul class="outcomes">
    <li>
      <strong>Write it once, run it on every screen.</strong>
      The same source runs on Linux, Windows, macOS, and Android, and looks
      the same on each, because SolidRT draws every pixel itself.
    </li>
    <li>
      <strong>You already know how.</strong>
      If you have built a web app, you can build this one: JSX, components,
      and signals from <a href="https://www.solidjs.com/">SolidJS</a>, and
      <a href="/core/layout/">flexbox and grid</a> with the CSS names you
      expect.
    </li>
    <li>
      <strong>Components and routing to start with.</strong>
      Themable widgets (buttons, inputs, sliders, dialogs, menus) and a
      router that turns screens into routes. Switch the theme and the running
      UI recolors without remounting.
    </li>
    <li>
      <strong>A spatial core for fast 2D and 3D.</strong>
      Thousands of animated sprites in one draw call, 3D scenes, and
      <a href="/core/shaders/">custom shaders</a>, all in the same tree as
      your UI. A dashboard and a game are the same kind of project.
    </li>
    <li>
      <strong>Reach the device and the network.</strong>
      Camera, microphone, audio, video, files, HTTP, SQLite, and peer-to-peer
      have one API on desktop and phone, built into
      <a href="/runtime/">the runtime</a>, with no plugin per platform.
    </li>
    <li>
      <strong>Save once, see it everywhere.</strong>
      Save a file and your desktop window and your phone reload together, with
      no rebuild and no cable. See <a href="/guides/dev-loop/">the dev
      loop</a>.
    </li>
    <li>
      <strong>Your agent sees what it built.</strong>
      A coding agent reads the running app's logs, layout, and screenshots on
      each connected device, so it checks its own work on every screen
      instead of guessing.
    </li>
    <li>
      <strong>Tests that never flake.</strong>
      <a href="/tools/test/">App tests</a> run without a window and count
      frames instead of wall-clock time, so every run gives the same result.
    </li>
    <li>
      <strong>One app file, one binary per platform.</strong>
      The app is a single <code>.solapp</code>, the same file on every
      platform. <code>sol pack</code> combines it with the runtime into a
      standalone executable, or an installable APK with no Android SDK
      needed.
    </li>
  </ul>
</section>

<section>
  <h2 class="eyebrow">About</h2>
  <p>
    SolidRT is made by Antoine van Wel at
    <a href="https://wellawaretech.com/">Well-Aware Technologies</a> and
    developed in the open on
    <a href="https://github.com/wellawaretech/solidrt">GitHub</a>. Why it
    exists is written up in
    <a href="https://wellawaretech.com/blog/why/">
    Why?</a>
  </p>
  <p>
    SolidRT is free and open source. If it is useful to you or your
    company, consider
    <a href="https://github.com/sponsors/wellawaretech">sponsoring the
    project</a>: it helps keep development going.
  </p>
</section>

<section class="closing">
  <h2 class="eyebrow">Get started</h2>
  <p class="closing-title"><span>Your first window,</span><span class="hero-accent">in one minute.</span></p>
  <p class="hero-lead">
    All you need is <a href="https://bun.sh/">Bun</a>. One command scaffolds
    the project, develops it live on every device, and packs it into a
    standalone app, an Android APK included. No Android Studio, no SDK, no
    other tooling to install: <a href="/tools/">the CLI</a> takes care of
    the rest.
  </p>
  <p class="hero-lead">
    And you never have to leave. When an app needs more, add Rust to your
    toolchain and build <a href="/guides/custom-runtime/">a runtime of your own</a>: a
    customized, optimized executable around the same project and the same
    code. Nothing to eject from, because it is an addition, not a rewrite.
  </p>
  <div class="hero-actions">
    <a href="/guides/the-basics/" role="button">Get started &rarr;</a>
    <span class="hero-command"><span>$</span>bun create solidrt my-app</span>
  </div>
</section>
