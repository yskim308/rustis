use bytes::Bytes;
use rustis::kv::{DatabaseError, KvStore, RedisValue};

// =================== HAPPY PATH TESTS ===================

#[test]
fn happy_set_get() {
    let store = KvStore::new();
    let key = Bytes::from("key");
    let val = Bytes::from("value");

    store.set(key.clone(), val.clone()).unwrap();

    let result = store.get(&key).unwrap();
    assert_eq!(result, Some(RedisValue::String(val)));
}

#[test]
fn happy_lpush() {
    let store = KvStore::new();
    let key = Bytes::from("list");

    let len = store
        .lpush(key.clone(), vec![Bytes::from("b"), Bytes::from("a")])
        .unwrap();
    assert_eq!(len, 2);

    if let Some(RedisValue::List(list)) = store.get(&key).unwrap() {
        assert_eq!(list[0], Bytes::from("a"));
        assert_eq!(list[1], Bytes::from("b"));
    } else {
        panic!("Expected list");
    }
}

#[test]
fn happy_rpush() {
    let store = KvStore::new();
    let key = Bytes::from("list");

    let len = store
        .rpush(key.clone(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();
    assert_eq!(len, 2);

    if let Some(RedisValue::List(list)) = store.get(&key).unwrap() {
        assert_eq!(list[0], Bytes::from("a"));
        assert_eq!(list[1], Bytes::from("b"));
    } else {
        panic!("Expected list");
    }
}

#[test]
fn happy_lrange() {
    let store = KvStore::new();
    let key = Bytes::from("list");

    store
        .rpush(
            key.clone(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("c")],
        )
        .unwrap();

    let result = store.lrange(&key, 0, 1).unwrap();
    assert_eq!(result, vec![Bytes::from("a"), Bytes::from("b")]);
}

// =================== UNHAPPY PATH TESTS ===================

#[test]
fn unhappy_get_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn unhappy_lrange_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert_eq!(store.lrange(&key, 0, 10).unwrap(), Vec::<Bytes>::new());
}

// =================== LIST POP TESTS ===================

#[test]
fn happy_lpop() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    // lpush adds to front: "a" then "b" -> ["b", "a"]
    store
        .lpush(key.clone(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();

    // Pop 1 element from left (front) -> "b"
    let result = store.lpop(&key, 1).unwrap();
    assert_eq!(result, vec![Bytes::from("b")]);

    // Verify "a" remains
    let remaining = store.lrange(&key, 0, 10).unwrap();
    assert_eq!(remaining, vec![Bytes::from("a")]);
}

#[test]
fn happy_rpop() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    // rpush adds to back: "a" then "b" -> ["a", "b"]
    store
        .rpush(key.clone(), vec![Bytes::from("a"), Bytes::from("b")])
        .unwrap();

    // Pop 1 element from right (back) -> "b"
    let result = store.rpop(&key, 1).unwrap();
    assert_eq!(result, vec![Bytes::from("b")]);

    // Verify "a" remains
    let remaining = store.lrange(&key, 0, 10).unwrap();
    assert_eq!(remaining, vec![Bytes::from("a")]);
}

#[test]
fn unhappy_lpop_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert_eq!(store.lpop(&key, 1).unwrap(), Vec::<Bytes>::new());
}

#[test]
fn unhappy_rpop_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert_eq!(store.rpop(&key, 1).unwrap(), Vec::<Bytes>::new());
}

// =================== SET TESTS ===================

#[test]
fn happy_sadd_and_smembers() {
    let store = KvStore::new();
    let key = Bytes::from("set");

    // Add "a", "b", and duplicate "a". Should return 2 new items.
    let count = store
        .sadd(
            key.clone(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("a")],
        )
        .unwrap();
    assert_eq!(count, 2);

    let mut members = store.smembers(&key).unwrap();
    // Sort to ensure deterministic comparison since sets are unordered
    members.sort();

    assert_eq!(members, vec![Bytes::from("a"), Bytes::from("b")]);
}

#[test]
fn happy_spop() {
    let store = KvStore::new();
    let key = Bytes::from("set");

    store
        .sadd(
            key.clone(),
            vec![Bytes::from("a"), Bytes::from("b"), Bytes::from("c")],
        )
        .unwrap();

    // Pop 1 random element
    let popped = store.spop(&key, 1).unwrap();
    assert_eq!(popped.len(), 1);

    // Should have 2 elements remaining
    let remaining = store.smembers(&key).unwrap();
    assert_eq!(remaining.len(), 2);

    // Ensure the popped element is no longer in the set
    assert!(!remaining.contains(&popped[0]));
}

#[test]
fn unhappy_smembers_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert_eq!(store.smembers(&key).unwrap(), Vec::<Bytes>::new());
}

#[test]
fn unhappy_spop_missing_key() {
    let store = KvStore::new();
    let key = Bytes::from("missing");
    assert_eq!(store.spop(&key, 1).unwrap(), Vec::<Bytes>::new());
}

// =================== TYPE MISMATCH TESTS ===================

#[test]
fn type_mismatch_lpush_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();

    let result = store.lpush(key, vec![Bytes::from("item")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_rpush_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();

    let result = store.rpush(key, vec![Bytes::from("item")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_lrange_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();

    let result = store.lrange(&key, 0, 10);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_lpop_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();
    assert!(matches!(store.lpop(&key, 1), Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_rpop_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();
    assert!(matches!(store.rpop(&key, 1), Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_sadd_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();

    let result = store.sadd(key, vec![Bytes::from("a")]);
    assert!(matches!(result, Err(DatabaseError::WrongType)));
}

#[test]
fn type_mismatch_smembers_on_list() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.lpush(key.clone(), vec![Bytes::from("val")]).unwrap();

    assert!(matches!(
        store.smembers(&key),
        Err(DatabaseError::WrongType)
    ));
}

#[test]
fn type_mismatch_spop_on_string() {
    let store = KvStore::new();
    let key = Bytes::from("key");

    store.set(key.clone(), Bytes::from("value")).unwrap();

    assert!(matches!(store.spop(&key, 1), Err(DatabaseError::WrongType)));
}
