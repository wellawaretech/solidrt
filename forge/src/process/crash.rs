//! The Windows crash line (`process::report_crashes`): a last-chance
//! exception filter that names what killed the process before Windows ends
//! it. A crashed child's status is the only trace its spawner gets, and the
//! usual spawner (bun) keeps one byte of it, so an access violation read as
//! "exit 5" with nothing in the log: a bisection for what one line names.
//! Unix needs none of this: the signal reaches every spawner by name.
//!
//! The filter runs on the faulting thread with the process in an unknown
//! state: it allocates nothing and takes no lock (the heap or the logger may
//! be what broke), formats into a stack buffer, writes that to the raw
//! stderr handle, and returns "continue search", so the exit status and
//! Windows Error Reporting are what they would have been. A stack overflow
//! is left alone: Rust's own handler reports it, and the faulting thread's
//! stack is gone. A fast-fail abort (0xC0000409) never reaches any filter.
//!
//! The wording lives outside the `cfg(windows)` half so a test reads it on
//! every host.

use std::fmt::{self, Write};

/// The crash statuses worth a name; any other reads as "exception" plus
/// its hex.
const NAMES: [(u32, &str); 11] = [
  (0xC0000005, "access violation"),
  (0xC0000006, "in-page error"),
  (0xC0000008, "invalid handle"),
  (0xC000001D, "illegal instruction"),
  (0xC0000094, "integer divide by zero"),
  (0xC0000096, "privileged instruction"),
  (0xC0000135, "DLL not found"),
  (0xC0000139, "entry point not found"),
  (0xC0000142, "DLL initialization failed"),
  (0xC0000374, "heap corruption"),
  (0xC0000409, "fast-fail abort"),
];

/// The status an access violation reports, the one fault with an address
/// to name.
const ACCESS_VIOLATION: u32 = 0xC0000005;

/// Room for the line: the longest wording plus a module basename and three
/// numbers is well inside; a longer basename is cut, never overrun.
const LINE_CAPACITY: usize = 512;

/// A line formatted into a stack buffer: no allocation, a full buffer drops
/// the rest.
pub(crate) struct Line {
  buf: [u8; LINE_CAPACITY],
  len: usize,
}

impl Line {
  pub(crate) fn new() -> Self {
    Line { buf: [0; LINE_CAPACITY], len: 0 }
  }

  pub(crate) fn as_bytes(&self) -> &[u8] {
    &self.buf[..self.len]
  }
}

impl Write for Line {
  fn write_str(&mut self, s: &str) -> fmt::Result {
    let n = s.len().min(LINE_CAPACITY - self.len);
    self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
    self.len += n;
    Ok(())
  }
}

/// What an access violation was doing at its address: the first exception
/// parameter (0 read, 1 write, 8 execute under DEP).
pub(crate) enum Access {
  Read,
  Write,
  Execute,
}

impl Access {
  pub(crate) fn from_parameter(p: usize) -> Access {
    match p {
      1 => Access::Write,
      8 => Access::Execute,
      _ => Access::Read,
    }
  }
}

/// The line: `[sol] Crashed: access violation (0xC0000005) reading 0x20 in
/// libGLESv2.dll+0x2E2F16`. `module` is the faulting module's basename as
/// chars (decoded from the OS path, no allocation) and the fault's offset
/// in it; `access` is the address an access violation touched. Both absent
/// when the OS could not say.
pub(crate) fn describe(
  line: &mut Line,
  status: u32,
  module: Option<(impl Iterator<Item = char>, usize)>,
  access: Option<(Access, usize)>,
) {
  // A full buffer is the only way a write fails, and the line is cut there
  // by design.
  let _ = match NAMES.iter().find(|(code, _)| *code == status) {
    Some((_, name)) => write!(line, "[sol] Crashed: {name} (0x{status:08X})"),
    None => write!(line, "[sol] Crashed: exception 0x{status:08X}"),
  };
  if let Some((access, address)) = access {
    let what = match access {
      Access::Read => "reading",
      Access::Write => "writing",
      Access::Execute => "executing",
    };
    let _ = write!(line, " {what} 0x{address:X}");
  }
  if let Some((name, offset)) = module {
    let _ = line.write_str(" in ");
    for c in name {
      let _ = line.write_char(c);
    }
    let _ = write!(line, "+0x{offset:X}");
  }
  let _ = line.write_char('\n');
}

#[cfg(windows)]
pub(super) use win::install;

#[cfg(windows)]
mod win {
  use super::{describe, Access, Line, ACCESS_VIOLATION};
  use std::char::{decode_utf16, REPLACEMENT_CHARACTER};
  use std::ptr::null_mut;
  use windows_sys::Win32::Foundation::{HMODULE, STATUS_STACK_OVERFLOW};
  use windows_sys::Win32::Storage::FileSystem::WriteFile;
  use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE};
  use windows_sys::Win32::System::Diagnostics::Debug::{
    SetUnhandledExceptionFilter, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
  };
  use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
  };

  /// Room for the faulting module's path (UTF-16 units); a longer one is
  /// cut by the OS, and the basename with it, which only costs the name.
  const MODULE_PATH_CAPACITY: usize = 1024;

  pub(crate) fn install() {
    // SAFETY: registers a plain `extern "system"` function as the
    // process's last-chance filter; nothing of ours outlives the call.
    unsafe {
      SetUnhandledExceptionFilter(Some(filter));
    }
  }

  unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: the system hands a valid EXCEPTION_POINTERS whose record is
    // valid for the duration of the call.
    let record = unsafe { &*(*info).ExceptionRecord };
    let status = record.ExceptionCode as u32;
    if status == STATUS_STACK_OVERFLOW as u32 {
      return EXCEPTION_CONTINUE_SEARCH;
    }
    let address = record.ExceptionAddress as usize;
    let mut path = [0u16; MODULE_PATH_CAPACITY];
    let mut handle: HMODULE = null_mut();
    // SAFETY: the flags ask for the module containing `address` without
    // touching its reference count; the path buffer is ours and its
    // length is passed.
    let found = unsafe {
      GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        address as *const u16,
        &mut handle,
      ) != 0
    };
    let module = found.then(|| {
      // SAFETY: `handle` is a loaded module and the buffer is as long as
      // the length passed.
      let n = unsafe { GetModuleFileNameW(handle, path.as_mut_ptr(), MODULE_PATH_CAPACITY as u32) } as usize;
      let full = &path[..n];
      let start = full.iter().rposition(|&u| u == b'\\' as u16 || u == b'/' as u16).map_or(0, |i| i + 1);
      let name = decode_utf16(full[start..].iter().copied()).map(|c| c.unwrap_or(REPLACEMENT_CHARACTER));
      (name, address - handle as usize)
    });
    let access = (status == ACCESS_VIOLATION && record.NumberParameters >= 2)
      .then(|| (Access::from_parameter(record.ExceptionInformation[0]), record.ExceptionInformation[1]));
    let mut line = Line::new();
    describe(&mut line, status, module, access);
    let bytes = line.as_bytes();
    let mut written = 0u32;
    // SAFETY: a synchronous write of our own buffer to the process's
    // stderr handle; a failure (no stderr) is ignored, there is nothing
    // else to do with it.
    unsafe {
      WriteFile(GetStdHandle(STD_ERROR_HANDLE), bytes.as_ptr(), bytes.len() as u32, &mut written, null_mut());
    }
    EXCEPTION_CONTINUE_SEARCH
  }
}
