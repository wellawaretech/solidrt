use rquickjs::function::{MutFn, Opt};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Function, IntoJs, Object, TypedArray, Value};
use std::io;
use std::process::Stdio;
use std::rc::Rc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;

use crate::pending::PendingOps;

// flux:subprocess - spawn child processes and collect their output.
//
//   import { command } from "flux:subprocess"
//   let c = command("nmap", ["-sn", "192.168.1.0/24"])
//   let { code, success, stdout, stderr } = await c.output()
//
// The shape mirrors flux:fs: a lowercase `command(cmd, args?, opts?)` factory
// returns a reusable reference object, and async work hangs off it as methods.
// `output()` runs the child to completion and buffers stdout/stderr. A streaming
// `spawn()` (live stdout/stderr, stdin writes, kill) is a planned later stage.
//
// Arguments are always passed as an array and never through a shell, so there is
// no per-platform shell or quoting to reason about (and no shell injection). The
// JS is identical on every OS; whether the target binary (e.g. nmap) exists is
// the caller's concern, and a missing binary rejects with "command not found".
//
// opts: { cwd, env, stdin, timeoutMs, encoding }
//   cwd       working directory for the child
//   env       object of extra env vars, added to / overriding the inherited env
//   stdin     string | Uint8Array written to the child's stdin, then closed
//   timeoutMs kill the child if it has not exited within this many ms
//   encoding  "buffer" -> stdout/stderr as Uint8Array; default utf8 strings

// A parsed, reusable command spec. Shared (Rc) into each output() call so the
// same reference can be run more than once, like a re-readable file().
struct CommandSpec {
  cmd: String,
  args: Vec<String>,
  cwd: Option<String>,
  env: Vec<(String, String)>,
  stdin: Option<Vec<u8>>,
  timeout_ms: Option<u64>,
  as_bytes: bool,
}

// The buffered result of a finished child.
struct CommandOutput {
  code: Option<i32>,
  signal: Option<String>,
  stdout: Vec<u8>,
  stderr: Vec<u8>,
  as_bytes: bool,
}

impl<'js> IntoJs<'js> for CommandOutput {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    match self.code {
      Some(c) => obj.set("code", c)?,
      None => obj.set("code", Value::new_null(ctx.clone()))?,
    }
    match self.signal {
      Some(s) => obj.set("signal", s)?,
      None => obj.set("signal", Value::new_null(ctx.clone()))?,
    }
    obj.set("success", self.code == Some(0))?;
    obj.set("stdout", bytes_to_js(ctx, self.stdout, self.as_bytes)?)?;
    obj.set("stderr", bytes_to_js(ctx, self.stderr, self.as_bytes)?)?;
    Ok(obj.into_value())
  }
}

fn bytes_to_js<'js>(ctx: &Ctx<'js>, bytes: Vec<u8>, as_bytes: bool) -> rquickjs::Result<Value<'js>> {
  if as_bytes {
    Ok(TypedArray::new(ctx.clone(), bytes)?.into_value())
  } else {
    String::from_utf8_lossy(&bytes).into_owned().into_js(ctx)
  }
}

fn parse_spec(cmd: String, args: Option<Vec<String>>, opts: Option<Object<'_>>) -> rquickjs::Result<CommandSpec> {
  let mut spec = CommandSpec {
    cmd,
    args: args.unwrap_or_default(),
    cwd: None,
    env: Vec::new(),
    stdin: None,
    timeout_ms: None,
    as_bytes: false,
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
      spec.stdin = Some(value_to_bytes(&stdin)?);
    }
    if let Some(ms) = opts.get::<_, Option<f64>>("timeoutMs")? {
      spec.timeout_ms = Some(ms.max(0.0) as u64);
    }
    if let Some(enc) = opts.get::<_, Option<String>>("encoding")? {
      spec.as_bytes = enc == "buffer";
    }
  }
  Ok(spec)
}

