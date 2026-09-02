use rquickjs::function::MutFn;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, Function, Object, TypedArray, Value};
use std::rc::Rc;

use crate::pending::PendingOps;
use crate::plugins::marshal::{with_pending, OptArg};
use crate::standards_plugins::body::byte_stream_iterable;
use crate::plugins::value::Neutral;
use forge::subprocess::{self, CommandSpec, Spawned};

// flux:subprocess - spawn child processes and collect their output.
//
//   import { command } from "flux:subprocess"
//   let c = command("nmap", ["-sn", "192.168.1.0/24"])
//   let { code, success, stdout, stderr } = await c.output()
//
// Marshalling only: decode JS args into the engine-free `forge::subprocess`
// types, drive `CommandSpec`/`Child` methods, and encode results back to JS. The
// process machinery (buffered run, the spawn supervisor, stdin serialization)
// lives in `forge::subprocess`.
//
// The shape mirrors flux:fs: a lowercase `command(cmd, args?, opts?)` factory
// returns a reusable reference object, and async work hangs off it as methods.
// `output()` runs the child to completion and buffers stdout/stderr; `spawn()`
// returns a live child handle (stdout/stderr streams, stdin write/closeWrite, kill,
// status).
//
// Arguments are always passed as an array and never through a shell, so there is
// no per-platform shell or quoting to reason about (and no shell injection).
//
// opts: { cwd, env, stdin, timeoutMs, encoding, detached }
//   cwd       working directory for the child
//   env       object of extra env vars, added to / overriding the inherited env
//   stdin     string | Uint8Array written to the child's stdin, then closed
//   timeoutMs kill the child if it has not exited within this many ms
//   encoding  "buffer" -> stdout/stderr as Uint8Array; default utf8 strings
//   detached  spawn() only: the child outlives this engine and process (null
//             stdio, own process group, never killed on drop); its supervisor
//             runs on the process runtime so it is still reaped. stdin and
//             detached together is an error: there is no pipe to write.

fn parse_spec<'js>(
  ctx: &Ctx<'js>,
  cmd: String,
  args: Option<Vec<String>>,
  opts: Option<Object<'js>>,
) -> rquickjs::Result<CommandSpec> {
  let mut spec = CommandSpec {
    cmd,
    args: args.unwrap_or_default(),
    cwd: None,
    env: Vec::new(),
    stdin: None,
    timeout_ms: None,
    as_bytes: false,
    detached: false,
  };
  if let Some(opts) = opts {
    if let Some(cwd) = opts.get::<_, Option<String>>("cwd")? {
      spec.cwd = Some(cwd);
    }
    if let Some(env) = opts.get::<_, Option<Object>>("env")? {
      for entry in env.props::<String, String>() {
        let (k, v) = entry?;
        spec.env.push((k, v));
      }
    }
    if let Some(stdin) = opts.get::<_, Option<Value>>("stdin")? {
      spec.stdin = Some(value_to_bytes(ctx, &stdin)?);
    }
    if let Some(ms) = opts.get::<_, Option<f64>>("timeoutMs")? {
      spec.timeout_ms = Some(ms.max(0.0) as u64);
    }
    if let Some(enc) = opts.get::<_, Option<String>>("encoding")? {
      spec.as_bytes = enc == "buffer";
    }
    if let Some(detached) = opts.get::<_, Option<bool>>("detached")? {
      spec.detached = detached;
    }
  }
  if spec.detached && spec.stdin.is_some() {
    return Err(Exception::throw_message(ctx, "A detached child has no stdin pipe; drop the stdin option"));
  }
  Ok(spec)
}

fn value_to_bytes(ctx: &Ctx<'_>, value: &Value<'_>) -> rquickjs::Result<Vec<u8>> {
  if let Some(s) = value.as_string() {
    Ok(s.to_string()?.into_bytes())
  } else if let Ok(ta) = TypedArray::<u8>::from_value(value.clone()) {
    Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
  } else {
    Err(Exception::throw_message(ctx, "stdin must be a string or Uint8Array"))
  }
}

fn build_command<'js>(
  ctx: Ctx<'js>,
  cmd: String,
  args: OptArg<Vec<String>>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let spec = Rc::new(parse_spec(&ctx, cmd, args.0, opts.0)?);

  let obj = Object::new(ctx.clone())?;
  obj.set("cmd", spec.cmd.clone())?;
  obj.set("args", spec.args.clone())?;

  let output_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let spec = spec.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let spec = spec.clone();
        Ok(with_pending(&ctx, async move { spec.run_output().await.map(|o| Neutral(o.into())) }))
      }
    }),
  )
  .expect("create output function");
  obj.set("output", output_fn)?;

  let spawn_fn = Function::new(
    ctx.clone(),
    object_builder({
      let spec = spec.clone();
      move |ctx| build_child(ctx, &spec)
    }),
  )
  .expect("create spawn function");
  obj.set("spawn", spawn_fn)?;

  Ok(obj)
}

