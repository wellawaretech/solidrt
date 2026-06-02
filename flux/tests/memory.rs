#![cfg(feature = "compile")]

mod common;

use common::run_source;

#[tokio::test]
async fn import_alloc() {
  let out = run_source(
    r#"
            import { alloc } from "flux:memory";
            let buf = alloc(16);
            console.log(buf.byteLength);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "16");
}

#[tokio::test]
async fn import_memset() {
  let out = run_source(
    r#"
            import { alloc, memset } from "flux:memory";
            let buf = alloc(4);
            memset(buf, 0, 4, 0xAB);
            console.log(buf[0], buf[1], buf[2], buf[3]);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "171 171 171 171");
}

#[tokio::test]
async fn import_memset32() {
  let out = run_source(
    r#"
            import { alloc, memset32 } from "flux:memory";
            let buf = alloc(8);
            memset32(buf, 0, 2, 0x01020304);
            console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  // 0x01020304 in little-endian bytes: 4, 3, 2, 1
  assert_eq!(out.log(), "4 3 2 1 4 3 2 1");
}

#[tokio::test]
async fn memset_offset() {
  let out = run_source(
    r#"
            import { alloc, memset } from "flux:memory";
            let buf = alloc(8);
            memset(buf, 2, 3, 0xFF);
            console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "0 0 255 255 255 0 0 0");
}

#[tokio::test]
async fn import_free() {
  let out = run_source(
    r#"
            import { alloc, free, memset } from "flux:memory";
            let buf = alloc(4);
            memset(buf, 0, 4, 0x11);
            console.log(buf.byteLength);
            free(buf);
            console.log(buf.byteLength);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "4\n0");
}

#[tokio::test]
async fn memset_out_of_bounds() {
  let out = run_source(
    r#"
            import { alloc, memset } from "flux:memory";
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
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "memset: offset + length out of bounds");
}