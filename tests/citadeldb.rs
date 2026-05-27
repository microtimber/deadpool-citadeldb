use deadpool_citadeldb::{Config, InteractError, Pool, Runtime};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

fn create_pool() -> Pool {
    let cfg = Config {
        path: PathBuf::new(),
        passphrase: b"test-passphrase".to_vec(),
        pool: None,
    };
    cfg.create_pool(Runtime::Tokio1).unwrap()
}

fn create_pool_with_path(path: &str) -> Pool {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.citadel-keys"));
    let cfg = Config {
        path: PathBuf::from(path),
        passphrase: b"test-passphrase".to_vec(),
        pool: None,
    };
    cfg.create_pool(Runtime::Tokio1).unwrap()
}

#[tokio::test]
async fn basic() {
    let pool = create_pool();
    let conn = pool.get().await.unwrap();
    let result: i64 = conn
        .interact(|inner| {
            inner
                .execute("CREATE TABLE IF NOT EXISTS _t_basic (id INTEGER PRIMARY KEY, name TEXT)")
                .expect("Failed to create table");
            inner
                .execute("INSERT INTO _t_basic (id, name) VALUES (1, 'Alice')")
                .expect("Failed to insert");
            let result = inner
                .query("SELECT id FROM _t_basic WHERE name = 'Alice'")
                .expect("Failed to query");
            let row = result.rows.first().expect("No rows returned");
            let val = row.first().expect("Empty row");
            match val {
                citadel_sql::Value::Integer(i) => *i,
                _ => panic!("Expected integer, got {:?}", val),
            }
        })
        .await
        .unwrap();
    assert_eq!(result, 1);
}

