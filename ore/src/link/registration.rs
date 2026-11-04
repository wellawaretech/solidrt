//! Making this executable the handler for the app's own scheme, for this
//! user, on an explicit call (the web's `registerProtocolHandler`), never
//! an implicit first-run write: a packed app is a single executable
//! nothing installs, so the app itself makes this copy the handler, and
//! does it again after it moved. Linux writes the freedesktop pieces (a
//! .desktop file and the mimeapps.list default), Windows the
//! HKCU\Software\Classes protocol key. Android's package and macOS's
//! bundle declare the scheme, so there is nothing to write at runtime.

use std::path::Path;

/// Make this executable the handler for `scheme` on this machine, for this
/// user. `display_name` is what the OS shows for the handler.
pub fn register(scheme: &str, display_name: &str) -> Result<(), String> {
  let exe = std::env::current_exe().map_err(|e| format!("cannot resolve the executable path: {e}"))?;
  platform::register(scheme, display_name, &exe)
}

#[cfg(target_os = "macos")]
mod platform {
  use super::*;

  pub(super) fn register(_scheme: &str, _display_name: &str, _exe: &Path) -> Result<(), String> {
    Err("on macOS the app bundle declares its scheme (CFBundleURLTypes); nothing is registered at runtime".to_string())
  }
}

#[cfg(target_os = "android")]
mod platform {
  use super::*;

  // The package declares the scheme (its manifest's intent filter).
  pub(super) fn register(_scheme: &str, _display_name: &str, _exe: &Path) -> Result<(), String> {
    Ok(())
  }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
mod platform {
  use super::*;

  pub(super) fn register(scheme: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    freedesktop::register(scheme, display_name, exe)
  }
}

// The freedesktop way (Linux and the BSDs): a desktop entry for this
// executable declaring the scheme's MIME type, and the user's mimeapps.list
// naming it the default handler, exactly what `xdg-mime default` writes.
// Both written directly rather than through xdg-utils, so registration
// does not depend on tools on PATH and behaves the same on every desktop.
// GIO and xdg-open read mimeapps.list for the default and find the entry by
// its id under the applications dirs; no cache rebuild is involved.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
pub(crate) mod freedesktop {
  use super::*;
  use std::path::PathBuf;

  const DEFAULTS_SECTION: &str = "[Default Applications]";
  const ADDED_SECTION: &str = "[Added Associations]";

  pub(super) fn register(scheme: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let desktop_id = format!("{scheme}.desktop");

    let applications = data_home.join("applications");
    std::fs::create_dir_all(&applications).map_err(|e| format!("cannot create {}: {e}", applications.display()))?;
    write_atomically(&applications.join(&desktop_id), desktop_entry(scheme, display_name, exe).as_bytes())?;

    std::fs::create_dir_all(&config_home).map_err(|e| format!("cannot create {}: {e}", config_home.display()))?;
    let mimeapps = config_home.join("mimeapps.list");
    let existing = match std::fs::read_to_string(&mimeapps) {
      Ok(text) => text,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
      Err(e) => return Err(format!("cannot read {}: {e}", mimeapps.display())),
    };
    let updated = with_default(&existing, &scheme_mime(scheme), &desktop_id);
    write_atomically(&mimeapps, updated.as_bytes())
  }

  pub(crate) fn scheme_mime(scheme: &str) -> String {
    format!("x-scheme-handler/{scheme}")
  }

  /// The desktop entry: `%u` receives the link, so the executable gets it as
  /// its first argument (main.rs recognizes the app's own scheme there).
  pub(crate) fn desktop_entry(scheme: &str, display_name: &str, exe: &Path) -> String {
    let name = display_name.replace(['\n', '\r'], " ");
    format!(
      "[Desktop Entry]\nType=Application\nName={name}\nExec={} %u\nTerminal=false\nMimeType={};\n",
      exec_quote(&exe.to_string_lossy()),
      scheme_mime(scheme)
    )
  }