// Coerces a capturing closure to the `for<'js>` HRTB that rquickjs needs to
// return a `'js`-bound Object. A capturing closure does not infer this on its
// own (Object is invariant over `'js`); a plain fn item like build_file does.
fn object_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Object<'js>>,
{
  f
}

// Spawn the child (via the forge core) and build its JS handle: live
// stdout/stderr async-iterables, stdin write()/closeWrite(), kill(), and status().
// spawn() is synchronous (the process is launched here); a failure to launch
// throws a clean Error. The supervisor and initial-stdin tasks are spawned here
// because spawning is host-specific.
fn build_child<'js>(ctx: Ctx<'js>, spec: &Rc<CommandSpec>) -> rquickjs::Result<Object<'js>> {
  let Spawned { child, stdout, stderr, supervisor, initial_stdin } =
    spec.spawn().map_err(|m| Exception::throw_message(&ctx, &m))?;
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();

  // opts.stdin (if given) is written first; the pipe then stays open for further
  // writes. Held alive (PendingOps) until the write completes.
  if let Some(bytes) = initial_stdin {
    let child = child.clone();
    let pending = pending.clone();
    ctx.spawn(async move {
      pending.hold();
      let _ = child.write_stdin(bytes).await;
      pending.release();
    });
  }

  // The supervisor owns the child and waits for exit (or a kill request),
  // publishing the status. Holds a pending op for the child's lifetime so the
  // engine stays alive until it exits. A detached child is the opposite on
  // both counts: its supervisor runs on the process runtime, which outlives
  // this context (an engine rebuild drops every ctx.spawn task, and with it
  // a kill-on-drop child), and it holds nothing, so the engine may idle or
  // exit with the child still running.
  if spec.detached {
    tokio::spawn(supervisor.run());
  } else {
    let pending = pending.clone();
    ctx.spawn(async move {
      pending.hold();
      supervisor.run().await;
      pending.release();
    });
  }

  // A detached child has null stdio; forge hands back empty streams for it, so
  // they iterate to nothing.
  let obj = Object::new(ctx.clone())?;
  obj.set("pid", child.pid())?;
  obj.set("stdout", byte_stream_iterable(&ctx, stdout, pending.clone())?)?;
  obj.set("stderr", byte_stream_iterable(&ctx, stderr, pending.clone())?)?;

  // write(data) -> Promise: serialized behind the stdin lock.
  let write_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let child = child.clone();
      move |ctx: Ctx<'_>, data: Value<'_>| -> rquickjs::Result<Promised<_>> {
        let bytes = value_to_bytes(&ctx, &data)?;
        let child = child.clone();
        Ok(with_pending(&ctx, async move { child.write_stdin(bytes).await }))
      }
    }),
  )
  .expect("create write function");
  obj.set("write", write_fn)?;

  // closeWrite() -> Promise: close stdin so the child sees EOF, after any queued
  // writes have drained (it takes the same lock).
  let close_write_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let child = child.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let child = child.clone();
        Ok(with_pending(&ctx, async move {
          child.end_stdin().await;
          Ok::<(), String>(())
        }))
      }
    }),
  )
  .expect("create closeWrite function");
  obj.set("closeWrite", close_write_fn)?;

  // kill(): request termination (portable; SIGKILL / TerminateProcess).
  let kill_fn = Function::new(ctx.clone(), {
    let child = child.clone();
    move || {
      child.kill();
    }
  })
  .expect("create kill function");
  obj.set("kill", kill_fn)?;

  // status() -> Promise<{ code, signal, success }>, resolves when the child exits.
  let status_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let child = child.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let child = child.clone();
        Ok(with_pending(&ctx, async move { Ok::<Neutral, String>(Neutral(child.status().await.into())) }))
      }
    }),
  )
  .expect("create status function");
  obj.set("status", status_fn)?;

  Ok(obj)
}

pub struct SubprocessModule;

impl ModuleDef for SubprocessModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("command")?;
    decl.declare("which")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let command_fn = Function::new(ctx.clone(), build_command).expect("create command function");
    exports.export("command", command_fn)?;
    exports.export("which", Function::new(ctx.clone(), subprocess::which)?)?;
    Ok(())
  }
}