#[tokio::test]
async fn create_table_and_query() {
    let pool = create_pool();
    let conn = pool.get().await.unwrap();
    conn.interact(|inner| {
        inner.execute(
            "CREATE TABLE IF NOT EXISTS _t_test (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
        )
        .expect("Failed to create table");
        inner
            .execute("INSERT INTO _t_test (id, name, age) VALUES (1, 'Alice', 30)")
            .expect("Failed to insert Alice");
        inner
            .execute("INSERT INTO _t_test (id, name, age) VALUES (2, 'Bob', 25)")
            .expect("Failed to insert Bob");
        let result = inner
            .query("SELECT name, age FROM _t_test ORDER BY id")
            .expect("Failed to query");
        assert_eq!(result.columns, vec!["name", "age"]);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(
            result.rows[0],
            vec![
                citadel_sql::Value::Text("Alice".into()),
                citadel_sql::Value::Integer(30)
            ]
        );
        assert_eq!(
            result.rows[1],
            vec![
                citadel_sql::Value::Text("Bob".into()),
                citadel_sql::Value::Integer(25)
            ]
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn panic() {
    let pool = create_pool();
    {
        let conn = pool.get().await.unwrap();
        let result = conn
            .interact::<_, ()>(|_| {
                panic!("Whopsies!");
            })
            .await;
        assert!(matches!(result, Err(InteractError::Panic(_))));
    }
    let conn = pool.get().await.unwrap();
    conn.interact(|inner| {
        inner.execute("SELECT 1").expect("Failed to execute");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn file_based_database() {
    let dir = std::env::temp_dir().join(format!("deadpool-citadeldb-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("test.db");
    let path = db_path.to_str().unwrap();

    let pool = create_pool_with_path(path);
    let conn = pool.get().await.unwrap();
    let result: i64 = conn
        .interact(|inner| {
            inner
                .execute("CREATE TABLE IF NOT EXISTS _t_file (id INTEGER PRIMARY KEY, name TEXT)")
                .expect("Failed to create table");
            inner
                .execute("INSERT INTO _t_file (id, name) VALUES (42, 'test')")
                .expect("Failed to insert");
            let result = inner
                .query("SELECT id FROM _t_file WHERE name = 'test'")
                .expect("Failed to query");
            let row = result.rows.first().expect("No rows returned");
            let val = row.first().expect("Empty row");
            match val {
                citadel_sql::Value::Integer(i) => *i,
                _ => panic!("Expected integer, got {:?}", val),
            }
        })
        .await
        .unwrap();
    assert_eq!(result, 42);

    drop(pool);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn multiple_connections() {
    let dir = std::env::temp_dir().join(format!(
        "deadpool-citadeldb-test-{}",
        std::process::id() + 1
    ));
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("test.db");
    let path = db_path.to_str().unwrap();

    let pool = create_pool_with_path(path);
    let conn1 = pool.get().await.unwrap();

    conn1
        .interact(|inner| {
            inner
                .execute("CREATE TABLE IF NOT EXISTS _t_multi (id INTEGER PRIMARY KEY, val TEXT)")
                .expect("Failed to create table");
            inner
                .execute("INSERT INTO _t_multi (id, val) VALUES (1, 'hello')")
                .expect("Failed to insert");
        })
        .await
        .unwrap();

    let conn2 = pool.get().await.unwrap();
    let result: String = conn2
        .interact(|inner| {
            let result = inner
                .query("SELECT val FROM _t_multi WHERE id = 1")
                .expect("Failed to query");
            let row = result.rows.first().expect("No rows returned");
            let val = row.first().expect("Empty row");
            match val {
                citadel_sql::Value::Text(s) => s.to_string(),
                _ => panic!("Expected text, got {:?}", val),
            }
        })
        .await
        .unwrap();
    assert_eq!(result, "hello");

    drop(pool);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn concurrent_connections() {
    let dir = std::env::temp_dir().join(format!(
        "deadpool-citadeldb-test-{}",
        std::process::id() + 2
    ));
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("test.db");
    let path = db_path.to_str().unwrap();

    let pool = Arc::new(create_pool_with_path(path));

    // Insert test data first (sequentially — CitadelDB serializes writes)
    {
        let conn = pool.get().await.unwrap();
        conn.interact(|inner| {
            inner
                .execute(
                    "CREATE TABLE IF NOT EXISTS _t_concurrent (id INTEGER PRIMARY KEY, val TEXT)",
                )
                .expect("Failed to create table");
            for i in 0..10 {
                inner
                    .execute(&format!(
                        "INSERT OR IGNORE INTO _t_concurrent (id, val) VALUES ({i}, 'data_{i}')"
                    ))
                    .expect("Failed to insert");
            }
        })
        .await
        .unwrap();
    }

    // Then concurrently read
    let mut handles = Vec::new();
    for i in 0..10 {
        let pool = Arc::clone(&pool);
        handles.push(tokio::spawn(async move {
            let conn = pool.get().await.expect("Failed to get connection");
            conn.interact(move |inner| {
                let result = inner
                    .query(&format!("SELECT val FROM _t_concurrent WHERE id = {i}"))
                    .expect("Failed to query");
                let row = result.rows.first().expect("No rows returned");
                let val = row.first().expect("Empty row");
                match val {
                    citadel_sql::Value::Text(s) => assert_eq!(s.as_str(), format!("data_{i}")),
                    _ => panic!("Expected text, got {:?}", val),
                }
            })
            .await
            .expect("Interact failed");
        }));
    }

    for handle in handles {
        handle.await.expect("Task failed");
    }

    drop(pool);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn concurrent_write() {
    let dir = std::env::temp_dir().join(format!(
        "deadpool-citadeldb-test-{}",
        std::process::id() + 3
    ));
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("test.db");
    let path = db_path.to_str().unwrap();

    let pool = Arc::new(create_pool_with_path(path));

    {
        let conn = pool.get().await.unwrap();
        conn.interact(|inner| {
            inner
                .execute(
                    "CREATE TABLE IF NOT EXISTS _t_concurrent_write (id INTEGER PRIMARY KEY AUTOINCREMENT, thread INTEGER, seq INTEGER)",
                )
                .expect("Failed to create table");
        })
        .await
        .expect("Interact failed");
    }

    // Insert test data concurrently, 8*10 rows
    // citadeldb only supports one concurrent writer — serialize via mutex
    let write_lock = Arc::new(Mutex::new(()));
    let writers: Vec<_> = (0..8)
        .map(|i| {
            let pool = Arc::clone(&pool);
            let write_lock = Arc::clone(&write_lock);
            tokio::spawn(async move {
                let _guard = write_lock.lock().await;
                let conn = pool.get().await.unwrap();
                conn.interact(move |inner| {
                    let stmt = inner
                        .prepare(
                            "INSERT INTO _t_concurrent_write (id, thread, seq) VALUES ($1, $2, $3)",
                        )
                        .expect("Failed to prepare statement");
                    for j in 0..10 {
                        let id = i * 10 + j + 1;
                        stmt.execute(&[
                            citadel_sql::Value::Integer(id as i64),
                            citadel_sql::Value::Integer(i as i64),
                            citadel_sql::Value::Integer(j as i64),
                        ])
                        .expect("Failed to insert");
                    }
                })
                .await
                .expect("Interact failed");
            })
        })
        .collect();

    for writer in writers {
        writer.await.expect("Task failed");
    }

    {
        let conn = pool.get().await.unwrap();
        let count: i64 = conn
            .interact(|inner| {
                let result = inner
                    .query("SELECT count(*) FROM _t_concurrent_write")
                    .expect("Failed to query");
                let row = result.rows.first().expect("No rows returned");
                let val = row.first().expect("Empty row");
                match val {
                    citadel_sql::Value::Integer(n) => *n,
                    _ => panic!("Expected 80 rows, got {:?}", val),
                }
            })
            .await
            .unwrap();
        assert_eq!(count, 80);
    }

    drop(pool);
    let _ = std::fs::remove_dir_all(&dir);
}
