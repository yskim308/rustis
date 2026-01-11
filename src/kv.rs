use bytes::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// We use Arc to share it across connection tasks and RwLock to allow
/// concurrent reads but exclusive writes.
#[derive(Clone, Debug)]
pub struct KvStore {
    // We use Bytes because it's cheap to clone (reference counted)
    db: Arc<RwLock<HashMap<String, Bytes>>>,
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KvStore {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Sets a value in the store.
    pub fn set(&self, key: String, value: Bytes) {
        let mut db = self.db.write().unwrap();
        db.insert(key, value);
    }

    /// Gets a value from the store.
    /// Returns None if the key does not exist.
    pub fn get(&self, key: &str) -> Option<Bytes> {
        let db = self.db.read().unwrap();
        db.get(key).cloned() // Cloning Bytes is O(1)
    }

    /// Removes a key from the store.
    pub fn del(&self, key: &str) -> bool {
        let mut db = self.db.write().unwrap();
        db.remove(key).is_some()
    }
}
