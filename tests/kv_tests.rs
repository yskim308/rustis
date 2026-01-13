use bytes::Bytes;
use rustis::kv::{DatabaseError, KvStore, RedisValue};

// =================== HAPPY PATH TESTS ===================

#[test]
fn happy_set_get() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    let result = store.get("key").unwrap();
    assert_eq!(result, Some(RedisValue::String(Bytes::from("value"))));
}

#[test]
fn happy_del() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    assert!(store.del("key").unwrap());
    assert!(store.get("key").unwrap().is_none());
}

#[test]
fn happy_lpush() {
    let store = KvStore::new();

    let len = store
        .lpush("list".into(), vec![Bytes::from("b"), Bytes::from("a")])
        .unwrap();
    assert_eq!(len, 2);

    if let Some(RedisValue::List(list)) = store.get("list").unwrap() {
        assert_eq!(list[0], Bytes::from("a"));
        assert_eq!(list[1], Bytes::from("b"));
    } else {
        panic!("Expected list");
    }
}

#[test]
fn happy_rpush() {
    let store = KvStore::new();

    let len = store
        .rpush("list".into(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();
    assert_eq!(len, 2);

    if let Some(RedisValue::List(list)) = store.get("list").unwrap() {
        assert_eq!(list[0], Bytes::from("a"));
        assert_eq!(list[1], Bytes::from("b"));
    } else {
        panic!("Expected list");
    }
}

#[test]
fn happy_lrange() {
    let store = KvStore::new();
    store
        .rpush(
            "list".into(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("c")],
        )
        .unwrap();

    let result = store.lrange("list", 0, 1).unwrap();
    assert_eq!(result, vec![Bytes::from("a"), Bytes::from("b")]);
}

// =================== UNHAPPY PATH TESTS ===================

#[test]
fn unhappy_get_missing_key() {
    let store = KvStore::new();
    assert!(store.get("missing").unwrap().is_none());
}

#[test]
fn unhappy_del_missing_key() {
    let store = KvStore::new();
    assert!(!store.del("missing").unwrap());
}

#[test]
fn unhappy_lrange_missing_key() {
    let store = KvStore::new();
    assert_eq!(store.lrange("missing", 0, 10).unwrap(), Vec::<Bytes>::new());
}

// =================== TYPE MISMATCH TESTS ===================

#[test]
fn type_mismatch_lpush_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    let result = store.lpush("key".into(), vec![Bytes::from("item")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_rpush_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    let result = store.rpush("key".into(), vec![Bytes::from("item")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_lrange_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    let result = store.lrange("key", 0, 10);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}
