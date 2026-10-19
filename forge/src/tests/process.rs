use crate::process::crash::{describe, Access, Line};
use crate::process::{alive, exec_path, pid};

fn crash_line(status: u32, module: Option<(&str, usize)>, access: Option<(Access, usize)>) -> String {
  let mut line = Line::new();
  describe(&mut line, status, module.map(|(name, offset)| (name.chars(), offset)), access);
  String::from_utf8(line.as_bytes().to_vec()).expect("the line is ASCII")
}

#[test]
fn crash_line_names_the_fault_its_address_and_module() {
  let line = crash_line(0xC0000005, Some(("libGLESv2.dll", 0x2e2f16)), Some((Access::Read, 0x20)));
  assert_eq!(line, "[sol] Crashed: access violation (0xC0000005) reading 0x20 in libGLESv2.dll+0x2E2F16\n");
}

#[test]
fn crash_line_reads_an_unnamed_status_as_hex_with_what_it_knows() {
  assert_eq!(crash_line(0xC0000420, None, None), "[sol] Crashed: exception 0xC0000420\n");
  assert_eq!(
    crash_line(0xC0000374, Some(("solidrt-go.exe", 0x10)), None),
    "[sol] Crashed: heap corruption (0xC0000374) in solidrt-go.exe+0x10\n"
  );
}

#[test]
fn crash_line_cuts_at_its_buffer_instead_of_overrunning() {
  let long = "x".repeat(2000);
  let line = crash_line(0xC0000005, Some((&long, 1)), None);
  assert_eq!(line.len(), 512, "the line fills its buffer and stops");
  assert!(line.starts_with("[sol] Crashed: access violation (0xC0000005) in xxx"));
}

#[test]
fn alive_reports_own_process_and_not_a_free_pid() {
  assert!(alive(pid()), "the current process is alive");
  // A pid nobody is likely to hold: the highest pids are far past any test
  // runner, and a false positive would need that exact process to exist.
  assert!(!alive(u32::MAX - 1), "a free pid is not alive");
}

#[test]
fn exec_path_names_an_existing_file() {
  let path = exec_path().expect("the test runner is a named executable");
  assert!(std::path::Path::new(&path).is_file(), "exec_path points at a file: {path}");
}
