use bytes::Bytes;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub enum DatabaseError {
    PoisonedLock,
    WrongType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RedisValue {
    String(Bytes),
    List(VecDeque<Bytes>),
    Set(HashSet<Bytes>),
}
/// We use Arc to share it across connection tasks and RwLock to allow
/// concurrent reads but exclusive writes.
#[derive(Clone, Debug)]
pub struct KvStore {
    // We use Bytes because it's cheap to clone (reference counted)
    db: Arc<RwLock<HashMap<String, RedisValue>>>,
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_range(start: i64, stop: i64, len: usize) -> (usize, usize) {
    let len = len as i64;

    let mut start = if start < 0 { len + start } else { start };
    let mut stop = if stop < 0 { len + stop } else { stop };

    start = start.clamp(0, len);
    stop = stop.clamp(0, len - 1);

    if start > stop || len == 0 {
        return (0, 0); // Empty range
    }

    (start as usize, stop as usize)
}

impl KvStore {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Sets a value in the store.
    pub fn set(&self, key: String, value: Bytes) -> Result<(), DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        db.insert(key, RedisValue::String(value));
        Ok(())
    }

    /// Gets a value from the store.
    /// Returns None if the key does not exist.
    pub fn get(&self, key: &str) -> Result<Option<RedisValue>, DatabaseError> {
        let db = self.db.read().map_err(|_| DatabaseError::PoisonedLock)?;
        Ok(db.get(key).cloned()) // Cloning Bytes is O(1)
    }

    /// Removes a key from the store.
    pub fn del(&self, key: &str) -> Result<bool, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        Ok(db.remove(key).is_some())
    }

    pub fn lpush(&self, key: String, values: Vec<Bytes>) -> Result<i64, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        let entry = db
            .entry(key)
            .or_insert_with(|| RedisValue::List(VecDeque::new()));
        match entry {
            RedisValue::List(list) => {
                for val in values {
                    list.push_front(val);
                }
                Ok(list.len() as i64)
            }
            _ => Err(DatabaseError::WrongType),
        }
    }

    pub fn lpop(&self, key: &str, count: i64) -> Result<Vec<Bytes>, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        let (popped_elements, should_remove) = match db.get_mut(key) {
            Some(RedisValue::List(list)) => {
                let length = list.len();
                let num_pop = std::cmp::min(length, count as usize);
                let popped: Vec<Bytes> = list.drain(..num_pop).collect();
                (popped, list.is_empty())
            }
            Some(_) => return Err(DatabaseError::WrongType),
            None => return Ok(vec![]),
        };

        if should_remove {
            db.remove(key);
        }

        Ok(popped_elements)
    }

    pub fn rpush(&self, key: String, values: Vec<Bytes>) -> Result<i64, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        let entry = db
            .entry(key)
            .or_insert_with(|| RedisValue::List(VecDeque::new()));
        match entry {
            RedisValue::List(list) => {
                for val in values {
                    list.push_back(val);
                }
                Ok(list.len() as i64)
            }
            _ => Err(DatabaseError::WrongType),
        }
    }

    pub fn rpop(&self, key: &str, count: i64) -> Result<Vec<Bytes>, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        let (popped_elements, should_remove) = match db.get_mut(key) {
            Some(RedisValue::List(list)) => {
                let length = list.len();
                let num_pop = std::cmp::min(length, count as usize);
                let popped: Vec<Bytes> = list.drain((length - num_pop)..).collect();
                (popped, list.is_empty())
            }
            Some(_) => return Err(DatabaseError::WrongType),
            None => return Ok(vec![]),
        };

        if should_remove {
            db.remove(key);
        }

        Ok(popped_elements)
    }

    pub fn lrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<Bytes>, DatabaseError> {
        let db = self.db.read().map_err(|_| DatabaseError::PoisonedLock)?;

        let val = match db.get(key) {
            Some(RedisValue::List(list)) => list,
            Some(_) => return Err(DatabaseError::WrongType),
            None => return Ok(vec![]),
        };

        let len = val.len();
        if len == 0 {
            return Ok(vec![]);
        }

        let (start_idx, stop_idx) = resolve_range(start, stop, len);

        if start_idx > stop_idx && len > 0 && !(start_idx == 0 && stop_idx == 0) {
            return Ok(vec![]);
        }

        let count = (stop_idx - start_idx) + 1;
        let result = val
            .iter()
            .skip(start_idx)
            .take(count)
            .cloned() // Increments ref-count on Bytes, very fast
            .collect();

        Ok(result)
    }

    pub fn sadd(&self, key: String, values: Vec<Bytes>) -> Result<i64, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        let entry = db
            .entry(key)
            .or_insert_with(|| RedisValue::Set(HashSet::new()));

        match entry {
            RedisValue::Set(set) => {
                for val in values {
                    set.insert(val);
                }
                Ok(set.len() as i64)
            }
            _ => Err(DatabaseError::WrongType),
        }
    }

    pub fn spop(&self, key: &str, count: i64) -> Result<Vec<Bytes>, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;

        let (popped_elements, should_remove) = match db.get_mut(key) {
            Some(RedisValue::Set(set)) => {
                let num_to_pop = std::cmp::min(set.len(), count as usize);
                let mut popped = Vec::with_capacity(num_to_pop);

                for _ in 0..num_to_pop {
                    if let Some(member) = set.iter().next().cloned() {
                        set.remove(&member);
                        popped.push(member);
                    }
                }
                (popped, set.is_empty())
            }
            Some(_) => return Err(DatabaseError::WrongType),
            None => return Ok(vec![]),
        };

        if should_remove {
            db.remove(key);
        }

        Ok(popped_elements)
    }

    pub fn smembers(&self, key: &str) -> Result<Vec<Bytes>, DatabaseError> {
        let db = self.db.read().map_err(|_| DatabaseError::PoisonedLock)?;

        match db.get(key) {
            Some(RedisValue::Set(set)) => {
                let members: Vec<Bytes> = set.iter().cloned().collect();
                Ok(members)
            }
            Some(_) => Err(DatabaseError::WrongType),
            None => Ok(vec![]),
        }
    }
}

// =================== UNIT TESTS ========================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn poison_store(store: &KvStore) {
        let store_clone = store.clone();
        let handle = thread::spawn(move || {
            let _guard = store_clone.db.write().unwrap();
            panic!("Intentional panic to poison the lock");
        });
        let _ = handle.join();
    }

    #[test]
    fn test_set_error_poisoned() {
        let store = KvStore::new();
        poison_store(&store);

        let res = store.set("key".into(), Bytes::from("val"));
        assert!(matches!(res, Err(DatabaseError::PoisonedLock)));
    }

    #[test]
    fn test_get_error_poisoned() {
        let store = KvStore::new();
        poison_store(&store);

        let res = store.get("key");
        assert!(matches!(res, Err(DatabaseError::PoisonedLock)));
    }

    #[test]
    fn test_del_error_poisoned() {
        let store = KvStore::new();
        poison_store(&store);

        let res = store.del("key");
        assert!(matches!(res, Err(DatabaseError::PoisonedLock)));
    }
}
