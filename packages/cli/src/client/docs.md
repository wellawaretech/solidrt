# srt client

{{ usage client }}

The client half of [srt run](../server/docs.md), on its own. Without flags
it attaches to the dev server of the project (or file) in the current
directory; `--port` picks a local server by port and `--server` names any
address, which is how a second machine joins a server started with `--lan`.
Without a running server the client starts on its own, into the launcher
(`--port` and `--server` must name a live server). A phone or tablet is
[srt android](../android/docs.md).