fn value_to_bytes(value: &Value<'_>) -> rquickjs::Result<Vec<u8>> {
  if let Some(s) = value.as_string() {
    Ok(s.to_string()?.into_bytes())
  } else if let Ok(ta) = TypedArray::<u8>::from_value(value.clone()) {
    Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
  } else {
    Err(rquickjs::Error::Io(io::Error::new(
      io::ErrorKind::InvalidInput,
      "stdin must be a string or Uint8Array",
    )))
  }
}

// Runs the child to completion, holding the engine alive (PendingOps) while it
// runs. kill_on_drop ensures a timed-out child is reaped: on timeout the future
// owning the child is dropped, which kills it.
async fn run_output(spec: Rc<CommandSpec>, pending: PendingOps) -> rquickjs::Result<CommandOutput> {
  pending.hold();
  let result = run_output_inner(&spec).await;
  pending.release();
  result
}

async fn run_output_inner(spec: &CommandSpec) -> rquickjs::Result<CommandOutput> {
  let mut command = TokioCommand::new(&spec.cmd);
  command.args(&spec.args);
  command.kill_on_drop(true);
  command.stdin(if spec.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
  command.stdout(Stdio::piped());
  command.stderr(Stdio::piped());
  if let Some(cwd) = &spec.cwd {
    command.current_dir(cwd);
  }
  for (k, v) in &spec.env {
    command.env(k, v);
  }

  let mut child = command.spawn().map_err(|e| spawn_err(&spec.cmd, e))?;

  if let Some(bytes) = &spec.stdin {
    if let Some(mut si) = child.stdin.take() {
      si.write_all(bytes).await.map_err(rquickjs::Error::Io)?;
      // Dropping si closes stdin so the child sees EOF.
    }
  }

  let output = match spec.timeout_ms {
    Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), child.wait_with_output()).await {
      Ok(r) => r.map_err(rquickjs::Error::Io)?,
      Err(_) => {
        return Err(rquickjs::Error::Io(io::Error::new(
          io::ErrorKind::TimedOut,
          format!("command timed out after {ms}ms: {}", spec.cmd),
        )))
      }
    },
    None => child.wait_with_output().await.map_err(rquickjs::Error::Io)?,
  };

  Ok(CommandOutput {
    code: output.status.code(),
    signal: exit_signal(&output.status),
    stdout: output.stdout,
    stderr: output.stderr,
    as_bytes: spec.as_bytes,
  })
}

fn spawn_err(cmd: &str, e: io::Error) -> rquickjs::Error {
  let msg = if e.kind() == io::ErrorKind::NotFound {
    format!("command not found: {cmd}")
  } else {
    format!("failed to spawn {cmd}: {e}")
  };
  rquickjs::Error::Io(io::Error::new(e.kind(), msg))
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<String> {
  use std::os::unix::process::ExitStatusExt;
  status.signal().map(signal_name)
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<String> {
  None
}

#[cfg(unix)]
fn signal_name(num: i32) -> String {
  match num {
    1 => "SIGHUP".to_string(),
    2 => "SIGINT".to_string(),
    3 => "SIGQUIT".to_string(),
    9 => "SIGKILL".to_string(),
    11 => "SIGSEGV".to_string(),
    13 => "SIGPIPE".to_string(),
    15 => "SIGTERM".to_string(),
    other => format!("SIG{other}"),
  }
}

fn build_command<'js>(
  ctx: Ctx<'js>,
  cmd: String,
  args: Opt<Vec<String>>,
  opts: Opt<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let spec = Rc::new(parse_spec(cmd, args.0, opts.0)?);

  let obj = Object::new(ctx.clone())?;
  obj.set("cmd", spec.cmd.clone())?;
  obj.set("args", spec.args.clone())?;

  let output_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let spec = spec.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
        let spec = spec.clone();
        Ok(Promised(run_output(spec, pending)))
      }
    }),
  )
  .expect("create output function");
  obj.set("output", output_fn)?;

  Ok(obj)
}

pub struct SubprocessModule;

impl ModuleDef for SubprocessModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("command")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let command_fn = Function::new(ctx.clone(), build_command).expect("create command function");
    exports.export("command", command_fn)?;
    Ok(())
  }
}