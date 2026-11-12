//! The platform's log sink: where a line of the process log goes, as SDL
//! routes it (logcat on Android). Which crates log at which level is the
//! caller's filter; this is only the write.

/// Write one line to the platform log, at SDL's application category and
/// info priority. The message is written as is (no format expansion).
pub fn write(message: &str) {
  sdl3::log::log(message);
}
