#![cfg(feature = "compile")]

mod common;

use common::run_source;
use flux::LogLevel;

#[tokio::test]
async fn console_log_prints_to_stdout() {
  let out = run_source("console.log('hello')").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec!["hello"]);
  assert!(out.lines_at(LogLevel::Error).is_empty());
}

#[tokio::test]
async fn console_warn_prints_to_stderr() {
  let out = run_source("console.warn('warning')").await;
  assert!(out.lines_at(LogLevel::Log).is_empty());
  assert_eq!(out.lines_at(LogLevel::Warn), vec!["warning"]);
}

#[tokio::test]
async fn console_error_prints_to_stderr() {
  let out = run_source("console.error('oops')").await;
  assert!(out.lines_at(LogLevel::Log).is_empty());
  assert_eq!(out.lines_at(LogLevel::Error), vec!["oops"]);
}

#[tokio::test]
async fn console_debug_prints_at_debug_level() {
  let out = run_source("console.debug('details')").await;
  assert!(out.lines_at(LogLevel::Log).is_empty());
  assert_eq!(out.lines_at(LogLevel::Debug), vec!["details"]);
}

#[tokio::test]
async fn console_log_multiple_args() {
  let out = run_source("console.log('a', 'b', 'c')").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec!["a b c"]);
}

#[tokio::test]
async fn console_log_mixed_types() {
  let out = run_source("console.log('count:', 42, true, null)").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec!["count: 42 true null"]);
}

#[tokio::test]
async fn console_log_no_args() {
  let out = run_source("console.log()").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec![""]);
}

#[tokio::test]
async fn console_error_formats_error_objects() {
  let out = run_source("console.error(new Error('boom'))").await;
  let lines = out.lines_at(LogLevel::Error);
  assert_eq!(lines.len(), 1);
  assert!(lines[0].starts_with("Error: boom"), "unexpected format: {}", lines[0]);
  assert!(lines[0].contains('\n'), "stack missing: {}", lines[0]);
}

#[tokio::test]
async fn console_error_keeps_error_subclass_name() {
  let out = run_source("console.error(new TypeError('bad'))").await;
  let lines = out.lines_at(LogLevel::Error);
  assert_eq!(lines.len(), 1);
  assert!(lines[0].starts_with("TypeError: bad"), "unexpected format: {}", lines[0]);
}

#[tokio::test]
async fn console_log_object_is_json() {
  let out = run_source("console.log({ a: 1 })").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec![r#"{"a":1}"#]);
}

#[tokio::test]
async fn console_log_array_is_json() {
  let out = run_source("console.log([1, 2, 3])").await;
  assert_eq!(out.lines_at(LogLevel::Log), vec!["[1,2,3]"]);
}
