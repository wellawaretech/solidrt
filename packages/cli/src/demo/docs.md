# srt demo

{{ usage demo }}

The demos the SolidRT packages come with, shipped pre-bundled with the CLI so
every install has all of them, whether or not the packages themselves are
installed. Without an argument it lists them, numbered:

```
  1  3d/the-third-dimension
  2  components/gallery
```

`srt demo 1` runs that one, exactly as [srt run](../server/docs.md) runs any
project: a dev server and a local client window, with the same server
options (`--lan` to reach it from a device, `--port`, `--tunnel`). So a
running demo is an app the [console](../console/docs.md) picks up like any
other, with its tree, stats and control API. The qualified name works too
(`srt demo 3d/the-third-dimension`), which is the stable way to name one in
a script - the numbers follow the list.

A package keeps its demos in `demos/` in the repository, and that folder is
one project: its own package.json, one `assets/` folder, and `src/<name>.tsx`
per demo. The CLI ships that project bundled (`dist/demos/<package>/`, built
by `make -C packages/cli demos`), and `srt demo` serves it with the chosen
bundle as its entry. So the demos of one package share a dev server and a
port - start a second one while the first is up and it says so. Demos from
different packages run side by side.

The bundles are what runs, so editing a demo's source does not reload it. In
a checkout, work on a demo with `srt run src/<name>.tsx --project` from the
package's `demos/` folder, and rebuild the shipped bundles with
`make -C packages/cli demos` when done.
