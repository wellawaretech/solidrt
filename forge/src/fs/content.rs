//! Android's content providers: the documents a file dialog picks are
//! `content://` URIs that only the app's ContentResolver can open, so every
//! operation is a JNI call into a static helper of the activity's class
//! (`contentOpen`, `contentStat`, `contentDisplayName`, `contentChildren`,
//! `contentDelete` on `SolidRTActivity`), on whatever thread it is made from
//! (the thread is attached to the VM for its lifetime). The JavaVM and the
//! activity are the process-wide ones the platform (alloy) publishes in
//! ndk-context at startup; a process without them has no content providers
//! to reach, and nothing hands it a content URI. A document opens as a
//! descriptor the resolver hands out, which becomes a plain `File`; a folder
//! (a tree URI) lists its children through DocumentsContract, each child as
//! the tree-scoped document URI that opens it.

use std::os::fd::FromRawFd;

use jni::objects::{JObject, JObjectArray, JString, JValue};

use super::{DirEntry, OpenMode, StatInfo};

// ContentResolver open modes: read, write truncating ("wt": a bare "w"
// does not truncate on every provider since Android 10, and a shorter
// document would keep the old tail), append.
fn mode_str(mode: OpenMode) -> &'static str {
  match mode {
    OpenMode::Read => "r",
    OpenMode::Write => "wt",
    OpenMode::Append => "wa",
  }
}

/// Run `f` with a JNI env on this thread and the activity, the thread
/// attached to the VM.
fn with_activity<T>(
  what: &str,
  f: impl FnOnce(&mut jni::Env<'_>, &JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
  let context = ndk_context::android_context();
  if context.vm().is_null() || context.context().is_null() {
    return Err(format!("no Android context for the {what}"));
  }
  let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
  vm.attach_current_thread(|env| {
    // The activity is a global reference the platform keeps for the process;
    // this wrapper does not own it.
    let activity = unsafe { JObject::from_raw(env, context.context().cast()) };
    f(env, &activity)
  })
  .map_err(|e: jni::errors::Error| format!("{what} JNI call failed: {e}"))
}

fn java_string(env: &mut jni::Env<'_>, value: JObject<'_>) -> Result<String, jni::errors::Error> {
  if value.is_null() {
    return Ok(String::new());
  }
  let string = env.cast_local::<JString>(value)?;
  let chars = string.mutf8_chars(env)?;
  let owned = chars.to_str().into_owned();
  Ok(owned)
}

/// The strings of a Java `String[]`, in order.
fn java_strings(
  env: &mut jni::Env<'_>,
  array: &JObjectArray<'_, JString<'_>>,
) -> Result<Vec<String>, jni::errors::Error> {
  let len = array.len(env)?;
  let mut out = Vec::with_capacity(len);
  for i in 0..len {
    let element = array.get_element(env, i)?;
    out.push(element.mutf8_chars(env)?.to_str().into_owned());
  }
  Ok(out)
}

pub(super) fn open(uri: &str, mode: OpenMode) -> Result<std::fs::File, String> {
  let fd = with_activity("content open", |env, activity| {
    let class = env.get_object_class(activity)?;
    let juri = env.new_string(uri)?;
    let jmode = env.new_string(mode_str(mode))?;
    env
      .call_static_method(
        &class,
        jni::jni_str!("contentOpen"),
        jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)I"),
        &[JValue::Object(&juri), JValue::Object(&jmode)],
      )?
      .i()
  })?;
  if fd < 0 {
    return Err(format!("open {uri}: no such document, or no access to it"));
  }
  // The descriptor was detached from its ParcelFileDescriptor on the Java
  // side: this File is its only owner.
  Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

pub(super) fn stat(uri: &str) -> Result<StatInfo, String> {
  // "size\tmtime\tkind", kind "file" or "directory"; "" for no such
  // document.
  let record = with_activity("content stat", |env, activity| {
    let class = env.get_object_class(activity)?;
    let juri = env.new_string(uri)?;
    let value = env
      .call_static_method(
        &class,
        jni::jni_str!("contentStat"),
        jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
        &[JValue::Object(&juri)],
      )?
      .l()?;
    java_string(env, value)
  })?;
  let mut fields = record.split('\t');
  let (Some(size), Some(mtime), Some(kind)) = (fields.next(), fields.next(), fields.next()) else {
    return Err(format!("stat {uri}: no such document"));
  };
  Ok(StatInfo {
    size: size.parse::<u64>().unwrap_or(0),
    file_type: if kind == "directory" { "directory" } else { "file" },
    mtime_ms: mtime.parse::<i64>().ok().filter(|m| *m > 0),
  })
}

pub(super) fn display_name(uri: &str) -> Result<String, String> {
  let name = with_activity("content display name", |env, activity| {
    let class = env.get_object_class(activity)?;
    let juri = env.new_string(uri)?;
    let value = env
      .call_static_method(
        &class,
        jni::jni_str!("contentDisplayName"),
        jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
        &[JValue::Object(&juri)],
      )?
      .l()?;
    java_string(env, value)
  })?;
  if name.is_empty() {
    return Err(format!("{uri}: no display name"));
  }
  Ok(name)
}

pub(super) fn list(uri: &str) -> Result<Vec<DirEntry>, String> {
  // Flat triples: child URI, display name, kind ("file" or "directory").
  let triples = with_activity("content children", |env, activity| {
    let class = env.get_object_class(activity)?;
    let juri = env.new_string(uri)?;
    let value = env
      .call_static_method(
        &class,
        jni::jni_str!("contentChildren"),
        jni::jni_sig!("(Ljava/lang/String;)[Ljava/lang/String;"),
        &[JValue::Object(&juri)],
      )?
      .l()?;
    if value.is_null() {
      return Ok(None);
    }
    let array = env.cast_local::<JObjectArray<JString>>(value)?;
    java_strings(env, &array).map(Some)
  })?
  .ok_or_else(|| format!("read dir {uri}: not a folder, or no access to it"))?;
  Ok(
    triples
      .chunks_exact(3)
      .map(|t| DirEntry {
        name: t[1].clone(),
        kind: if t[2] == "directory" { "directory" } else { "file" },
        path: t[0].clone(),
      })
      .collect(),
  )
}

pub(super) fn remove(uri: &str) -> Result<(), String> {
  let removed = with_activity("content delete", |env, activity| {
    let class = env.get_object_class(activity)?;
    let juri = env.new_string(uri)?;
    env
      .call_static_method(
        &class,
        jni::jni_str!("contentDelete"),
        jni::jni_sig!("(Ljava/lang/String;)Z"),
        &[JValue::Object(&juri)],
      )?
      .z()
  })?;
  if removed {
    Ok(())
  } else {
    Err(format!("remove {uri}: the provider refused"))
  }
}
