use crate::sqlite::SqliteConnection;

async fn open_with(schema: &str) -> SqliteConnection {
  let conn = SqliteConnection::open(":memory:".to_string(), Some("rw+".to_string()))
    .await
    .expect("open in-memory database");
  conn.exec(schema.to_string()).await.expect("apply schema");
  conn
}

#[tokio::test]
async fn read_set_reports_tables() {
  let conn = open_with("CREATE TABLE a (x); CREATE TABLE b (y);").await;
  let tables = conn
    .read_set("SELECT x, (SELECT y FROM b) FROM a".to_string())
    .await
    .expect("read set");
  assert_eq!(tables, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn read_set_sees_through_views() {
  let conn = open_with("CREATE TABLE a (x); CREATE VIEW v AS SELECT x FROM a;").await;
  let tables = conn.read_set("SELECT * FROM v".to_string()).await.expect("read set");
  // The base table must be present; the view name may or may not be, and
  // either is fine for invalidation (writes only ever name base tables).
  assert!(tables.contains(&"a".to_string()), "expected base table in {tables:?}");
}

#[tokio::test]
async fn read_set_rejects_invalid_sql() {
  let conn = open_with("CREATE TABLE a (x);").await;
  assert!(conn.read_set("SELECT nope FROM missing".to_string()).await.is_err());
}

#[tokio::test]
async fn writes_are_reported_per_command() {
  let conn = open_with("CREATE TABLE t (x); CREATE TABLE u (y);").await;
  let mut rx = conn.subscribe_writes().expect("subscribe");

  conn.run("INSERT INTO t VALUES (1)".to_string(), vec![], false).await.expect("insert");
  assert_eq!(rx.recv().await.expect("event"), vec!["t".to_string()]);

  // A transaction coalesces to one event, tables sorted.
  conn
    .transaction(vec![
      ("INSERT INTO u VALUES (1)".to_string(), vec![]),
      ("INSERT INTO t VALUES (2)".to_string(), vec![]),
      ("INSERT INTO t VALUES (3)".to_string(), vec![]),
    ])
    .await
    .expect("transaction");
  assert_eq!(rx.recv().await.expect("event"), vec!["t".to_string(), "u".to_string()]);

  // A read produces no event: the next event is the next write's.
  conn.query("SELECT * FROM t".to_string(), vec![], false).await.expect("select");
  conn.run("DELETE FROM u".to_string(), vec![], false).await.expect("delete");
  assert_eq!(rx.recv().await.expect("event"), vec!["u".to_string()]);
}

#[tokio::test]
async fn trigger_writes_are_reported() {
  let conn = open_with(
    "CREATE TABLE t (x); CREATE TABLE log (m); \
     CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES ('hit'); END;",
  )
  .await;
  let mut rx = conn.subscribe_writes().expect("subscribe");
  conn.run("INSERT INTO t VALUES (1)".to_string(), vec![], false).await.expect("insert");
  assert_eq!(rx.recv().await.expect("event"), vec!["log".to_string(), "t".to_string()]);
}
