# srt console

{{ usage console }}

The dev console: the dev servers running on this machine, the clients
attached to each, and a button to start a local client for one. It is not
served by a dev server: the CLI ships the console pre-compiled
(`dist/console.srtapp`, see [srt pack --app](../pack/docs.md)) and this
command starts the `solidrt` runner on that file, so it opens at once and
runs the same whether a project is open or not.

In a checkout the file is built with `make -C packages/cli
dist/console.srtapp`; rerun it after editing `apps/console`. Everything
after `--` reaches the console as its arguments.
