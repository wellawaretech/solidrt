//! Android's content providers: the documents a file dialog picks are
//! `content://` URIs that only the activity's ContentResolver can open, so
//! every operation is a JNI call into a static helper of `SolidRTActivity`
//! (`contentOpen`, `contentStat`, `contentDisplayName`, `contentChildren`,
//! `contentDelete`), on whatever thread it is made from (SDL attaches it to
//! the JVM). A document opens as a descriptor the resolver hands out, which
//! becomes a plain `File`; a folder (a tree URI) lists its children through
//! DocumentsContract, each child as the tree-scoped document URI that opens
//! it. Plain platform facts and files: the embedder adapts them to its file
//! system layer's provider seam.

use std::os::fd::FromRawFd;

use crate::sdl_utils::{java_strings, with_android_activity};

/// How a document is opened: for reading, for writing from scratch
/// (truncated), or for appending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentOpen {
  Read,
  Write,
  Append,
}

/// A document's metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentStat {
  pub size: u64,
  /// Milliseconds since the Unix epoch; None when the provider has none.
  pub mtime_ms: Option<i64>,
  pub directory: bool,
}

/// One child of a folder: the URI that opens it, its display name, and
/// whether it is a folder itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentEntry {
  pub uri: String,
  pub name: String,
  pub directory: bool,
}

pub struct AndroidContent;

// ContentResolver open modes: read, write truncating ("wt": a bare "w"
// does not truncate on every provider since Android 10, and a shorter
// document would keep the old tail), append.
fn mode_str(mode: ContentOpen) -> &'static str {
  match mode {
    ContentOpen::Read => "r",
    ContentOpen::Write => "wt",
    ContentOpen::Append => "wa",
  }
}

fn java_string(env: &mut jni::Env<'_>, value: jni::objects::JObject<'_>) -> Result<String, jni::errors::Error> {
  if value.is_null() {
    return Ok(String::new());
  }
  let string = env.cast_local::<jni::objects::JString>(value)?;
  let chars = string.mutf8_chars(env)?;
  let owned = chars.to_str().into_owned();
  Ok(owned)
}

impl AndroidContent {
  pub fn open(&self, uri: &str, mode: ContentOpen) -> Result<std::fs::File, String> {
    let fd = with_android_activity("content open", |env, activity| {
      let class = env.get_object_class(activity)?;
      let juri = env.new_string(uri)?;
      let jmode = env.new_string(mode_str(mode))?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("contentOpen"),
          jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)I"),
          &[jni::objects::JValue::Object(&juri), jni::objects::JValue::Object(&jmode)],
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

  pub fn stat(&self, uri: &str) -> Result<ContentStat, String> {
    // "size\tmtime\tkind", kind "file" or "directory"; "" for no such
    // document.
    let record = with_android_activity("content stat", |env, activity| {
      let class = env.get_object_class(activity)?;
      let juri = env.new_string(uri)?;
      let value = env
        .call_static_method(
          &class,
          jni::jni_str!("contentStat"),
          jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
          &[jni::objects::JValue::Object(&juri)],
        )?
        .l()?;
      java_string(env, value)
    })?;
    let mut fields = record.split('\t');
    let (Some(size), Some(mtime), Some(kind)) = (fields.next(), fields.next(), fields.next()) else {
      return Err(format!("stat {uri}: no such document"));
    };
    Ok(ContentStat {
      size: size.parse::<u64>().unwrap_or(0),
      mtime_ms: mtime.parse::<i64>().ok().filter(|m| *m > 0),
      directory: kind == "directory",
    })
  }

  pub fn display_name(&self, uri: &str) -> Result<String, String> {
    let name = with_android_activity("content display name", |env, activity| {
      let class = env.get_object_class(activity)?;
      let juri = env.new_string(uri)?;
      let value = env
        .call_static_method(
          &class,
          jni::jni_str!("contentDisplayName"),
          jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
          &[jni::objects::JValue::Object(&juri)],
        )?
        .l()?;
      java_string(env, value)
    })?;
    if name.is_empty() {
      return Err(format!("{uri}: no display name"));
    }
    Ok(name)
  }

  pub fn list(&self, uri: &str) -> Result<Vec<ContentEntry>, String> {
    // Flat triples: child URI, display name, kind ("file" or "directory").
    let triples = with_android_activity("content children", |env, activity| {
      let class = env.get_object_class(activity)?;
      let juri = env.new_string(uri)?;
      let value = env
        .call_static_method(
          &class,
          jni::jni_str!("contentChildren"),
          jni::jni_sig!("(Ljava/lang/String;)[Ljava/lang/String;"),
          &[jni::objects::JValue::Object(&juri)],
        )?
        .l()?;
      if value.is_null() {
        return Ok(None);
      }
      let array = env.cast_local::<jni::objects::JObjectArray<jni::objects::JString>>(value)?;
      java_strings(env, &array).map(Some)
    })?
    .ok_or_else(|| format!("read dir {uri}: not a folder, or no access to it"))?;
    Ok(
      triples
        .chunks_exact(3)
        .map(|t| ContentEntry { uri: t[0].clone(), name: t[1].clone(), directory: t[2] == "directory" })
        .collect(),
    )
  }

  pub fn remove(&self, uri: &str) -> Result<(), String> {
    let removed = with_android_activity("content delete", |env, activity| {
      let class = env.get_object_class(activity)?;
      let juri = env.new_string(uri)?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("contentDelete"),
          jni::jni_sig!("(Ljava/lang/String;)Z"),
          &[jni::objects::JValue::Object(&juri)],
        )?
        .z()
    })?;
    if removed {
      Ok(())
    } else {
      Err(format!("remove {uri}: the provider refused"))
    }
  }
}
