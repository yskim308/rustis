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

// =================== LIST POP TESTS ===================

#[test]
fn happy_lpop() {
    let store = KvStore::new();
    // lpush adds to front: "a" then "b" -> ["b", "a"]
    store
        .lpush("key".into(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();

    // Pop 1 element from left (front) -> "b"
    let result = store.lpop("key", 1).unwrap();
    assert_eq!(result, vec![Bytes::from("b")]);

    // Verify "a" remains
    let remaining = store.lrange("key", 0, 10).unwrap();
    assert_eq!(remaining, vec![Bytes::from("a")]);
}

#[test]
fn happy_rpop() {
    let store = KvStore::new();
    // rpush adds to back: "a" then "b" -> ["a", "b"]
    store
        .rpush("key".into(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();

    // Pop 1 element from right (back) -> "b"
    let result = store.rpop("key", 1).unwrap();
    assert_eq!(result, vec![Bytes::from("b")]);

    // Verify "a" remains
    let remaining = store.lrange("key", 0, 10).unwrap();
    assert_eq!(remaining, vec![Bytes::from("a")]);
}

#[test]
fn unhappy_lpop_missing_key() {
    let store = KvStore::new();
    assert_eq!(store.lpop("missing", 1).unwrap(), Vec::<Bytes>::new());
}

#[test]
fn unhappy_rpop_missing_key() {
    let store = KvStore::new();
    assert_eq!(store.rpop("missing", 1).unwrap(), Vec::<Bytes>::new());
}

// =================== SET TESTS ===================

#[test]
fn happy_sadd_and_smembers() {
    let store = KvStore::new();

    // Add "a", "b", and duplicate "a". Should return 2 new items.
    let count = store
        .sadd(
            "set".into(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("a")],
        )
        .unwrap();
    assert_eq!(count, 2);

    let mut members = store.smembers("set").unwrap();
    // Sort to ensure deterministic comparison since sets are unordered
    members.sort();

    assert_eq!(members, vec![Bytes::from("a"), Bytes::from("b")]);
}

#[test]
fn happy_spop() {
    let store = KvStore::new();
    store
        .sadd(
            "set".into(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("c")],
        )
        .unwrap();

    // Pop 1 random element
    let popped = store.spop("set", 1).unwrap();
    assert_eq!(popped.len(), 1);

    // Should have 2 elements remaining
    let remaining = store.smembers("set").unwrap();
    assert_eq!(remaining.len(), 2);

    // Ensure the popped element is no longer in the set
    assert!(!remaining.contains(&popped[0]));
}

#[test]
fn unhappy_smembers_missing_key() {
    let store = KvStore::new();
    assert_eq!(store.smembers("missing").unwrap(), Vec::<Bytes>::new());
}

#[test]
fn unhappy_spop_missing_key() {
    let store = KvStore::new();
    assert_eq!(store.spop("missing", 1).unwrap(), Vec::<Bytes>::new());
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

#[test]
fn type_mismatch_lpop_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();
    assert!(matches!(
        store.lpop("key", 1),
        Err(DatabaseError::WrongType)
    ));
}

#[test]
fn type_mismatch_rpop_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();
    assert!(matches!(
        store.rpop("key", 1),
        Err(DatabaseError::WrongType)
    ));
}

#[test]
fn type_mismatch_sadd_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    let result = store.sadd("key".into(), vec![Bytes::from("a")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_smembers_on_list() {
    let store = KvStore::new();
    store.lpush("key".into(), vec![Bytes::from("val")]).unwrap();

    assert!(matches!(
        store.smembers("key"),
        Err(DatabaseError::WrongType)
    ));
}

#[test]
fn type_mismatch_spop_on_string() {
    let store = KvStore::new();
    store.set("key".into(), Bytes::from("value")).unwrap();

    assert!(matches!(
        store.spop("key", 1),
        Err(DatabaseError::WrongType)
    ));
}
