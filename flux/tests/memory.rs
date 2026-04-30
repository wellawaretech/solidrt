#![cfg(feature = "compile")]

use qjsrt::{JsEngine, LogLevel};
use std::sync::{Arc, Mutex};

fn capture_log() -> (
    Arc<Mutex<Vec<(LogLevel, String)>>>,
    impl Fn(LogLevel, &str) + Send + Sync + 'static,
) {
    let log = Arc::new(Mutex::new(Vec::<(LogLevel, String)>::new()));
    let log2 = log.clone();
    let f = move |level: LogLevel, msg: &str| {
        log2.lock().unwrap().push((level, msg.to_string()));
    };
    (log, f)
}

fn log_output(log: &[(LogLevel, String)]) -> String {
    log.iter()
        .filter(|(l, _)| *l == LogLevel::Log)
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn error_output(log: &[(LogLevel, String)]) -> String {
    log.iter()
        .filter(|(l, _)| *l == LogLevel::Error)
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn import_alloc() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc } from "qjs:memory";
            let buf = alloc(16);
            console.log(buf.byteLength);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    assert_eq!(log_output(&log), "16");
}

#[tokio::test]
async fn import_memset() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc, memset } from "qjs:memory";
            let buf = alloc(4);
            memset(buf, 0, 4, 0xAB);
            console.log(buf[0], buf[1], buf[2], buf[3]);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    assert_eq!(log_output(&log), "171 171 171 171");
}

#[tokio::test]
async fn import_memset32() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc, memset32 } from "qjs:memory";
            let buf = alloc(8);
            memset32(buf, 0, 2, 0x01020304);
            console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    // 0x01020304 in little-endian bytes: 4, 3, 2, 1
    assert_eq!(log_output(&log), "4 3 2 1 4 3 2 1");
}

#[tokio::test]
async fn memset_offset() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc, memset } from "qjs:memory";
            let buf = alloc(8);
            memset(buf, 2, 3, 0xFF);
            console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    assert_eq!(log_output(&log), "0 0 255 255 255 0 0 0");
}

#[tokio::test]
async fn import_free() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc, free, memset } from "qjs:memory";
            let buf = alloc(4);
            memset(buf, 0, 4, 0x11);
            console.log(buf.byteLength);
            free(buf);
            console.log(buf.byteLength);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    assert_eq!(log_output(&log), "4\n0");
}

#[tokio::test]
async fn memset_out_of_bounds() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            import { alloc, memset } from "qjs:memory";
            let buf = alloc(4);
            try {
                memset(buf, 2, 4, 0xFF);
                console.log("no error");
            } catch (e) {
                console.log(String(e));
            }
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(error_output(&log).is_empty(), "stderr: {}", error_output(&log));
    assert_eq!(log_output(&log), "memset: offset + length out of bounds");
}