  // The characters the Desktop Entry specification reserves in an Exec
  // argument: an argument holding any of them must be quoted.
  const EXEC_RESERVED: &[char] =
    &[' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')', '`', '%'];

  /// One Exec argument per the Desktop Entry specification: as is when it
  /// holds no reserved character, since some launchers (xdg-open's generic
  /// path among them) take an unquoted first word literally; otherwise
  /// double-quoted, with the four characters the quoting reserves
  /// backslash-escaped, and `%` doubled since it introduces field codes.
  pub(crate) fn exec_quote(arg: &str) -> String {
    if !arg.contains(EXEC_RESERVED) {
      return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    for c in arg.chars() {
      match c {
        '"' | '`' | '$' | '\\' => {
          out.push('\\');
          out.push(c);
        }
        '%' => out.push_str("%%"),
        _ => out.push(c),
      }
    }
    out.push('"');
    out
  }

  /// mimeapps.list with `desktop_id` as the default for `mime` and listed
  /// among its added associations, the two entries `xdg-mime default`
  /// writes. Every other line is kept as it was.
  pub(crate) fn with_default(existing: &str, mime: &str, desktop_id: &str) -> String {
    let mut text = set_key(existing, DEFAULTS_SECTION, mime, desktop_id, false);
    text = set_key(&text, ADDED_SECTION, mime, desktop_id, true);
    text
  }

  // Set `key` in `section` to `value`; `accumulate` keeps an existing
  // semicolon list and adds `value` to it when missing (the associations
  // form), else the value replaces. A missing section is appended.
  fn set_key(existing: &str, section: &str, key: &str, value: &str, accumulate: bool) -> String {
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let start = match lines.iter().position(|l| l.trim() == section) {
      Some(i) => i,
      None => {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
          lines.push(String::new());
        }
        lines.push(section.to_string());
        lines.len() - 1
      }
    };
    let end =
      lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |n| start + 1 + n);
    let prefix = format!("{key}=");
    let new_line = |current: Option<&str>| {
      if accumulate {
        let mut ids: Vec<&str> = current.map(|c| c.split(';').filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        if !ids.contains(&value) {
          ids.push(value);
        }
        format!("{prefix}{};", ids.join(";"))
      } else {
        format!("{prefix}{value}")
      }
    };
    match lines[start + 1..end].iter().position(|l| l.starts_with(&prefix)) {
      Some(n) => {
        let i = start + 1 + n;
        lines[i] = new_line(Some(&lines[i][prefix.len()..]));
      }
      None => lines.insert(end, new_line(None)),
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
  }

  // A config file is replaced whole or not at all: written next to its
  // place and renamed over it.
  fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
  }
}

#[cfg(windows)]
mod platform {
  use super::*;

  // The per-user protocol key the shell consults for `<scheme>:` links:
  // HKCU\Software\Classes\<scheme> marked as a URL protocol, its open
  // command this executable with the link as the one argument.
  pub(super) fn register(scheme: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    use windows_registry::CURRENT_USER;
    let exe = exe.to_string_lossy();
    let key = CURRENT_USER
      .create(format!(r"Software\Classes\{scheme}"))
      .map_err(|e| format!("cannot create the protocol key: {e}"))?;
    key.set_string("", format!("URL:{display_name}")).map_err(|e| format!("cannot name the protocol: {e}"))?;
    key.set_string("URL Protocol", "").map_err(|e| format!("cannot mark the protocol: {e}"))?;
    key
      .create("DefaultIcon")
      .and_then(|k| k.set_string("", format!("\"{exe}\",0")))
      .map_err(|e| format!("cannot set the icon: {e}"))?;
    key
      .create(r"shell\open\command")
      .and_then(|k| k.set_string("", format!("\"{exe}\" \"%1\"")))
      .map_err(|e| format!("cannot set the open command: {e}"))
  }
}
