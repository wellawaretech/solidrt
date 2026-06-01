use rquickjs::{function::MutFn, promise::Promised, Array, Ctx, Function, IntoJs, Object, Value};
use std::rc::Rc;

use crate::pending::PendingOps;

struct DirEntries(Vec<(String, &'static str)>);

impl<'js> IntoJs<'js> for DirEntries {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (i, (name, kind)) in self.0.into_iter().enumerate() {
      let entry = Object::new(ctx.clone())?;
      entry.set("name", name)?;
      entry.set("type", kind)?;
      arr.set(i, entry)?;
    }
    Ok(arr.into_value())
  }
}

fn entry_type(ft: std::fs::FileType) -> &'static str {
  if ft.is_file() {
    "file"
  } else if ft.is_dir() {
    "directory"
  } else if ft.is_symlink() {
    "symlink"
  } else {
    "other"
  }
}

async fn read_entries(path: &str) -> rquickjs::Result<Vec<(String, &'static str)>> {
  let mut entries = tokio::fs::read_dir(path).await.map_err(rquickjs::Error::Io)?;
  let mut out = Vec::new();
  while let Some(entry) = entries.next_entry().await.map_err(rquickjs::Error::Io)? {
    let name = entry.file_name().to_string_lossy().into_owned();
    let ft = entry.file_type().await.map_err(rquickjs::Error::Io)?;
    out.push((name, entry_type(ft)));
  }
  Ok(out)
}

fn build_dir<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Object<'js>> {
  let path = Rc::new(path);
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path.as_ref().clone())?;

  let entries_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
        let path = path.clone();
        Ok(Promised(async move {
          pending.hold();
          let r = read_entries(&path).await;
          pending.release();
          r.map(DirEntries)
        }))
      }
    }),
  )
  .expect("create entries function");
  obj.set("entries", entries_fn)?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
        let path = path.clone();
        Ok(Promised(async move {
          pending.hold();
          let exists = tokio::fs::metadata(&**path).await.map(|m| m.is_dir()).unwrap_or(false);
          pending.release();
          Ok::<bool, rquickjs::Error>(exists)
        }))
      }
    }),
  )
  .expect("create exists function");
  obj.set("exists", exists_fn)?;

  Ok(obj)
}

pub(crate) fn init_dir<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let dir_fn = Function::new(ctx.clone(), build_dir).expect("create Flux.dir function");
  flux.set("dir", dir_fn).expect("set Flux.dir");
}
